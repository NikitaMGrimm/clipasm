use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::SourceTimeRange;

#[derive(Clone, Debug)]
pub struct Workflow {
    pub source_path: PathBuf,
    pub version: u64,
    pub video: VideoSettings,
    pub clips: Vec<NamedClip>,
    pub timeline: Vec<Item>,
    pub output: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct VideoSettings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NamedClip {
    pub name: String,
    pub body: Vec<Item>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub id: Option<(String, SourceSpan)>,
    pub during: Option<(SourceTimeRange, SourceSpan)>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    Call {
        program: String,
        arguments: BTreeMap<String, Argument>,
    },
    Then(Vec<Item>),
    Join(Vec<Item>),
    Timeline(Vec<Item>),
}

#[derive(Clone, Debug)]
pub enum Argument {
    Reference(String, SourceSpan),
    String(String, SourceSpan),
    Integer(i64, SourceSpan),
    List(Vec<Argument>, SourceSpan),
}

impl Argument {
    #[must_use]
    pub fn span(&self) -> &SourceSpan {
        match self {
            Self::Reference(_, span)
            | Self::String(_, span)
            | Self::Integer(_, span)
            | Self::List(_, span) => span,
        }
    }
}

#[derive(Clone, Debug)]
struct RawNode {
    kind: RawKind,
    span: SourceSpan,
}

#[derive(Clone, Debug)]
enum RawKind {
    Scalar { value: String, style: TScalarStyle },
    Sequence(Vec<RawNode>),
    Mapping(Vec<(String, SourceSpan, RawNode)>),
}

struct EventSink {
    events: Vec<(Event, Marker)>,
}

impl MarkedEventReceiver for EventSink {
    fn on_event(&mut self, event: Event, marker: Marker) {
        self.events.push((event, marker));
    }
}

/// Parse and normalize a restricted `RhythmCut` YAML document.
///
/// # Errors
///
/// Returns a source-located diagnostic when the file cannot be read or violates
/// the restricted YAML or workflow grammar.
pub fn parse_file(path: &Path) -> Result<Workflow> {
    let source =
        fs::read_to_string(path).map_err(|error| Diagnostic::io("E_WORKFLOW_IO", path, &error))?;
    let source_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    parse_str(&source_path, &source)
}

/// Parse workflow source supplied by tests or an embedding application.
///
/// # Errors
///
/// Returns a source-located diagnostic for invalid YAML or workflow syntax.
pub fn parse_str(path: &Path, source: &str) -> Result<Workflow> {
    let mut sink = EventSink { events: Vec::new() };
    Parser::new_from_str(source)
        .load(&mut sink, true)
        .map_err(|error| {
            let marker = error.marker();
            Diagnostic::new(
                "E_YAML_SYNTAX",
                error.info().to_owned(),
                SourceSpan::new(path, marker.line(), marker.col()),
            )
        })?;

    let document_count = sink
        .events
        .iter()
        .filter(|(event, _)| matches!(event, Event::DocumentStart))
        .count();
    if document_count != 1 {
        return Err(Diagnostic::new(
            "E_YAML_DOCUMENT_COUNT",
            "a workflow must contain exactly one YAML document",
            SourceSpan::file_start(path),
        ));
    }

    let start = sink
        .events
        .iter()
        .position(|(event, _)| matches!(event, Event::DocumentStart))
        .map_or(0, |index| index + 1);
    let mut cursor = start;
    let root = raw_node(&sink.events, &mut cursor, path)?;
    parse_workflow(source_path(path), root)
}

fn source_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn raw_node(events: &[(Event, Marker)], cursor: &mut usize, path: &Path) -> Result<RawNode> {
    let Some((event, marker)) = events.get(*cursor) else {
        return Err(Diagnostic::new(
            "E_YAML_SYNTAX",
            "unexpected end of YAML input",
            SourceSpan::file_start(path),
        ));
    };
    *cursor += 1;
    let span = SourceSpan::new(path, marker.line(), marker.col());
    match event {
        Event::Alias(_) => Err(Diagnostic::new(
            "E_YAML_ALIAS",
            "YAML aliases are not supported",
            span,
        )),
        Event::Scalar(value, style, anchor, tag) => {
            reject_properties(*anchor, tag.is_some(), &span)?;
            Ok(RawNode {
                kind: RawKind::Scalar {
                    value: value.clone(),
                    style: *style,
                },
                span,
            })
        }
        Event::SequenceStart(anchor, tag) => {
            reject_properties(*anchor, tag.is_some(), &span)?;
            let mut values = Vec::new();
            while !matches!(
                events.get(*cursor).map(|item| &item.0),
                Some(Event::SequenceEnd)
            ) {
                values.push(raw_node(events, cursor, path)?);
            }
            *cursor += 1;
            Ok(RawNode {
                kind: RawKind::Sequence(values),
                span,
            })
        }
        Event::MappingStart(anchor, tag) => {
            reject_properties(*anchor, tag.is_some(), &span)?;
            let mut entries = Vec::new();
            let mut keys = BTreeSet::new();
            while !matches!(
                events.get(*cursor).map(|item| &item.0),
                Some(Event::MappingEnd)
            ) {
                let key = raw_node(events, cursor, path)?;
                let (key_text, key_span) = match key.kind {
                    RawKind::Scalar { value, .. } => (value, key.span),
                    _ => {
                        return Err(Diagnostic::new(
                            "E_YAML_MAPPING_KEY",
                            "YAML mapping keys must be strings",
                            key.span,
                        ));
                    }
                };
                if !keys.insert(key_text.clone()) {
                    return Err(Diagnostic::new(
                        "E_DUPLICATE_YAML_KEY",
                        format!("duplicate mapping key `{key_text}`"),
                        key_span,
                    ));
                }
                let value = raw_node(events, cursor, path)?;
                entries.push((key_text, key_span, value));
            }
            *cursor += 1;
            Ok(RawNode {
                kind: RawKind::Mapping(entries),
                span,
            })
        }
        _ => Err(Diagnostic::new(
            "E_YAML_SYNTAX",
            "expected a YAML value",
            span,
        )),
    }
}

fn reject_properties(anchor: usize, has_tag: bool, span: &SourceSpan) -> Result<()> {
    if anchor != 0 {
        return Err(Diagnostic::new(
            "E_YAML_ANCHOR",
            "YAML anchors are not supported",
            span.clone(),
        ));
    }
    if has_tag {
        return Err(Diagnostic::new(
            "E_YAML_TAG",
            "custom YAML tags are not supported",
            span.clone(),
        ));
    }
    Ok(())
}

fn parse_workflow(source_path: PathBuf, root: RawNode) -> Result<Workflow> {
    let root_span = root.span.clone();
    let entries = into_mapping(root, "the workflow root")?;
    let mut version = None;
    let mut video = VideoSettings::default();
    let mut clips = Vec::new();
    let mut timeline = None;
    let mut output = None;

    for (key, key_span, value) in entries {
        match key.as_str() {
            "version" => {
                let (text, _) = scalar(&value, "`version`")?;
                version = Some(text.parse::<u64>().map_err(|_| {
                    Diagnostic::new(
                        "E_UNSUPPORTED_VERSION",
                        "`version` must be the integer 1",
                        value.span.clone(),
                    )
                })?);
            }
            "project" => video = parse_project(value)?,
            "clips" => clips = parse_clips(value)?,
            "timeline" => timeline = Some(parse_body(value, "`timeline`")?),
            "output" => {
                let (text, _) = scalar(&value, "`output`")?;
                output = Some(PathBuf::from(text));
            }
            _ => {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_TOP_LEVEL_FIELD",
                    format!("unknown top-level field `{key}`"),
                    key_span,
                ));
            }
        }
    }

    let version = version.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_VERSION",
            "missing required top-level field `version`",
            root_span.clone(),
        )
    })?;
    if version != 1 {
        return Err(Diagnostic::new(
            "E_UNSUPPORTED_VERSION",
            format!("unsupported workflow version {version}; this engine supports version 1"),
            root_span.clone(),
        ));
    }
    let timeline = timeline.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_TIMELINE",
            "missing required top-level field `timeline`",
            root_span,
        )
    })?;

    Ok(Workflow {
        source_path,
        version,
        video,
        clips,
        timeline,
        output,
    })
}

fn parse_project(node: RawNode) -> Result<VideoSettings> {
    let entries = into_mapping(node, "`project`")?;
    let mut video = VideoSettings::default();
    for (key, key_span, value) in entries {
        if key != "video" {
            return Err(Diagnostic::new(
                "E_UNKNOWN_PROJECT_FIELD",
                format!("unknown project field `{key}`"),
                key_span,
            ));
        }
        for (setting, setting_span, value) in into_mapping(value, "`project.video`")? {
            let (text, _) = scalar(&value, "a video setting")?;
            match setting.as_str() {
                "width" => video.width = Some(parse_u32(text, &value.span, "width")?),
                "height" => video.height = Some(parse_u32(text, &value.span, "height")?),
                "fps" => video.fps = Some(text.to_owned()),
                _ => {
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_VIDEO_FIELD",
                        format!("unknown project video field `{setting}`"),
                        setting_span,
                    ));
                }
            }
        }
    }
    Ok(video)
}

fn parse_u32(text: &str, span: &SourceSpan, field: &str) -> Result<u32> {
    let value = text.parse::<u32>().map_err(|_| {
        Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            format!("`{field}` must be a positive integer"),
            span.clone(),
        )
    })?;
    if value == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            format!("`{field}` must be greater than zero"),
            span.clone(),
        ));
    }
    Ok(value)
}

fn parse_clips(node: RawNode) -> Result<Vec<NamedClip>> {
    let mut clips = Vec::new();
    for (name, span, value) in into_mapping(node, "`clips`")? {
        validate_name(&name, &span)?;
        let body = match value.kind {
            RawKind::Sequence(_) => parse_body(value, "a clip body")?,
            RawKind::Mapping(_) | RawKind::Scalar { .. } => vec![parse_item(value)?],
        };
        clips.push(NamedClip { name, body, span });
    }
    Ok(clips)
}

fn parse_body(node: RawNode, owner: &str) -> Result<Vec<Item>> {
    let span = node.span.clone();
    let RawKind::Sequence(values) = node.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_SEQUENCE",
            format!("{owner} must be a YAML sequence"),
            span,
        ));
    };
    values.into_iter().map(parse_item).collect()
}

fn parse_item(node: RawNode) -> Result<Item> {
    let item_span = node.span.clone();
    match node.kind {
        RawKind::Scalar { value, style } => {
            if style == TScalarStyle::Plain && value.starts_with('$') {
                let reference = parse_reference(&value, &item_span)?;
                let mut arguments = BTreeMap::new();
                arguments.insert(
                    "video".to_owned(),
                    Argument::Reference(reference, item_span.clone()),
                );
                Ok(Item {
                    kind: ItemKind::Call {
                        program: "clip".to_owned(),
                        arguments,
                    },
                    id: None,
                    during: None,
                    span: item_span,
                })
            } else if style == TScalarStyle::Plain {
                Ok(Item {
                    kind: ItemKind::Call {
                        program: value,
                        arguments: BTreeMap::new(),
                    },
                    id: None,
                    during: None,
                    span: item_span,
                })
            } else {
                Err(Diagnostic::new(
                    "E_INVALID_SEQUENCE_ITEM",
                    "a scalar sequence item must be a plain `$reference` or no-argument program name",
                    item_span,
                ))
            }
        }
        RawKind::Mapping(entries) => parse_invocation(entries, item_span),
        RawKind::Sequence(_) => Err(Diagnostic::new(
            "E_INVALID_SEQUENCE_ITEM",
            "a sequence item must be a program invocation or reference",
            item_span,
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_invocation(entries: Vec<(String, SourceSpan, RawNode)>, span: SourceSpan) -> Result<Item> {
    let mut id = None;
    let mut during = None;
    let mut program_entries = Vec::new();
    for (key, key_span, value) in entries {
        match key.as_str() {
            "id" => {
                let (name, _) = scalar(&value, "`id`")?;
                validate_name(name, &value.span)?;
                id = Some((name.to_owned(), value.span));
            }
            "during" => {
                let (range, _) = scalar(&value, "`during`")?;
                during = Some((SourceTimeRange::parse(range, &value.span)?, value.span));
            }
            _ => program_entries.push((key, key_span, value)),
        }
    }
    if program_entries.is_empty() {
        return Err(Diagnostic::new(
            "E_MISSING_PROGRAM_KEY",
            "an invocation mapping must contain one program key",
            span,
        ));
    }

    let candidates = program_entries
        .iter()
        .enumerate()
        .filter(|(_, (key, _, _))| {
            known_program(key) || matches!(key.as_str(), "then" | "join" | "timeline")
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let program_index = match candidates.as_slice() {
        [index] => *index,
        [] if program_entries.len() == 1 => 0,
        _ => {
            return Err(Diagnostic::new(
                "E_MULTIPLE_PROGRAM_KEYS",
                "an invocation mapping must identify exactly one program key",
                span,
            ));
        }
    };
    let (program, _program_span, primary_value) = program_entries.remove(program_index);
    let kind = match program.as_str() {
        "then" | "join" | "timeline" => {
            if !program_entries.is_empty() {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_INVOCATION_FIELD",
                    format!("compound `{program}` accepts only `id` and `during` fields"),
                    program_entries[0].1.clone(),
                ));
            }
            let body = parse_body(primary_value, &format!("`{program}` body"))?;
            match program.as_str() {
                "then" => ItemKind::Then(body),
                "join" => ItemKind::Join(body),
                "timeline" => ItemKind::Timeline(body),
                _ => unreachable!("matched above"),
            }
        }
        _ => {
            let mut arguments = BTreeMap::new();
            if is_empty_scalar(&primary_value) {
                // No primary shorthand argument.
            } else if matches!(primary_value.kind, RawKind::Mapping(_)) {
                for (name, name_span, value) in
                    into_mapping(primary_value, "a full argument mapping")?
                {
                    if arguments
                        .insert(name.clone(), parse_argument(value)?)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            "E_DUPLICATE_ARGUMENT",
                            format!("duplicate argument `{name}`"),
                            name_span,
                        ));
                    }
                }
            } else {
                let Some(primary) = primary_argument(&program) else {
                    if known_program(&program) {
                        return Err(Diagnostic::new(
                            "E_INVALID_PRIMARY_ARGUMENT",
                            format!("program `{program}` has no primary shorthand argument"),
                            primary_value.span,
                        ));
                    }
                    // Preserve an unknown program for the registry diagnostic.
                    arguments.insert("value".to_owned(), parse_argument(primary_value)?);
                    return finish_invocation(
                        program,
                        arguments,
                        program_entries,
                        id,
                        during,
                        span,
                    );
                };
                arguments.insert(primary.to_owned(), parse_argument(primary_value)?);
            }
            finish_arguments(&program, &mut arguments, program_entries)?;
            ItemKind::Call { program, arguments }
        }
    };

    Ok(Item {
        kind,
        id,
        during,
        span,
    })
}

fn finish_invocation(
    program: String,
    mut arguments: BTreeMap<String, Argument>,
    entries: Vec<(String, SourceSpan, RawNode)>,
    id: Option<(String, SourceSpan)>,
    during: Option<(SourceTimeRange, SourceSpan)>,
    span: SourceSpan,
) -> Result<Item> {
    finish_arguments(&program, &mut arguments, entries)?;
    Ok(Item {
        kind: ItemKind::Call { program, arguments },
        id,
        during,
        span,
    })
}

fn finish_arguments(
    program: &str,
    arguments: &mut BTreeMap<String, Argument>,
    entries: Vec<(String, SourceSpan, RawNode)>,
) -> Result<()> {
    for (name, span, value) in entries {
        if arguments
            .insert(name.clone(), parse_argument(value)?)
            .is_some()
        {
            return Err(Diagnostic::new(
                "E_DUPLICATE_ARGUMENT",
                format!("argument `{name}` was supplied twice"),
                span,
            ));
        }
    }
    validate_argument_names(program, arguments)
}

fn validate_argument_names(program: &str, arguments: &BTreeMap<String, Argument>) -> Result<()> {
    let allowed: &[&str] = match program {
        "image" => &["path", "duration", "fit"],
        "clip" => &["video"],
        "concat" => &["videos"],
        "repeat" => &["video", "count"],
        _ => return Ok(()),
    };
    for (name, value) in arguments {
        if !allowed.contains(&name.as_str()) {
            return Err(Diagnostic::new(
                "E_UNKNOWN_PROGRAM_ARGUMENT",
                format!("unknown argument `{name}` for program `{program}`"),
                value.span().clone(),
            ));
        }
    }
    Ok(())
}

fn parse_argument(node: RawNode) -> Result<Argument> {
    let span = node.span.clone();
    match node.kind {
        RawKind::Scalar { value, style } => {
            if style == TScalarStyle::Plain && value.starts_with('$') {
                Ok(Argument::Reference(parse_reference(&value, &span)?, span))
            } else if style == TScalarStyle::Plain {
                if let Ok(value) = value.parse::<i64>() {
                    Ok(Argument::Integer(value, span))
                } else {
                    Ok(Argument::String(value, span))
                }
            } else {
                Ok(Argument::String(value, span))
            }
        }
        RawKind::Sequence(values) => Ok(Argument::List(
            values
                .into_iter()
                .map(parse_argument)
                .collect::<Result<Vec<_>>>()?,
            span,
        )),
        RawKind::Mapping(_) => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT",
            "nested argument mappings are not supported",
            span,
        )),
    }
}

fn parse_reference(text: &str, span: &SourceSpan) -> Result<String> {
    let Some(name) = text.strip_prefix('$') else {
        return Err(Diagnostic::new(
            "E_INVALID_REFERENCE",
            "a reference must begin with `$`",
            span.clone(),
        ));
    };
    validate_name(name, span)?;
    Ok(name.to_owned())
}

/// Validate a user-visible clip or invocation name.
///
/// # Errors
///
/// Returns `E_INVALID_NAME` when `name` does not match the public identifier
/// grammar.
pub fn validate_name(name: &str, span: &SourceSpan) -> Result<()> {
    let mut chars = name.chars();
    let valid_start = chars
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    let valid_rest =
        chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if !valid_start || !valid_rest {
        return Err(Diagnostic::new(
            "E_INVALID_NAME",
            format!("`{name}` is not a valid name; use [A-Za-z_][A-Za-z0-9_-]*"),
            span.clone(),
        ));
    }
    Ok(())
}

fn primary_argument(program: &str) -> Option<&'static str> {
    match program {
        "image" => Some("path"),
        "clip" => Some("video"),
        "concat" => Some("videos"),
        "repeat" => Some("count"),
        _ => None,
    }
}

fn known_program(program: &str) -> bool {
    matches!(program, "image" | "clip" | "concat" | "repeat")
}

fn is_empty_scalar(node: &RawNode) -> bool {
    matches!(
        &node.kind,
        RawKind::Scalar {
            value,
            style: TScalarStyle::Plain
        } if value.is_empty()
    )
}

fn scalar<'a>(node: &'a RawNode, owner: &str) -> Result<(&'a str, TScalarStyle)> {
    let RawKind::Scalar { value, style } = &node.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_SCALAR",
            format!("{owner} must be a scalar value"),
            node.span.clone(),
        ));
    };
    Ok((value, *style))
}

fn into_mapping(node: RawNode, owner: &str) -> Result<Vec<(String, SourceSpan, RawNode)>> {
    let span = node.span.clone();
    let RawKind::Mapping(entries) = node.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_MAPPING",
            format!("{owner} must be a mapping"),
            span,
        ));
    };
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<Workflow> {
        parse_str(Path::new("workflow.yaml"), source)
    }

    #[test]
    fn normalizes_reference_and_primary_shorthand() {
        let workflow = parse(
            "version: 1\nclips:\n  a:\n    image: a.png\n    duration: 1s\ntimeline:\n  - $a\n",
        )
        .expect("workflow");
        assert!(matches!(
            workflow.timeline[0].kind,
            ItemKind::Call { ref program, .. } if program == "clip"
        ));
        assert_eq!(workflow.clips[0].body.len(), 1);
    }

    #[test]
    fn rejects_duplicate_keys() {
        let error = parse("version: 1\nversion: 1\ntimeline: []\n").expect_err("duplicate");
        assert_eq!(error.code, "E_DUPLICATE_YAML_KEY");
    }

    #[test]
    fn rejects_aliases() {
        let error = parse("version: 1\nclips: &clips {}\ntimeline: *clips\n").expect_err("aliases");
        assert!(matches!(error.code, "E_YAML_ANCHOR" | "E_YAML_ALIAS"));
    }

    #[test]
    fn retains_during_span_and_normalized_range() {
        let workflow = parse(
            "version: 1\ntimeline:\n  - image: a.png\n    duration: 2s\n    during: 0s..1s\n",
        )
        .expect("workflow");
        assert!(workflow.timeline[0].during.is_some());
    }
}
