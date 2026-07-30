//! Stable downstream document formats.

use serde::Serialize;

use crate::semantic::SourceOrigin;
use crate::source::{SourceSpan, Spanned};

pub(crate) mod json;
#[cfg(feature = "native")]
pub(crate) mod prepared_json;

#[derive(Serialize)]
pub(crate) struct SourceOriginDocument<'a> {
    construct: &'a str,
    span: SourceSpanDocument<'a>,
}

#[derive(Serialize)]
pub(crate) struct SourceSpanDocument<'a> {
    file: &'a std::path::Path,
    line: usize,
    column: usize,
}

#[derive(Serialize)]
pub(crate) struct SpannedDocument<'a, T> {
    value: &'a T,
    span: SourceSpanDocument<'a>,
}

pub(crate) fn source_origin_document(origin: &SourceOrigin) -> SourceOriginDocument<'_> {
    SourceOriginDocument {
        construct: &origin.construct,
        span: source_span_document(&origin.span),
    }
}

pub(crate) fn source_span_document(span: &SourceSpan) -> SourceSpanDocument<'_> {
    SourceSpanDocument {
        file: span.file(),
        line: span.line,
        column: span.column,
    }
}

pub(crate) fn spanned_document<T>(spanned: &Spanned<T>) -> SpannedDocument<'_, T> {
    SpannedDocument {
        value: &spanned.value,
        span: source_span_document(&spanned.span),
    }
}
