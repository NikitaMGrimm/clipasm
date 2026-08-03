use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::compiler::CompiledProgram;
use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::model::{AudioDomain, AudioSpec, FrameCount, VideoSpec};
use crate::semantic::SourceOrigin;
use crate::source::{SourceFile, SourceSpan};

use super::tools::{
    ToolIdentity, verify_audio_decodable, verify_image_decodable, verify_video_decodable,
};
use super::{PreparedAsset, PreparedNode, PreparedSourceColor, RenderPolicy};

pub(super) fn prepare_output_path(
    compiled: &CompiledProgram,
    render_policy: RenderPolicy,
) -> Result<PathBuf> {
    let output = compiled.output().ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::MissingOutput,
            "`render` requires `config.output`",
            SourceSpan::source_start(compiled.entrypoint_source().clone()),
        )
    })?;
    render_policy.validate_output_path(&output.value, &output.span)?;
    resolve_authored_path(&output.value, &output.span)
}

pub(super) fn validate_destination(path: &Path, role: &str, code: BuiltinDiagnostic) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Diagnostic::builtin(
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
        Err(error) => Err(Diagnostic::builtin(
            code,
            format!(
                "{role} destination `{}` cannot be inspected: {error}",
                path.display()
            ),
            SourceSpan::file_start(path),
        )),
    }
}

fn invalid_destination(path: &Path, role: &str, code: BuiltinDiagnostic) -> Diagnostic {
    Diagnostic::builtin(
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
    code: BuiltinDiagnostic,
) -> Result<()> {
    if canonical_path_identity(left)? != canonical_path_identity(right)? {
        return Ok(());
    }
    Err(Diagnostic::builtin(
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
        node.try_visit_resources(|resource| {
            reject_path_collision(
                output,
                "output",
                resource.path(),
                resource.role(),
                BuiltinDiagnostic::OutputCollision,
            )?;
            reject_path_collision(
                manifest,
                "manifest",
                resource.path(),
                resource.role(),
                BuiltinDiagnostic::ManifestCollision,
            )
        })?;
    }
    Ok(())
}

fn canonical_path_identity(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                Diagnostic::builtin(
                    BuiltinDiagnostic::PathResolution,
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
        Diagnostic::builtin(
            BuiltinDiagnostic::PathResolution,
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
) -> Result<(PreparedAsset, PreparedSourceColor)> {
    let asset = prepare_file_asset(
        authored,
        origin,
        "image",
        BuiltinDiagnostic::MissingImageFile,
    )?;
    let color = verify_image_decodable(asset.source_path(), &origin.span, ffmpeg, ffprobe)?;
    Ok((asset, color))
}

pub(super) fn prepare_video_asset(
    authored: &Path,
    video: &VideoSpec,
    origin: &SourceOrigin,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<(PreparedAsset, FrameCount, bool, PreparedSourceColor)> {
    let asset = prepare_file_asset(
        authored,
        origin,
        "video",
        BuiltinDiagnostic::MissingVideoFile,
    )?;
    let (frames, has_audio, color) =
        verify_video_decodable(asset.source_path(), video, &origin.span, ffmpeg, ffprobe)?;
    Ok((asset, frames, has_audio, color))
}

pub(super) fn prepare_audio_asset(
    authored: &Path,
    audio: AudioSpec,
    origin: &SourceOrigin,
    ffmpeg: &ToolIdentity,
    ffprobe: &ToolIdentity,
) -> Result<(PreparedAsset, AudioDomain)> {
    let asset = prepare_file_asset(
        authored,
        origin,
        "audio",
        BuiltinDiagnostic::MissingAudioFile,
    )?;
    let domain = verify_audio_decodable(asset.source_path(), audio, &origin.span, ffmpeg, ffprobe)?;
    Ok((asset, domain))
}

fn prepare_file_asset(
    authored: &Path,
    origin: &SourceOrigin,
    role: &str,
    missing_code: BuiltinDiagnostic,
) -> Result<PreparedAsset> {
    let source_path = resolve_authored_path(authored, &origin.span)?;
    let metadata = fs::metadata(&source_path).map_err(|error| {
        Diagnostic::builtin(
            missing_code,
            format!(
                "{role} file `{}` is not accessible: {error}",
                source_path.display()
            ),
            origin.span.clone(),
        )
    })?;
    if !metadata.is_file() {
        return Err(Diagnostic::builtin(
            missing_code,
            format!("{role} path `{}` is not a file", source_path.display()),
            origin.span.clone(),
        ));
    }
    let source_path = fs::canonicalize(&source_path).map_err(|error| {
        Diagnostic::builtin(
            missing_code,
            format!(
                "could not resolve {role} file `{}` after inspection: {error}",
                source_path.display()
            ),
            origin.span.clone(),
        )
    })?;
    let content_hash = hash_file(&source_path, &origin.span)?;
    Ok(PreparedAsset::new(source_path, content_hash))
}

pub(super) fn prepare_external_file_asset(
    authored: &Path,
    span: &SourceSpan,
) -> Result<PreparedAsset> {
    prepare_file_asset(
        authored,
        &SourceOrigin::new("external file parameter", span.clone()),
        "external parameter",
        BuiltinDiagnostic::MissingExternalFile,
    )
}

pub(crate) fn verify_prepared_asset(asset: &PreparedAsset, span: &SourceSpan) -> Result<()> {
    let actual = hash_file(asset.source_path(), span)?;
    if actual == asset.content_hash() {
        Ok(())
    } else {
        Err(Diagnostic::builtin(
            BuiltinDiagnostic::AssetChanged,
            format!(
                "asset `{}` changed after preflight",
                asset.source_path().display()
            ),
            span.clone(),
        ))
    }
}

fn hash_file(path: &Path, span: &SourceSpan) -> Result<String> {
    crate::identity::hash_file(path).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::InputHash,
            format!("could not hash asset `{}`: {error}", path.display()),
            span.clone(),
        )
    })
}

pub(super) fn resolve_authored_path(value: &Path, span: &SourceSpan) -> Result<PathBuf> {
    if value.is_absolute() {
        return Ok(value.to_path_buf());
    }
    let base = span.source().base_directory().ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::RelativePathWithoutBase,
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
        Diagnostic::builtin(
            BuiltinDiagnostic::SourceWithoutBase,
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
        let main = SourceFile::with_base("main.clipasm", Some(PathBuf::from("/project")), "");
        let imported = SourceFile::with_base(
            "effects/intro.clipasm",
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
    fn prepared_file_assets_record_the_canonical_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target.bin");
        let alias = directory.path().join("alias.bin");
        fs::write(&target, b"content").expect("target");
        symlink("target.bin", &alias).expect("alias");
        let source =
            SourceFile::with_base("effect.clipasm", Some(directory.path().to_path_buf()), "");
        let span = SourceSpan::source_start(source);

        let asset =
            prepare_external_file_asset(Path::new("alias.bin"), &span).expect("prepared asset");

        assert_eq!(
            asset.source_path(),
            fs::canonicalize(target).expect("canonical target")
        );
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
