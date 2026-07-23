use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};
use crate::model::{
    FrameCount, FrameRange, FrameRate, ImageFit, NodeId, PixelFormat, SourceTime, ValueId,
    VideoSpec,
};
use crate::program::{Cardinality, ProgramDescriptor, ProgramRegistry};
use crate::syntax::{Argument, Item, ItemKind, Workflow};

const ENGINE_PLAN_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompiledPlan {
    pub plan_version: u32,
    pub workflow_version: u64,
    pub engine_version: String,
    pub plan_hash: String,
    pub video: VideoSpec,
    pub nodes: Vec<PlanNode>,
    pub root: NodeId,
    pub named_values: BTreeMap<String, NodeId>,
    pub explain: Vec<ExplainEntry>,
    pub output: Option<PathBuf>,
}

impl CompiledPlan {
    /// Serialize this plan as stable, pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if serialization fails.
    pub fn canonical_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            Diagnostic::new(
                "E_PLAN_SERIALIZATION",
                format!("could not serialize compiled plan: {error}"),
                SourceSpan::file_start("<compiled-plan>"),
            )
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanNode {
    pub id: NodeId,
    pub kind: PrimitiveNodeKind,
    pub frames: FrameCount,
    pub origin: SourceOrigin,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum PrimitiveNodeKind {
    ImageVideo {
        path: PathBuf,
        source_sha256: String,
        frames: FrameCount,
        fit: ImageFit,
    },
    Slice {
        input: NodeId,
        range: FrameRange,
    },
    Concat {
        inputs: Vec<NodeId>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceOrigin {
    pub construct: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplainEntry {
    pub construct: String,
    pub output: NodeId,
    pub id: Option<String>,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
struct SemanticNode {
    kind: SemanticKind,
    origin: SourceOrigin,
}

#[derive(Clone, Debug)]
enum SemanticKind {
    Image {
        path: PathBuf,
        source_sha256: String,
        frames: FrameCount,
        fit: ImageFit,
    },
    Reference(String),
    Concat(Vec<ValueId>),
    Slice {
        input: ValueId,
        range: FrameRange,
    },
    During {
        base: ValueId,
        processed: ValueId,
        range: FrameRange,
    },
}

#[derive(Clone, Debug)]
struct Symbol {
    span: SourceSpan,
    value: Option<ValueId>,
}

struct SurfaceRecord {
    construct: String,
    value: ValueId,
    id: Option<String>,
    span: SourceSpan,
}

struct Compiler<'a> {
    workflow: &'a Workflow,
    spec: VideoSpec,
    registry: ProgramRegistry,
    nodes: Vec<SemanticNode>,
    symbols: BTreeMap<String, Symbol>,
    symbol_order: Vec<String>,
    surface: Vec<SurfaceRecord>,
}

/// Parse and compile a workflow file without rendering it.
///
/// # Errors
///
/// Returns a source-located diagnostic for syntax, semantic, graph, file, or
/// domain errors.
pub fn compile_file(path: &Path) -> Result<CompiledPlan> {
    let workflow = crate::syntax::parse_file(path)?;
    compile(&workflow)
}

/// Compile an already parsed workflow into primitive executable IR.
///
/// # Errors
///
/// Returns a source-located diagnostic when the workflow violates a language
/// invariant or an input cannot be preflighted.
pub fn compile(workflow: &Workflow) -> Result<CompiledPlan> {
    let spec = resolve_video_spec(workflow)?;
    let mut compiler = Compiler {
        workflow,
        spec,
        registry: ProgramRegistry,
        nodes: Vec::new(),
        symbols: BTreeMap::new(),
        symbol_order: Vec::new(),
        surface: Vec::new(),
    };
    compiler.collect_names()?;
    let root = compiler.evaluate_all()?;
    compiler.resolve_references_and_cycles(root)?;
    let domains = compiler.infer_domains(root)?;
    compiler.lower(root, &domains)
}

fn resolve_video_spec(workflow: &Workflow) -> Result<VideoSpec> {
    let mut spec = VideoSpec::default();
    if let Some(width) = workflow.video.width {
        spec.width = width;
    }
    if let Some(height) = workflow.video.height {
        spec.height = height;
    }
    if let Some(fps) = &workflow.video.fps {
        spec.fps = FrameRate::parse(fps, &SourceSpan::file_start(&workflow.source_path))?;
    }
    if spec.width % 2 != 0 || spec.height % 2 != 0 {
        return Err(Diagnostic::new(
            "E_INVALID_VIDEO_SPEC",
            "yuv420p output requires even project width and height",
            SourceSpan::file_start(&workflow.source_path),
        ));
    }
    spec.pixel_format = PixelFormat::Yuv420p;
    spec.square_pixels = true;
    Ok(spec)
}

impl Compiler<'_> {
    fn collect_names(&mut self) -> Result<()> {
        for clip in &self.workflow.clips {
            self.add_symbol(&clip.name, &clip.span)?;
        }
        for clip in &self.workflow.clips {
            self.collect_item_names(&clip.body)?;
        }
        self.collect_item_names(&self.workflow.timeline)?;
        self.symbol_order.sort();
        Ok(())
    }

    fn collect_item_names(&mut self, items: &[Item]) -> Result<()> {
        for item in items {
            if let Some((name, span)) = &item.id {
                self.add_symbol(name, span)?;
            }
            match &item.kind {
                ItemKind::Then(body) | ItemKind::Join(body) | ItemKind::Timeline(body) => {
                    self.collect_item_names(body)?;
                }
                ItemKind::Call { .. } => {}
            }
        }
        Ok(())
    }

    fn add_symbol(&mut self, name: &str, span: &SourceSpan) -> Result<()> {
        if let Some(previous) = self.symbols.get(name) {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("duplicate user-visible name `{name}`"),
                span.clone(),
            )
            .note(format!(
                "the first `{name}` was declared at {}:{}:{}",
                previous.span.file.display(),
                previous.span.line,
                previous.span.column
            )));
        }
        self.symbol_order.push(name.to_owned());
        self.symbols.insert(
            name.to_owned(),
            Symbol {
                span: span.clone(),
                value: None,
            },
        );
        Ok(())
    }

    fn evaluate_all(&mut self) -> Result<ValueId> {
        let mut clips = self.workflow.clips.iter().collect::<Vec<_>>();
        clips.sort_by(|left, right| left.name.cmp(&right.name));
        for clip in clips {
            let mut stack = Vec::new();
            self.evaluate_body(&clip.body, &mut stack, None)?;
            if stack.len() != 1 {
                return Err(output_count_error(
                    "E_CLIP_OUTPUT_COUNT",
                    &format!("named clip `{}`", clip.name),
                    stack.len(),
                    &clip.span,
                ));
            }
            self.bind_symbol(&clip.name, stack[0])?;
            self.surface.push(SurfaceRecord {
                construct: "named clip".to_owned(),
                value: stack[0],
                id: Some(clip.name.clone()),
                span: clip.span.clone(),
            });
        }

        let mut root_stack = Vec::new();
        self.evaluate_body(&self.workflow.timeline, &mut root_stack, None)?;
        match root_stack.len() {
            0 => Err(Diagnostic::new(
                "E_EMPTY_TIMELINE",
                "timeline must produce at least one Video",
                SourceSpan::file_start(&self.workflow.source_path),
            )),
            1 => Ok(root_stack[0]),
            _ => {
                let inputs = std::mem::take(&mut root_stack);
                self.push_node(
                    SemanticKind::Concat(inputs),
                    "timeline",
                    SourceSpan::file_start(&self.workflow.source_path),
                )
            }
        }
    }

    fn evaluate_body(
        &mut self,
        items: &[Item],
        stack: &mut Vec<ValueId>,
        requested_frames: Option<FrameCount>,
    ) -> Result<()> {
        for item in items {
            let output = if let Some((range, range_span)) = &item.during {
                let base = pop_one(stack, "during", &item.span)?;
                let frame_range = range.to_frames(self.spec.fps, range_span)?;
                let selected = self.push_node(
                    SemanticKind::Slice {
                        input: base,
                        range: frame_range,
                    },
                    "during selection",
                    range_span.clone(),
                )?;
                let mut local = vec![selected];
                let result = self.evaluate_item_kind(
                    &item.kind,
                    &item.span,
                    &mut local,
                    Some(frame_range.frames()),
                )?;
                local.push(result);
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_DURING_OUTPUT_COUNT",
                        "`during` body",
                        local.len(),
                        &item.span,
                    ));
                }
                self.push_node(
                    SemanticKind::During {
                        base,
                        processed: local[0],
                        range: frame_range,
                    },
                    "during",
                    item.span.clone(),
                )?
            } else {
                self.evaluate_item_kind(&item.kind, &item.span, stack, requested_frames)?
            };
            stack.push(output);
            if let Some((name, _)) = &item.id {
                self.bind_symbol(name, output)?;
            }
            self.surface.push(SurfaceRecord {
                construct: if item.during.is_some() {
                    "during".to_owned()
                } else {
                    item_construct(&item.kind)
                },
                value: output,
                id: item.id.as_ref().map(|(name, _)| name.clone()),
                span: item.span.clone(),
            });
        }
        Ok(())
    }

    fn evaluate_item_kind(
        &mut self,
        kind: &ItemKind,
        span: &SourceSpan,
        outer_stack: &mut Vec<ValueId>,
        requested_frames: Option<FrameCount>,
    ) -> Result<ValueId> {
        match kind {
            ItemKind::Call { program, arguments } => {
                self.evaluate_call(program, arguments, outer_stack, requested_frames, span)
            }
            ItemKind::Then(body) => {
                let input = pop_one(outer_stack, "then", span)?;
                let mut local = vec![input];
                self.evaluate_body(body, &mut local, requested_frames)?;
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_THEN_OUTPUT_COUNT",
                        "`then` body",
                        local.len(),
                        span,
                    ));
                }
                Ok(local[0])
            }
            ItemKind::Join(body) => {
                if outer_stack.len() < 2 {
                    return Err(stack_underflow("join", 2, outer_stack.len(), span));
                }
                let split = outer_stack.len() - 2;
                let mut local = outer_stack.split_off(split);
                self.evaluate_body(body, &mut local, requested_frames)?;
                if local.len() != 1 {
                    return Err(output_count_error(
                        "E_JOIN_OUTPUT_COUNT",
                        "`join` body",
                        local.len(),
                        span,
                    ));
                }
                Ok(local[0])
            }
            ItemKind::Timeline(body) => {
                let mut local = Vec::new();
                self.evaluate_body(body, &mut local, requested_frames)?;
                match local.len() {
                    0 => Err(Diagnostic::new(
                        "E_EMPTY_TIMELINE",
                        "nested timeline must produce at least one Video",
                        span.clone(),
                    )),
                    1 => Ok(local[0]),
                    _ => self.push_node(SemanticKind::Concat(local), "timeline", span.clone()),
                }
            }
        }
    }

    fn evaluate_call(
        &mut self,
        program: &str,
        arguments: &BTreeMap<String, Argument>,
        stack: &mut Vec<ValueId>,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<ValueId> {
        let descriptor = self.registry.get(program).ok_or_else(|| {
            Diagnostic::new(
                "E_UNKNOWN_PROGRAM",
                format!("unknown program `{program}`"),
                span.clone(),
            )
        })?;
        let inputs = self.bind_inputs(descriptor, arguments, stack, span)?;
        match program {
            "image" => self.lower_image(arguments, requested_frames, span),
            "clip" => Ok(inputs[0]),
            "concat" => self.lower_concat(inputs, "concat", span),
            "repeat" => {
                let count = integer_argument(arguments, "count", span)?;
                if count < 1 {
                    return Err(Diagnostic::new(
                        "E_INVALID_REPEAT_COUNT",
                        "`repeat.count` must be an integer greater than or equal to one",
                        argument_span(arguments, "count", span),
                    ));
                }
                let count = usize::try_from(count).map_err(|_| {
                    Diagnostic::new(
                        "E_INVALID_REPEAT_COUNT",
                        "`repeat.count` is too large",
                        argument_span(arguments, "count", span),
                    )
                })?;
                let mut repeated = Vec::new();
                repeated.try_reserve_exact(count).map_err(|_| {
                    Diagnostic::new(
                        "E_INVALID_REPEAT_COUNT",
                        "`repeat.count` is too large to compile",
                        argument_span(arguments, "count", span),
                    )
                })?;
                repeated.resize(count, inputs[0]);
                self.lower_concat(repeated, "repeat", span)
            }
            _ => unreachable!("registry and lowering switch must agree"),
        }
    }

    fn bind_inputs(
        &mut self,
        descriptor: &ProgramDescriptor,
        arguments: &BTreeMap<String, Argument>,
        stack: &mut Vec<ValueId>,
        span: &SourceSpan,
    ) -> Result<Vec<ValueId>> {
        let mut bound = Vec::new();
        for port in descriptor.inputs {
            if let Some(argument) = arguments.get(port.name) {
                match port.cardinality {
                    Cardinality::One => {
                        bound.push(self.reference_argument(
                            argument,
                            descriptor.name,
                            port.name,
                        )?);
                    }
                    Cardinality::Variadic { min } => {
                        let values = match argument {
                            Argument::List(values, _) => values
                                .iter()
                                .map(|value| {
                                    self.reference_argument(value, descriptor.name, port.name)
                                })
                                .collect::<Result<Vec<_>>>()?,
                            _ => vec![self.reference_argument(
                                argument,
                                descriptor.name,
                                port.name,
                            )?],
                        };
                        if values.len() < min {
                            return Err(Diagnostic::new(
                                "E_MISSING_REQUIRED_INPUT",
                                format!(
                                    "`{}.{}` requires at least {min} Video value(s)",
                                    descriptor.name, port.name
                                ),
                                argument.span().clone(),
                            ));
                        }
                        bound.extend(values);
                    }
                }
                continue;
            }
            match port.cardinality {
                Cardinality::One => {
                    bound.push(pop_one(stack, descriptor.name, span)?);
                }
                Cardinality::Variadic { min } => {
                    if stack.len() < min {
                        let code = if descriptor.name == "concat" {
                            "E_EMPTY_CONCAT"
                        } else {
                            "E_MISSING_REQUIRED_INPUT"
                        };
                        return Err(Diagnostic::new(
                            code,
                            format!(
                                "`{}` needs at least {min} Video value(s), but the local stack has {}",
                                descriptor.name,
                                stack.len()
                            ),
                            span.clone(),
                        ));
                    }
                    bound.append(stack);
                }
            }
        }
        Ok(bound)
    }

    fn reference_argument(
        &mut self,
        argument: &Argument,
        program: &str,
        port: &str,
    ) -> Result<ValueId> {
        let Argument::Reference(name, span) = argument else {
            return Err(Diagnostic::new(
                "E_TYPE_MISMATCH",
                format!("`{program}.{port}` expects a Video reference"),
                argument.span().clone(),
            ));
        };
        self.push_node(
            SemanticKind::Reference(name.clone()),
            "reference",
            span.clone(),
        )
    }

    fn lower_image(
        &mut self,
        arguments: &BTreeMap<String, Argument>,
        requested_frames: Option<FrameCount>,
        span: &SourceSpan,
    ) -> Result<ValueId> {
        let (path_text, path_span) = string_argument(arguments, "path", span)?;
        let frames = if let Some(argument) = arguments.get("duration") {
            let (text, duration_span) = string_value(argument, "image.duration")?;
            FrameCount(
                SourceTime::parse(text, duration_span)?.to_frames(self.spec.fps, duration_span)?,
            )
        } else {
            requested_frames.ok_or_else(|| {
                Diagnostic::new(
                    "E_MISSING_IMAGE_DURATION",
                    "`image.duration` is required outside a context with a requested duration",
                    span.clone(),
                )
            })?
        };
        if frames.0 == 0 {
            return Err(Diagnostic::new(
                "E_INVALID_DURATION",
                "image duration must contain at least one frame",
                span.clone(),
            ));
        }
        let fit = if let Some(argument) = arguments.get("fit") {
            let (text, fit_span) = string_value(argument, "image.fit")?;
            ImageFit::parse(text, fit_span)?
        } else {
            ImageFit::Cover
        };
        let path = resolve_path(&self.workflow.source_path, path_text);
        let metadata = fs::metadata(&path).map_err(|error| {
            Diagnostic::new(
                "E_MISSING_IMAGE_FILE",
                format!("image file `{}` is not accessible: {error}", path.display()),
                path_span.clone(),
            )
        })?;
        if !metadata.is_file() {
            return Err(Diagnostic::new(
                "E_MISSING_IMAGE_FILE",
                format!("image path `{}` is not a file", path.display()),
                path_span.clone(),
            ));
        }
        let canonical = fs::canonicalize(&path).unwrap_or(path);
        let source_sha256 = hash_file(&canonical, path_span)?;
        self.push_node(
            SemanticKind::Image {
                path: canonical,
                source_sha256,
                frames,
                fit,
            },
            "image",
            span.clone(),
        )
    }

    fn lower_concat(
        &mut self,
        inputs: Vec<ValueId>,
        construct: &str,
        span: &SourceSpan,
    ) -> Result<ValueId> {
        match inputs.as_slice() {
            [] => Err(Diagnostic::new(
                "E_EMPTY_CONCAT",
                "`concat` requires at least one Video",
                span.clone(),
            )),
            [only] => Ok(*only),
            _ => self.push_node(SemanticKind::Concat(inputs), construct, span.clone()),
        }
    }

    fn push_node(
        &mut self,
        kind: SemanticKind,
        construct: impl Into<String>,
        span: SourceSpan,
    ) -> Result<ValueId> {
        let id = ValueId(u32::try_from(self.nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "semantic graph contains too many values",
                span.clone(),
            )
        })?);
        self.nodes.push(SemanticNode {
            kind,
            origin: SourceOrigin {
                construct: construct.into(),
                span,
            },
        });
        Ok(id)
    }

    fn bind_symbol(&mut self, name: &str, value: ValueId) -> Result<()> {
        let symbol = self
            .symbols
            .get_mut(name)
            .expect("all symbols were collected before evaluation");
        if symbol.value.replace(value).is_some() {
            return Err(Diagnostic::new(
                "E_DUPLICATE_NAME",
                format!("name `{name}` was bound more than once"),
                symbol.span.clone(),
            ));
        }
        Ok(())
    }

    fn resolve_references_and_cycles(&self, root: ValueId) -> Result<()> {
        for node in &self.nodes {
            if let SemanticKind::Reference(name) = &node.kind {
                let Some(symbol) = self.symbols.get(name) else {
                    return Err(Diagnostic::new(
                        "E_MISSING_REFERENCE",
                        format!("reference `${name}` does not name any clip or invocation id"),
                        node.origin.span.clone(),
                    ));
                };
                if symbol.value.is_none() {
                    return Err(Diagnostic::new(
                        "E_MISSING_REFERENCE",
                        format!("name `{name}` has no compiled value"),
                        node.origin.span.clone(),
                    ));
                }
            }
        }

        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for name in &self.symbol_order {
            let value = self.symbols[name]
                .value
                .expect("every collected symbol is evaluated");
            let mut references = BTreeSet::new();
            self.collect_direct_references(value, &mut references, &mut BTreeSet::new());
            edges.insert(name.clone(), references);
        }
        let mut states = BTreeMap::<String, u8>::new();
        let mut path = Vec::new();
        for name in &self.symbol_order {
            detect_symbol_cycle(name, &edges, &mut states, &mut path, &self.symbols)?;
        }

        // Also walk root to ensure every reference reachable only from the timeline was checked.
        let mut root_refs = BTreeSet::new();
        self.collect_direct_references(root, &mut root_refs, &mut BTreeSet::new());
        Ok(())
    }

    fn collect_direct_references(
        &self,
        value: ValueId,
        output: &mut BTreeSet<String>,
        visited: &mut BTreeSet<ValueId>,
    ) {
        if !visited.insert(value) {
            return;
        }
        match &self.nodes[value.0 as usize].kind {
            SemanticKind::Image { .. } => {}
            SemanticKind::Reference(name) => {
                output.insert(name.clone());
            }
            SemanticKind::Concat(inputs) => {
                for input in inputs {
                    self.collect_direct_references(*input, output, visited);
                }
            }
            SemanticKind::Slice { input, .. } => {
                self.collect_direct_references(*input, output, visited);
            }
            SemanticKind::During {
                base, processed, ..
            } => {
                self.collect_direct_references(*base, output, visited);
                self.collect_direct_references(*processed, output, visited);
            }
        }
    }

    fn infer_domains(&self, root: ValueId) -> Result<Vec<Option<FrameCount>>> {
        let mut domains = vec![None; self.nodes.len()];
        let mut visiting = BTreeSet::new();
        for name in &self.symbol_order {
            let value = self.symbols[name]
                .value
                .expect("every collected symbol is evaluated");
            self.infer_value(value, &mut domains, &mut visiting)?;
        }
        self.infer_value(root, &mut domains, &mut visiting)?;
        Ok(domains)
    }

    fn infer_value(
        &self,
        value: ValueId,
        domains: &mut [Option<FrameCount>],
        visiting: &mut BTreeSet<ValueId>,
    ) -> Result<FrameCount> {
        if let Some(frames) = domains[value.0 as usize] {
            return Ok(frames);
        }
        if !visiting.insert(value) {
            return Err(Diagnostic::new(
                "E_DEPENDENCY_CYCLE",
                "dependency cycle encountered while inferring video duration",
                self.nodes[value.0 as usize].origin.span.clone(),
            ));
        }
        let node = &self.nodes[value.0 as usize];
        let frames = match &node.kind {
            SemanticKind::Image { frames, .. } => *frames,
            SemanticKind::Reference(name) => {
                let target = self.symbols[name]
                    .value
                    .expect("references were resolved before domain inference");
                self.infer_value(target, domains, visiting)?
            }
            SemanticKind::Concat(inputs) => {
                let mut total = FrameCount(0);
                for input in inputs {
                    total = total.checked_add(
                        self.infer_value(*input, domains, visiting)?,
                        &node.origin.span,
                    )?;
                }
                total
            }
            SemanticKind::Slice { input, range } => {
                let input_frames = self.infer_value(*input, domains, visiting)?;
                validate_range(*range, input_frames, &node.origin.span)?;
                range.frames()
            }
            SemanticKind::During {
                base,
                processed,
                range,
            } => {
                let base_frames = self.infer_value(*base, domains, visiting)?;
                validate_range(*range, base_frames, &node.origin.span)?;
                let processed_frames = self.infer_value(*processed, domains, visiting)?;
                FrameCount(base_frames.0 - range.frames().0)
                    .checked_add(processed_frames, &node.origin.span)?
            }
        };
        visiting.remove(&value);
        domains[value.0 as usize] = Some(frames);
        Ok(frames)
    }

    fn lower(&self, root: ValueId, domains: &[Option<FrameCount>]) -> Result<CompiledPlan> {
        let mut lowerer = Lowerer {
            compiler: self,
            domains,
            plan_nodes: Vec::new(),
            lowered: HashMap::new(),
        };
        let mut named_values = BTreeMap::new();
        for name in &self.symbol_order {
            let value = self.symbols[name]
                .value
                .expect("every collected symbol is evaluated");
            let node = lowerer.lower_value(value)?;
            named_values.insert(name.clone(), node);
        }
        let root = lowerer.lower_value(root)?;
        let mut explain = Vec::with_capacity(self.surface.len() + 1);
        for record in &self.surface {
            explain.push(ExplainEntry {
                construct: record.construct.clone(),
                output: lowerer.lower_value(record.value)?,
                id: record.id.clone(),
                span: record.span.clone(),
            });
        }
        explain.push(ExplainEntry {
            construct: "root timeline".to_owned(),
            output: root,
            id: None,
            span: SourceSpan::file_start(&self.workflow.source_path),
        });
        let output =
            self.workflow.output.as_ref().map(|path| {
                resolve_path(&self.workflow.source_path, path.to_string_lossy().as_ref())
            });
        let plan_hash = plan_hash(&self.spec, root, &named_values, &lowerer.plan_nodes)?;
        Ok(CompiledPlan {
            plan_version: ENGINE_PLAN_VERSION,
            workflow_version: self.workflow.version,
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
            plan_hash,
            video: self.spec.clone(),
            nodes: lowerer.plan_nodes,
            root,
            named_values,
            explain,
            output,
        })
    }
}

struct Lowerer<'a> {
    compiler: &'a Compiler<'a>,
    domains: &'a [Option<FrameCount>],
    plan_nodes: Vec<PlanNode>,
    lowered: HashMap<ValueId, NodeId>,
}

impl Lowerer<'_> {
    #[allow(clippy::too_many_lines)]
    fn lower_value(&mut self, value: ValueId) -> Result<NodeId> {
        if let Some(node) = self.lowered.get(&value) {
            return Ok(*node);
        }
        let semantic = &self.compiler.nodes[value.0 as usize];
        let result = match &semantic.kind {
            SemanticKind::Image {
                path,
                source_sha256,
                frames,
                fit,
            } => self.add_node(
                PrimitiveNodeKind::ImageVideo {
                    path: path.clone(),
                    source_sha256: source_sha256.clone(),
                    frames: *frames,
                    fit: *fit,
                },
                *frames,
                semantic.origin.clone(),
            )?,
            SemanticKind::Reference(name) => {
                let target = self.compiler.symbols[name]
                    .value
                    .expect("references were resolved");
                self.lower_value(target)?
            }
            SemanticKind::Concat(inputs) => {
                let inputs = inputs
                    .iter()
                    .map(|input| self.lower_value(*input))
                    .collect::<Result<Vec<_>>>()?;
                self.add_node(
                    PrimitiveNodeKind::Concat { inputs },
                    domain(self.domains, value),
                    semantic.origin.clone(),
                )?
            }
            SemanticKind::Slice { input, range } => {
                let input = self.lower_value(*input)?;
                self.add_node(
                    PrimitiveNodeKind::Slice {
                        input,
                        range: *range,
                    },
                    range.frames(),
                    semantic.origin.clone(),
                )?
            }
            SemanticKind::During {
                base,
                processed,
                range,
            } => {
                let base_node = self.lower_value(*base)?;
                let processed_node = self.lower_value(*processed)?;
                let base_frames = domain(self.domains, *base);
                let mut pieces = Vec::new();
                if range.start > 0 {
                    pieces.push(self.add_node(
                        PrimitiveNodeKind::Slice {
                            input: base_node,
                            range: FrameRange {
                                start: 0,
                                end: range.start,
                            },
                        },
                        FrameCount(range.start),
                        SourceOrigin {
                            construct: "during prefix".to_owned(),
                            span: semantic.origin.span.clone(),
                        },
                    )?);
                }
                pieces.push(processed_node);
                if range.end < base_frames.0 {
                    pieces.push(self.add_node(
                        PrimitiveNodeKind::Slice {
                            input: base_node,
                            range: FrameRange {
                                start: range.end,
                                end: base_frames.0,
                            },
                        },
                        FrameCount(base_frames.0 - range.end),
                        SourceOrigin {
                            construct: "during suffix".to_owned(),
                            span: semantic.origin.span.clone(),
                        },
                    )?);
                }
                if pieces.len() == 1 {
                    pieces[0]
                } else {
                    self.add_node(
                        PrimitiveNodeKind::Concat { inputs: pieces },
                        domain(self.domains, value),
                        semantic.origin.clone(),
                    )?
                }
            }
        };
        self.lowered.insert(value, result);
        Ok(result)
    }

    fn add_node(
        &mut self,
        kind: PrimitiveNodeKind,
        frames: FrameCount,
        origin: SourceOrigin,
    ) -> Result<NodeId> {
        let id = NodeId(u32::try_from(self.plan_nodes.len()).map_err(|_| {
            Diagnostic::new(
                "E_GRAPH_TOO_LARGE",
                "compiled graph contains too many primitive nodes",
                origin.span.clone(),
            )
        })?);
        let fingerprint = fingerprint_node(&kind, frames, &self.compiler.spec, &self.plan_nodes)?;
        self.plan_nodes.push(PlanNode {
            id,
            kind,
            frames,
            origin,
            fingerprint,
        });
        Ok(id)
    }
}

fn plan_hash(
    spec: &VideoSpec,
    root: NodeId,
    names: &BTreeMap<String, NodeId>,
    nodes: &[PlanNode],
) -> Result<String> {
    #[derive(Serialize)]
    struct PlanIdentity<'a> {
        engine_version: &'a str,
        video: &'a VideoSpec,
        root: &'a str,
        names: BTreeMap<&'a str, &'a str>,
    }
    let named_fingerprints = names
        .iter()
        .map(|(name, id)| (name.as_str(), nodes[id.0 as usize].fingerprint.as_str()))
        .collect();
    let identity = PlanIdentity {
        engine_version: env!("CARGO_PKG_VERSION"),
        video: spec,
        root: &nodes[root.0 as usize].fingerprint,
        names: named_fingerprints,
    };
    let bytes = serde_json::to_vec(&identity).map_err(|error| {
        Diagnostic::new(
            "E_FINGERPRINT",
            format!("could not fingerprint compiled plan: {error}"),
            SourceSpan::file_start("<compiled-plan>"),
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn fingerprint_node(
    kind: &PrimitiveNodeKind,
    frames: FrameCount,
    spec: &VideoSpec,
    existing: &[PlanNode],
) -> Result<String> {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        primitive_version: u32,
        engine_version: &'a str,
        kind: serde_json::Value,
        frames: FrameCount,
        video: &'a VideoSpec,
        upstream: Vec<&'a str>,
    }
    let (kind_identity, input_ids): (serde_json::Value, Vec<NodeId>) = match kind {
        PrimitiveNodeKind::ImageVideo {
            path,
            source_sha256,
            frames,
            fit,
        } => (
            serde_json::json!({
                "operation": "image_video",
                "path": path,
                "source_sha256": source_sha256,
                "frames": frames,
                "fit": fit,
            }),
            Vec::new(),
        ),
        PrimitiveNodeKind::Slice { input, range } => (
            serde_json::json!({
                "operation": "slice",
                "range": range,
            }),
            vec![*input],
        ),
        PrimitiveNodeKind::Concat { inputs } => (
            serde_json::json!({
                "operation": "concat",
            }),
            inputs.clone(),
        ),
    };
    let upstream = input_ids
        .iter()
        .map(|id| existing[id.0 as usize].fingerprint.as_str())
        .collect();
    let bytes = serde_json::to_vec(&Fingerprint {
        primitive_version: 1,
        engine_version: env!("CARGO_PKG_VERSION"),
        kind: kind_identity,
        frames,
        video: spec,
        upstream,
    })
    .map_err(|error| {
        Diagnostic::new(
            "E_FINGERPRINT",
            format!("could not fingerprint primitive node: {error}"),
            SourceSpan::file_start("<compiled-plan>"),
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn detect_symbol_cycle(
    name: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    states: &mut BTreeMap<String, u8>,
    path: &mut Vec<String>,
    symbols: &BTreeMap<String, Symbol>,
) -> Result<()> {
    match states.get(name).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => {
            let start = path.iter().position(|entry| entry == name).unwrap_or(0);
            let mut cycle = path[start..].to_vec();
            cycle.push(name.to_owned());
            return Err(Diagnostic::new(
                "E_DEPENDENCY_CYCLE",
                format!("named-value dependency cycle: {}", cycle.join(" -> ")),
                symbols[name].span.clone(),
            ));
        }
        _ => {}
    }
    states.insert(name.to_owned(), 1);
    path.push(name.to_owned());
    if let Some(targets) = edges.get(name) {
        for target in targets {
            detect_symbol_cycle(target, edges, states, path, symbols)?;
        }
    }
    path.pop();
    states.insert(name.to_owned(), 2);
    Ok(())
}

fn validate_range(range: FrameRange, input: FrameCount, span: &SourceSpan) -> Result<()> {
    if range.start >= range.end || range.end > input.0 {
        return Err(Diagnostic::new(
            "E_INVALID_DURING_RANGE",
            format!(
                "frame range {}..{} is outside the base Video domain of {} frames",
                range.start, range.end, input.0
            ),
            span.clone(),
        ));
    }
    Ok(())
}

fn domain(domains: &[Option<FrameCount>], value: ValueId) -> FrameCount {
    domains[value.0 as usize].expect("all semantic domains were inferred")
}

fn pop_one(stack: &mut Vec<ValueId>, program: &str, span: &SourceSpan) -> Result<ValueId> {
    stack
        .pop()
        .ok_or_else(|| stack_underflow(program, 1, 0, span))
}

fn stack_underflow(
    program: &str,
    required: usize,
    available: usize,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        "E_STACK_UNDERFLOW",
        format!(
            "`{program}` needs {required} preceding Video value(s), but the local stack has {available}"
        ),
        span.clone(),
    )
}

fn output_count_error(
    code: &'static str,
    owner: &str,
    count: usize,
    span: &SourceSpan,
) -> Diagnostic {
    Diagnostic::new(
        code,
        format!("{owner} must leave exactly one Video, but {count} values remain"),
        span.clone(),
    )
}

fn item_construct(kind: &ItemKind) -> String {
    match kind {
        ItemKind::Call { program, .. } => program.clone(),
        ItemKind::Then(_) => "then".to_owned(),
        ItemKind::Join(_) => "join".to_owned(),
        ItemKind::Timeline(_) => "timeline".to_owned(),
    }
}

fn integer_argument(
    arguments: &BTreeMap<String, Argument>,
    name: &str,
    span: &SourceSpan,
) -> Result<i64> {
    match arguments.get(name) {
        Some(Argument::Integer(value, _)) => Ok(*value),
        Some(value) => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!("`{name}` must be an integer"),
            value.span().clone(),
        )),
        None => Err(Diagnostic::new(
            "E_MISSING_ARGUMENT",
            format!("missing required argument `{name}`"),
            span.clone(),
        )),
    }
}

fn string_argument<'a>(
    arguments: &'a BTreeMap<String, Argument>,
    name: &str,
    span: &SourceSpan,
) -> Result<(&'a str, &'a SourceSpan)> {
    let Some(value) = arguments.get(name) else {
        return Err(Diagnostic::new(
            "E_MISSING_ARGUMENT",
            format!("missing required argument `{name}`"),
            span.clone(),
        ));
    };
    string_value(value, name)
}

fn string_value<'a>(argument: &'a Argument, name: &str) -> Result<(&'a str, &'a SourceSpan)> {
    match argument {
        Argument::String(value, span) => Ok((value, span)),
        _ => Err(Diagnostic::new(
            "E_INVALID_ARGUMENT_TYPE",
            format!("`{name}` must be a string"),
            argument.span().clone(),
        )),
    }
}

fn argument_span(
    arguments: &BTreeMap<String, Argument>,
    name: &str,
    fallback: &SourceSpan,
) -> SourceSpan {
    arguments
        .get(name)
        .map_or_else(|| fallback.clone(), |argument| argument.span().clone())
}

fn resolve_path(workflow: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workflow
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

fn hash_file(path: &Path, span: &SourceSpan) -> Result<String> {
    let bytes = fs::read(path).map_err(|error| {
        Diagnostic::new(
            "E_INPUT_HASH",
            format!("could not read image `{}`: {error}", path.display()),
            span.clone(),
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
