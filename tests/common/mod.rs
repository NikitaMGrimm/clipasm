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
