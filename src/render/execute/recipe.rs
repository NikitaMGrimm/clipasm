use std::path::{Path, PathBuf};
#[cfg(feature = "native")]
use std::process::Command;

#[cfg(feature = "native")]
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioSpec, NodeId, VideoSpec};
use crate::preflight::{PreparedNode, RenderPolicy};
use crate::source::SourceSpan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FfmpegRecipe {
    arguments: Vec<FfmpegArgument>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FfmpegArgument {
    Text(String),
    Asset(PathBuf),
    Artifact(NodeId),
}

pub(crate) struct RecipeContext<'a> {
    video: &'a VideoSpec,
    audio: &'a AudioSpec,
    nodes: &'a [PreparedNode],
    policy: RenderPolicy,
    span: &'a SourceSpan,
}

impl<'a> RecipeContext<'a> {
    pub(crate) const fn new(
        video: &'a VideoSpec,
        audio: &'a AudioSpec,
        nodes: &'a [PreparedNode],
        policy: RenderPolicy,
        span: &'a SourceSpan,
    ) -> Self {
        Self {
            video,
            audio,
            nodes,
            policy,
            span,
        }
    }

    pub(super) const fn video(&self) -> &VideoSpec {
        self.video
    }

    pub(super) const fn audio(&self) -> AudioSpec {
        *self.audio
    }

    pub(super) const fn nodes(&self) -> &[PreparedNode] {
        self.nodes
    }

    pub(super) const fn policy(&self) -> RenderPolicy {
        self.policy
    }

    pub(super) const fn span(&self) -> &SourceSpan {
        self.span
    }

    pub(super) fn append_video_output(&self, recipe: &mut FfmpegRecipe) {
        recipe
            .args(["-c:v", self.policy.native_video_encoder()])
            .args([
                "-level",
                &self.policy.native_video_level().to_string(),
                "-pix_fmt",
                self.policy.working_pixel_format(),
                "-r",
            ])
            .arg(format!(
                "{}/{}",
                self.video.fps().numerator(),
                self.video.fps().denominator()
            ))
            .args([
                "-c:a",
                self.policy.native_audio_encoder(),
                "-ar",
                &self.audio.sample_rate().to_string(),
                "-ac",
                &self.audio.channels().to_string(),
                "-f",
                self.policy.native_container(),
            ]);
    }

    pub(super) fn append_audio_output(&self, recipe: &mut FfmpegRecipe) {
        recipe.args([
            "-c:a",
            self.policy.native_audio_encoder(),
            "-ar",
            &self.audio.sample_rate().to_string(),
            "-ac",
            &self.audio.channels().to_string(),
            "-f",
            self.policy.native_container(),
        ]);
    }
}

impl FfmpegRecipe {
    pub(super) fn new() -> Self {
        let mut recipe = Self {
            arguments: Vec::new(),
        };
        recipe.args(["-y", "-v", "error"]);
        recipe
    }

    pub(super) fn arg(&mut self, argument: impl Into<String>) -> &mut Self {
        self.arguments.push(FfmpegArgument::Text(argument.into()));
        self
    }

    pub(super) fn args<const N: usize>(&mut self, arguments: [&str; N]) -> &mut Self {
        self.arguments.extend(
            arguments
                .into_iter()
                .map(|argument| FfmpegArgument::Text(argument.to_owned())),
        );
        self
    }

    pub(super) fn asset(&mut self, path: &Path) -> &mut Self {
        self.arguments
            .push(FfmpegArgument::Asset(path.to_path_buf()));
        self
    }

    pub(super) fn artifact(&mut self, node: NodeId) -> &mut Self {
        self.arguments.push(FfmpegArgument::Artifact(node));
        self
    }

    pub(crate) fn arguments(&self) -> &[FfmpegArgument] {
        &self.arguments
    }

    #[cfg(feature = "native")]
    pub(super) fn materialize<'a>(
        &self,
        ffmpeg: &Path,
        output: &Path,
        span: &SourceSpan,
        mut artifact: impl FnMut(NodeId) -> Option<&'a Path>,
    ) -> Result<Command> {
        let mut command = Command::new(ffmpeg);
        for argument in &self.arguments {
            match argument {
                FfmpegArgument::Text(text) => {
                    command.arg(text);
                }
                FfmpegArgument::Asset(path) => {
                    command.arg(path);
                }
                FfmpegArgument::Artifact(node) => {
                    let path = artifact(*node).ok_or_else(|| {
                        Diagnostic::builtin(
                            BuiltinDiagnostic::InvalidPlan,
                            format!("primitive input {} is not available", node.get()),
                            span.clone(),
                        )
                    })?;
                    command.arg(path);
                }
            }
        }
        command.arg(output);
        Ok(command)
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn materializes_arguments_in_recipe_order_and_appends_output() {
        let artifact_path = Path::new("cache/input.mkv");
        let mut recipe = FfmpegRecipe::new();
        recipe
            .args(["-i"])
            .asset(Path::new("assets/card.png"))
            .args(["-i"])
            .artifact(NodeId::new(7))
            .args(["-filter_complex", "[0:v][1:v]concat[v]", "-map", "[v]"]);

        let command = recipe
            .materialize(
                Path::new("ffmpeg"),
                Path::new("cache/output.mkv"),
                &SourceSpan::file_start("program.clipasm"),
                |node| (node == NodeId::new(7)).then_some(artifact_path),
            )
            .expect("materialized command");
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(
            arguments,
            [
                OsStr::new("-y"),
                OsStr::new("-v"),
                OsStr::new("error"),
                OsStr::new("-i"),
                OsStr::new("assets/card.png"),
                OsStr::new("-i"),
                OsStr::new("cache/input.mkv"),
                OsStr::new("-filter_complex"),
                OsStr::new("[0:v][1:v]concat[v]"),
                OsStr::new("-map"),
                OsStr::new("[v]"),
                OsStr::new("cache/output.mkv"),
            ]
        );
    }

    #[test]
    fn rejects_a_missing_artifact_at_materialization() {
        let mut recipe = FfmpegRecipe::new();
        recipe.args(["-i"]).artifact(NodeId::new(12));

        let error = recipe
            .materialize(
                Path::new("ffmpeg"),
                Path::new("output.mkv"),
                &SourceSpan::file_start("program.clipasm"),
                |_| None,
            )
            .expect_err("missing artifact");

        assert_eq!(error.code, "E_INVALID_PLAN");
        assert!(error.message.contains("primitive input 12"));
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_asset_artifact_and_output_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let asset = PathBuf::from(OsString::from_vec(b"asset-\xFF.png".to_vec()));
        let artifact = PathBuf::from(OsString::from_vec(b"artifact-\xFE.mkv".to_vec()));
        let output = PathBuf::from(OsString::from_vec(b"output-\xFD.mkv".to_vec()));
        let mut recipe = FfmpegRecipe::new();
        recipe
            .args(["-i"])
            .asset(&asset)
            .args(["-i"])
            .artifact(NodeId::new(1));

        let command = recipe
            .materialize(
                Path::new("ffmpeg"),
                &output,
                &SourceSpan::file_start("program.clipasm"),
                |node| (node == NodeId::new(1)).then_some(artifact.as_path()),
            )
            .expect("non-UTF-8 paths");
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(arguments[4].as_bytes(), b"asset-\xFF.png");
        assert_eq!(arguments[6].as_bytes(), b"artifact-\xFE.mkv");
        assert_eq!(
            arguments.last().expect("output").as_bytes(),
            b"output-\xFD.mkv"
        );
    }
}
