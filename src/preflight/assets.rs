use std::fs;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::compiler::CompiledProgram;
use crate::diagnostic::{Diagnostic, Result};
use crate::model::{FrameCount, VideoSpec};
use crate::semantic::SourceOrigin;
use crate::source::{SourceFile, SourceSpan};

use super::tools::{ToolIdentity, verify_image_decodable, verify_video_decodable};
use super::{PreparedAsset, PreparedNode, PreparedNodeKind};

pub(super) fn prepare_output_path(compiled: &CompiledProgram) -> Result<PathBuf> {
    let output = compiled.output().ok_or_else(|| {
        Diagnostic::new(
            "E_MISSING_OUTPUT",
            "`render` requires `program.output`",
            SourceSpan::source_start(compiled.entrypoint_source().clone()),
        )
    })?;
    if output
        .value
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("mp4"))
    {
        return Err(Diagnostic::new(
            "E_INVALID_OUTPUT_EXTENSION",
            "the foundation export profile requires an `.mp4` output path",
            output.span.clone(),
        ));
    }
    resolve_authored_path(&output.value, &output.span)
}

pub(super) fn validate_destination(path: &Path, role: &str, code: &'static str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Diagnostic::new(
            code,
            format!(
                "{role} destination `{}` is a symlink; publication destinations must be regular files",
                path.display()
            ),
            SourceSpan::file_start(path),
        )),
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_destination(path, role, code)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Diagnostic::new(
            code,
            format!(
                "{role} destination `{}` cannot be inspected: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )),
    }
}

fn invalid_destination(path: &Path, role: &str, code: &'static str) -> Diagnostic {
    Diagnostic::new(
        code,
        format!(
            "{role} destination `{}` is not a regular file",
            path.display()
        ),
        SourceSpan::file_start(path),
    )
}

pub(super) fn reject_path_collision(
    left: &Path,
    left_role: &str,
    right: &Path,
    right_role: &str,
    code: &'static str,
) -> Result<()> {
    if canonical_path_identity(left)? != canonical_path_identity(right)? {
        return Ok(());
    }
    Err(Diagnostic::new(
        code,
        format!(
            "{left_role} path `{}` collides with {right_role} path `{}`",
            left.display(),
            right.display()
        ),
        SourceSpan::file_start(left),
    ))
}

pub(super) fn reject_asset_collisions(
    output: &Path,
    manifest: &Path,
    nodes: &[PreparedNode],
) -> Result<()> {
    for node in nodes {
        let (asset, role) = match node.kind() {
            PreparedNodeKind::ImageVideo { asset, .. } => (asset, "image asset"),
            PreparedNodeKind::VideoSource { asset, .. } => (asset, "video asset"),
            PreparedNodeKind::Slice { .. }
            | PreparedNodeKind::Repeat { .. }
            | PreparedNodeKind::Zoom { .. }
            | PreparedNodeKind::Wobble { .. }
            | PreparedNodeKind::FlashJoin { .. }
            | PreparedNodeKind::Concat { .. } => continue,
        };
        reject_path_collision(
            output,
            "output",
            asset.source_path(),
            role,
            "E_OUTPUT_COLLISION",
        )?;
        reject_path_collision(
            manifest,
            "manifest",
            asset.source_path(),
            role,
            "E_MANIFEST_COLLISION",
        )?;
    }
    Ok(())
}

fn canonical_path_identity(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                Diagnostic::new(
                    "E_PATH_RESOLUTION",
                    format!("could not resolve `{}`: {error}", path.display()),
                    SourceSpan::file_start(path),
                )
            })?
            .join(path)
    };
    let normalized = normalize_path(&absolute);
    let mut existing = normalized.as_path();
    let mut suffix = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        suffix.push(name.to_os_string());
        existing = existing.parent().unwrap_or(existing);
    }
    let mut identity = fs::canonicalize(existing).map_err(|error| {
        Diagnostic::new(
            "E_PATH_RESOLUTION",
            format!(
                "could not canonicalize path identity for `{}`: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )
    })?;
    for component in suffix.into_iter().rev() {
        identity.push(component);
    }
    Ok(normalize_path(&identity))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

pub(super) fn prepare_image_asset(
    authored: &Path,
    origin: &SourceOrigin,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<PreparedAsset> {
    let asset = prepare_file_asset(authored, origin, "image", "E_MISSING_IMAGE_FILE")?;
    verify_image_decodable(asset.source_path(), &origin.span, ffmpeg, ffprobe)?;
    Ok(asset)
}

pub(super) fn prepare_video_asset(
    authored: &Path,
    video: &VideoSpec,
    origin: &SourceOrigin,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<(PreparedAsset, FrameCount)> {
    let asset = prepare_file_asset(authored, origin, "video", "E_MISSING_VIDEO_FILE")?;
    let frames = verify_video_decodable(asset.source_path(), video, &origin.span, ffmpeg, ffprobe)?;
    Ok((asset, frames))
}

fn prepare_file_asset(
    authored: &Path,
    origin: &SourceOrigin,
    role: &str,
    missing_code: &'static str,
) -> Result<PreparedAsset> {
    let source_path = resolve_authored_path(authored, &origin.span)?;
    let metadata = fs::metadata(&source_path).map_err(|error| {
        Diagnostic::new(
            missing_code,
            format!(
                "{role} file `{}` is not accessible: {error}",
                source_path.display()
            ),
            origin.span.clone(),
        )
    })?;
    if !metadata.is_file() {
        return Err(Diagnostic::new(
            missing_code,
            format!("{role} path `{}` is not a file", source_path.display()),
            origin.span.clone(),
        ));
    }
    let source_path = fs::canonicalize(&source_path).unwrap_or(source_path);
    let content_hash = hash_file(&source_path, &origin.span)?;
    Ok(PreparedAsset {
        source_path,
        content_hash,
    })
}

pub(crate) fn verify_prepared_asset(asset: &PreparedAsset, span: &SourceSpan) -> Result<()> {
    let actual = hash_file(asset.source_path(), span)?;
    if actual == asset.content_hash() {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "E_ASSET_CHANGED",
            format!(
                "asset `{}` changed after preflight",
                asset.source_path().display()
            ),
            span.clone(),
        ))
    }
}

fn hash_file(path: &Path, span: &SourceSpan) -> Result<String> {
    let file = fs::File::open(path).map_err(|error| {
        Diagnostic::new(
            "E_INPUT_HASH",
            format!("could not read asset `{}`: {error}", path.display()),
            span.clone(),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = reader.read(&mut buffer).map_err(|error| {
            Diagnostic::new(
                "E_INPUT_HASH",
                format!("could not hash asset `{}`: {error}", path.display()),
                span.clone(),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub(super) fn resolve_authored_path(value: &Path, span: &SourceSpan) -> Result<PathBuf> {
    if value.is_absolute() {
        return Ok(value.to_path_buf());
    }
    let base = span.source().base_directory().ok_or_else(|| {
        Diagnostic::new(
            "E_RELATIVE_PATH_WITHOUT_BASE",
            format!(
                "relative authored path `{}` has no source directory",
                value.display()
            ),
            span.clone(),
        )
    })?;
    Ok(base.join(value))
}

pub(super) fn entrypoint_directory(source: &SourceFile) -> Result<&Path> {
    source.base_directory().ok_or_else(|| {
        Diagnostic::new(
            "E_SOURCE_WITHOUT_BASE",
            "rendering requires the entrypoint source to have a base directory",
            SourceSpan::source_start(source.clone()),
        )
    })
}

pub(super) fn manifest_path(output: &Path) -> PathBuf {
    let mut value = output.as_os_str().to_os_string();
    value.push(".manifest.json");
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_resolve_from_their_own_source_units() {
        let main = SourceFile::with_base("main.yaml", Some(PathBuf::from("/project")), "");
        let imported = SourceFile::with_base(
            "effects/intro.yaml",
            Some(PathBuf::from("/project/effects")),
            "",
        );

        assert_eq!(
            resolve_authored_path(Path::new("card.png"), &SourceSpan::source_start(main),)
                .expect("main path"),
            PathBuf::from("/project/card.png")
        );
        assert_eq!(
            resolve_authored_path(Path::new("card.png"), &SourceSpan::source_start(imported),)
                .expect("imported path"),
            PathBuf::from("/project/effects/card.png")
        );
    }

    #[test]
    fn relative_paths_require_a_source_base() {
        let source = SourceFile::with_base("<memory>", None, "");
        let error = resolve_authored_path(Path::new("card.png"), &SourceSpan::source_start(source))
            .expect_err("missing base");

        assert_eq!(error.code, "E_RELATIVE_PATH_WITHOUT_BASE");
    }

    #[cfg(unix)]
    #[test]
    fn manifest_path_preserves_non_utf8_output_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let output = PathBuf::from(OsString::from_vec(b"video-\xFF.mp4".to_vec()));
        let manifest = manifest_path(&output);
        assert_eq!(
            manifest.as_os_str().as_bytes(),
            b"video-\xFF.mp4.manifest.json"
        );
    }
}
