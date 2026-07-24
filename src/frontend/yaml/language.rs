use crate::diagnostic::Result;
use crate::program::{ProgramImplementation, ProgramRegistry, definition_error};

pub(crate) const ID_FIELD: &str = "id";
pub(crate) const IDS_FIELD: &str = "ids";
pub(crate) const BODY_FIELD: &str = "body";
pub(crate) const PROGRAM_HEADER_FIELD: &str = "program";
pub(crate) const STACK_ACCESS_FIELD: &str = "stack_access";

#[derive(Clone, Debug)]
pub(crate) struct Language {
    pub(crate) programs: ProgramRegistry,
}

impl Default for Language {
    fn default() -> Self {
        Self::new(ProgramRegistry::default()).expect("built-in language is valid")
    }
}

impl Language {
    pub(crate) fn new(programs: ProgramRegistry) -> Result<Self> {
        validate_syntax_collisions(&programs)?;
        Ok(Self { programs })
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
        {
            return Err(definition_error(format!(
                "program `{}` has an input named `{STACK_ACCESS_FIELD}`",
                descriptor.name
            )));
        }
        if descriptor
            .parameters
            .iter()
            .any(|parameter| parameter.name == STACK_ACCESS_FIELD)
        {
            return Err(definition_error(format!(
                "program `{}` has a parameter named `{STACK_ACCESS_FIELD}`",
                descriptor.name
            )));
        }
        if !matches!(definition.implementation, ProgramImplementation::Body(_)) {
            continue;
        }
        if descriptor
            .inputs
            .iter()
            .any(|input| input.name == BODY_FIELD)
        {
            return Err(definition_error(format!(
                "body program `{}` has an input named `{BODY_FIELD}`",
                descriptor.name
            )));
        }
        if descriptor
            .parameters
            .iter()
            .any(|parameter| parameter.name == BODY_FIELD)
        {
            return Err(definition_error(format!(
                "body program `{}` has a parameter named `{BODY_FIELD}`",
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
        BodyPlan, Cardinality, InputPort, ParameterDescriptor, ParameterType, PostfixSyntax,
        ProgramDefinition, ProgramDescriptor, ResolvedCall, StackAccess,
    };
    use crate::semantic::GraphBuilder;

    fn prepare(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        unreachable!("language validation does not execute programs")
    }

    fn direct(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<Vec<ValueRef>> {
        unreachable!("language validation does not execute programs")
    }

    fn definition(
        name: &str,
        implementation: ProgramImplementation,
        inputs: Vec<InputPort>,
        parameters: Vec<ParameterDescriptor>,
        output: ValueType,
        postfix: Option<PostfixSyntax>,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name: name.to_owned(),
                semantic_version: 1,
                default_stack_access: StackAccess::Owned,
                inputs,
                parameters,
                primary_parameter: None,
                outputs: vec![output],
            },
            implementation,
            postfix,
        }
    }

    fn language_with(extra: ProgramDefinition) -> Result<Language> {
        Language::new(ProgramRegistry::from_definitions(vec![extra])?)
    }

    #[test]
    fn rejects_only_real_syntax_collisions() {
        for name in [ID_FIELD, IDS_FIELD, PROGRAM_HEADER_FIELD] {
            language_with(definition(
                name,
                ProgramImplementation::Direct(direct),
                vec![],
                vec![],
                ValueType::Video,
                None,
            ))
            .expect_err("source syntax collision");
        }

        for collision in [
            definition(
                "stack_access_input",
                ProgramImplementation::Direct(direct),
                vec![InputPort {
                    name: STACK_ACCESS_FIELD.to_owned(),
                    value_type: ValueType::Video,
                    cardinality: Cardinality::One,
                }],
                vec![],
                ValueType::Video,
                None,
            ),
            definition(
                "stack_access_parameter",
                ProgramImplementation::Direct(direct),
                vec![],
                vec![ParameterDescriptor {
                    name: STACK_ACCESS_FIELD.to_owned(),
                    parameter_type: ParameterType::File,
                    required: false,
                }],
                ValueType::Video,
                None,
            ),
            definition(
                "body_input",
                ProgramImplementation::Body(prepare),
                vec![InputPort {
                    name: BODY_FIELD.to_owned(),
                    value_type: ValueType::Video,
                    cardinality: Cardinality::One,
                }],
                vec![],
                ValueType::Video,
                None,
            ),
            definition(
                "body_parameter",
                ProgramImplementation::Body(prepare),
                vec![],
                vec![ParameterDescriptor {
                    name: BODY_FIELD.to_owned(),
                    parameter_type: ParameterType::File,
                    required: false,
                }],
                ValueType::Video,
                None,
            ),
        ] {
            language_with(collision).expect_err("syntax collision");
        }

        for name in ["ref", "clip"] {
            language_with(definition(
                name,
                ProgramImplementation::Direct(direct),
                vec![],
                vec![],
                ValueType::Video,
                None,
            ))
            .expect("ordinary item-level program name");
        }
    }
}
