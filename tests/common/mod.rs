#![allow(dead_code)]

use std::process::{Command, Stdio};

pub fn executable_available(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn media_tools_available() -> bool {
    executable_available("ffmpeg") && executable_available("ffprobe")
}
