#[cfg(feature = "native")]
mod native;
mod probe;

#[cfg(feature = "native")]
pub(super) use native::{
    FfmpegRequirements, inspect_ffmpeg, inspect_ffprobe, validate_ffmpeg_capabilities,
};
#[cfg(feature = "native")]
pub(crate) use native::{
    ToolIdentity, inspect_external_tool, verify_external_tool, verify_tool_identity,
};
#[cfg(feature = "native")]
pub(crate) use probe::decoded_audio_samples;
pub(super) use probe::{validate_image_probe_json, validate_video_probe_json};
#[cfg(feature = "native")]
pub(super) use probe::{verify_audio_decodable, verify_image_decodable, verify_video_decodable};

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
/// One resolved external executable and the content hash verified by preflight.
pub struct ExternalToolIdentity {
    executable: PathBuf,
    content_hash: String,
}

impl ExternalToolIdentity {
    /// Return the resolved executable path used during preflight.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Return the executable content hash recorded during preflight.
    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}
