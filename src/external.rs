//! Neutral external-process invocation data shared across semantic and runtime phases.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::source::Spanned;

#[derive(Clone, Debug)]
pub(crate) struct ExternalInvocation {
    pub(crate) executable: Spanned<PathBuf>,
    pub(crate) arguments: Vec<ExternalArgumentValue>,
    pub(crate) preserve_input: String,
    pub(crate) inputs: BTreeMap<String, crate::model::ValueRef>,
    pub(crate) parameters: BTreeMap<String, ExternalParameterValue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExternalArgumentValue {
    Text { value: String },
    File { path: Spanned<PathBuf> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExternalParameterValue {
    Integer(i64),
    Keyword(String),
    File(Spanned<PathBuf>),
}
