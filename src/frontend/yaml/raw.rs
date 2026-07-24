use std::collections::BTreeSet;

use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::{SourceFile, SourceSpan};

#[derive(Clone, Debug)]
pub(super) struct RawNode {
    pub(super) kind: RawKind,
    pub(super) span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(super) enum RawKind {
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

pub(super) fn parse(source: &SourceFile) -> Result<RawNode> {
    let mut sink = EventSink { events: Vec::new() };
    Parser::new_from_str(source.text())
        .load(&mut sink, true)
        .map_err(|error| {
            let marker = error.marker();
            Diagnostic::new(
                "E_YAML_SYNTAX",
                error.info().to_owned(),
                SourceSpan::at(source.clone(), marker.line(), marker.col()),
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
            "a source program must contain exactly one YAML document",
            SourceSpan::source_start(source.clone()),
        ));
    }

    let start = sink
        .events
        .iter()
        .position(|(event, _)| matches!(event, Event::DocumentStart))
        .map_or(0, |index| index + 1);
    let mut cursor = start;
    raw_node(&sink.events, &mut cursor, source)
}

fn raw_node(
    events: &[(Event, Marker)],
    cursor: &mut usize,
    source: &SourceFile,
) -> Result<RawNode> {
    let Some((event, marker)) = events.get(*cursor) else {
        return Err(Diagnostic::new(
            "E_YAML_SYNTAX",
            "unexpected end of YAML input",
            SourceSpan::source_start(source.clone()),
        ));
    };
    *cursor += 1;
    let span = SourceSpan::at(source.clone(), marker.line(), marker.col());
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
                values.push(raw_node(events, cursor, source)?);
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
                let key = raw_node(events, cursor, source)?;
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
                let value = raw_node(events, cursor, source)?;
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
