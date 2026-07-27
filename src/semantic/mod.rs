mod builder;
mod operation;

use serde::Serialize;

use crate::model::{ValueId, ValueType, VideoDomain};
use crate::source::SourceSpan;

pub(crate) use builder::{GraphBuilder, require_value_type};
pub(crate) use operation::{SemanticDependency, SemanticNodeKind, SymbolId};

#[derive(Clone, Debug)]
pub(crate) struct CompiledNode {
    id: ValueId,
    kind: SemanticNodeKind,
    domain: Option<VideoDomain>,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl CompiledNode {
    pub(crate) fn from_draft(id: ValueId, draft: &DraftNode, domain: Option<VideoDomain>) -> Self {
        Self {
            id,
            kind: draft.kind.clone(),
            domain,
            semantic_version: draft.semantic_version,
            origin: draft.origin.clone(),
        }
    }

    pub(crate) const fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) const fn id(&self) -> ValueId {
        self.id
    }

    pub(crate) fn value_type(&self) -> ValueType {
        self.kind.value_type()
    }

    pub(crate) const fn domain(&self) -> Option<&VideoDomain> {
        self.domain.as_ref()
    }

    pub(crate) const fn semantic_version(&self) -> u32 {
        self.semantic_version
    }

    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, Serialize)]
/// Authored construct and source location responsible for a semantic value.
///
/// Program constructs are static registry names; compiler-generated labels
/// such as `reference` are also stable identifiers.
pub struct SourceOrigin {
    /// Registered program name or stable compiler-generated construct label.
    pub construct: String,
    /// Most relevant authored source location.
    pub span: SourceSpan,
}

impl SourceOrigin {
    #[must_use]
    pub(crate) fn new(construct: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            construct: construct.into(),
            span,
        }
    }

    #[must_use]
    pub(crate) fn clone_with_construct(&self, construct: impl Into<String>) -> Self {
        Self::new(construct, self.span.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DraftNode {
    kind: SemanticNodeKind,
    semantic_version: u32,
    origin: SourceOrigin,
}

impl DraftNode {
    pub(crate) const fn kind(&self) -> &SemanticNodeKind {
        &self.kind
    }

    pub(crate) fn value_type(&self) -> ValueType {
        self.kind.value_type()
    }

    pub(crate) const fn semantic_version(&self) -> u32 {
        self.semantic_version
    }

    pub(crate) const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}
