use serde::Serialize;

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, VideoDomain, VideoSpec};
use crate::preflight::{PreparedNode, PreparedNodeMedia, PreparedPlan};
use crate::source::SourceSpan;

use super::CacheMode;

#[derive(Serialize)]
struct ManifestDocument<'a> {
    format_version: u32,
    engine_version: &'static str,
    semantic_hash: &'a str,
    project: ProjectDocument<'a>,
    result: ResultDocument<'a>,
    tools: ToolDocument<'a>,
    cache: CacheDocument,
}

#[derive(Serialize)]
struct ProjectDocument<'a> {
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
}

#[derive(Serialize)]
struct ResultDocument<'a> {
    fingerprint: &'a str,
    domain: &'a VideoDomain,
    has_audio: bool,
}

#[derive(Serialize)]
struct ToolDocument<'a> {
    ffmpeg: &'a str,
    ffprobe: &'a str,
}

#[derive(Serialize)]
struct CacheDocument {
    mode: &'static str,
    hits: usize,
    misses: usize,
}

pub(super) fn serialize(
    plan: &PreparedPlan,
    result: &PreparedNode,
    cache_mode: CacheMode,
    cache_hits: usize,
    cache_misses: usize,
) -> Result<Vec<u8>> {
    let PreparedNodeMedia::Video {
        domain, has_audio, ..
    } = result.media()
    else {
        return Err(Diagnostic::builtin(
            BuiltinDiagnostic::InvalidPlan,
            "prepared result is Audio, but a render manifest requires Video",
            result.origin().span.clone(),
        ));
    };
    let document = ManifestDocument {
        format_version: crate::contracts::RENDER_MANIFEST_FORMAT_VERSION,
        engine_version: env!("CARGO_PKG_VERSION"),
        semantic_hash: plan.semantic_hash(),
        project: ProjectDocument {
            video: plan.video(),
            audio: plan.audio(),
        },
        result: ResultDocument {
            fingerprint: result.fingerprint(),
            domain,
            has_audio,
        },
        tools: ToolDocument {
            ffmpeg: plan.ffmpeg().version(),
            ffprobe: plan.ffprobe().version(),
        },
        cache: CacheDocument {
            mode: cache_mode.label(),
            hits: cache_hits,
            misses: cache_misses,
        },
    };
    serde_json::to_vec_pretty(&document).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::Manifest,
            format!("could not serialize render manifest: {error}"),
            SourceSpan::source_start(plan.entrypoint_source().clone()),
        )
    })
}
