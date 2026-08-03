use std::fs;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Deserialize;

use clipasm::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use clipasm::source::{SourceFile, SourceSpan};

const MANIFEST_NAME: &str = "clipasm.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    project: ProjectTable,
    #[serde(default)]
    render: RenderTable,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectTable {
    entrypoint: toml::Spanned<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderTable {
    #[serde(default)]
    cache: CacheSetting,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(super) enum CacheSetting {
    #[default]
    Persistent,
    None,
}

impl CacheSetting {
    pub(super) const fn render_mode(self) -> clipasm::render::CacheMode {
        match self {
            Self::Persistent => clipasm::render::CacheMode::Persistent,
            Self::None => clipasm::render::CacheMode::None,
        }
    }
}

#[derive(Debug)]
pub(super) struct Project {
    root: PathBuf,
    entrypoint: PathBuf,
    cache: CacheSetting,
}

impl Project {
    pub(super) fn entrypoint(&self) -> &Path {
        &self.entrypoint
    }

    pub(super) fn cache_root(&self) -> PathBuf {
        self.root.join(".clipasm").join("cache")
    }

    pub(super) const fn cache(&self) -> CacheSetting {
        self.cache
    }
}

pub(super) fn discover() -> Result<Project> {
    let current = std::env::current_dir().map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ProjectIo,
            format!("could not determine the current directory: {error}"),
            SourceSpan::file_start("<current-directory>"),
        )
    })?;
    let manifest_path = find_manifest(&current)?.ok_or_else(|| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ProjectNotFound,
            format!(
                "could not find `{MANIFEST_NAME}` in `{}` or any parent directory",
                current.display()
            ),
            SourceSpan::file_start(&current),
        )
        .note("supply an explicit `.clipasm` source path or run `clipasm init`")
    })?;
    load(&manifest_path)
}

fn find_manifest(start: &Path) -> Result<Option<PathBuf>> {
    for directory in start.ancestors() {
        let candidate = directory.join(MANIFEST_NAME);
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => match fs::metadata(&candidate) {
                Ok(target) if target.is_file() => return Ok(Some(candidate)),
                Ok(_) => {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::ProjectIo,
                        format!(
                            "project manifest path `{}` does not resolve to a regular file",
                            candidate.display()
                        ),
                        SourceSpan::file_start(candidate),
                    ));
                }
                Err(error) => {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::ProjectIo,
                        format!(
                            "could not resolve project manifest path `{}`: {error}",
                            candidate.display()
                        ),
                        SourceSpan::file_start(candidate),
                    ));
                }
            },
            Ok(metadata) if metadata.is_file() => return Ok(Some(candidate)),
            Ok(_) => {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::ProjectIo,
                    format!(
                        "project manifest path `{}` is not a regular file",
                        candidate.display()
                    ),
                    SourceSpan::file_start(candidate),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(Diagnostic::builtin(
                    BuiltinDiagnostic::ProjectIo,
                    format!(
                        "could not inspect project manifest path `{}`: {error}",
                        candidate.display()
                    ),
                    SourceSpan::file_start(candidate),
                ));
            }
        }
    }
    Ok(None)
}

fn load(manifest_path: &Path) -> Result<Project> {
    let bytes = fs::read(manifest_path).map_err(|error| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ProjectIo,
            format!(
                "could not read project manifest `{}`: {error}",
                manifest_path.display()
            ),
            SourceSpan::file_start(manifest_path),
        )
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        Diagnostic::builtin(
            BuiltinDiagnostic::ProjectManifest,
            "project manifest must be valid UTF-8",
            SourceSpan::file_start(manifest_path),
        )
    })?;
    let source = SourceFile::new(manifest_path, text.clone());
    let manifest: Manifest = toml::from_str(&text).map_err(|error| {
        let (line, column) = error
            .span()
            .map_or((1, 1), |span| line_column(&text, span.start));
        Diagnostic::builtin(
            BuiltinDiagnostic::ProjectManifest,
            format!("invalid project manifest: {}", error.message()),
            SourceSpan::at(source.clone(), line, column),
        )
    })?;
    let entrypoint = validate_entrypoint(manifest.project.entrypoint, &source)?;
    let root = manifest_path
        .parent()
        .expect("a discovered manifest path has a parent directory");
    Ok(Project {
        root: root.to_path_buf(),
        entrypoint: root.join(entrypoint),
        cache: manifest.render.cache,
    })
}

fn validate_entrypoint(entrypoint: toml::Spanned<String>, source: &SourceFile) -> Result<PathBuf> {
    let span = entrypoint.span();
    let value = entrypoint.into_inner();
    let segments = value.split('/').collect::<Vec<_>>();
    let valid = !segments.is_empty()
        && segments
            .iter()
            .all(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        && !value.contains(['\\', ':'])
        && segments
            .last()
            .is_some_and(|segment| segment.to_ascii_lowercase().ends_with(".clipasm"));
    if valid {
        return Ok(segments.into_iter().collect());
    }
    let (line, column) = line_column(source.text(), span.start);
    Err(Diagnostic::builtin(
        BuiltinDiagnostic::ProjectManifest,
        "`project.entrypoint` must be a forward-slash relative path ending in `.clipasm`",
        SourceSpan::at(source.clone(), line, column),
    ))
}

fn line_column(text: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = text.get(..byte_offset).unwrap_or(text);
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.chars().count(), |(_, tail)| tail.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_entrypoints_are_forward_slash_relative_paths() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join(MANIFEST_NAME);
        fs::write(&manifest, "[project]\nentrypoint = \"src/main.clipasm\"\n").expect("manifest");
        assert_eq!(
            load(&manifest).expect("project").entrypoint,
            directory.path().join("src/main.clipasm")
        );

        for invalid in [
            "../main.clipasm",
            "/main.clipasm",
            "src\\main.clipasm",
            "C:/main.clipasm",
            "main.txt",
        ] {
            fs::write(
                &manifest,
                format!("[project]\nentrypoint = \"{invalid}\"\n"),
            )
            .expect("invalid manifest");
            let error = load(&manifest).expect_err("invalid entrypoint");
            assert_eq!(error.code, "E_PROJECT_MANIFEST");
            assert_eq!(error.span.line, 2);
        }
    }

    #[test]
    fn manifests_reject_unknown_fields() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join(MANIFEST_NAME);
        fs::write(
            &manifest,
            "unknown = true\n[project]\nentrypoint = \"main.clipasm\"\n",
        )
        .expect("unknown field");
        assert_eq!(
            load(&manifest).expect_err("unknown field").code,
            "E_PROJECT_MANIFEST"
        );
    }

    #[test]
    fn manifests_configure_render_cache_and_default_to_persistent() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join(MANIFEST_NAME);
        fs::write(&manifest, "[project]\nentrypoint = \"main.clipasm\"\n").expect("manifest");
        assert!(matches!(
            load(&manifest).expect("default project").cache,
            CacheSetting::Persistent
        ));

        fs::write(
            &manifest,
            "[project]\nentrypoint = \"main.clipasm\"\n\n[render]\ncache = \"none\"\n",
        )
        .expect("manifest");
        assert!(matches!(
            load(&manifest).expect("uncached project").cache,
            CacheSetting::None
        ));

        fs::write(
            &manifest,
            "[project]\nentrypoint = \"main.clipasm\"\n\n[render]\ncache = \"sometimes\"\n",
        )
        .expect("manifest");
        assert_eq!(
            load(&manifest).expect_err("invalid cache mode").code,
            "E_PROJECT_MANIFEST"
        );
    }

    #[test]
    fn invalid_toml_uses_clipasms_single_source_excerpt() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join(MANIFEST_NAME);
        fs::write(&manifest, "[project\n").expect("manifest");

        let error = load(&manifest).expect_err("invalid TOML");
        assert_eq!(error.code, "E_PROJECT_MANIFEST");
        assert_eq!(error.span.line, 1);
        assert!(!error.message.contains("\n  |"));
    }

    #[test]
    fn manifests_require_utf8() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manifest = directory.path().join(MANIFEST_NAME);
        fs::write(&manifest, [0xff]).expect("manifest");

        let error = load(&manifest).expect_err("non-UTF-8 manifest");
        assert_eq!(error.code, "E_PROJECT_MANIFEST");
        assert_eq!(error.message, "project manifest must be valid UTF-8");
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_local_manifest_symlink_blocks_parent_discovery() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(
            directory.path().join(MANIFEST_NAME),
            "[project]\nentrypoint = \"main.clipasm\"\n",
        )
        .expect("parent manifest");
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).expect("nested directory");
        symlink("missing.toml", nested.join(MANIFEST_NAME)).expect("broken manifest symlink");

        let error = find_manifest(&nested).expect_err("broken local manifest");
        assert_eq!(error.code, "E_PROJECT_IO");
        assert!(error.message.contains("could not resolve"));
    }
}
