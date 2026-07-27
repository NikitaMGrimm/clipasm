//! Browser execution documents for prepared virtual media projects.
//!
//! This adapter materializes renderer-owned `FFmpeg` recipes as virtual paths.
//! It performs no filesystem, process, or media I/O; the browser host owns the
//! worker, virtual filesystem, artifact verification, and result lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, NodeId, TimelineRate, ValueType};
use crate::preflight::browser::BrowserPreparedPlan;
use crate::preflight::{
    PreparedAudioKind, PreparedNode, PreparedNodeMedia, PreparedVideoKind, RenderPolicy,
};
use crate::source::SourceSpan;

use super::execute::{FfmpegArgument, FfmpegRecipe, RecipeContext, export_recipe, ffmpeg_recipe};

const FFMPEG_WRAPPER_VERSION: &str = "0.12.15";
const FFMPEG_CORE_VERSION: &str = "0.12.10";
const BROWSER_RUNTIME_POLICY: &str = "ffv1-flac-matroska-v1";

/// Serialize a prepared browser graph as closed `FFmpeg` recipes and virtual
/// artifact contracts for `ClipAsm`'s matching bundled browser host.
///
/// The document contains no executable program names or shell commands. A
/// browser host mounts each requested asset at the supplied virtual path,
/// executes the argument arrays in order, verifies their contracts, and reads
/// the final MP4.
///
/// # Errors
///
/// Returns a diagnostic if a virtual asset path cannot be represented, recipe
/// generation detects an invalid prepared graph, the browser work budget is
/// exceeded, or the document cannot be serialized.
pub fn render_json(plan: &BrowserPreparedPlan) -> Result<String> {
    let document = render_document(plan)?;
    serde_json::to_string(&document).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::BrowserRenderJson,
            format!("could not serialize the browser render plan: {error}"),
            result_span(plan),
        )
    })
}

fn render_document(plan: &BrowserPreparedPlan) -> Result<BrowserRenderDocument<'_>> {
    validate_browser_budget(plan)?;
    let policy = RenderPolicy::CURRENT;
    let assets = browser_mounts(plan.nodes())?;
    let asset_paths = assets
        .iter()
        .map(|asset| (PathBuf::from(&asset.path), asset.virtual_path.clone()))
        .collect::<BTreeMap<_, _>>();

    Ok(BrowserRenderDocument {
        version: crate::contracts::BROWSER_RENDER_PLAN_VERSION,
        recipe_contract: crate::contracts::BROWSER_RECIPE_CONTRACT_VERSION,
        runtime: BrowserRuntime {
            wrapper: FFMPEG_WRAPPER_VERSION,
            core: FFMPEG_CORE_VERSION,
            policy: BROWSER_RUNTIME_POLICY,
        },
        semantic_hash: plan.semantic_hash(),
        steps: render_steps(plan, &asset_paths, policy)?,
        export: browser_export(plan, &asset_paths, policy)?,
        assets,
    })
}

fn render_steps(
    plan: &BrowserPreparedPlan,
    asset_paths: &BTreeMap<PathBuf, String>,
    policy: RenderPolicy,
) -> Result<Vec<BrowserRenderStep>> {
    let nodes = plan.nodes();
    let last_uses = last_uses(nodes, plan.result());
    let mut steps = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        let context = RecipeContext::new(
            plan.video(),
            plan.audio(),
            nodes,
            policy,
            &node.origin().span,
        );
        let recipe = ffmpeg_recipe(node, &context)?;
        let output = artifact_path(node.id(), node.value_type(), policy);
        let delete_after = last_uses
            .iter()
            .filter(|(_, last_use)| **last_use == index)
            .map(|(node, _)| {
                let node = &nodes[node.get() as usize];
                artifact_path(node.id(), node.value_type(), policy)
            })
            .collect();
        steps.push(BrowserRenderStep {
            node: node.id().get(),
            arguments: browser_arguments(
                &recipe,
                asset_paths,
                nodes,
                policy,
                &output,
                &node.origin().span,
            )?,
            output,
            contract: artifact_contract(node, *plan.audio(), policy)?,
            delete_after,
        });
    }
    Ok(steps)
}

fn browser_export(
    plan: &BrowserPreparedPlan,
    asset_paths: &BTreeMap<PathBuf, String>,
    policy: RenderPolicy,
) -> Result<BrowserExport> {
    let result = result_node(plan)?;
    let final_output = "/output/clipasm.mp4".to_owned();
    let recipe = export_recipe(
        plan.result(),
        plan.video(),
        *plan.audio(),
        result.has_audio(),
        policy,
    );
    Ok(BrowserExport {
        arguments: browser_arguments(
            &recipe,
            asset_paths,
            plan.nodes(),
            policy,
            &final_output,
            &result.origin().span,
        )?,
        output: final_output,
        contract: final_contract(plan, result, policy)?,
        delete_after: vec![artifact_path(plan.result(), ValueType::Video, policy)],
    })
}

fn final_contract(
    plan: &BrowserPreparedPlan,
    result: &PreparedNode,
    policy: RenderPolicy,
) -> Result<BrowserArtifactContract> {
    let domain = result.video_domain().ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "browser render result is not Video",
            result.origin().span.clone(),
        )
    })?;
    Ok(BrowserArtifactContract::Video {
        width: domain.width(),
        height: domain.height(),
        fps_numerator: domain.frame_rate().numerator(),
        fps_denominator: domain.frame_rate().denominator(),
        frames: domain.frames().0,
        pixel_format: policy.export_pixel_format(),
        audio: result.has_audio(),
        sample_rate: plan.audio().sample_rate(),
        channels: plan.audio().channels(),
        exact_audio_samples: false,
        samples: result
            .has_audio()
            .then(|| {
                TimelineRate::new(*plan.video(), *plan.audio())
                    .samples_for_frames(domain.frames(), &result.origin().span)
            })
            .transpose()?,
    })
}

fn result_node(plan: &BrowserPreparedPlan) -> Result<&PreparedNode> {
    plan.nodes()
        .get(plan.result().get() as usize)
        .ok_or_else(|| {
            Diagnostic::builtin(
                BuiltinDiagnostic::InvalidPlan,
                format!(
                    "browser render result {} is not available",
                    plan.result().get()
                ),
                SourceSpan::file_start("browser"),
            )
        })
}

fn result_span(plan: &BrowserPreparedPlan) -> SourceSpan {
    result_node(plan).map_or_else(
        |_| SourceSpan::file_start("browser"),
        |node| node.origin().span.clone(),
    )
}

#[derive(Serialize)]
struct BrowserRenderDocument<'a> {
    version: u32,
    recipe_contract: u32,
    runtime: BrowserRuntime<'static>,
    semantic_hash: &'a str,
    assets: Vec<BrowserMount>,
    steps: Vec<BrowserRenderStep>,
    export: BrowserExport,
}

#[derive(Serialize)]
struct BrowserRuntime<'a> {
    wrapper: &'a str,
    core: &'a str,
    policy: &'a str,
}

#[derive(Serialize)]
struct BrowserMount {
    path: String,
    virtual_path: String,
}

#[derive(Serialize)]
struct BrowserRenderStep {
    node: u32,
    arguments: Vec<String>,
    output: String,
    contract: BrowserArtifactContract,
    delete_after: Vec<String>,
}

#[derive(Serialize)]
struct BrowserExport {
    arguments: Vec<String>,
    output: String,
    contract: BrowserArtifactContract,
    delete_after: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "media", rename_all = "snake_case")]
enum BrowserArtifactContract {
    Video {
        width: u32,
        height: u32,
        fps_numerator: u32,
        fps_denominator: u32,
        frames: u64,
        pixel_format: &'static str,
        audio: bool,
        sample_rate: u32,
        channels: u8,
        exact_audio_samples: bool,
        samples: Option<u64>,
    },
    Audio {
        sample_rate: u32,
        channels: u8,
        samples: u64,
    },
}

fn browser_mounts(nodes: &[PreparedNode]) -> Result<Vec<BrowserMount>> {
    let mut paths = BTreeSet::new();
    for node in nodes {
        match node.media() {
            PreparedNodeMedia::Video {
                kind:
                    PreparedVideoKind::ImageVideo { asset, .. }
                    | PreparedVideoKind::VideoSource { asset, .. },
                ..
            }
            | PreparedNodeMedia::Audio {
                kind: PreparedAudioKind::AudioSource { asset },
                ..
            } => {
                let path = asset.source_path().to_str().ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::BrowserAssetPath,
                        "browser asset paths must be UTF-8",
                        node.origin().span.clone(),
                    )
                })?;
                paths.insert(path.to_owned());
            }
            PreparedNodeMedia::Video { .. } | PreparedNodeMedia::Audio { .. } => {}
        }
    }
    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let extension = Path::new(&path)
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 12
                        && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
                .map_or_else(String::new, |value| format!(".{value}"));
            Ok(BrowserMount {
                path,
                virtual_path: format!("/inputs/{index}/asset{extension}"),
            })
        })
        .collect()
}

fn browser_arguments(
    recipe: &FfmpegRecipe,
    assets: &BTreeMap<PathBuf, String>,
    nodes: &[PreparedNode],
    policy: RenderPolicy,
    output: &str,
    span: &SourceSpan,
) -> Result<Vec<String>> {
    let mut arguments = Vec::with_capacity(recipe.arguments().len() + 1);
    for argument in recipe.arguments() {
        match argument {
            FfmpegArgument::Text(value) => arguments.push(value.clone()),
            FfmpegArgument::Asset(path) => {
                let Some(path) = assets.get(path) else {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InvalidPlan,
                        format!(
                            "browser recipe references unavailable asset `{}`",
                            path.display()
                        ),
                        span.clone(),
                    ));
                };
                arguments.push(path.clone());
            }
            FfmpegArgument::Artifact(id) => {
                let Some(node) = nodes.get(id.get() as usize) else {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::InvalidPlan,
                        format!("browser recipe references unavailable node {}", id.get()),
                        span.clone(),
                    ));
                };
                arguments.push(artifact_path(*id, node.value_type(), policy));
            }
        }
    }
    arguments.push(output.to_owned());
    Ok(arguments)
}

fn artifact_path(node: NodeId, value_type: ValueType, policy: RenderPolicy) -> String {
    let extension = match value_type {
        ValueType::Video => policy.working_video_extension(),
        ValueType::Audio => policy.working_audio_extension(),
    };
    format!("/work/node-{}.{extension}", node.get())
}

fn artifact_contract(
    node: &PreparedNode,
    audio: AudioSpec,
    policy: RenderPolicy,
) -> Result<BrowserArtifactContract> {
    match node.media() {
        PreparedNodeMedia::Video { domain, .. } => {
            let samples = TimelineRate::new(domain.video_spec(), audio)
                .samples_for_frames(domain.frames(), &node.origin().span)?;
            Ok(BrowserArtifactContract::Video {
                width: domain.width(),
                height: domain.height(),
                fps_numerator: domain.frame_rate().numerator(),
                fps_denominator: domain.frame_rate().denominator(),
                frames: domain.frames().0,
                pixel_format: policy.working_pixel_format(),
                audio: true,
                sample_rate: audio.sample_rate(),
                channels: audio.channels(),
                exact_audio_samples: true,
                samples: Some(samples),
            })
        }
        PreparedNodeMedia::Audio { domain, .. } => Ok(BrowserArtifactContract::Audio {
            sample_rate: domain.audio_spec().sample_rate(),
            channels: domain.audio_spec().channels(),
            samples: domain.samples(),
        }),
    }
}

fn last_uses(nodes: &[PreparedNode], result: NodeId) -> BTreeMap<NodeId, usize> {
    let mut uses = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        node.visit_inputs(|input| {
            uses.insert(input, index);
        });
    }
    uses.insert(result, nodes.len());
    uses
}

fn validate_browser_budget(plan: &BrowserPreparedPlan) -> Result<()> {
    const MAX_PIXEL_FRAMES: u128 = 1_000_000_000;
    const MAX_NODES: usize = 512;

    if plan.nodes().len() > MAX_NODES {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::BrowserRenderLimit,
            format!(
                "browser rendering supports at most {MAX_NODES} prepared operations; this graph has {}",
                plan.nodes().len()
            ),
            result_span(plan),
        ));
    }
    let mut pixel_frames = 0_u128;
    for node in plan.nodes() {
        if let Some(domain) = node.video_domain() {
            pixel_frames = pixel_frames
                .checked_add(
                    u128::from(domain.width())
                        * u128::from(domain.height())
                        * u128::from(domain.frames().0),
                )
                .ok_or_else(|| {
                    Diagnostic::builtin(
                        BuiltinDiagnostic::BrowserRenderLimit,
                        "browser render work exceeds the supported size",
                        node.origin().span.clone(),
                    )
                })?;
        }
    }
    if pixel_frames > MAX_PIXEL_FRAMES {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::BrowserRenderLimit,
            format!(
                "browser render work is {pixel_frames} pixel-frames, above the {MAX_PIXEL_FRAMES} pixel-frame limit"
            ),
            result_span(plan),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::compiler::CompiledProgram;
    use crate::preflight::browser::{BrowserAsset, prepare};

    fn compiled(source: &str) -> CompiledProgram {
        let package =
            crate::language::parse_str(Path::new("playground.clipasm"), source).expect("source");
        crate::compiler::compile(&package).expect("compiled")
    }

    #[test]
    fn scenic_plan_serializes_runtime_artifacts_and_cleanup_contracts() {
        let compiled = compiled(include_str!("../../examples/scenic-sequence.clipasm"));
        let assets = [
            BrowserAsset::new("assets/morning.png", "11".repeat(32)),
            BrowserAsset::new("assets/meadow.png", "22".repeat(32)),
            BrowserAsset::new("assets/evening.png", "33".repeat(32)),
        ];
        let plan = prepare(&compiled, &assets).expect("browser plan");
        let document: serde_json::Value =
            serde_json::from_str(&render_json(&plan).expect("render JSON")).expect("valid JSON");

        assert_eq!(
            document["version"],
            crate::contracts::BROWSER_RENDER_PLAN_VERSION
        );
        assert_eq!(
            document["recipe_contract"],
            crate::contracts::BROWSER_RECIPE_CONTRACT_VERSION
        );
        assert_eq!(
            document["runtime"],
            serde_json::json!({
                "wrapper": FFMPEG_WRAPPER_VERSION,
                "core": FFMPEG_CORE_VERSION,
                "policy": BROWSER_RUNTIME_POLICY,
            })
        );
        assert_eq!(
            document["assets"],
            serde_json::json!([
                {
                    "path": "assets/evening.png",
                    "virtual_path": "/inputs/0/asset.png",
                },
                {
                    "path": "assets/meadow.png",
                    "virtual_path": "/inputs/1/asset.png",
                },
                {
                    "path": "assets/morning.png",
                    "virtual_path": "/inputs/2/asset.png",
                },
            ])
        );

        let steps = document["steps"].as_array().expect("steps");
        assert_eq!(steps.len(), 4);
        for (index, step) in steps.iter().enumerate() {
            assert_eq!(step["node"], u64::try_from(index).expect("small index"));
            assert_eq!(
                step["arguments"]
                    .as_array()
                    .and_then(|arguments| arguments.last()),
                Some(&step["output"])
            );
            assert_eq!(step["contract"]["media"], "video");
            assert_eq!(step["contract"]["sample_rate"], 48_000);
            assert_eq!(step["contract"]["channels"], 2);
            assert_eq!(step["contract"]["exact_audio_samples"], true);
        }
        assert_eq!(steps[0]["contract"]["frames"], 36);
        assert_eq!(steps[0]["contract"]["samples"], 72_000);
        assert_eq!(steps[3]["contract"]["frames"], 108);
        assert_eq!(steps[3]["contract"]["samples"], 216_000);
        assert_eq!(
            steps[3]["delete_after"],
            serde_json::json!(["/work/node-0.mkv", "/work/node-1.mkv", "/work/node-2.mkv",])
        );

        let export = &document["export"];
        assert_eq!(
            export["arguments"],
            serde_json::json!([
                "-y",
                "-v",
                "error",
                "-i",
                "/work/node-3.mkv",
                "-map",
                "0:v:0",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-r",
                "24/1",
                "-an",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
                "/output/clipasm.mp4",
            ])
        );
        assert_eq!(export["contract"]["frames"], 108);
        assert_eq!(export["contract"]["audio"], false);
        assert_eq!(export["contract"]["sample_rate"], 48_000);
        assert_eq!(export["contract"]["channels"], 2);
        assert_eq!(
            export["delete_after"],
            serde_json::json!(["/work/node-3.mkv"])
        );
    }
}
