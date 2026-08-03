#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn executable_available(name: &str, version_argument: &str) -> bool {
    Command::new(name)
        .arg(version_argument)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn media_tools_available() -> bool {
    executable_available("ffmpeg", "-version") && executable_available("ffprobe", "-version")
}

pub fn configure_bt709_video_output(
    command: &mut Command,
    pixel_format: &str,
    chroma_location: Option<&str>,
) {
    // `setparams` tags frames while the output options tag the encoded stream.
    // FFmpeg builds differ in whether either set propagates to the other.
    command.args([
        "-vf",
        "setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
        "-pix_fmt",
        pixel_format,
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-color_range",
        "tv",
    ]);
    if let Some(location) = chroma_location {
        command.args(["-chroma_sample_location", location]);
    }
}

pub fn cache_artifact(directory: &Path, fingerprint: &str, extension: &str) -> PathBuf {
    let cache = directory.join(".clipasm").join("cache");
    let namespaces = std::fs::read_dir(&cache)
        .expect("cache directory")
        .filter_map(|entry| {
            let entry = entry.expect("cache entry");
            entry
                .file_type()
                .expect("cache entry type")
                .is_dir()
                .then(|| entry.path())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        namespaces.len(),
        1,
        "unexpected cache namespaces: {namespaces:?}"
    );
    namespaces[0].join(format!("{fingerprint}.{extension}"))
}

pub fn cache_metadata(artifact: &Path) -> PathBuf {
    let mut name = artifact
        .file_name()
        .expect("cache artifact file name")
        .to_os_string();
    name.push(".cache.json");
    artifact.parent().expect("cache artifact parent").join(name)
}
