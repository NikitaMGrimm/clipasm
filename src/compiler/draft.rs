use std::collections::BTreeMap;
use std::marker::PhantomData;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::ValueType;
use crate::program::{
    Cardinality, ProgramDefinition, ProgramId, ProgramImplementation, StackAccess,
};
use crate::source::{
    ArgumentValue, ItemKind, ItemOrigin, OutputBindings, ProgramBody, ScalarExpression,
    SourceProgram, SourceSpan, Spanned, StackBlock,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct InvocationId(pub(super) usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct StackBlockId(pub(super) usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct BodyId(pub(super) usize);

impl BodyId {
    pub(super) const fn index(self) -> usize {
        self.0
    }
}

#[derive(Default)]
struct DraftIds {
    bodies: usize,
    invocations: usize,
    stack_blocks: usize,
}

impl DraftIds {
    fn body(&mut self) -> BodyId {
        let id = BodyId(self.bodies);
        self.bodies = self
            .bodies
            .checked_add(1)
            .expect("draft body count fits in usize");
        id
    }

    fn invocation(&mut self) -> InvocationId {
        let id = InvocationId(self.invocations);
        self.invocations = self
            .invocations
            .checked_add(1)
            .expect("draft invocation count fits in usize");
        id
    }

    fn stack_block(&mut self) -> StackBlockId {
        let id = StackBlockId(self.stack_blocks);
        self.stack_blocks = self
            .stack_blocks
            .checked_add(1)
            .expect("draft stack block count fits in usize");
        id
    }
}

pub(super) trait TableId: Copy {
    fn index(self) -> usize;
}

impl TableId for InvocationId {
    fn index(self) -> usize {
        self.0
    }
}

impl TableId for StackBlockId {
    fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug)]
pub(super) struct IdTable<I, T> {
    slots: Vec<Option<T>>,
    id: PhantomData<fn(I) -> I>,
}

impl<I: TableId, T> IdTable<I, T> {
    pub(super) fn with_slot_count(len: usize) -> Self {
        Self {
            slots: std::iter::repeat_with(|| None).take(len).collect(),
            id: PhantomData,
        }
    }

    pub(super) fn get(&self, id: I) -> Option<&T> {
        self.slots[id.index()].as_ref()
    }

    pub(super) fn insert(&mut self, id: I, value: T) {
        let previous = self.slots[id.index()].replace(value);
        assert!(previous.is_none(), "draft ID table slot was filled twice");
    }

    pub(super) fn take(&mut self, id: I) -> Option<T> {
        self.slots[id.index()].take()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().flatten()
    }

    pub(super) fn first_missing(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_none)
    }

    pub(super) fn first_present(&self) -> Option<usize> {
        self.slots.iter().position(Option::is_some)
    }
}

#[derive(Debug)]
pub(super) struct DraftProgram {
    pub(super) span: SourceSpan,
    pub(super) body: DraftBody,
    pub(super) body_count: usize,
    pub(super) invocation_count: usize,
    pub(super) stack_block_count: usize,
}

#[derive(Debug)]
pub(super) struct DraftBody {
    pub(super) id: BodyId,
    pub(super) span: SourceSpan,
    pub(super) items: Vec<DraftItem>,
}

#[derive(Debug)]
pub(super) struct DraftItem {
    pub(super) origin: ItemOrigin,
    pub(super) output_bindings: OutputBindings,
    pub(super) kind: DraftItemKind,
}

impl DraftItem {
    pub(super) fn validate_output_binding_count(&self, output_count: usize) -> Result<()> {
        let (binding, span, valid) = match &self.output_bindings {
            OutputBindings::None => return Ok(()),
            OutputBindings::One(name) => (
                "`as name` requires exactly one output".to_owned(),
                &name.span,
                output_count == 1,
            ),
            OutputBindings::Many(names, span) => (
                format!("`as (...)` contains {} name(s)", names.len()),
                span,
                names.len() > 1 && output_count == names.len(),
            ),
        };
        if valid {
            return Ok(());
        }
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::OutputBindingCount,
            format!(
                "`{}` produces {output_count} value(s), but {binding}",
                self.origin.construct
            ),
            span.clone(),
        ))
    }
}

#[derive(Debug)]
pub(super) enum DraftItemKind {
    Reference(Spanned<String>),
    ScalarBinding {
        name: Spanned<String>,
        value: ScalarExpression,
    },
    Invocation(DraftInvocation),
    StackBlock(DraftStackBlock),
}

#[derive(Debug)]
pub(super) struct DraftStackBlock {
    pub(super) id: StackBlockId,
    pub(super) access: StackAccess,
    pub(super) body: Box<DraftBody>,
}

#[derive(Debug)]
pub(super) struct DraftInvocation {
    pub(super) id: InvocationId,
    pub(super) name: Spanned<String>,
    pub(super) program: ProgramId,
    pub(super) access: StackAccess,
    pub(super) type_argument: Option<Spanned<ValueType>>,
    pub(super) inputs: Vec<Option<DraftInput>>,
    pub(super) parameters: Vec<Option<DraftParameter>>,
    pub(super) body: Option<Box<DraftBody>>,
}

#[derive(Debug)]
pub(super) enum DraftInput {
    Reference(Spanned<String>),
    Body(Box<DraftBody>),
}

impl DraftInput {
    pub(super) const fn span(&self) -> &SourceSpan {
        match self {
            Self::Reference(reference) => &reference.span,
            Self::Body(body) => &body.span,
        }
    }
}

#[derive(Debug)]
pub(super) enum DraftParameter {
    Expression(ScalarExpression),
}

impl DraftProgram {
    pub(super) fn build(
        source: &SourceProgram,
        body: &ProgramBody,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
    ) -> Result<Self> {
        let mut ids = DraftIds::default();
        let body = DraftBody::build(body, definitions, builtins, namespace, 0, &mut ids)?;
        Ok(Self {
            span: source.span().clone(),
            body,
            body_count: ids.bodies,
            invocation_count: ids.invocations,
            stack_block_count: ids.stack_blocks,
        })
    }
}

impl DraftBody {
    fn empty(span: SourceSpan, ids: &mut DraftIds) -> Self {
        Self {
            id: ids.body(),
            span,
            items: Vec::new(),
        }
    }

    fn build(
        source: &ProgramBody,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
        depth: usize,
        ids: &mut DraftIds,
    ) -> Result<Self> {
        if depth > crate::source::MAX_SYNTAX_NESTING {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::BodyNestingDepth,
                format!(
                    "program body nesting exceeds the supported depth of {}",
                    crate::source::MAX_SYNTAX_NESTING
                ),
                source.span.clone(),
            ));
        }
        Ok(Self {
            id: ids.body(),
            span: source.span.clone(),
            items: source
                .items
                .iter()
                .map(|item| {
                    let kind = match &item.kind {
                        ItemKind::Reference(reference) => {
                            DraftItemKind::Reference(reference.name.clone())
                        }
                        ItemKind::ScalarBinding(binding) => DraftItemKind::ScalarBinding {
                            name: binding.name.clone(),
                            value: binding.value.clone(),
                        },
                        ItemKind::Invocation(invocation) => {
                            DraftItemKind::Invocation(DraftInvocation::build(
                                invocation,
                                definitions,
                                builtins,
                                namespace,
                                depth,
                                ids,
                            )?)
                        }
                        ItemKind::StackBlock(block) => {
                            DraftItemKind::StackBlock(DraftStackBlock::build(
                                block,
                                definitions,
                                builtins,
                                namespace,
                                depth,
                                ids,
                            )?)
                        }
                    };
                    Ok(DraftItem {
                        origin: item.origin.clone(),
                        output_bindings: item.output_bindings.clone(),
                        kind,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl DraftInvocation {
    #[expect(
        clippy::too_many_lines,
        reason = "one invocation draft pass validates descriptor shape and preserves every authored argument and body role"
    )]
    fn build(
        source: &crate::source::Invocation,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
        depth: usize,
        ids: &mut DraftIds,
    ) -> Result<Self> {
        let id = ids.invocation();
        let program = program_id_for(
            &source.program.value,
            builtins,
            namespace,
            &source.program.span,
        )?;
        let definition = &definitions[program.index()];
        if let Some(type_argument) = &source.type_argument
            && !definition.descriptor.is_generic()
        {
            return Err(Diagnostic::builtin(
                BuiltinDiagnostic::UnexpectedTypeArgument,
                format!("program `{}` is not generic", source.program.value),
                type_argument.span.clone(),
            ));
        }
        let access = source
            .stack_access
            .as_ref()
            .map_or(definition.descriptor.default_stack_access, |access| {
                access.value
            });
        let mut inputs = std::iter::repeat_with(|| None)
            .take(definition.descriptor.inputs.len())
            .collect::<Vec<_>>();
        let mut parameters = std::iter::repeat_with(|| None)
            .take(definition.descriptor.parameters.len())
            .collect::<Vec<_>>();

        for (name, argument) in &source.arguments {
            if let Some(slot) = definition.descriptor.input_slot(name) {
                let port = definition.descriptor.input(slot);
                let input = match argument {
                    ArgumentValue::Reference(reference) => DraftInput::Reference(reference.clone()),
                    ArgumentValue::Body(body) => {
                        if matches!(port.cardinality, Cardinality::Variadic { .. }) {
                            return Err(Diagnostic::builtin(
                                BuiltinDiagnostic::InvalidArgumentType,
                                format!(
                                    "explicit variadic input `{}.{}` must use `$name` references",
                                    source.program.value, port.name
                                ),
                                body.span.clone(),
                            ));
                        }
                        DraftInput::Body(Box::new(DraftBody::build(
                            body,
                            definitions,
                            builtins,
                            namespace,
                            depth + 1,
                            ids,
                        )?))
                    }
                    ArgumentValue::Scalar(_) => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidArgumentType,
                            format!(
                                "input `{}.{}` requires a graph value",
                                source.program.value, port.name
                            ),
                            argument.span().clone(),
                        ));
                    }
                };
                let count = 1;
                match port.cardinality {
                    Cardinality::One if count != 1 => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidArgumentType,
                            format!(
                                "input `{}.{}` requires exactly one value",
                                source.program.value, port.name
                            ),
                            input.span().clone(),
                        ));
                    }
                    Cardinality::Variadic { min } if count < min => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::MissingRequiredInput,
                            format!(
                                "input `{}.{}` requires at least {min} value(s)",
                                source.program.value, port.name
                            ),
                            input.span().clone(),
                        ));
                    }
                    _ => {}
                }
                inputs[slot.index()] = Some(input);
            } else if let Some(slot) = definition.descriptor.parameter_slot(name) {
                parameters[slot.index()] = Some(match argument {
                    ArgumentValue::Scalar(expression) => {
                        DraftParameter::Expression(expression.clone())
                    }
                    ArgumentValue::Reference(reference) => {
                        DraftParameter::Expression(ScalarExpression::Reference(reference.clone()))
                    }
                    ArgumentValue::Body(_) => {
                        return Err(Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidArgumentType,
                            format!(
                                "parameter `{}.{name}` requires a scalar value",
                                source.program.value
                            ),
                            argument.span().clone(),
                        ));
                    }
                });
            } else {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::UnknownProgramArgument,
                    format!(
                        "unknown argument `{name}` for program `{}`",
                        source.program.value
                    ),
                    argument.span().clone(),
                ));
            }
        }

        for (descriptor, argument) in definition.descriptor.parameters.iter().zip(&parameters) {
            if descriptor.required && argument.is_none() {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::MissingArgument,
                    format!(
                        "missing required parameter `{}.{}`",
                        source.program.value, descriptor.name
                    ),
                    source.program.span.clone(),
                ));
            }
        }

        let body = match definition.implementation {
            ProgramImplementation::Direct(_)
            | ProgramImplementation::ClipAsm(_)
            | ProgramImplementation::External(_) => {
                if source.body.is_some() {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::UnexpectedProgramBody,
                        format!(
                            "program `{}` does not accept a caller-supplied body",
                            source.program.value
                        ),
                        source.program.span.clone(),
                    ));
                }
                None
            }
            ProgramImplementation::Body { .. } => {
                let body = source
                    .body
                    .as_ref()
                    .map(|body| {
                        DraftBody::build(body, definitions, builtins, namespace, depth + 1, ids)
                    })
                    .transpose()?
                    .unwrap_or_else(|| DraftBody::empty(source.program.span.clone(), ids));
                Some(Box::new(body))
            }
        };

        Ok(Self {
            id,
            name: source.program.clone(),
            program,
            access,
            type_argument: source.type_argument.clone(),
            inputs,
            parameters,
            body,
        })
    }
}

impl DraftStackBlock {
    fn build(
        source: &StackBlock,
        definitions: &[ProgramDefinition],
        builtins: &BTreeMap<String, ProgramId>,
        namespace: &BTreeMap<String, ProgramId>,
        depth: usize,
        ids: &mut DraftIds,
    ) -> Result<Self> {
        let id = ids.stack_block();
        Ok(Self {
            id,
            access: source.stack_access,
            body: Box::new(DraftBody::build(
                &source.body,
                definitions,
                builtins,
                namespace,
                depth + 1,
                ids,
            )?),
        })
    }
}

fn program_id_for(
    name: &str,
    builtins: &BTreeMap<String, ProgramId>,
    namespace: &BTreeMap<String, ProgramId>,
    span: &SourceSpan,
) -> Result<ProgramId> {
    builtins
        .get(name)
        .or_else(|| namespace.get(name))
        .copied()
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::UnknownProgram,
                format!("unknown program `{name}`"),
                span.clone(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_id_table_consumes_entries_by_id() {
        let mut table = IdTable::<InvocationId, &str>::with_slot_count(3);
        assert_eq!(table.first_missing(), Some(0));
        table.insert(InvocationId(2), "third");
        table.insert(InvocationId(0), "first");
        table.insert(InvocationId(1), "second");
        assert_eq!(table.first_missing(), None);

        assert_eq!(table.take(InvocationId(1)), Some("second"));
        assert_eq!(table.take(InvocationId(1)), None);
        assert_eq!(table.take(InvocationId(2)), Some("third"));
        assert_eq!(table.take(InvocationId(0)), Some("first"));
        assert_eq!(table.first_present(), None);
    }
}
