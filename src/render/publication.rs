use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostic::{BuiltinDiagnostic, Diagnostic, Result};
use crate::source::SourceSpan;

use super::staging::StagingDirectory;

pub(super) struct PublicationTransaction {
    staging: Option<StagingDirectory>,
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
    pub(super) fn new(output: &Path, manifest: &Path) -> Result<Self> {
        let staging =
            StagingDirectory::beside(output, "publication", BuiltinDiagnostic::Publication)?;
        Ok(Self {
            output: PublicationFile::new(
                "output",
                output,
                staging.path("output.mp4"),
                staging.path("output.bak"),
            ),
            manifest: PublicationFile::new(
                "manifest",
                manifest,
                staging.path("manifest.json"),
                staging.path("manifest.bak"),
            ),
            staging: Some(staging),
        })
    }

    pub(super) fn staged_output(&self) -> &Path {
        &self.output.staged
    }

    pub(super) fn stage_manifest(&self, contents: &[u8]) -> Result<()> {
        fs::write(&self.manifest.staged, contents).map_err(|error| {
            Diagnostic::builtin(
                BuiltinDiagnostic::Manifest,
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
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::Publication,
                        format!(
                            "staged {} `{}` is not a regular file",
                            file.role,
                            file.staged.display()
                        ),
                        SourceSpan::file_start(&file.destination),
                    ));
                }
                Err(error) => {
                    return Err(Diagnostic::builtin(
                        BuiltinDiagnostic::Publication,
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
        if self.output.backed_up || self.manifest.backed_up {
            let recovery = self
                .staging
                .take()
                .expect("active publication transaction owns its staging directory")
                .keep();
            notes.push(format!(
                "publication recovery files were retained in `{}`",
                recovery.display()
            ));
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
    fn new(role: &'static str, destination: &Path, staged: PathBuf, backup: PathBuf) -> Self {
        Self {
            role,
            destination: destination.to_path_buf(),
            staged,
            backup,
            backed_up: false,
            published: false,
        }
    }
}

fn validate_destination(file: &PublicationFile) -> Result<()> {
    match fs::symlink_metadata(&file.destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Diagnostic::builtin(
            destination_code(file.role),
            format!(
                "{} destination `{}` is a symlink; publication destinations must be regular files",
                file.role,
                file.destination.display()
            ),
            SourceSpan::file_start(&file.destination),
        )),
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(invalid_destination(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Diagnostic::builtin(
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
    Diagnostic::builtin(
        destination_code(file.role),
        format!(
            "{} destination `{}` is not a regular file",
            file.role,
            file.destination.display()
        ),
        SourceSpan::file_start(&file.destination),
    )
}

fn destination_code(role: &str) -> BuiltinDiagnostic {
    match role {
        "output" => BuiltinDiagnostic::InvalidOutputDestination,
        "manifest" => BuiltinDiagnostic::InvalidManifestDestination,
        _ => BuiltinDiagnostic::Publication,
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
        Err(error) => Err(Diagnostic::builtin(
            BuiltinDiagnostic::Publication,
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
    Diagnostic::builtin(
        BuiltinDiagnostic::Publication,
        format!(
            "could not {action} for {} from `{}` to `{}`: {error}",
            file.role,
            source.display(),
            destination.display()
        ),
        SourceSpan::file_start(&file.destination),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transaction(directory: &Path) -> PublicationTransaction {
        PublicationTransaction::new(
            &directory.join("final.mp4"),
            &directory.join("final.mp4.manifest.json"),
        )
        .expect("publication transaction")
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
    fn stages_and_backups_live_inside_a_private_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let publication = transaction(directory.path());
        let staging = publication
            .staged_output()
            .parent()
            .expect("staging directory");
        assert_ne!(staging, directory.path());
        assert_eq!(publication.manifest.staged.parent(), Some(staging));
        assert_eq!(publication.output.backup.parent(), Some(staging));
        assert_eq!(publication.manifest.backup.parent(), Some(staging));
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

    #[cfg(unix)]
    #[test]
    fn commit_rejects_an_output_symlink_without_replacing_its_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("existing.mp4");
        fs::write(&target, b"old target").expect("output target");
        symlink(&target, directory.path().join("final.mp4")).expect("output symlink");
        fs::write(
            directory.path().join("final.mp4.manifest.json"),
            b"old manifest",
        )
        .expect("old manifest");

        let publication = transaction(directory.path());
        fs::write(publication.staged_output(), b"new output").expect("staged output");
        publication
            .stage_manifest(b"new manifest")
            .expect("staged manifest");

        let error = publication.commit().expect_err("output symlink");

        assert_eq!(error.code, "E_INVALID_OUTPUT_DESTINATION");
        assert!(error.message.contains("is a symlink"));
        assert!(
            fs::symlink_metadata(directory.path().join("final.mp4"))
                .expect("output link")
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).expect("output target"), b"old target");
        assert_eq!(
            fs::read(directory.path().join("final.mp4.manifest.json")).expect("old manifest"),
            b"old manifest"
        );
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
    fn failed_backup_restore_retains_recovery_files() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write_old_pair(directory.path());
        let mut publication = transaction(directory.path());
        fs::write(publication.staged_output(), b"new output").expect("staged output");
        publication
            .stage_manifest(b"new manifest")
            .expect("staged manifest");
        let manifest_stage = publication.manifest.staged.clone();
        let output_backup = publication.output.backup.clone();

        let error = publication
            .commit_with(|source, destination| {
                if source == manifest_stage {
                    Err(io::Error::other("injected manifest publication failure"))
                } else if source == output_backup {
                    Err(io::Error::other("injected output restore failure"))
                } else {
                    fs::rename(source, destination)
                }
            })
            .expect_err("injected publication and rollback failures");
        assert!(
            error
                .notes
                .iter()
                .any(|note| note.contains("could not restore previous output"))
        );
        assert!(error.notes.iter().any(|note| {
            note.contains("publication recovery files were retained")
                && note.contains(
                    output_backup
                        .parent()
                        .expect("backup directory")
                        .to_string_lossy()
                        .as_ref(),
                )
        }));

        drop(publication);
        assert_eq!(
            fs::read(&output_backup).expect("retained output backup"),
            b"old output"
        );
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
