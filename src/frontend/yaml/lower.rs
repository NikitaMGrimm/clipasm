use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use yaml_rust2::scanner::TScalarStyle;

use super::language::{
    BODY_FIELD, ID_FIELD, IDS_FIELD, Language, PROGRAM_HEADER_FIELD, STACK_ACCESS_FIELD,
};
use super::raw::{RawKind, RawNode};
use crate::diagnostic::{Diagnostic, Result};
use crate::program::{
    Cardinality, InputPort, ParameterType, ProgramDefinition, ProgramImplementation, StackAccess,
};
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, Literal, NamedClip, OutputBindings, ProgramBody,
    ProjectSettings, Reference, SOURCE_PROGRAM_DEFAULT_STACK_ACCESS, SourceExternalImport,
    SourceImport, SourcePackage, SourceParameter, SourceProgram, SourceUnit, SourceUnitId,
    UnlinkedSourceUnit, VideoSettings,
};
use crate::source::{SourceFile, SourceSpan, Spanned};

/// Parse source-program text supplied by tests or an embedding application.
///
/// # Errors
///
/// Returns a source-located diagnostic for invalid YAML or source-program syntax.
pub fn parse_str(path: &Path, source: &str) -> Result<SourcePackage> {
    let language = Language::default();
    let unit = parse_source_with_language(
        SourceFile::new(path.to_path_buf(), source.to_owned()),
        &language,
    )?;
    if let Some(import) = unit.imports.first() {
        return Err(Diagnostic::new(
            "E_IMPORT_REQUIRES_FILE",
            "imports require `parse_file` so relative source files can be loaded",
            import.path.span.clone(),
        ));
    }
    if let Some(external) = unit.externals.first() {
        return Err(Diagnostic::new(
            "E_EXTERNAL_REQUIRES_FILE",
            "external program manifests require `parse_file` so relative files can be loaded",
            external.path.span.clone(),
        ));
    }
    Ok(SourcePackage {
        root: SourceUnitId(0),
        units: vec![SourceUnit {
            source: unit.source,
            imports: Vec::new(),
            externals: Vec::new(),
            project: unit.project,
            program: unit.program,
            output: unit.output,
        }],
        external_programs: Vec::new(),
    })
}

/// Parse source-program text with an explicit language.
///
/// # Errors
///
/// Returns a source-located diagnostic for invalid YAML or source-program syntax.
#[cfg(test)]
pub(crate) fn parse_str_with_language(
    path: &Path,
    source: &str,
    language: &Language,
) -> Result<SourcePackage> {
    let unit = parse_source_with_language(
        SourceFile::new(path.to_path_buf(), source.to_owned()),
        language,
    )?;
    if let Some(import) = unit.imports.first() {
        return Err(Diagnostic::new(
            "E_IMPORT_REQUIRES_FILE",
            "imports require file-backed package loading",
            import.path.span.clone(),
        ));
    }
    if let Some(external) = unit.externals.first() {
        return Err(Diagnostic::new(
            "E_EXTERNAL_REQUIRES_FILE",
            "external program manifests require file-backed package loading",
            external.path.span.clone(),
        ));
    }
    Ok(SourcePackage {
        root: SourceUnitId(0),
        units: vec![SourceUnit {
            source: unit.source,
            imports: Vec::new(),
            externals: Vec::new(),
            project: unit.project,
            program: unit.program,
            output: unit.output,
        }],
        external_programs: Vec::new(),
    })
}

pub(crate) fn parse_source_with_language(
    source: SourceFile,
    language: &Language,
) -> Result<UnlinkedSourceUnit> {
    let root = super::raw::parse(&source)?;
    parse_source_program(source, root, language)
}

#[allow(clippy::too_many_lines)]
fn parse_source_program(
    source: SourceFile,
    root: RawNode,
    language: &Language,
) -> Result<UnlinkedSourceUnit> {
    let root_span = root.span.clone();
    let RawKind::Sequence(mut items) = root.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_SOURCE_PROGRAM",
            "a ClipAsm source program must be a YAML sequence beginning with `program`",
            root_span.clone(),
        ));
    };
    if items.is_empty() {
        return Err(Diagnostic::new(
            "E_MISSING_PROGRAM_HEADER",
            "a source program must begin with a `program` header",
            root_span.clone(),
        ));
    }
    let header = items.remove(0);
    let header_span = header.span.clone();
    let mut header_entries = into_mapping(header, "the source program header item")?;
    if header_entries.len() != 1 || header_entries[0].0 != PROGRAM_HEADER_FIELD {
        return Err(Diagnostic::new(
            "E_MISSING_PROGRAM_HEADER",
            "the first source item must be exactly the `program` header",
            header_span,
        ));
    }
    let (_, _, definition) = header_entries.remove(0);
    let entries = into_mapping(definition, "the `program` header")?;
    let mut version = None;
    let mut project = None;
    let mut imports = Vec::new();
    let mut externals = Vec::new();
    let mut inputs = Vec::new();
    let mut parameters = Vec::new();
    let mut clips = Vec::new();
    let mut output = None;
    let mut stack_access = SOURCE_PROGRAM_DEFAULT_STACK_ACCESS;

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
            "project" => {
                let span = value.span.clone();
                project = Some(Spanned::new(
                    ProjectSettings {
                        video: parse_project(value)?,
                    },
                    span,
                ));
            }
            "imports" => imports = parse_imports(value)?,
            "externals" => externals = parse_externals(value)?,
            "inputs" => inputs = parse_inputs(value)?,
            "parameters" => parameters = parse_parameters(value)?,
            "clips" => clips = parse_clips(value, language)?,
            "output" => {
                let (text, _) = scalar(&value, "`output`")?;
                output = Some(Spanned::new(PathBuf::from(text), value.span));
            }
            STACK_ACCESS_FIELD => stack_access = parse_stack_access(&value)?.value,
            _ => {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_PROGRAM_HEADER_FIELD",
                    format!("unknown program header field `{key}`"),
                    key_span,
                ));
            }
        }
    }

    let version = version.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_VERSION",
            "missing required program header field `version`",
            header_span.clone(),
        )
    })?;
    if version != 1 {
        return Err(Diagnostic::new(
            "E_UNSUPPORTED_VERSION",
            format!("unsupported source program version {version}; this engine supports version 1"),
            header_span.clone(),
        ));
    }
    let body = ProgramBody {
        items: items
            .into_iter()
            .map(|item| parse_item(item, language))
            .collect::<Result<Vec<_>>>()?,
        span: root_span,
    };

    Ok(UnlinkedSourceUnit {
        source,
        imports,
        externals,
        project,
        program: SourceProgram {
            inputs,
            parameters,
            clips,
            body,
            span: header_span,
            stack_access,
        },
        output,
    })
}

fn parse_imports(node: RawNode) -> Result<Vec<SourceImport>> {
    into_mapping(node, "`imports`")?
        .into_iter()
        .map(|(alias, alias_span, value)| {
            validate_name(&alias, &alias_span)?;
            let (path, _) = scalar(&value, "an import path")?;
            Ok(SourceImport {
                alias: Spanned::new(alias, alias_span),
                path: Spanned::new(PathBuf::from(path), value.span),
            })
        })
        .collect()
}

fn parse_externals(node: RawNode) -> Result<Vec<SourceExternalImport>> {
    into_mapping(node, "`externals`")?
        .into_iter()
        .map(|(alias, alias_span, value)| {
            validate_name(&alias, &alias_span)?;
            let (path, _) = scalar(&value, "an external program manifest path")?;
            Ok(SourceExternalImport {
                alias: Spanned::new(alias, alias_span),
                path: Spanned::new(PathBuf::from(path), value.span),
            })
        })
        .collect()
}

fn parse_inputs(node: RawNode) -> Result<Vec<InputPort>> {
    let span = node.span.clone();
    let RawKind::Sequence(values) = node.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_SEQUENCE",
            "`inputs` must be a sequence because input order controls stack binding",
            span,
        ));
    };
    values
        .into_iter()
        .map(|value| {
            let entry_span = value.span.clone();
            let mut entries = into_mapping(value, "an `inputs` entry")?;
            if entries.len() != 1 {
                return Err(Diagnostic::new(
                    "E_INVALID_PROGRAM_INPUT",
                    "each `inputs` entry must contain exactly one `name: Video` pair",
                    entry_span,
                ));
            }
            let (name, name_span, value_type) = entries.remove(0);
            validate_name(&name, &name_span)?;
            let type_span = value_type.span.clone();
            let (value_type, _) = scalar(&value_type, "an input type")?;
            let value_type = match value_type {
                "Video" => crate::model::ValueType::Video,
                "Audio" => crate::model::ValueType::Audio,
                _ => {
                    return Err(Diagnostic::new(
                        "E_UNKNOWN_VALUE_TYPE",
                        format!("unknown input type `{value_type}`; expected `Video` or `Audio`"),
                        type_span,
                    ));
                }
            };
            Ok(InputPort {
                name,
                value_type: value_type.into(),
                cardinality: Cardinality::One,
            })
        })
        .collect()
}

fn parse_parameters(node: RawNode) -> Result<Vec<SourceParameter>> {
    into_mapping(node, "`parameters`")?
        .into_iter()
        .map(|(name, name_span, value)| {
            validate_name(&name, &name_span)?;
            let (parameter_type, default) = parse_parameter_declaration(value)?;
            Ok(SourceParameter {
                name: Spanned::new(name, name_span),
                parameter_type,
                default,
            })
        })
        .collect()
}

fn parse_parameter_declaration(node: RawNode) -> Result<(ParameterType, Option<Literal>)> {
    if matches!(node.kind, RawKind::Scalar { .. }) {
        let (name, _) = scalar(&node, "a parameter type")?;
        return Ok((parse_parameter_type(name, None, &node.span)?, None));
    }
    let declaration_span = node.span.clone();
    let mut type_name = None;
    let mut values = None;
    let mut default = None;
    for (field, field_span, value) in into_mapping(node, "a parameter declaration")? {
        match field.as_str() {
            "type" => type_name = Some(scalar(&value, "`type`")?.0.to_owned()),
            "values" => {
                let values_span = value.span.clone();
                let RawKind::Sequence(entries) = value.kind else {
                    return Err(Diagnostic::new(
                        "E_EXPECTED_SEQUENCE",
                        "keyword `values` must be a sequence",
                        values_span,
                    ));
                };
                values = Some(
                    entries
                        .into_iter()
                        .map(|entry| {
                            scalar(&entry, "a keyword value").map(|(value, _)| value.to_owned())
                        })
                        .collect::<Result<Vec<_>>>()?,
                );
            }
            "default" => default = Some(parse_literal(&value)?),
            _ => {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_PARAMETER_FIELD",
                    format!("unknown parameter declaration field `{field}`"),
                    field_span,
                ));
            }
        }
    }
    let type_name = type_name.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_PARAMETER_TYPE",
            "a parameter declaration mapping requires `type`",
            declaration_span.clone(),
        )
    })?;
    Ok((
        parse_parameter_type(&type_name, values, &declaration_span)?,
        default,
    ))
}

fn parse_parameter_type(
    name: &str,
    values: Option<Vec<String>>,
    span: &SourceSpan,
) -> Result<ParameterType> {
    match name {
        "Integer" => Ok(ParameterType::Integer),
        "File" => Ok(ParameterType::File),
        "Duration" => Ok(ParameterType::Duration),
        "TimeRange" => Ok(ParameterType::TimeRange),
        "Keyword" => {
            let values = values.filter(|values| !values.is_empty()).ok_or_else(|| {
                Diagnostic::new(
                    "E_MISSING_KEYWORD_VALUES",
                    "a `Keyword` parameter requires a nonempty `values` sequence",
                    span.clone(),
                )
            })?;
            Ok(ParameterType::Keyword(values))
        }
        _ => Err(Diagnostic::new(
            "E_UNKNOWN_PARAMETER_TYPE",
            format!("unknown parameter type `{name}`"),
            span.clone(),
        )),
    }
}

fn parse_literal(node: &RawNode) -> Result<Literal> {
    let span = node.span.clone();
    let (value, style) = scalar(node, "a parameter default")?;
    if style == TScalarStyle::Plain
        && let Ok(value) = value.parse::<i64>()
    {
        Ok(Literal::Integer(value, span))
    } else {
        Ok(Literal::String(value.to_owned(), span))
    }
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
                "width" => {
                    video.width = Some(Spanned::new(
                        parse_u32(text, &value.span, "width")?,
                        value.span,
                    ));
                }
                "height" => {
                    video.height = Some(Spanned::new(
                        parse_u32(text, &value.span, "height")?,
                        value.span,
                    ));
                }
                "fps" => {
                    video.fps = Some(Spanned::new(text.to_owned(), value.span));
                }
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
    text.parse::<u32>().map_err(|_| {
        Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            format!("`{field}` must be an unsigned integer"),
            span.clone(),
        )
    })
}

fn parse_clips(node: RawNode, language: &Language) -> Result<Vec<NamedClip>> {
    let mut clips = Vec::new();
    for (name, span, value) in into_mapping(node, "`clips`")? {
        validate_name(&name, &span)?;
        let body = match value.kind {
            RawKind::Sequence(_) => parse_body(value, "a clip body", language)?,
            RawKind::Mapping(_) | RawKind::Scalar { .. } => {
                let body_span = value.span.clone();
                ProgramBody {
                    items: vec![parse_item(value, language)?],
                    span: body_span,
                }
            }
        };
        clips.push(NamedClip { name, body, span });
    }
    Ok(clips)
}

fn parse_body(node: RawNode, owner: &str, language: &Language) -> Result<ProgramBody> {
    let span = node.span.clone();
    let RawKind::Sequence(values) = node.kind else {
        return Err(Diagnostic::new(
            "E_EXPECTED_SEQUENCE",
            format!("{owner} must be a YAML sequence"),
            span,
        ));
    };
    let items = values
        .into_iter()
        .map(|value| parse_item(value, language))
        .collect::<Result<Vec<_>>>()?;
    Ok(ProgramBody { items, span })
}

fn parse_item(node: RawNode, language: &Language) -> Result<Item> {
    let item_span = node.span.clone();
    match node.kind {
        RawKind::Scalar { value, style } => {
            if style == TScalarStyle::Plain && value.starts_with('$') {
                let reference = parse_reference(&value, &item_span)?;
                Ok(Item {
                    kind: ItemKind::Reference(Reference {
                        name: Spanned::new(reference, item_span.clone()),
                    }),
                    output_bindings: OutputBindings::None,
                    span: item_span,
                })
            } else if style == TScalarStyle::Plain {
                Ok(Item {
                    kind: ItemKind::Invocation(Invocation {
                        program: Spanned::new(value, item_span.clone()),
                        stack_access: None,
                        arguments: BTreeMap::new(),
                        body: None,
                    }),
                    output_bindings: OutputBindings::None,
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
        RawKind::Mapping(entries) => {
            if entries
                .iter()
                .any(|(name, _, _)| name == PROGRAM_HEADER_FIELD)
            {
                return Err(Diagnostic::new(
                    "E_MISPLACED_PROGRAM_HEADER",
                    "the `program` header is only allowed as the first source item",
                    item_span,
                ));
            }
            parse_invocation(entries, item_span, language)
        }
        RawKind::Sequence(_) => Err(Diagnostic::new(
            "E_INVALID_SEQUENCE_ITEM",
            "a sequence item must be a program invocation or reference",
            item_span,
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn parse_invocation(
    entries: Vec<(String, SourceSpan, RawNode)>,
    span: SourceSpan,
    language: &Language,
) -> Result<Item> {
    let mut output_bindings = OutputBindings::None;
    let mut program_entries = Vec::new();
    for (key, key_span, value) in entries {
        if key == ID_FIELD {
            if !matches!(output_bindings, OutputBindings::None) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_OUTPUT_BINDING",
                    "an item may use either `id` or `ids`, but not both",
                    key_span,
                ));
            }
            let (name, _) = scalar(&value, "`id`")?;
            validate_name(name, &value.span)?;
            output_bindings = OutputBindings::One(Spanned::new(name.to_owned(), value.span));
        } else if key == IDS_FIELD {
            if !matches!(output_bindings, OutputBindings::None) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_OUTPUT_BINDING",
                    "an item may use either `id` or `ids`, but not both",
                    key_span,
                ));
            }
            let ids_span = value.span.clone();
            let RawKind::Sequence(values) = value.kind else {
                return Err(Diagnostic::new(
                    "E_INVALID_OUTPUT_BINDING",
                    "`ids` must be a sequence of names",
                    ids_span,
                ));
            };
            if values.is_empty() {
                return Err(Diagnostic::new(
                    "E_INVALID_OUTPUT_BINDING",
                    "`ids` must contain at least one name",
                    ids_span,
                ));
            }
            let mut names = Vec::with_capacity(values.len());
            for value in values {
                let (name, _) = scalar(&value, "an `ids` entry")?;
                validate_name(name, &value.span)?;
                names.push(Spanned::new(name.to_owned(), value.span));
            }
            output_bindings = OutputBindings::Many(names, ids_span);
        } else {
            program_entries.push((key, key_span, value));
        }
    }
    if program_entries.is_empty() {
        return Err(Diagnostic::new(
            "E_MISSING_PROGRAM_KEY",
            "an invocation mapping must contain one program key",
            span,
        ));
    }

    if program_entries.len() == 1 {
        let (program, program_span, value) = program_entries.remove(0);
        let invocation = if let Some((program_id, definition)) = language.resolve(&program) {
            let syntax = language.syntax(program_id);
            let invocation =
                normalize_invocation(definition, &syntax, program_span.clone(), value, language)?;
            if syntax.postfix && invocation.body.is_none() {
                return Err(Diagnostic::new(
                    "E_POSTFIX_REQUIRES_EXPRESSION",
                    format!("postfix program `{program}` requires an expression to wrap"),
                    program_span,
                ));
            }
            invocation
        } else {
            normalize_generic_invocation(program, program_span, value, language)?
        };
        return Ok(Item {
            kind: ItemKind::Invocation(invocation),
            output_bindings,
            span,
        });
    }

    let postfix_indices = program_entries
        .iter()
        .enumerate()
        .filter_map(|(index, (name, _, value))| {
            language
                .resolve(name)
                .filter(|(program, _)| {
                    language.syntax(*program).postfix
                        && (matches!(value.kind, RawKind::Mapping(_))
                            || matches!(value.kind, RawKind::Scalar { .. })
                                && !is_empty_scalar(value))
                })
                .map(|_| index)
        })
        .collect::<Vec<_>>();
    if program_entries.len() != 2 || postfix_indices.len() != 1 {
        let offending = program_entries
            .get(1)
            .unwrap_or_else(|| program_entries.first().expect("nonempty entries"));
        return Err(Diagnostic::new(
            "E_UNKNOWN_INVOCATION_FIELD",
            "program parameters must be nested inside the program mapping; only `id` or `ids` may annotate an item",
            offending.1.clone(),
        ));
    }

    let wrapper_index = postfix_indices[0];
    let (wrapper_name, wrapper_span, wrapper_value) = program_entries.remove(wrapper_index);
    let (head_name, head_span, head_value) = program_entries.remove(0);
    let (head_id, head_definition) = require_program(language, &head_name, &head_span)?;
    let head_syntax = language.syntax(head_id);
    let head_invocation = normalize_invocation(
        head_definition,
        &head_syntax,
        head_span.clone(),
        head_value,
        language,
    )?;
    let inner = Item {
        kind: ItemKind::Invocation(head_invocation),
        output_bindings: OutputBindings::None,
        span: head_span,
    };

    let (wrapper_id, wrapper_definition) = require_program(language, &wrapper_name, &wrapper_span)?;
    let wrapper_syntax = language.syntax(wrapper_id);
    let mut wrapper_invocation = normalize_invocation(
        wrapper_definition,
        &wrapper_syntax,
        wrapper_span.clone(),
        wrapper_value,
        language,
    )?;
    if wrapper_invocation.body.is_some() {
        return Err(Diagnostic::new(
            "E_POSTFIX_BODY_CONFLICT",
            format!("postfix program `{wrapper_name}` cannot declare its own `body`"),
            wrapper_span,
        ));
    }
    wrapper_invocation.body = Some(ProgramBody {
        items: vec![inner],
        span: span.clone(),
    });
    Ok(Item {
        kind: ItemKind::Invocation(wrapper_invocation),
        output_bindings,
        span,
    })
}

fn normalize_generic_invocation(
    program: String,
    program_span: SourceSpan,
    value: RawNode,
    language: &Language,
) -> Result<Invocation> {
    let mut arguments = BTreeMap::new();
    let mut stack_access = None;
    let mut body = None;
    if is_empty_scalar(&value) {
        // Missing inputs are bound from the invocation's accessible stack suffix.
    } else if matches!(value.kind, RawKind::Mapping(_)) {
        for (name, name_span, value) in into_mapping(value, "a full invocation mapping")? {
            if name == STACK_ACCESS_FIELD {
                stack_access = Some(parse_stack_access(&value)?);
            } else if arguments
                .insert(name.clone(), parse_argument_value(value, language)?)
                .is_some()
            {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_ARGUMENT",
                    format!("duplicate argument `{name}`"),
                    name_span,
                ));
            }
        }
    } else if matches!(value.kind, RawKind::Sequence(_)) {
        body = Some(parse_body(value, &format!("`{program}` body"), language)?);
    } else {
        return Err(Diagnostic::new(
            "E_INVALID_PRIMARY_ARGUMENT",
            format!(
                "authored program `{program}` has no representation-specific primary shorthand"
            ),
            value.span,
        ));
    }
    Ok(Invocation {
        program: Spanned::new(program, program_span),
        stack_access,
        arguments,
        body,
    })
}

fn require_program<'a>(
    language: &'a Language,
    program: &str,
    span: &SourceSpan,
) -> Result<(crate::program::ProgramId, &'a ProgramDefinition)> {
    language.resolve(program).ok_or_else(|| {
        Diagnostic::new(
            "E_UNKNOWN_PROGRAM",
            format!("unknown program `{program}`"),
            span.clone(),
        )
    })
}

fn normalize_invocation(
    definition: &ProgramDefinition,
    syntax: &super::language::ProgramSyntax,
    program_span: SourceSpan,
    value: RawNode,
    language: &Language,
) -> Result<Invocation> {
    let program = &definition.descriptor.name;
    let mut arguments = BTreeMap::new();
    let mut body = None;
    let mut stack_access = None;
    if is_empty_scalar(&value) {
        // Missing inputs are bound from the invocation's accessible stack suffix.
    } else if matches!(value.kind, RawKind::Mapping(_)) {
        for (name, name_span, value) in into_mapping(value, "a full invocation mapping")? {
            if name == STACK_ACCESS_FIELD {
                stack_access = Some(parse_stack_access(&value)?);
            } else if matches!(
                definition.implementation,
                ProgramImplementation::Body { .. }
            ) && name == BODY_FIELD
            {
                body = Some(parse_body(value, &format!("`{program}` body"), language)?);
            } else {
                let argument = parse_argument_value(value, language)?;
                if arguments.insert(name.clone(), argument).is_some() {
                    return Err(Diagnostic::new(
                        "E_DUPLICATE_ARGUMENT",
                        format!("duplicate argument `{name}`"),
                        name_span,
                    ));
                }
            }
        }
    } else if matches!(
        definition.implementation,
        ProgramImplementation::Body { .. }
    ) && matches!(value.kind, RawKind::Sequence(_))
    {
        body = Some(parse_body(value, &format!("`{program}` body"), language)?);
    } else {
        let Some(primary) = syntax.primary_parameter else {
            return Err(Diagnostic::new(
                "E_INVALID_PRIMARY_ARGUMENT",
                format!("program `{program}` has no primary shorthand parameter"),
                value.span,
            ));
        };
        arguments.insert(primary.to_owned(), parse_argument_value(value, language)?);
    }
    Ok(Invocation {
        program: Spanned::new(program.clone(), program_span),
        stack_access,
        arguments,
        body,
    })
}

fn parse_stack_access(node: &RawNode) -> Result<Spanned<StackAccess>> {
    let span = node.span.clone();
    let (value, _) = scalar(node, "`stack_access`")?;
    let value = match value {
        "owned" => StackAccess::Owned,
        "visible" => StackAccess::Visible,
        _ => {
            return Err(Diagnostic::new(
                "E_INVALID_STACK_ACCESS",
                "`stack_access` must be `owned` or `visible`",
                span,
            ));
        }
    };
    Ok(Spanned::new(value, span))
}

fn parse_argument_value(node: RawNode, language: &Language) -> Result<ArgumentValue> {
    let span = node.span.clone();
    match node.kind {
        RawKind::Scalar { value, style } => {
            if style == TScalarStyle::Plain && value.starts_with('$') {
                Ok(ArgumentValue::Reference(Spanned::new(
                    parse_reference(&value, &span)?,
                    span,
                )))
            } else if style == TScalarStyle::Plain {
                if let Ok(value) = value.parse::<i64>() {
                    Ok(ArgumentValue::Literal(Literal::Integer(value, span)))
                } else {
                    Ok(ArgumentValue::Literal(Literal::String(value, span)))
                }
            } else {
                Ok(ArgumentValue::Literal(Literal::String(value, span)))
            }
        }
        RawKind::Sequence(values) => {
            if values.iter().all(|value| {
                matches!(
                    &value.kind,
                    RawKind::Scalar {
                        value,
                        style: TScalarStyle::Plain,
                    } if value.starts_with('$')
                )
            }) {
                let references = values
                    .into_iter()
                    .map(|value| {
                        let value_span = value.span.clone();
                        let RawKind::Scalar {
                            value: text,
                            style: TScalarStyle::Plain,
                        } = value.kind
                        else {
                            return Err(Diagnostic::new(
                                "E_INVALID_ARGUMENT_TYPE",
                                "explicit variadic inputs must be `$name` references",
                                value_span,
                            ));
                        };
                        Ok(Spanned::new(
                            parse_reference(&text, &value_span)?,
                            value_span,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(ArgumentValue::References(references, span))
            } else {
                Ok(ArgumentValue::Body(parse_body(
                    RawNode {
                        kind: RawKind::Sequence(values),
                        span: span.clone(),
                    },
                    "an inline argument body",
                    language,
                )?))
            }
        }
        RawKind::Mapping(entries) => Ok(ArgumentValue::Body(ProgramBody {
            items: vec![parse_item(
                RawNode {
                    kind: RawKind::Mapping(entries),
                    span: span.clone(),
                },
                language,
            )?],
            span,
        })),
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
fn validate_name(name: &str, span: &SourceSpan) -> Result<()> {
    if !crate::source::is_valid_public_name(name) {
        return Err(Diagnostic::new(
            "E_INVALID_NAME",
            format!(
                "`{name}` is not a valid name; use {}",
                crate::source::PUBLIC_NAME_GRAMMAR
            ),
            span.clone(),
        ));
    }
    Ok(())
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
    use crate::model::{FrameCount, ImageFit, ValueRef, ValueType};
    use crate::program::{
        BodyFinalizer, BodyPlan, ParameterDescriptor, ParameterType, ProgramDescriptor,
        ProgramRegistry, ResolvedCall, StackAccess,
    };
    use crate::semantic::GraphBuilder;

    fn parse(source: &str) -> Result<SourcePackage> {
        parse_str(Path::new("workflow.yaml"), source)
    }

    #[test]
    fn normalizes_reference_and_full_arguments() {
        let program = parse(
            "- program:\n    version: 1\n    clips:\n      a:\n        image:\n          path: a.png\n          duration: 1s\n\n- $a\n",
        )
        .expect("source program");
        assert!(matches!(
            program.root().program.body.items[0].kind,
            ItemKind::Reference(Reference { .. })
        ));
        assert_eq!(program.root().program.clips[0].body.items.len(), 1);
    }

    #[test]
    fn source_and_invocation_stack_access_are_normalized_independently() {
        let program = parse(
            "- program:\n    version: 1\n    stack_access: visible\n\n- image:\n    path: a.png\n    duration: 1s\n    stack_access: visible\n",
        )
        .expect("source program");
        assert_eq!(program.root().program.stack_access, StackAccess::Visible);
        let ItemKind::Invocation(invocation) = &program.root().program.body.items[0].kind else {
            panic!("image invocation");
        };
        assert_eq!(
            invocation.stack_access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
    }

    #[test]
    fn source_stack_access_defaults_explicitly_to_owned() {
        let program = parse("- program:\n    version: 1\n\n- image: {path: a.png, duration: 1s}\n")
            .expect("source program");
        assert_eq!(
            program.root().program.stack_access,
            SOURCE_PROGRAM_DEFAULT_STACK_ACCESS
        );
        assert_eq!(program.root().program.stack_access, StackAccess::Owned);
    }

    #[test]
    fn rejects_invalid_stack_access() {
        for source in [
            "- program:\n    version: 1\n    stack_access: inherited\n\n- image: {path: a.png, duration: 1s}\n",
            "- program:\n    version: 1\n\n- image:\n    path: a.png\n    duration: 1s\n    stack_access: inherited\n",
        ] {
            let error = parse(source).expect_err("invalid stack access");
            assert_eq!(error.code, "E_INVALID_STACK_ACCESS");
        }
    }

    #[test]
    fn rejects_duplicate_keys() {
        let error = parse("- program:\n    version: 1\n    version: 1\n").expect_err("duplicate");
        assert_eq!(error.code, "E_DUPLICATE_YAML_KEY");
    }

    #[test]
    fn rejects_aliases() {
        let error = parse("- program:\n    version: 1\n    clips: &clips {}\n\n- *clips\n")
            .expect_err("aliases");
        assert!(matches!(error.code, "E_YAML_ANCHOR" | "E_YAML_ALIAS"));
    }

    #[test]
    fn postfix_program_normalizes_to_an_outer_invocation() {
        let program = parse(
            "- program:\n    version: 1\n\n- image:\n    path: a.png\n    duration: 2s\n  during: 0s..1s\n",
        )
        .expect("source program");
        let outer = &program.root().program.body.items[0];
        let ItemKind::Invocation(during) = &outer.kind else {
            panic!("during invocation");
        };
        assert_eq!(during.program.value, "during");
        assert_eq!(during.body.as_ref().expect("during body").items.len(), 1);
    }

    #[test]
    fn postfix_mapping_preserves_independent_stack_access() {
        let program = parse(
            "- program:\n    version: 1\n\n- image:\n    path: a.png\n    duration: 2s\n    stack_access: visible\n  during:\n    range: 0s..1s\n    stack_access: visible\n",
        )
        .expect("source program");
        let ItemKind::Invocation(during) = &program.root().program.body.items[0].kind else {
            panic!("during invocation");
        };
        assert_eq!(
            during.stack_access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
        let ItemKind::Invocation(image) = &during.body.as_ref().expect("body").items[0].kind else {
            panic!("image invocation");
        };
        assert_eq!(
            image.stack_access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
    }

    #[test]
    fn postfix_mapping_rejects_an_explicit_body() {
        let error = parse(
            "- program:\n    version: 1\n\n- image: {path: a.png, duration: 2s}\n  during:\n    range: 0s..1s\n    body: []\n",
        )
        .expect_err("postfix body conflict");
        assert_eq!(error.code, "E_POSTFIX_BODY_CONFLICT");
    }

    #[test]
    fn body_full_form_classifies_inputs_parameters_and_body() {
        parse(
            "- program:\n    version: 1\n    clips:\n      clip: {image: {path: a.png, duration: 2s}}\n\n- during:\n    video: $clip\n    range: 0s..1s\n    body:\n      - repeat: 2\n",
        )
        .expect("full body form");
    }

    #[test]
    fn semantic_parameter_errors_belong_to_compilation() {
        let workflow = parse(
            "- program:\n    version: 1\n\n- image:\n    path: a.png\n    duration: 1s\n- repeat: wrong\n",
        )
        .expect("syntax normalization");
        let error = crate::compiler::compile(&workflow).expect_err("parameter type");
        assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");

        let workflow = parse("- program:\n    version: 1\n\n- image:\n    duration: 1s\n")
            .expect("syntax normalization");
        let error = crate::compiler::compile(&workflow).expect_err("missing path");
        assert_eq!(error.code, "E_MISSING_ARGUMENT");
    }

    fn lower_synthetic_source(
        call: &ResolvedCall,
        builder: &mut GraphBuilder<'_>,
    ) -> Result<Vec<ValueRef>> {
        let (path, _) = call.file_parameter("path")?;
        Ok(vec![builder.image_video(
            path.to_path_buf(),
            FrameCount(1),
            ImageFit::Cover,
        )?])
    }

    #[allow(clippy::unnecessary_wraps)]
    fn prepare_synthetic_body(
        call: &ResolvedCall,
        _builder: &mut GraphBuilder<'_>,
    ) -> Result<BodyPlan> {
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_frames: call.requested_frames(),
            finalizer: Box::new(OneValue),
        })
    }

    fn prepare_synthetic_postfix(
        call: &ResolvedCall,
        _builder: &mut GraphBuilder<'_>,
    ) -> Result<BodyPlan> {
        let _ = call.time_range_parameter("range")?;
        Ok(BodyPlan {
            initial_values: Vec::new(),
            requested_frames: call.requested_frames(),
            finalizer: Box::new(OneValue),
        })
    }

    struct OneValue;

    impl BodyFinalizer for OneValue {
        fn finish(
            self: Box<Self>,
            stack: Vec<ValueRef>,
            _builder: &mut GraphBuilder<'_>,
        ) -> Result<Vec<ValueRef>> {
            let [value] = stack.as_slice() else {
                return Err(Diagnostic::new(
                    "E_TEST_OUTPUT",
                    "synthetic body requires one value",
                    SourceSpan::file_start("workflow.yaml"),
                ));
            };
            Ok(vec![*value])
        }
    }

    fn synthetic_programs() -> Vec<ProgramDefinition> {
        vec![
            ProgramDefinition {
                descriptor: ProgramDescriptor {
                    name: "synthetic_direct".to_owned(),
                    semantic_version: 1,
                    default_stack_access: StackAccess::Owned,
                    inputs: vec![],
                    parameters: vec![ParameterDescriptor {
                        name: "path".to_owned(),
                        parameter_type: ParameterType::File,
                        required: true,
                    }],
                    type_selector: None,
                    outputs: vec![ValueType::Video.into()],
                },
                implementation: ProgramImplementation::Direct(lower_synthetic_source),
            },
            ProgramDefinition {
                descriptor: ProgramDescriptor {
                    name: "synthetic_body".to_owned(),
                    semantic_version: 1,
                    default_stack_access: StackAccess::Owned,
                    inputs: vec![],
                    parameters: vec![],
                    type_selector: None,
                    outputs: vec![ValueType::Video.into()],
                },
                implementation: ProgramImplementation::Body {
                    prepare: prepare_synthetic_body,
                    contract: crate::program::BodyContract {
                        initial_values: Vec::new(),
                        outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                            ValueType::Video.into(),
                        ]),
                        count_error_code: "E_BODY_OUTPUT_COUNT",
                    },
                },
            },
            ProgramDefinition {
                descriptor: ProgramDescriptor {
                    name: "synthetic_postfix".to_owned(),
                    semantic_version: 1,
                    default_stack_access: StackAccess::Owned,
                    inputs: vec![],
                    parameters: vec![ParameterDescriptor {
                        name: "range".to_owned(),
                        parameter_type: ParameterType::TimeRange,
                        required: true,
                    }],
                    type_selector: None,
                    outputs: vec![ValueType::Video.into()],
                },
                implementation: ProgramImplementation::Body {
                    prepare: prepare_synthetic_postfix,
                    contract: crate::program::BodyContract {
                        initial_values: Vec::new(),
                        outputs: crate::program::BodyOutputConstraint::Exactly(vec![
                            ValueType::Video.into(),
                        ]),
                        count_error_code: "E_BODY_OUTPUT_COUNT",
                    },
                },
            },
        ]
    }

    #[test]
    fn postfix_output_bindings_belong_to_the_outer_invocation() {
        let registry =
            ProgramRegistry::from_definitions(synthetic_programs()).expect("synthetic registry");
        let language = Language::with_test_syntax(
            registry.clone(),
            [
                ("synthetic_direct", Some("path"), false),
                ("synthetic_postfix", Some("range"), true),
            ],
        )
        .expect("synthetic language");
        let workflow = parse_str_with_language(
            Path::new("workflow.yaml"),
            "- program:\n    version: 1\n\n- synthetic_direct: asset.any\n  synthetic_postfix: 0s..1s\n  ids: [first, second]\n",
            &language,
        )
        .expect("generic parse");
        let OutputBindings::Many(names, _) = &workflow.root().program.body.items[0].output_bindings
        else {
            panic!("outer ids");
        };
        assert_eq!(
            names
                .iter()
                .map(|name| name.value.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    #[test]
    fn registry_metadata_extends_parser_and_evaluator() {
        let registry =
            ProgramRegistry::from_definitions(synthetic_programs()).expect("synthetic registry");
        let language = Language::with_test_syntax(
            registry.clone(),
            [
                ("synthetic_direct", Some("path"), false),
                ("synthetic_postfix", Some("range"), true),
            ],
        )
        .expect("synthetic language");
        let workflow = parse_str_with_language(
            Path::new("workflow.yaml"),
            "- program:\n    version: 1\n\n- synthetic_direct: asset.any\n  synthetic_postfix: 0s..1s\n",
            &language,
        )
        .expect("generic parse");
        let compiled =
            crate::compiler::compile_with_registry(&workflow, &registry).expect("generic evaluate");
        assert_eq!(
            compiled.result_domain().expect("known domain").frames().0,
            1
        );
    }
}
