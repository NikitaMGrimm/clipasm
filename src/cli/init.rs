use std::fs;
use std::fs::OpenOptions;
use std::io;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use clipasm::diagnostic::{Diagnostic, Result};
use clipasm::source::SourceSpan;

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
        let target = normalized_absolute_target(target)?;

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
        if let Some(symlink) = first_symlink_component(&self.target)? {
            conflicts.push(format!(
                "refusing to initialize through symbolic link `{}`",
                symlink.display()
            ));
        }

        for directory in &self.directories {
            match fs::symlink_metadata(directory) {
                Ok(metadata) if !metadata.file_type().is_dir() => conflicts.push(format!(
                    "`{}` exists but is not a directory",
                    directory.display()
                )),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
                    conflicts.push(format!(
                        "an ancestor of `{}` is not a directory",
                        directory.display()
                    ));
                }
                Err(error) => {
                    return Err(init_io_error(
                        directory,
                        format!("could not inspect `{}`: {error}", directory.display()),
                    ));
                }
            }
        }

        for file in &self.files {
            match fs::symlink_metadata(&file.path) {
                Ok(_) => conflicts.push(format!(
                    "refusing to replace existing path `{}`",
                    file.path.display()
                )),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
                    conflicts.push(format!(
                        "an ancestor of `{}` is not a directory",
                        file.path.display()
                    ));
                }
                Err(error) => {
                    return Err(init_io_error(
                        &file.path,
                        format!("could not inspect `{}`: {error}", file.path.display()),
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
                    self.target.display()
                ),
                SourceSpan::file_start(&self.target),
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

fn normalized_absolute_target(target: &Path) -> Result<PathBuf> {
    let absolute = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                init_io_error(
                    target,
                    format!("could not determine the current directory: {error}"),
                )
            })?
            .join(target)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(init_path_error(
                        target,
                        format!(
                            "initialization path `{}` escapes its filesystem root",
                            target.display()
                        ),
                    ));
                }
            }
        }
    }
    Ok(normalized)
}

fn first_symlink_component(path: &Path) -> Result<Option<PathBuf>> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Ok(Some(current)),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(init_io_error(
                    &current,
                    format!("could not inspect `{}`: {error}", current.display()),
                ));
            }
        }
    }
    Ok(None)
}

fn reject_symlink_ancestor(path: &Path) -> Result<()> {
    if let Some(symlink) = first_symlink_component(path)? {
        return Err(Diagnostic::new(
            "E_INIT_CONFLICT",
            format!(
                "refusing to initialize through symbolic link `{}`",
                symlink.display()
            ),
            SourceSpan::file_start(symlink),
        ));
    }
    Ok(())
}

fn create_directory(path: &Path, created_paths: &mut Vec<CreatedPath>) -> Result<()> {
    reject_symlink_ancestor(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => return Ok(()),
        Ok(_) => {
            return Err(Diagnostic::new(
                "E_INIT_CONFLICT",
                format!("`{}` exists but is not a directory", path.display()),
                SourceSpan::file_start(path),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(init_io_error(
                path,
                format!("could not inspect `{}`: {error}", path.display()),
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
            match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
                Ok(_) => Err(Diagnostic::new(
                    "E_INIT_CONFLICT",
                    format!("`{}` exists but is not a directory", path.display()),
                    SourceSpan::file_start(path),
                )),
                Err(inspect_error) => Err(init_io_error(
                    path,
                    format!(
                        "could not inspect `{}` after a concurrent change: {inspect_error}",
                        path.display()
                    ),
                )),
            }
        }
        Err(error) => Err(init_io_error(
            path,
            format!("could not create directory `{}`: {error}", path.display()),
        )),
    }
}

fn write_new_file(file: &PlannedFile, created_paths: &mut Vec<CreatedPath>) -> Result<()> {
    reject_symlink_ancestor(&file.path)?;
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
                    file.path.display()
                ),
                SourceSpan::file_start(&file.path),
            ));
        }
        Err(error) => {
            return Err(init_io_error(
                &file.path,
                format!("could not create `{}`: {error}", file.path.display()),
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
            format!("could not write `{}`: {error}", file.path.display()),
        )
    })
}

fn clean_up_created_paths(mut diagnostic: Diagnostic, created_paths: &[CreatedPath]) -> Diagnostic {
    for created_path in created_paths.iter().rev() {
        let path = match created_path {
            CreatedPath::Directory(path) | CreatedPath::File { path, .. } => path,
        };
        if let Err(error) = reject_symlink_ancestor(path) {
            diagnostic = diagnostic.note(format!(
                "preserved incomplete scaffold path `{}` because its ancestry changed: {}",
                path.display(),
                error.message
            ));
            continue;
        }
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
                        path.display()
                    ));
                    continue;
                }
                Err(error) => Err(error),
            },
            (_, Ok(_)) => {
                diagnostic = diagnostic.note(format!(
                    "preserved `{}` because its file type changed during initialization",
                    path.display()
                ));
                continue;
            }
            (_, Err(error)) if error.kind() == io::ErrorKind::NotFound => continue,
            (_, Err(error)) => Err(error),
        };
        if let Err(error) = result {
            diagnostic = diagnostic.note(format!(
                "could not remove incomplete scaffold path `{}`: {error}",
                path.display()
            ));
        }
    }
    diagnostic
}

fn init_path_error(target: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "E_INIT_PATH",
        message,
        SourceSpan::file_start(if target.as_os_str().is_empty() {
            Path::new("<command-line>")
        } else {
            target
        }),
    )
}

fn init_io_error(path: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E_INIT_IO", message, SourceSpan::file_start(path))
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
