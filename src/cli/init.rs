use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use clipasm::diagnostic::{Diagnostic, Result};
use clipasm::source::SourceSpan;

use super::safe_display_path;

const DIRECTORIES: &[&str] = &["assets"];
const FILES: &[ScaffoldFile] = &[
    ScaffoldFile::new(
        ".gitignore",
        include_bytes!("../../examples/starter/.gitignore"),
    ),
    ScaffoldFile::new(
        "README.md",
        include_bytes!("../../examples/starter/README.md"),
    ),
    ScaffoldFile::new(
        "main.clipasm",
        include_bytes!("../../examples/scenic-sequence.clipasm"),
    ),
    ScaffoldFile::new(
        "assets/morning.png",
        include_bytes!("../../examples/assets/morning.png"),
    ),
    ScaffoldFile::new(
        "assets/meadow.png",
        include_bytes!("../../examples/assets/meadow.png"),
    ),
    ScaffoldFile::new(
        "assets/evening.png",
        include_bytes!("../../examples/assets/evening.png"),
    ),
];

#[derive(Clone, Copy, Debug)]
struct ScaffoldFile {
    relative_path: &'static str,
    contents: &'static [u8],
}

impl ScaffoldFile {
    const fn new(relative_path: &'static str, contents: &'static [u8]) -> Self {
        Self {
            relative_path,
            contents,
        }
    }
}

#[derive(Debug)]
struct PlannedFile {
    path: PathBuf,
    contents: &'static [u8],
}

#[derive(Debug)]
enum CreatedPath {
    Directory(PathBuf),
    File {
        path: PathBuf,
        expected_contents: &'static [u8],
    },
}

#[derive(Debug)]
struct ProjectPlan {
    target: PathBuf,
    directories: Vec<PathBuf>,
    files: Vec<PlannedFile>,
}

impl ProjectPlan {
    fn new(target: &Path) -> Result<Self> {
        if target.as_os_str().is_empty() {
            return Err(init_path_error(
                target,
                "the initialization path must not be empty",
            ));
        }
        let target = absolute_target(target)?;

        let mut directories = Vec::with_capacity(DIRECTORIES.len() + 1);
        directories.push(target.clone());
        for relative_path in DIRECTORIES {
            validate_relative_path(relative_path, &target)?;
            directories.push(target.join(relative_path));
        }

        let mut files = Vec::with_capacity(FILES.len());
        for file in FILES {
            validate_relative_path(file.relative_path, &target)?;
            files.push(PlannedFile {
                path: target.join(file.relative_path),
                contents: file.contents,
            });
        }

        Ok(Self {
            target,
            directories,
            files,
        })
    }

    fn detect_conflicts(&self) -> Result<()> {
        let mut conflicts = Vec::new();

        for directory in &self.directories {
            if let Some(conflict) = directory_conflict(directory)? {
                conflicts.push(conflict);
            }
        }

        for file in &self.files {
            match fs::symlink_metadata(&file.path) {
                Ok(_) => conflicts.push(format!(
                    "refusing to replace existing path `{}`",
                    safe_display_path(&file.path)
                )),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
                    conflicts.push(format!(
                        "an ancestor of `{}` is not a directory",
                        safe_display_path(&file.path)
                    ));
                }
                Err(error) => {
                    return Err(init_io_error(
                        &file.path,
                        format!(
                            "could not inspect `{}`: {error}",
                            safe_display_path(&file.path)
                        ),
                    ));
                }
            }
        }

        if conflicts.is_empty() {
            return Ok(());
        }

        let count = conflicts.len();
        Err(conflicts.into_iter().fold(
            Diagnostic::new(
                "E_INIT_CONFLICT",
                format!(
                    "cannot initialize `{}` because {count} scaffold path(s) conflict",
                    safe_display_path(&self.target)
                ),
                init_source_span(&self.target),
            ),
            Diagnostic::note,
        ))
    }

    fn write(self) -> Result<()> {
        let mut created_paths = Vec::new();

        for directory in &self.directories {
            if let Err(error) = create_directory(directory, &mut created_paths) {
                return Err(clean_up_created_paths(error, &created_paths));
            }
        }

        for file in &self.files {
            if let Err(error) = write_new_file(file, &mut created_paths) {
                return Err(clean_up_created_paths(error, &created_paths));
            }
        }

        Ok(())
    }
}

pub(super) fn initialize(target: &Path) -> Result<PathBuf> {
    let plan = ProjectPlan::new(target)?;
    plan.detect_conflicts()?;
    let initialized_target = plan.target.clone();
    plan.write()?;
    Ok(initialized_target)
}

fn validate_relative_path(relative_path: &str, target: &Path) -> Result<()> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(init_path_error(
            target,
            format!("starter path `{relative_path}` is not a safe relative path"),
        ));
    }
    Ok(())
}

fn absolute_target(target: &Path) -> Result<PathBuf> {
    if target.is_absolute() {
        Ok(target.to_path_buf())
    } else {
        // Retain `..`: filesystem traversal resolves it after directory links,
        // unlike lexical normalization of `link/../project`.
        std::env::current_dir()
            .map_err(|error| {
                init_io_error(
                    target,
                    format!("could not determine the current directory: {error}"),
                )
            })
            .map(|current_directory| current_directory.join(target))
    }
}

fn directory_conflict(path: &Path) -> Result<Option<String>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(None),
        Ok(_) => Ok(Some(format!(
            "`{}` exists but is not a directory",
            safe_display_path(path)
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Ok(Some(format!(
                "`{}` is a symbolic link that does not resolve to a directory",
                safe_display_path(path)
            ))),
            Ok(_) => Err(init_io_error(
                path,
                format!(
                    "could not inspect `{}` after resolving it: {error}",
                    safe_display_path(path)
                ),
            )),
            Err(inspect_error) if inspect_error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(inspect_error) if inspect_error.kind() == io::ErrorKind::NotADirectory => {
                Ok(Some(format!(
                    "an ancestor of `{}` is not a directory",
                    safe_display_path(path)
                )))
            }
            Err(inspect_error) => Err(init_io_error(
                path,
                format!(
                    "could not inspect `{}`: {inspect_error}",
                    safe_display_path(path)
                ),
            )),
        },
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => Ok(Some(format!(
            "an ancestor of `{}` is not a directory",
            safe_display_path(path)
        ))),
        Err(error) => Err(init_io_error(
            path,
            format!("could not inspect `{}`: {error}", safe_display_path(path)),
        )),
    }
}

fn create_directory(path: &Path, created_paths: &mut Vec<CreatedPath>) -> Result<()> {
    match directory_conflict(path)? {
        None => {
            if fs::symlink_metadata(path).is_ok() {
                return Ok(());
            }
        }
        Some(conflict) => {
            return Err(Diagnostic::new(
                "E_INIT_CONFLICT",
                conflict,
                init_source_span(path),
            ));
        }
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_directory(parent, created_paths)?;
    }

    match fs::create_dir(path) {
        Ok(()) => {
            created_paths.push(CreatedPath::Directory(path.to_path_buf()));
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            match directory_conflict(path)? {
                None => Ok(()),
                Some(conflict) => Err(Diagnostic::new(
                    "E_INIT_CONFLICT",
                    conflict,
                    init_source_span(path),
                )),
            }
        }
        Err(error) => Err(init_io_error(
            path,
            format!(
                "could not create directory `{}`: {error}",
                safe_display_path(path)
            ),
        )),
    }
}

fn write_new_file(file: &PlannedFile, created_paths: &mut Vec<CreatedPath>) -> Result<()> {
    let mut destination = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file.path)
    {
        Ok(destination) => destination,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Diagnostic::new(
                "E_INIT_CONFLICT",
                format!(
                    "refusing to replace existing path `{}`",
                    safe_display_path(&file.path)
                ),
                init_source_span(&file.path),
            ));
        }
        Err(error) => {
            return Err(init_io_error(
                &file.path,
                format!(
                    "could not create `{}`: {error}",
                    safe_display_path(&file.path)
                ),
            ));
        }
    };
    created_paths.push(CreatedPath::File {
        path: file.path.clone(),
        expected_contents: file.contents,
    });

    destination.write_all(file.contents).map_err(|error| {
        init_io_error(
            &file.path,
            format!(
                "could not write `{}`: {error}",
                safe_display_path(&file.path)
            ),
        )
    })
}

fn clean_up_created_paths(mut diagnostic: Diagnostic, created_paths: &[CreatedPath]) -> Diagnostic {
    for created_path in created_paths.iter().rev() {
        let path = match created_path {
            CreatedPath::Directory(path) | CreatedPath::File { path, .. } => path,
        };
        let result = match (created_path, fs::symlink_metadata(path)) {
            (CreatedPath::Directory(_), Ok(metadata)) if metadata.file_type().is_dir() => {
                fs::remove_dir(path)
            }
            (
                CreatedPath::File {
                    expected_contents, ..
                },
                Ok(metadata),
            ) if metadata.file_type().is_file() => match fs::read(path) {
                Ok(contents) if expected_contents.starts_with(&contents) => fs::remove_file(path),
                Ok(_) => {
                    diagnostic = diagnostic.note(format!(
                        "preserved `{}` because its contents changed during initialization",
                        safe_display_path(path)
                    ));
                    continue;
                }
                Err(error) => Err(error),
            },
            (_, Ok(_)) => {
                diagnostic = diagnostic.note(format!(
                    "preserved `{}` because its file type changed during initialization",
                    safe_display_path(path)
                ));
                continue;
            }
            (_, Err(error)) if error.kind() == io::ErrorKind::NotFound => continue,
            (_, Err(error)) => Err(error),
        };
        if let Err(error) = result {
            diagnostic = diagnostic.note(format!(
                "could not remove incomplete scaffold path `{}`: {error}",
                safe_display_path(path)
            ));
        }
    }
    diagnostic
}

fn init_path_error(target: &Path, message: impl Into<String>) -> Diagnostic {
    let target = if target.as_os_str().is_empty() {
        Path::new("<command-line>")
    } else {
        target
    };
    Diagnostic::new("E_INIT_PATH", message, init_source_span(target))
}

fn init_io_error(path: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E_INIT_IO", message, init_source_span(path))
}

fn init_source_span(path: &Path) -> SourceSpan {
    SourceSpan::file_start(safe_display_path(path))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn starter_paths_must_be_normal_relative_paths() {
        let target = Path::new("project");

        for invalid in ["", ".", "..", "../main.clipasm", "/main.clipasm"] {
            let error = validate_relative_path(invalid, target).expect_err("invalid starter path");
            assert_eq!(error.code, "E_INIT_PATH");
        }
    }

    #[test]
    fn a_concurrent_file_conflict_rolls_back_only_created_paths() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("project");
        let plan = ProjectPlan::new(&target).expect("project plan");
        plan.detect_conflicts().expect("initially conflict-free");

        fs::create_dir(&target).expect("concurrent target directory");
        fs::write(target.join("main.clipasm"), b"owned source").expect("concurrent file");

        let error = plan.write().expect_err("concurrent conflict");

        assert_eq!(error.code, "E_INIT_CONFLICT");
        assert_eq!(
            fs::read(target.join("main.clipasm")).expect("preserved file"),
            b"owned source"
        );
        assert_eq!(
            fs::read_dir(&target)
                .expect("remaining target")
                .map(|entry| entry.expect("entry").file_name())
                .collect::<Vec<_>>(),
            [OsString::from("main.clipasm")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_a_preexisting_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("project");
        let asset_store = directory.path().join("asset-store");
        let plan = ProjectPlan::new(&target).expect("project plan");
        plan.detect_conflicts().expect("initially conflict-free");

        fs::create_dir(&target).expect("concurrent target directory");
        fs::create_dir(&asset_store).expect("asset store");
        fs::write(target.join("notes.txt"), b"keep me").expect("unrelated content");
        symlink(&asset_store, target.join("assets")).expect("assets symlink");
        fs::write(target.join("main.clipasm"), b"owned source").expect("concurrent file");

        let error = plan.write().expect_err("concurrent conflict");

        assert_eq!(error.code, "E_INIT_CONFLICT");
        assert_eq!(
            fs::read(target.join("notes.txt")).expect("preserved content"),
            b"keep me"
        );
        assert_eq!(
            fs::read(target.join("main.clipasm")).expect("preserved source"),
            b"owned source"
        );
        assert!(
            fs::symlink_metadata(target.join("assets"))
                .expect("preserved assets link")
                .file_type()
                .is_symlink()
        );
        assert!(!target.join(".gitignore").exists());
        assert!(!target.join("README.md").exists());
        assert!(
            fs::read_dir(&asset_store)
                .expect("preserved asset store")
                .next()
                .is_none()
        );
    }

    #[test]
    fn cleanup_preserves_a_created_path_whose_contents_were_replaced() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("README.md");
        fs::write(&path, b"user replacement").expect("replacement");
        let created_paths = [CreatedPath::File {
            path: path.clone(),
            expected_contents: b"generated contents",
        }];
        let diagnostic = Diagnostic::new(
            "E_INIT_IO",
            "later failure",
            SourceSpan::file_start(directory.path()),
        );

        let diagnostic = clean_up_created_paths(diagnostic, &created_paths);

        assert_eq!(
            fs::read(&path).expect("preserved replacement"),
            b"user replacement"
        );
        assert!(
            diagnostic
                .notes
                .iter()
                .any(|note| note.contains("contents changed"))
        );
    }
}
