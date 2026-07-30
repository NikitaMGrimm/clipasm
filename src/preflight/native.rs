use std::collections::{BTreeMap, HashMap};

use crate::compiler::CompiledProgram;
use crate::diagnostic::{BuiltinDiagnostic, Result};
use crate::source::SourceSpan;

use super::assets::{
    entrypoint_directory, manifest_path, prepare_output_path, reject_asset_collisions,
    reject_path_collision, validate_destination,
};
use super::capabilities::ffmpeg_requirements;
use super::identity::{cache_execution_namespace, prepared_semantic_hash};
use super::lower::{NativePreparationHost, PreflightLowerer};
use super::tools::{inspect_ffmpeg, inspect_ffprobe, validate_ffmpeg_capabilities};
use super::{PreparedPlan, RenderPolicy};

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
    validate_destination(
        &output,
        "output",
        BuiltinDiagnostic::InvalidOutputDestination,
    )?;
    validate_destination(
        &manifest,
        "manifest",
        BuiltinDiagnostic::InvalidManifestDestination,
    )?;
    for source_path in compiled.source_paths() {
        reject_path_collision(
            &output,
            "output",
            source_path,
            "source program",
            BuiltinDiagnostic::OutputCollision,
        )?;
        reject_path_collision(
            &manifest,
            "manifest",
            source_path,
            "source program",
            BuiltinDiagnostic::ManifestCollision,
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
    let mut host = NativePreparationHost::new(&ffmpeg, &ffprobe);
    let mut lowerer = PreflightLowerer {
        compiled,
        host: &mut host,
        nodes: Vec::new(),
        lowered: HashMap::new(),
    };
    let order = crate::semantic::topological_order(
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
        crate::contracts::PREPARED_INSPECTION_FORMAT_VERSION,
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
        compiled.source_paths().to_vec(),
    ))
}
