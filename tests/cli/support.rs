use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(
        directory.path().join("card.ppm"),
        b"P3\n1 1\n255\n255 0 0\n",
    )
    .expect("image");
    let workflow = directory.path().join("workflow.clipasm");
    fs::write(
        &workflow,
        "clipasm 1\n{\n  image(\"card.ppm\", 1s)\n  concat\n}\n",
    )
    .expect("workflow");
    (directory, workflow)
}

pub(crate) fn run_clipasm(current_directory: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_clipasm"))
        .current_dir(current_directory)
        .args(arguments)
        .output()
        .expect("run clipasm")
}

pub(crate) fn project_inventory(root: &Path) -> Vec<PathBuf> {
    fn collect(root: &Path, directory: &Path, inventory: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).expect("read project directory") {
            let entry = entry.expect("project entry");
            let path = entry.path();
            inventory.push(
                path.strip_prefix(root)
                    .expect("project-relative path")
                    .into(),
            );
            if path.is_dir() {
                collect(root, &path, inventory);
            }
        }
    }

    let mut inventory = Vec::new();
    collect(root, root, &mut inventory);
    inventory.sort();
    inventory
}
