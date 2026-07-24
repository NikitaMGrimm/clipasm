use crate::diagnostic::Result;
use crate::program::{ProgramImplementation, ProgramRegistry, definition_error};

pub(crate) const ID_FIELD: &str = "id";
pub(crate) const BODY_FIELD: &str = "body";
pub(crate) const PROGRAM_HEADER_FIELD: &str = "program";

#[derive(Clone, Copy, Debug)]
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
        validate_syntax_collisions(programs)?;
        Ok(Self { programs })
    }
}

fn validate_syntax_collisions(programs: ProgramRegistry) -> Result<()> {
    for definition in programs.definitions() {
        let descriptor = &definition.descriptor;
        if matches!(descriptor.name, ID_FIELD | PROGRAM_HEADER_FIELD) {
            return Err(definition_error(format!(
                "program name `{}` collides with source syntax",
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
        ProgramDefinition, ProgramDescriptor, ResolvedCall,
    };
    use crate::semantic::GraphBuilder;

    const BODY_INPUT: &[InputPort] = &[InputPort {
        name: BODY_FIELD,
        value_type: ValueType::Video,
        cardinality: Cardinality::One,
    }];
    const BODY_PARAMETER: &[ParameterDescriptor] = &[ParameterDescriptor {
        name: BODY_FIELD,
        parameter_type: ParameterType::File,
        required: false,
    }];

    fn prepare(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<BodyPlan> {
        unreachable!("language validation does not execute programs")
    }

    fn direct(_call: &ResolvedCall, _builder: &mut GraphBuilder<'_>) -> Result<ValueRef> {
        unreachable!("language validation does not execute programs")
    }

    fn definition(
        name: &'static str,
        implementation: ProgramImplementation,
        inputs: &'static [InputPort],
        parameters: &'static [ParameterDescriptor],
        output: ValueType,
        postfix: Option<PostfixSyntax>,
    ) -> ProgramDefinition {
        ProgramDefinition {
            descriptor: ProgramDescriptor {
                name,
                semantic_version: 1,
                inputs,
                parameters,
                primary_parameter: None,
                output,
            },
            implementation,
            postfix,
        }
    }

    fn language_with(extra: ProgramDefinition) -> Result<Language> {
        let definitions = Box::leak(vec![extra].into_boxed_slice());
        Language::new(ProgramRegistry::from_definitions(definitions)?)
    }

    #[test]
    fn rejects_only_real_syntax_collisions() {
        for name in [ID_FIELD, PROGRAM_HEADER_FIELD] {
            language_with(definition(
                name,
                ProgramImplementation::Direct(direct),
                &[],
                &[],
                ValueType::Video,
                None,
            ))
            .expect_err("source syntax collision");
        }

        for collision in [
            definition(
                "body_input",
                ProgramImplementation::Body(prepare),
                BODY_INPUT,
                &[],
                ValueType::Video,
                None,
            ),
            definition(
                "body_parameter",
                ProgramImplementation::Body(prepare),
                &[],
                BODY_PARAMETER,
                ValueType::Video,
                None,
            ),
        ] {
            language_with(collision).expect_err("body collision");
        }

        for name in ["ref", "clip"] {
            language_with(definition(
                name,
                ProgramImplementation::Direct(direct),
                &[],
                &[],
                ValueType::Video,
                None,
            ))
            .expect("ordinary item-level program name");
        }
    }
}
