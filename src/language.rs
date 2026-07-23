use crate::diagnostic::Result;
use crate::model::ValueType;
use crate::program::{ProgramImplementation, ProgramRegistry, definition_error};

pub(crate) const ROOT_PROGRAM: &str = "timeline";
pub(crate) const ID_FIELD: &str = "id";
pub(crate) const BODY_FIELD: &str = "body";

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
        validate_root_program(programs)?;
        validate_syntax_collisions(programs)?;
        Ok(Self { programs })
    }
}

fn validate_root_program(programs: ProgramRegistry) -> Result<()> {
    let root = programs.get(ROOT_PROGRAM).ok_or_else(|| {
        definition_error(format!(
            "required root program `{ROOT_PROGRAM}` is not registered"
        ))
    })?;
    if !matches!(root.implementation, ProgramImplementation::Body(_)) {
        return Err(definition_error(format!(
            "root program `{ROOT_PROGRAM}` must be a body program"
        )));
    }
    if !root.descriptor.inputs.is_empty() {
        return Err(definition_error(format!(
            "root program `{ROOT_PROGRAM}` must not declare value inputs"
        )));
    }
    if root.postfix.is_some() {
        return Err(definition_error(format!(
            "root program `{ROOT_PROGRAM}` must not declare postfix syntax"
        )));
    }
    if !root.descriptor.parameters.is_empty() {
        return Err(definition_error(format!(
            "root program `{ROOT_PROGRAM}` must not declare parameters"
        )));
    }
    if root.descriptor.output != ValueType::Video {
        return Err(definition_error(format!(
            "root program `{ROOT_PROGRAM}` must output Video"
        )));
    }
    Ok(())
}

fn validate_syntax_collisions(programs: ProgramRegistry) -> Result<()> {
    for definition in programs.definitions() {
        let descriptor = &definition.descriptor;
        if descriptor.name == ID_FIELD {
            return Err(definition_error(format!(
                "program name `{ID_FIELD}` collides with item syntax"
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

    const VIDEO_INPUT: &[InputPort] = &[InputPort {
        name: "input",
        value_type: ValueType::Video,
        cardinality: Cardinality::One,
    }];
    const BODY_INPUT: &[InputPort] = &[InputPort {
        name: BODY_FIELD,
        value_type: ValueType::Video,
        cardinality: Cardinality::One,
    }];
    const PARAMETER: &[ParameterDescriptor] = &[ParameterDescriptor {
        name: "range",
        parameter_type: ParameterType::TimeRange,
        required: false,
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

    fn timeline() -> ProgramDefinition {
        definition(
            ROOT_PROGRAM,
            ProgramImplementation::Body(prepare),
            &[],
            &[],
            ValueType::Video,
            None,
        )
    }

    fn language_with(extra: ProgramDefinition) -> Result<Language> {
        let definitions = Box::leak(vec![timeline(), extra].into_boxed_slice());
        Language::new(ProgramRegistry::from_definitions(definitions)?)
    }

    #[test]
    fn rejects_every_invalid_root_contract() {
        let cases = [
            Vec::new(),
            vec![definition(
                ROOT_PROGRAM,
                ProgramImplementation::Direct(direct),
                &[],
                &[],
                ValueType::Video,
                None,
            )],
            vec![definition(
                ROOT_PROGRAM,
                ProgramImplementation::Body(prepare),
                VIDEO_INPUT,
                &[],
                ValueType::Video,
                None,
            )],
            vec![definition(
                ROOT_PROGRAM,
                ProgramImplementation::Body(prepare),
                &[],
                PARAMETER,
                ValueType::Video,
                None,
            )],
            vec![definition(
                ROOT_PROGRAM,
                ProgramImplementation::Body(prepare),
                &[],
                &[],
                ValueType::Test,
                None,
            )],
            vec![definition(
                ROOT_PROGRAM,
                ProgramImplementation::Body(prepare),
                &[],
                PARAMETER,
                ValueType::Video,
                Some(PostfixSyntax { parameter: "range" }),
            )],
        ];

        for definitions in cases {
            let definitions = Box::leak(definitions.into_boxed_slice());
            let registry = ProgramRegistry::from_definitions(definitions).expect("valid registry");
            let error = Language::new(registry).expect_err("invalid root");
            assert_eq!(error.code, "E_INVALID_PROGRAM_DEFINITION");
        }
    }

    #[test]
    fn rejects_only_real_syntax_collisions() {
        let id = definition(
            ID_FIELD,
            ProgramImplementation::Direct(direct),
            &[],
            &[],
            ValueType::Video,
            None,
        );
        Language::new(
            ProgramRegistry::from_definitions(Box::leak(vec![timeline(), id].into_boxed_slice()))
                .expect("valid registry"),
        )
        .expect_err("id collision");

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
