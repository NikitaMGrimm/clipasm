use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::program::{Cardinality, InputPort, ProgramDescriptor, ProgramRegistry, StackAccess};
use crate::source::{
    ArgumentValue, Invocation, Item, ItemKind, ItemOrigin, Literal, OutputBindings, ProgramBody,
    ProjectSettings, Reference, SOURCE_PROGRAM_DEFAULT_STACK_ACCESS,
    STACK_BLOCK_DEFAULT_STACK_ACCESS, SourceExternalImplementation, SourceFile, SourceImport,
    SourcePackage, SourceParameter, SourceProgram, SourceProgramImplementation, SourceUnit,
    SourceUnitId, Spanned, StackBlock, UnlinkedSourceUnit, VideoSettings,
};

use super::syntax::{
    Argument, Block, Declaration, Expression, OutputBindings as SyntaxOutputBindings, Scalar,
    SourceFileSyntax, Statement,
};
use super::{parser, sugar};

/// Parse and lower one in-memory native `.clipasm` source program.
///
/// Imports require [`super::parse_file`] because they need package loading.
///
/// # Errors
///
/// Returns a source-located diagnostic for syntax or lowering failures.
pub fn parse_str(path: &Path, text: &str) -> Result<SourcePackage> {
    parse_str_with_registry(path, text, &ProgramRegistry::default())
}

pub(crate) fn parse_str_with_registry(
    path: &Path,
    text: &str,
    registry: &ProgramRegistry,
) -> Result<SourcePackage> {
    let source = SourceFile::new(path.to_path_buf(), text.to_owned());
    let syntax = parser::parse(source.clone())?;
    reject_file_backed_declarations(&syntax)?;
    let programs = registry_shapes(registry);
    let unit = lower_source(source, syntax, &programs)?;
    Ok(SourcePackage {
        root: SourceUnitId(0),
        units: vec![SourceUnit {
            source: unit.source,
            imports: Vec::new(),
            project: unit.project,
            program: unit.program,
            output: unit.output,
        }],
    })
}

pub(crate) fn lower_source(
    source: SourceFile,
    syntax: SourceFileSyntax,
    programs: &BTreeMap<String, CallableShape>,
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
    let implementation = match declarations.external {
        Some(external) => {
            if !syntax.statements.is_empty() {
                return Err(Diagnostic::new(
                    "E_EXTERNAL_WITH_BODY",
                    "an external program cannot also contain executable statements",
                    syntax.statements[0].span.clone(),
                ));
            }
            SourceProgramImplementation::External(external)
        }
        None => SourceProgramImplementation::Body(ProgramBody {
            items: lowerer.lower_statements(&syntax.statements, &lexical)?,
            span: syntax.span.clone(),
        }),
    };
    Ok(UnlinkedSourceUnit {
        source,
        imports: declarations.imports,
        project: declarations.project,
        program: SourceProgram {
            inputs: declarations.inputs,
            parameters: declarations.parameters,
            implementation,
            span: syntax.version.span,
            stack_access: SOURCE_PROGRAM_DEFAULT_STACK_ACCESS,
        },
        output: declarations.output,
    })
}

pub(super) fn builtin_shapes() -> BTreeMap<String, CallableShape> {
    registry_shapes(&ProgramRegistry::default())
}

fn registry_shapes(registry: &ProgramRegistry) -> BTreeMap<String, CallableShape> {
    registry
        .definitions()
        .iter()
        .map(|definition| {
            (
                definition.descriptor.name.clone(),
                CallableShape::from_descriptor(&definition.descriptor),
            )
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(super) struct CallableShape {
    inputs: Vec<String>,
    parameters: Vec<String>,
}

impl CallableShape {
    pub(super) fn from_descriptor(descriptor: &ProgramDescriptor) -> Self {
        Self {
            inputs: descriptor
                .inputs
                .iter()
                .map(|input| input.name.clone())
                .collect(),
            parameters: descriptor
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
        }
    }

    pub(super) fn from_source(program: &SourceProgram) -> Self {
        Self {
            inputs: program
                .inputs()
                .iter()
                .map(|input| input.name.clone())
                .collect(),
            parameters: program
                .parameters()
                .iter()
                .map(|parameter| parameter.name.value.clone())
                .collect(),
        }
    }

    fn has_input(&self, name: &str) -> bool {
        self.inputs.iter().any(|input| input == name)
    }

    fn parameter_index(&self, name: &str) -> Option<usize> {
        self.parameters
            .iter()
            .position(|parameter| parameter == name)
    }
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
            Declaration::Config(_)
            | Declaration::External(_)
            | Declaration::Input(_)
            | Declaration::Parameter(_) => {}
        }
    }
    Ok(())
}

struct LoweredDeclarations {
    imports: Vec<SourceImport>,
    external: Option<SourceExternalImplementation>,
    project: Option<Spanned<ProjectSettings>>,
    output: Option<Spanned<PathBuf>>,
    inputs: Vec<InputPort>,
    parameters: Vec<SourceParameter>,
}

fn lower_declarations(declarations: Vec<Declaration>) -> Result<LoweredDeclarations> {
    let mut imports = Vec::new();
    let mut external = None;
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
            }),
            Declaration::External(declaration) => {
                if external.is_some() {
                    return Err(Diagnostic::new(
                        "E_DUPLICATE_EXTERNAL",
                        "a source file may declare at most one `external` block",
                        declaration.span,
                    ));
                }
                external = Some(lower_external_declaration(declaration)?);
            }
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
        external,
        project,
        output,
        inputs,
        parameters,
    })
}

fn lower_external_declaration(
    declaration: super::syntax::ExternalDeclaration,
) -> Result<SourceExternalImplementation> {
    let command = declaration.command.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_EXTERNAL_FIELD",
            "external program requires `command`",
            declaration.span.clone(),
        )
    })?;
    if command.value.is_empty() {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "external `command` must not be empty",
            command.span,
        ));
    }
    let semantic_version = declaration.semantic_version.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_EXTERNAL_FIELD",
            "external program requires `semantic_version`",
            declaration.span.clone(),
        )
    })?;
    let semantic_version_value = semantic_version.value.parse::<u32>().map_err(|_| {
        Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "external `semantic_version` must be a positive unsigned integer",
            semantic_version.span.clone(),
        )
    })?;
    if semantic_version_value == 0 {
        return Err(Diagnostic::new(
            "E_INVALID_EXTERNAL_PROGRAM",
            "external `semantic_version` must be greater than zero",
            semantic_version.span,
        ));
    }
    let preserve = declaration.preserve.ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_EXTERNAL_FIELD",
            "external program requires `preserve`",
            declaration.span,
        )
    })?;
    Ok(SourceExternalImplementation {
        command: Spanned::new(PathBuf::from(command.value), command.span),
        semantic_version: Spanned::new(semantic_version_value, semantic_version.span),
        preserve,
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
    programs: &'a BTreeMap<String, CallableShape>,
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
        if let Some(sugar) = sugar::resolve(&invocation.name.value) {
            return self.lower_sugar(sugar, invocation, output_bindings, lexical);
        }
        let shape = self.programs.get(&invocation.name.value).ok_or_else(|| {
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
            let lowered = if shape.has_input(&name.value) {
                has_named_graph_input = true;
                self.lower_explicit_input(value, lexical)?
            } else if let Some(slot) = shape.parameter_index(&name.value) {
                assigned_parameters.insert(slot);
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
                let Some(parameter) = shape.parameters.get(next_parameter) else {
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
                    parameter.clone(),
                    Self::lower_scalar_argument(value, &invocation.name.value, parameter)?,
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
        body_lexical.extend(shape.inputs.iter().cloned());
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

    fn lower_sugar(
        &self,
        sugar: sugar::Sugar,
        invocation: &super::syntax::Invocation,
        output_bindings: OutputBindings,
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        match sugar {
            sugar::Sugar::Clip => self.lower_clip(invocation, output_bindings, lexical),
        }
    }

    fn lower_clip(
        &self,
        invocation: &super::syntax::Invocation,
        output_bindings: OutputBindings,
        lexical: &BTreeSet<String>,
    ) -> Result<Vec<Item>> {
        if let Some(argument) = invocation.arguments.first() {
            let span = match argument {
                Argument::Positional(value) => value.span(),
                Argument::Named { name, .. } => &name.span,
            };
            return Err(Diagnostic::new(
                "E_UNEXPECTED_SUGAR_ARGUMENT",
                "`clip` does not accept arguments; put operations inside its body",
                span.clone(),
            ));
        }
        let body = invocation.body.as_ref().ok_or_else(|| {
            Diagnostic::new(
                "E_MISSING_PROGRAM_BODY",
                "`clip` requires a body",
                invocation.name.span.clone(),
            )
        })?;
        let body = self.lower_program_body(body, lexical)?;
        let expansion = sugar::Expansion::new(sugar::Sugar::Clip, invocation.span.clone());
        let access = invocation
            .access
            .as_ref()
            .map_or(StackAccess::Owned, |access| access.value);
        let access_span = invocation.access.as_ref().map_or_else(
            || invocation.name.span.clone(),
            |access| access.span.clone(),
        );

        Ok(vec![
            expansion.visible(
                "result",
                ItemKind::Invocation(Invocation {
                    program: Spanned::new("glue".to_owned(), invocation.name.span.clone()),
                    type_argument: invocation.type_argument.clone(),
                    stack_access: Some(Spanned::new(access, access_span)),
                    arguments: BTreeMap::new(),
                    body: Some(body),
                }),
                output_bindings,
            ),
            expansion.hidden(
                "cleanup",
                ItemKind::Invocation(Invocation {
                    program: Spanned::new("drop".to_owned(), invocation.name.span.clone()),
                    type_argument: None,
                    stack_access: Some(Spanned::new(
                        StackAccess::Owned,
                        invocation.name.span.clone(),
                    )),
                    arguments: BTreeMap::new(),
                    body: None,
                }),
            ),
        ])
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
                    .map_or(STACK_BLOCK_DEFAULT_STACK_ACCESS, |access| access.value),
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
    use crate::source::SurfaceVisibility;

    fn body(package: &SourcePackage) -> &ProgramBody {
        let SourceProgramImplementation::Body(body) = package.root().program().implementation()
        else {
            panic!("expected ClipAsm body");
        };
        body
    }

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
        let ItemKind::StackBlock(block) = &body(&package).items[0].kind else {
            panic!("stack block");
        };
        assert_eq!(block.stack_access, StackAccess::Visible);
        let compiled = compiler::compile(&package).expect("compiled source");
        assert!(compiled.outputs().is_empty());
    }

    #[test]
    fn explicit_owned_stack_block_overrides_the_visible_default() {
        let package = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\n@owned { image(\"card.png\", 1s) }\n",
        )
        .expect("native source");
        let ItemKind::StackBlock(block) = &body(&package).items[0].kind else {
            panic!("stack block");
        };
        assert_eq!(block.stack_access, StackAccess::Owned);
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
    fn string_parsing_rejects_imports_that_require_package_loading() {
        let import = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nimport \"effect.clipasm\" as effect\n",
        )
        .expect_err("file-backed import");
        assert_eq!(import.code, "E_IMPORT_REQUIRES_FILE");
    }

    #[test]
    fn clip_expands_with_surface_provenance_and_hidden_cleanup() {
        let package = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\n@visible clip<Audio> {\n  audio(\"sound.wav\")\n} as soundtrack\n$soundtrack\n",
        )
        .expect("native clip");
        let items = &body(&package).items;
        assert_eq!(items.len(), 3);

        let ItemKind::Invocation(glue) = &items[0].kind else {
            panic!("generated glue");
        };
        assert_eq!(glue.program.value, "glue");
        assert_eq!(
            glue.stack_access.as_ref().map(|access| access.value),
            Some(StackAccess::Visible)
        );
        assert_eq!(
            glue.type_argument.as_ref().map(|argument| argument.value),
            Some(ValueType::Audio)
        );
        assert_eq!(items[0].origin.construct, "clip");
        assert_eq!(items[0].origin.visibility, SurfaceVisibility::Visible);
        assert_eq!(items[0].origin.expansion.len(), 1);
        assert_eq!(items[0].origin.expansion[0].sugar, "clip");
        assert_eq!(items[0].origin.expansion[0].role, "result");

        let ItemKind::Invocation(drop) = &items[1].kind else {
            panic!("generated drop");
        };
        assert_eq!(drop.program.value, "drop");
        assert_eq!(
            drop.stack_access.as_ref().map(|access| access.value),
            Some(StackAccess::Owned)
        );
        assert_eq!(items[1].origin.construct, "clip");
        assert_eq!(items[1].origin.visibility, SurfaceVisibility::Hidden);
        assert_eq!(items[1].origin.expansion[0].role, "cleanup");

        let compiled = compiler::compile(&package).expect("compiled clip");
        assert_eq!(compiled.outputs().len(), 1);
        assert_eq!(compiled.outputs()[0].value_type(), ValueType::Audio);
        let constructs = compiled
            .explain()
            .iter()
            .map(crate::compiler::ExplainEntry::construct)
            .collect::<Vec<_>>();
        assert_eq!(
            constructs
                .iter()
                .filter(|construct| **construct == "clip")
                .count(),
            1
        );
        assert!(!constructs.contains(&"drop"));
    }

    #[test]
    fn clip_is_semantically_equal_to_explicit_glue_and_drop() {
        let sugar = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nclip {\n  image(\"card.png\", 1s)\n} as opening\n$opening\n",
        )
        .expect("sugar source");
        let explicit = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\n@owned glue {\n  image(\"card.png\", 1s)\n} as opening\n@owned drop\n$opening\n",
        )
        .expect("explicit source");
        assert_eq!(
            compiler::compile(&sugar)
                .expect("compiled sugar")
                .structure_hash(),
            compiler::compile(&explicit)
                .expect("compiled explicit form")
                .structure_hash()
        );
    }

    #[test]
    fn clip_reports_generated_body_errors_as_clip() {
        let package = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nclip {\n  image(\"card.png\", 1s)\n  audio(\"sound.wav\")\n}\n",
        )
        .expect("native clip");
        let error = compiler::compile(&package).expect_err("mixed clip body");
        assert!(error.message.contains("`clip`"), "{}", error.message);
        assert!(!error.message.contains("`glue`"), "{}", error.message);
    }

    #[test]
    fn clip_requires_a_body_and_rejects_arguments() {
        let missing = parse_str(Path::new("program.clipasm"), "clipasm 1\nclip\n")
            .expect_err("missing clip body");
        assert_eq!(missing.code, "E_MISSING_PROGRAM_BODY");

        let argument = parse_str(
            Path::new("program.clipasm"),
            "clipasm 1\nclip(1) {\n  image(\"card.png\", 1s)\n}\n",
        )
        .expect_err("clip argument");
        assert_eq!(argument.code, "E_UNEXPECTED_SUGAR_ARGUMENT");
    }
}
