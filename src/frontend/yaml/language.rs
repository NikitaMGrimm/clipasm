use std::collections::BTreeMap;

use crate::diagnostic::Result;
use crate::program::{ProgramDefinition, ProgramId, ProgramRegistry, definition_error};

pub(crate) const ID_FIELD: &str = "id";
pub(crate) const IDS_FIELD: &str = "ids";
pub(crate) const BODY_FIELD: &str = "body";
pub(crate) const PROGRAM_HEADER_FIELD: &str = "program";
pub(crate) const STACK_ACCESS_FIELD: &str = "stack_access";

#[derive(Clone, Debug, Default)]
pub(crate) struct ProgramSyntax {
    pub(crate) primary_parameter: Option<&'static str>,
    pub(crate) postfix: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Language {
    pub(crate) programs: ProgramRegistry,
    syntax: BTreeMap<ProgramId, ProgramSyntax>,
}

impl Default for Language {
    fn default() -> Self {
        let programs = ProgramRegistry::default();
        Self::with_syntax(programs, super::builtins::PROGRAM_SYNTAX.iter().copied())
            .expect("built-in YAML language is valid")
    }
}

impl Language {
    #[cfg(test)]
    pub(crate) fn new(programs: ProgramRegistry) -> Result<Self> {
        Self::with_syntax(programs, std::iter::empty())
    }

    #[cfg(test)]
    pub(crate) fn with_test_syntax(
        programs: ProgramRegistry,
        syntax: impl IntoIterator<Item = (&'static str, Option<&'static str>, bool)>,
    ) -> Result<Self> {
        Self::with_syntax(programs, syntax)
    }

    fn with_syntax(
        programs: ProgramRegistry,
        syntax: impl IntoIterator<Item = (&'static str, Option<&'static str>, bool)>,
    ) -> Result<Self> {
        validate_syntax_collisions(&programs)?;
        let mut by_program = BTreeMap::new();
        for (name, primary_parameter, postfix) in syntax {
            let id = programs.id(name).ok_or_else(|| {
                definition_error(format!("YAML syntax names unknown program `{name}`"))
            })?;
            let definition = programs.definition(id);
            if let Some(primary) = primary_parameter
                && !definition
                    .descriptor
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name == primary)
            {
                return Err(definition_error(format!(
                    "YAML syntax for `{name}` names nonexistent primary parameter `{primary}`"
                )));
            }
            if postfix && !definition.is_body() {
                return Err(definition_error(format!(
                    "YAML postfix syntax requires body program `{name}`"
                )));
            }
            by_program.insert(
                id,
                ProgramSyntax {
                    primary_parameter,
                    postfix,
                },
            );
        }
        Ok(Self {
            programs,
            syntax: by_program,
        })
    }

    pub(crate) fn syntax(&self, program: ProgramId) -> ProgramSyntax {
        self.syntax.get(&program).cloned().unwrap_or_default()
    }

    pub(crate) fn resolve(&self, name: &str) -> Option<(ProgramId, &ProgramDefinition)> {
        let id = self.programs.id(name)?;
        Some((id, self.programs.definition(id)))
    }
}

fn validate_syntax_collisions(programs: &ProgramRegistry) -> Result<()> {
    for definition in programs.definitions() {
        let descriptor = &definition.descriptor;
        if matches!(
            descriptor.name.as_str(),
            ID_FIELD | IDS_FIELD | PROGRAM_HEADER_FIELD
        ) {
            return Err(definition_error(format!(
                "program name `{}` collides with source syntax",
                descriptor.name
            )));
        }
        if descriptor
            .inputs
            .iter()
            .any(|input| input.name == STACK_ACCESS_FIELD)
            || descriptor
                .parameters
                .iter()
                .any(|parameter| parameter.name == STACK_ACCESS_FIELD)
        {
            return Err(definition_error(format!(
                "program `{}` has an argument named `{STACK_ACCESS_FIELD}`",
                descriptor.name
            )));
        }
        if definition.is_body()
            && (descriptor
                .inputs
                .iter()
                .any(|input| input.name == BODY_FIELD)
                || descriptor
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name == BODY_FIELD))
        {
            return Err(definition_error(format!(
                "body program `{}` has an argument named `{BODY_FIELD}`",
                descriptor.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ValueRef, ValueType};
    use crate::program::{
        Cardinality, InputPort, ProgramDefinition, ProgramDescriptor, ProgramImplementation,
        ResolvedCall, StackAccess,
    };
    use crate::semantic::GraphBuilder;

    fn direct(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        unreachable!("language validation does not execute programs")
    }

    fn definition(name: &str, inputs: Vec<InputPort>) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs,
                parameters: vec![],
                type_selector: None,
                outputs: vec![ValueType::Video.into()],
            },
            implementation: ProgramImplementation::Direct(direct),
        }
    }

    #[test]
    fn rejects_reserved_program_names_and_arguments() {
        for name in [ID_FIELD, IDS_FIELD, PROGRAM_HEADER_FIELD] {
            let registry = ProgramRegistry::from_definitions(vec![definition(name, vec![])])
                .expect("program registry");
            Language::new(registry).expect_err("source syntax collision");
        }

        let registry = ProgramRegistry::from_definitions(vec![definition(
            "collision",
            vec![InputPort {
                name: STACK_ACCESS_FIELD.to_owned(),
                value_type: ValueType::Video.into(),
                cardinality: Cardinality::One,
            }],
        )])
        .expect("program registry");
        Language::new(registry).expect_err("argument syntax collision");
    }
}
