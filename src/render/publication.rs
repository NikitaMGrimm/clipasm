use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::diagnostic::{Diagnostic, Result, SourceSpan};

static PUBLICATION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct PublicationTransaction {
    output: PublicationFile,
    manifest: PublicationFile,
}

struct PublicationFile {
    role: &'static str,
    destination: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
    backed_up: bool,
    published: bool,
}

impl PublicationTransaction {
    pub(super) fn new(output: &Path, manifest: &Path) -> Self {
        Self {
            output: PublicationFile::new("output", output, "mp4"),
            manifest: PublicationFile::new("manifest", manifest, "json"),
        }
    }

    pub(super) fn staged_output(&self) -> &Path {
        &self.output.staged
    }

    pub(super) fn stage_manifest(&self, contents: &[u8]) -> Result<()> {
        fs::write(&self.manifest.staged, contents).map_err(|error| {
            Diagnostic::new(
                "E_MANIFEST",
                format!(
                    "could not write staged manifest `{}`: {error}",
                    self.manifest.staged.display()
                ),
                SourceSpan::file_start(&self.manifest.destination),
            )
        })
    }

    pub(super) fn commit(mut self) -> Result<()> {
        self.commit_with(|source, destination| fs::rename(source, destination))
    }

    fn commit_with(
        &mut self,
        mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> Result<()> {
        self.validate_staged()?;
        validate_destination(&self.output)?;
        validate_destination(&self.manifest)?;

        if let Err(error) = backup(&mut self.output, &mut rename) {
            return Err(self.rollback(error, &mut rename));
        }
        if let Err(error) = backup(&mut self.manifest, &mut rename) {
            return Err(self.rollback(error, &mut rename));
        }
        if let Err(error) = publish(&mut self.output, &mut rename) {
            return Err(self.rollback(error, &mut rename));
        }
        if let Err(error) = publish(&mut self.manifest, &mut rename) {
            return Err(self.rollback(error, &mut rename));
        }

        self.remove_backups_best_effort();
        Ok(())
    }

    fn validate_staged(&self) -> Result<()> {
        for file in [&self.output, &self.manifest] {
            match fs::metadata(&file.staged) {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => {
                    return Err(Diagnostic::new(
                        "E_PUBLICATION",
                        format!(
                            "staged {} `{}` is not a regular file",
                            file.role,
                            file.staged.display()
                        ),
                        SourceSpan::file_start(&file.destination),
                    ));
                }
                Err(error) => {
                    return Err(Diagnostic::new(
                        "E_PUBLICATION",
                        format!(
                            "staged {} `{}` cannot be inspected: {error}",
                            file.role,
                            file.staged.display()
                        ),
                        SourceSpan::file_start(&file.destination),
                    ));
                }
            }
        }
        Ok(())
    }

    fn rollback(
        &mut self,
        error: Diagnostic,
        rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> Diagnostic {
        let mut notes = Vec::new();
        for file in [&mut self.output, &mut self.manifest] {
            if file.published {
                match fs::remove_file(&file.destination) {
                    Ok(()) => file.published = false,
                    Err(rollback_error) if rollback_error.kind() == io::ErrorKind::NotFound => {
                        file.published = false;
                    }
                    Err(rollback_error) => notes.push(format!(
                        "could not remove newly published {} `{}` during rollback: {rollback_error}",
                        file.role,
                        file.destination.display()
                    )),
                }
            }
        }
        for file in [&mut self.output, &mut self.manifest] {
            if file.backed_up {
                match rename(&file.backup, &file.destination) {
                    Ok(()) => file.backed_up = false,
                    Err(rollback_error) => notes.push(format!(
                        "could not restore previous {} from `{}` to `{}`: {rollback_error}",
                        file.role,
                        file.backup.display(),
                        file.destination.display()
                    )),
                }
            }
        }
        notes.into_iter().fold(error, Diagnostic::note)
    }

    fn remove_backups_best_effort(&mut self) {
        for file in [&mut self.output, &mut self.manifest] {
            if !file.backed_up {
                continue;
            }
            if fs::remove_file(&file.backup).is_ok() {
                file.backed_up = false;
            }
        }
    }
}

impl Drop for PublicationTransaction {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.output.staged);
        let _ = fs::remove_file(&self.manifest.staged);
    }
}

impl PublicationFile {
    fn new(role: &'static str, destination: &Path, extension: &str) -> Self {
        Self {
            role,
            destination: destination.to_path_buf(),
            staged: unique_sibling(
                destination,
                &format!("publication-{role}-staged"),
                extension,
            ),
            backup: unique_sibling(destination, &format!("publication-{role}-backup"), "bak"),
            backed_up: false,
            published: false,
        }
    }
}

fn validate_destination(file: &PublicationFile) -> Result<()> {
    match fs::symlink_metadata(&file.destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            match fs::metadata(&file.destination) {
                Ok(target) if target.is_file() => Ok(()),
                Ok(_) => Err(invalid_destination(file)),
                Err(error) => Err(Diagnostic::new(
                    destination_code(file.role),
                    format!(
                        "{} destination `{}` is an unsupported symlink: {error}",
                        file.role,
                        file.destination.display()
                    ),
                    SourceSpan::file_start(&file.destination),
                )),
            }
        }
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_destination(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Diagnostic::new(
            destination_code(file.role),
            format!(
                "{} destination `{}` cannot be inspected: {error}",
                file.role,
                file.destination.display()
            ),
            SourceSpan::file_start(&file.destination),
        )),
    }
}

fn invalid_destination(file: &PublicationFile) -> Diagnostic {
    Diagnostic::new(
        destination_code(file.role),
        format!(
            "{} destination `{}` is not a regular file",
            file.role,
            file.destination.display()
        ),
        SourceSpan::file_start(&file.destination),
    )
}

fn destination_code(role: &str) -> &'static str {
    match role {
        "output" => "E_INVALID_OUTPUT_DESTINATION",
        "manifest" => "E_INVALID_MANIFEST_DESTINATION",
        _ => "E_PUBLICATION",
    }
}

fn backup(
    file: &mut PublicationFile,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    match fs::symlink_metadata(&file.destination) {
        Ok(_) => {
            rename(&file.destination, &file.backup).map_err(|error| {
                rename_error(
                    file,
                    "move the previous destination to its backup",
                    &file.destination,
                    &file.backup,
                    &error,
                )
            })?;
            file.backed_up = true;
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Diagnostic::new(
            "E_PUBLICATION",
            format!(
                "could not inspect existing {} destination `{}`: {error}",
                file.role,
                file.destination.display()
            ),
            SourceSpan::file_start(&file.destination),
        )),
    }
}

fn publish(
    file: &mut PublicationFile,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    rename(&file.staged, &file.destination).map_err(|error| {
        rename_error(
            file,
            "publish the staged file",
            &file.staged,
            &file.destination,
            &error,
        )
    })?;
    file.published = true;
    Ok(())
}

fn rename_error(
    file: &PublicationFile,
    action: &str,
    source: &Path,
    destination: &Path,
    error: &io::Error,
) -> Diagnostic {
    Diagnostic::new(
        "E_PUBLICATION",
        format!(
            "could not {action} for {} from `{}` to `{}`: {error}",
            file.role,
            source.display(),
            destination.display()
        ),
        SourceSpan::file_start(&file.destination),
    )
}

fn unique_sibling(path: &Path, role: &str, extension: &str) -> PathBuf {
    let counter = PUBLICATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".{role}-{}-{counter}.{extension}",
        std::process::id()
    ));
    path.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transaction(directory: &Path) -> PublicationTransaction {
        PublicationTransaction::new(
            &directory.join("final.mp4"),
            &directory.join("final.mp4.manifest.json"),
        )
    }

    fn write_old_pair(directory: &Path) {
        fs::write(directory.join("final.mp4"), b"old output").expect("old output");
        fs::write(directory.join("final.mp4.manifest.json"), b"old manifest")
            .expect("old manifest");
    }

    fn assert_pair(directory: &Path, output: &[u8], manifest: &[u8]) {
        assert_eq!(
            fs::read(directory.join("final.mp4")).expect("output"),
            output
        );
        assert_eq!(
            fs::read(directory.join("final.mp4.manifest.json")).expect("manifest"),
            manifest
        );
    }

    fn assert_no_residue(directory: &Path) {
        let entries = fs::read_dir(directory)
            .expect("directory")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            entries.len(),
            2,
            "unexpected publication residue: {entries:?}"
        );
    }

    #[test]
    fn failure_before_commit_preserves_both_old_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write_old_pair(directory.path());
        let publication = transaction(directory.path());
        fs::write(publication.staged_output(), b"new output").expect("staged output");
        publication
            .stage_manifest(b"new manifest")
            .expect("staged manifest");

        drop(publication);

        assert_pair(directory.path(), b"old output", b"old manifest");
        assert_no_residue(directory.path());
    }

    #[test]
    fn failure_during_second_commit_rename_restores_the_first() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write_old_pair(directory.path());
        let mut publication = transaction(directory.path());
        fs::write(publication.staged_output(), b"new output").expect("staged output");
        publication
            .stage_manifest(b"new manifest")
            .expect("staged manifest");
        let manifest_stage = publication.manifest.staged.clone();

        publication
            .commit_with(|source, destination| {
                if source == manifest_stage {
                    Err(io::Error::other("injected second commit failure"))
                } else {
                    fs::rename(source, destination)
                }
            })
            .expect_err("injected publication failure");

        drop(publication);
        assert_pair(directory.path(), b"old output", b"old manifest");
        assert_no_residue(directory.path());
    }

    #[test]
    fn successful_commit_leaves_no_staged_or_backup_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write_old_pair(directory.path());
        let publication = transaction(directory.path());
        fs::write(publication.staged_output(), b"new output").expect("staged output");
        publication
            .stage_manifest(b"new manifest")
            .expect("staged manifest");

        publication.commit().expect("publication");

        assert_pair(directory.path(), b"new output", b"new manifest");
        assert_no_residue(directory.path());
    }
}
