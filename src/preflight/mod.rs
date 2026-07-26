//! Media-aware preparation of compiled programs for rendering.
//!
//! Preflight is the first pipeline phase that performs I/O. It resolves
//! result-reachable assets, verifies source contracts and required tool capabilities,
//! derives exact media domains, and lowers semantic operations into a
//! [`PreparedPlan`] containing renderer primitives. The plan remains directly
//! inspectable through Rust getters and [`PreparedPlan::prepared_json`], whose
//! explicit adapter is separate from the renderer's private representation.
//!
//! ```no_run
//! use std::path::Path;
//!
//! let source = clipasm::language::parse_file(Path::new("program.clipasm"))?;
//! let compiled = clipasm::compiler::compile(&source)?;
//! let plan = clipasm::preflight::preflight(&compiled)?;
//! let result = &plan.nodes()[plan.result().get() as usize];
//! println!("prepared {} frames", result.video_domain().expect("Video result").frames().0);
//! # Ok::<(), clipasm::diagnostic::Diagnostic>(())
//! ```

use std::collections::{BTreeMap, HashMap};

use crate::compiler::CompiledProgram;
use crate::diagnostic::Result;
use crate::source::SourceSpan;

mod assets;
mod capabilities;
mod identity;
mod lower;
mod plan;
mod policy;
pub(crate) mod tools;

pub(crate) use assets::verify_prepared_asset;
use assets::{
    entrypoint_directory, manifest_path, prepare_output_path, reject_asset_collisions,
    reject_path_collision, validate_destination,
};
use capabilities::ffmpeg_requirements;
use identity::{cache_execution_namespace, prepared_semantic_hash};
use lower::PreflightLowerer;
pub use plan::{
    PreparedAsset, PreparedAudioKind, PreparedExternalArgument, PreparedExternalParameterValue,
    PreparedNode, PreparedNodeMedia, PreparedPlan, PreparedVideoKind,
};
pub(crate) use policy::RenderPolicy;
pub use tools::ExternalToolIdentity;
use tools::{inspect_ffmpeg, inspect_ffprobe, validate_ffmpeg_capabilities};

const PREPARED_FORMAT_VERSION: u32 = 9;
/// Resolve and verify assets/tools, lower result-reachable primitives, and build
/// an invariant-protected renderer plan.
///
/// # Errors
///
/// Returns a diagnostic for invalid output configuration, unavailable tool
/// capabilities, inaccessible/undecodable assets, or preparation failures.
pub fn preflight(compiled: &CompiledProgram) -> Result<PreparedPlan> {
    let render_output = compiled.render_output()?;
    entrypoint_directory(compiled.entrypoint_source())?;
    let render_policy = RenderPolicy::CURRENT;
    let output = prepare_output_path(compiled, render_policy)?;
    let manifest = manifest_path(&output);
    validate_destination(&output, "output", "E_INVALID_OUTPUT_DESTINATION")?;
    validate_destination(&manifest, "manifest", "E_INVALID_MANIFEST_DESTINATION")?;
    if let Some(source_path) = compiled.entrypoint_source().filesystem_path() {
        reject_path_collision(
            &output,
            "output",
            source_path,
            "source program",
            "E_OUTPUT_COLLISION",
        )?;
        reject_path_collision(
            &manifest,
            "manifest",
            source_path,
            "source program",
            "E_MANIFEST_COLLISION",
        )?;
    }
    let video = *compiled.video();
    let audio = *compiled.audio();
    render_policy.validate_video_spec(
        &video,
        &compiled.output().map_or_else(
            || SourceSpan::source_start(compiled.entrypoint_source().clone()),
            |output| output.span.clone(),
        ),
    )?;

    let ffmpeg = inspect_ffmpeg()?;
    let ffprobe = inspect_ffprobe()?;
    let execution_namespace = cache_execution_namespace(render_policy, &ffmpeg, &ffprobe)?;
    let mut lowerer = PreflightLowerer {
        compiled,
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        nodes: Vec::new(),
        lowered: HashMap::new(),
    };
    let order = crate::compiler::traversal::topological_order(
        compiled.nodes(),
        compiled.symbol_values(),
        [render_output],
    )?;
    for value in order {
        lowerer.lower(value)?;
    }
    let result = lowerer.lowered[&render_output.id()];
    let requirements = ffmpeg_requirements(render_policy, &lowerer.nodes, result);
    validate_ffmpeg_capabilities(&ffmpeg, &requirements)?;
    let named_values = compiled
        .named_values()
        .iter()
        .filter_map(|(name, value)| {
            lowerer
                .lowered
                .get(&value.id())
                .copied()
                .map(|node| (name.clone(), node))
        })
        .collect::<BTreeMap<_, _>>();
    reject_asset_collisions(&output, &manifest, &lowerer.nodes)?;
    let semantic_hash =
        prepared_semantic_hash(&video, audio, result, &named_values, &lowerer.nodes)?;

    Ok(PreparedPlan::new(
        PREPARED_FORMAT_VERSION,
        env!("CARGO_PKG_VERSION").to_owned(),
        semantic_hash,
        render_policy,
        video,
        audio,
        lowerer.nodes,
        result,
        named_values,
        output,
        manifest,
        ffmpeg,
        ffprobe,
        execution_namespace,
        compiled.entrypoint_source().clone(),
    ))
}
