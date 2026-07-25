use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::program::{Cardinality, InputPort, ProgramDescriptor, ProgramRegistry, StackAccess};
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, ItemOrigin, Literal, OutputBindings, ProgramBody,
    ProjectSettings, Reference, SOURCE_PROGRAM_DEFAULT_STACK_ACCESS, SourceExternalImport,
    SourceFile, SourceImport, SourcePackage, SourceParameter, SourceProgram, SourceUnit,
    SourceUnitId, Spanned, StackBlock, UnlinkedSourceUnit, VideoSettings,
};

use super::parser;
use super::syntax::{
    Argument, Block, Declaration, Expression, OutputBindings as SyntaxOutputBindings, Scalar,
    SourceFileSyntax, Statement,
};

pub(crate) fn parse_str(path: &Path, text: &str) -> Result<SourcePackage> {
    let source = SourceFile::new(path.to_path_buf(), text.to_owned());
    let syntax = parser::parse(source.clone())?;
    reject_file_backed_declarations(&syntax)?;
    let programs = builtin_descriptors();
    let unit = lower_source(source, syntax, &programs)?;
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

pub(crate) fn lower_source(
    source: SourceFile,
    syntax: SourceFileSyntax,
    programs: &BTreeMap<String, ProgramDescriptor>,
) -> Result<UnlinkedSourceUnit> {
    let declarations = lower_declarations(syntax.declarations)?;
    let lexical = declarations
        .inputs
        .iter()
        .map(|input| input.name.clone())
        .collect();
    let parameters = declarations
        .parameters
        .iter()
        .map(|parameter| parameter.name.value.clone())
        .collect();
    let lowerer = Lowerer {
        programs,
        parameters,
    };
    let items = lowerer.lower_statements(&syntax.statements, &lexical)?;
    Ok(UnlinkedSourceUnit {
        source,
        imports: declarations.imports,
        externals: declarations.externals,
        project: declarations.project,
        program: SourceProgram {
            inputs: declarations.inputs,
            parameters: declarations.parameters,
            body: ProgramBody {
                items,
                span: syntax.span.clone(),
            },
            span: syntax.version.span,
            stack_access: SOURCE_PROGRAM_DEFAULT_STACK_ACCESS,
        },
        output: declarations.output,
    })
}

fn builtin_descriptors() -> BTreeMap<String, ProgramDescriptor> {
    ProgramRegistry::default()
        .definitions()
        .iter()
        .map(|definition| {
            (
                definition.descriptor.name.clone(),
                definition.descriptor.clone(),
            )
        })
        .collect()
}

fn reject_file_backed_declarations(syntax: &SourceFileSyntax) -> Result<()> {
    for declaration in &syntax.declarations {
        match declaration {
            Declaration::Import(import) => {
                return Err(Diagnostic::new(
                    "E_IMPORT_REQUIRES_FILE",
                    "imports require file-backed package loading",
                    import.path.span.clone(),
                ));
            }
            Declaration::External(external) => {
                return Err(Diagnostic::new(
                    "E_EXTERNAL_REQUIRES_FILE",
                    "external program manifests require file-backed package loading",
                    external.path.span.clone(),
                ));
            }
            Declaration::Config(_) | Declaration::Input(_) | Declaration::Parameter(_) => {}
        }
    }
    Ok(())
}

struct LoweredDeclarations {
    imports: Vec<SourceImport>,
    externals: Vec<SourceExternalImport>,
    project: Option<Spanned<ProjectSettings>>,
    output: Option<Spanned<PathBuf>>,
    inputs: Vec<InputPort>,
    parameters: Vec<SourceParameter>,
}

fn lower_declarations(declarations: Vec<Declaration>) -> Result<LoweredDeclarations> {
    let mut imports = Vec::new();
    let mut externals = Vec::new();
    let mut project = None;
    let mut output = None;
    let mut inputs = Vec::new();
    let mut parameters = Vec::new();
    let mut config_span = None;

    for declaration in declarations {
        match declaration {
            Declaration::Config(config) => {
                if config_span.replace(config.span.clone()).is_some() {
                    return Err(Diagnostic::new(
                        "E_DUPLICATE_CONFIG",
                        "a source file may declare at most one `config` block",
                        config.span,
                    ));
                }
                if let Some(video) = config.video {
                    let settings = VideoSettings {
                        width: video
                            .width
                            .map(|value| parse_u32(value, "width"))
                            .transpose()?,
                        height: video
                            .height
                            .map(|value| parse_u32(value, "height"))
                            .transpose()?,
                        fps: video.fps,
                    };
                    project = Some(Spanned::new(
                        ProjectSettings { video: settings },
                        config.span.clone(),
                    ));
                }
                output = config
                    .output
                    .map(|value| Spanned::new(PathBuf::from(value.value), value.span));
            }
            Declaration::Import(import) => imports.push(SourceImport {
                alias: import.alias,
                path: Spanned::new(PathBuf::from(import.path.value), import.path.span),
            }),
            Declaration::External(external) => externals.push(SourceExternalImport {
                alias: external.alias,
                path: Spanned::new(PathBuf::from(external.path.value), external.path.span),
            }),
            Declaration::Input(input) => inputs.push(InputPort {
                name: input.name.value,
                value_type: input.value_type.value.into(),
                cardinality: Cardinality::One,
            }),
            Declaration::Parameter(parameter) => parameters.push(SourceParameter {
                name: parameter.name,
                parameter_type: parameter.parameter_type.value,
                default: parameter.default.map(lower_scalar_literal),
            }),
        }
    }

    Ok(LoweredDeclarations {
        imports,
        externals,
        project,
        output,
        inputs,
        parameters,
    })
}

fn parse_u32(value: Spanned<String>, field: &str) -> Result<Spanned<u32>> {
    let parsed = value.value.parse::<u32>().map_err(|_| {
        Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            format!("`{field}` must be an unsigned integer"),
            value.span.clone(),
        )
    })?;
    Ok(Spanned::new(parsed, value.span))
}

fn lower_scalar_literal(value: Scalar) -> Literal {
    match value {
        Scalar::String(value) => Literal::String(value.value, value.span),
        Scalar::Atom(value) => match value.value.parse::<i64>() {
            Ok(integer) => Literal::Integer(integer, value.span),
            Err(_) => Literal::String(value.value, value.span),
        },
    }
}

struct Lowerer<'a> {
    programs: &'a BTreeMap<String, ProgramDescriptor>,
    parameters: BTreeSet<String>,
}

impl Lowerer<'_> {
    fn lower_statements(
        &self,
        statements: &[Statement],
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        for statement in statements {
            items.extend(self.lower_statement(statement, lexical)?);
        }
        Ok(items)
    }

    fn lower_statement(
        &self,
        statement: &Statement,
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        let bindings = lower_output_bindings(&statement.output_bindings);
        match &statement.expression {
            Expression::Reference(reference) => Ok(vec![Item {
                kind: ItemKind::Reference(Reference {
                    name: reference.clone(),
                }),
                output_bindings: bindings,
                origin: ItemOrigin::authored("reference", statement.span.clone()),
            }]),
            Expression::Invocation(invocation) => {
                self.lower_invocation(invocation, bindings, lexical)
            }
            Expression::Block(block) => Ok(vec![self.lower_stack_block(
                block,
                bindings,
                lexical,
                statement.span.clone(),
            )?]),
            Expression::String(_) | Expression::Atom(_) => Err(Diagnostic::new(
                "E_INVALID_STATEMENT",
                "scalar values cannot be used as executable statements",
                statement.span.clone(),
            )),
        }
    }

    fn lower_invocation(
        &self,
        invocation: &super::syntax::Invocation,
        output_bindings: OutputBindings,
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        let descriptor = self.programs.get(&invocation.name.value).ok_or_else(|| {
            Diagnostic::new(
                "E_UNKNOWN_PROGRAM",
                format!("unknown program `{}`", invocation.name.value),
                invocation.name.span.clone(),
            )
        })?;

        let mut preceding = Vec::new();
        let mut arguments = BTreeMap::new();
        let mut assigned_parameters = BTreeSet::new();
        let mut has_named_graph_input = false;

        for argument in &invocation.arguments {
            let Argument::Named { name, value } = argument else {
                continue;
            };
            if arguments.contains_key(&name.value) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_ARGUMENT",
                    format!("duplicate argument `{}`", name.value),
                    name.span.clone(),
                ));
            }
            let lowered = if descriptor.input_slot(&name.value).is_some() {
                has_named_graph_input = true;
                self.lower_explicit_input(value, lexical)?
            } else if let Some(slot) = descriptor.parameter_slot(&name.value) {
                assigned_parameters.insert(slot.index());
                Self::lower_scalar_argument(value, &invocation.name.value, &name.value)?
            } else {
                return Err(Diagnostic::new(
                    "E_UNKNOWN_PROGRAM_ARGUMENT",
                    format!(
                        "unknown argument `{}` for program `{}`",
                        name.value, invocation.name.value
                    ),
                    name.span.clone(),
                ));
            };
            arguments.insert(name.value.clone(), lowered);
        }

        let mut next_parameter = 0;
        for argument in &invocation.arguments {
            let Argument::Positional(value) = argument else {
                continue;
            };
            if self.is_scalar_expression(value, lexical) {
                while assigned_parameters.contains(&next_parameter) {
                    next_parameter += 1;
                }
                let Some(parameter) = descriptor.parameters.get(next_parameter) else {
                    return Err(Diagnostic::new(
                        "E_TOO_MANY_POSITIONAL_ARGUMENTS",
                        format!(
                            "program `{}` has no remaining scalar parameter for this argument",
                            invocation.name.value
                        ),
                        value.span().clone(),
                    ));
                };
                arguments.insert(
                    parameter.name.clone(),
                    Self::lower_scalar_argument(value, &invocation.name.value, &parameter.name)?,
                );
                assigned_parameters.insert(next_parameter);
                next_parameter += 1;
            } else {
                if has_named_graph_input {
                    return Err(Diagnostic::new(
                        "E_MIXED_GRAPH_ARGUMENT_STYLES",
                        "positional graph expressions cannot be mixed with named graph inputs",
                        value.span().clone(),
                    ));
                }
                preceding.extend(self.lower_graph_expression(value, lexical)?);
            }
        }

        let mut body_lexical = lexical.clone();
        body_lexical.extend(descriptor.inputs.iter().map(|input| input.name.clone()));
        let body = invocation
            .body
            .as_ref()
            .map(|block| self.lower_program_body(block, &body_lexical))
            .transpose()?;

        preceding.push(Item {
            kind: ItemKind::Invocation(Invocation {
                program: invocation.name.clone(),
                type_argument: invocation.type_argument.clone(),
                stack_access: invocation.access.clone(),
                arguments,
                body,
            }),
            output_bindings,
            origin: ItemOrigin::authored(invocation.name.value.clone(), invocation.span.clone()),
        });
        Ok(preceding)
    }

    fn lower_scalar_argument(
        expression: &Expression,
        program: &str,
        parameter: &str,
    ) -> Result<ArgumentValue> {
        match expression {
            Expression::String(value) => Ok(ArgumentValue::Literal(Literal::String(
                value.value.clone(),
                value.span.clone(),
            ))),
            Expression::Atom(value) => {
                Ok(ArgumentValue::Literal(match value.value.parse::<i64>() {
                    Ok(integer) => Literal::Integer(integer, value.span.clone()),
                    Err(_) => Literal::String(value.value.clone(), value.span.clone()),
                }))
            }
            Expression::Reference(reference) => Ok(ArgumentValue::Reference(reference.clone())),
            Expression::Invocation(_) | Expression::Block(_) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                format!("parameter `{program}.{parameter}` requires a scalar value"),
                expression.span().clone(),
            )),
        }
    }

    fn lower_explicit_input(
        &self,
        expression: &Expression,
        lexical: &BTreeSet<String>,
    ) -> Result<ArgumentValue> {
        match expression {
            Expression::Reference(reference) => Ok(ArgumentValue::Reference(reference.clone())),
            Expression::Invocation(_) | Expression::Block(_) => {
                Ok(ArgumentValue::Body(ProgramBody {
                    items: self.lower_graph_expression(expression, lexical)?,
                    span: expression.span().clone(),
                }))
            }
            Expression::String(_) | Expression::Atom(_) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                "a graph input requires a reference, invocation, or stack block",
                expression.span().clone(),
            )),
        }
    }

    fn lower_graph_expression(
        &self,
        expression: &Expression,
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        match expression {
            Expression::Reference(reference) => Ok(vec![Item {
                kind: ItemKind::Reference(Reference {
                    name: reference.clone(),
                }),
                output_bindings: OutputBindings::None,
                origin: ItemOrigin::authored("reference", reference.span.clone()),
            }]),
            Expression::Invocation(invocation) => {
                self.lower_invocation(invocation, OutputBindings::None, lexical)
            }
            Expression::Block(block) => Ok(vec![self.lower_stack_block(
                block,
                OutputBindings::None,
                lexical,
                block.span.clone(),
            )?]),
            Expression::String(_) | Expression::Atom(_) => Err(Diagnostic::new(
                "E_INVALID_ARGUMENT_TYPE",
                "expected a graph-producing expression",
                expression.span().clone(),
            )),
        }
    }

    fn lower_stack_block(
        &self,
        block: &Block,
        output_bindings: OutputBindings,
        lexical: &BTreeSet<String>,
        span: crate::source::SourceSpan,
    ) -> Result<Item> {
        Ok(Item {
            kind: ItemKind::StackBlock(StackBlock {
                stack_access: block
                    .access
                    .as_ref()
                    .map_or(StackAccess::Owned, |access| access.value),
                body: self.lower_program_body(block, lexical)?,
            }),
            output_bindings,
            origin: ItemOrigin::authored("stack block", span),
        })
    }

    fn lower_program_body(&self, block: &Block, lexical: &BTreeSet<String>) -> Result<ProgramBody> {
        Ok(ProgramBody {
            items: self.lower_statements(&block.statements, lexical)?,
            span: block.span.clone(),
        })
    }

    fn is_scalar_expression(&self, expression: &Expression, lexical: &BTreeSet<String>) -> bool {
        match expression {
            Expression::String(_) | Expression::Atom(_) => true,
            Expression::Reference(reference) => {
                !lexical.contains(&reference.value) && self.parameters.contains(&reference.value)
            }
            Expression::Invocation(_) | Expression::Block(_) => false,
        }
    }
}

fn lower_output_bindings(bindings: &SyntaxOutputBindings) -> OutputBindings {
    match bindings {
        SyntaxOutputBindings::None => OutputBindings::None,
        SyntaxOutputBindings::One(name) => OutputBindings::One(name.clone()),
        SyntaxOutputBindings::Many(names, span) => {
            OutputBindings::Many(names.clone(), span.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler;
    use crate::model::ValueType;

    #[test]
    fn lowers_and_compiles_native_positional_graph_expressions() {
        let package = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nconfig {\n  video {\n    width = 1280\n    height = 720\n    fps = 30\n  }\n}\nflash(image(\"before.png\", 1s), image(\"after.png\", 1s), 3)\n",
        )
        .expect("native source");
        let compiled = compiler::compile(&package).expect("compiled source");
        assert_eq!(compiled.outputs().len(), 1);
        assert_eq!(compiled.outputs()[0].value_type(), ValueType::Video);
    }

    #[test]
    fn lowers_parameters_named_inputs_and_stack_blocks() {
        let package = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nparam duration: Duration = 1s\n{\n  set_audio(video=video(\"source.mp4\"), audio=audio(\"sound.wav\"))\n  image(\"card.png\", $duration, contain)\n} as (video, card)\ndrop<Video>\ndrop<Video>\n",
        )
        .expect("native source");
        let compiled = compiler::compile(&package).expect("compiled source");
        assert!(compiled.outputs().is_empty());
    }

    #[test]
    fn rejects_mixed_positional_and_named_graph_inputs() {
        let error = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nflash(image(\"after.png\", 1s), before=image(\"before.png\", 1s), frames=3)\n",
        )
        .expect_err("mixed graph argument styles");
        assert_eq!(error.code, "E_MIXED_GRAPH_ARGUMENT_STYLES");
    }

    #[test]
    fn string_parsing_rejects_file_backed_declarations() {
        let import = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nimport \"effect.clipasm\" as effect\n",
        )
        .expect_err("file-backed import");
        assert_eq!(import.code, "E_IMPORT_REQUIRES_FILE");

        let external = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nexternal \"effect.json\" as effect\n",
        )
        .expect_err("file-backed external");
        assert_eq!(external.code, "E_EXTERNAL_REQUIRES_FILE");
    }
}
