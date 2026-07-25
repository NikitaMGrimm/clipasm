use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Result};
use crate::source::{
    ResolvedImport, SourceFile, SourcePackage, SourceProgramImplementation, SourceSpan, SourceUnit,
    SourceUnitId,
};

use super::lower::{CallableShape, builtin_shapes, lower_source};
use super::syntax::Declaration;
use super::{parser, sugar};

const MAX_IMPORT_DEPTH: usize = 128;

/// Parse, load, and link a native `.clipasm` package rooted at `path`.
///
/// Relative imports resolve from the file that declares them. Repeated
/// canonical paths are loaded once.
///
/// # Errors
///
/// Returns a source-located diagnostic for I/O, syntax, import, or
/// lowering failures.
pub fn parse_file(path: &Path) -> Result<SourcePackage> {
    let mut loader = Loader {
        builtins: builtin_shapes(),
        units: Vec::new(),
        loaded: BTreeMap::new(),
        visiting: Vec::new(),
        visiting_positions: BTreeMap::new(),
    };
    let root = loader.load(path, true, None, 0)?;
    Ok(SourcePackage {
        root,
        units: loader.units,
    })
}

struct Loader {
    builtins: BTreeMap<String, CallableShape>,
    units: Vec<SourceUnit>,
    loaded: BTreeMap<PathBuf, SourceUnitId>,
    visiting: Vec<PathBuf>,
    visiting_positions: BTreeMap<PathBuf, usize>,
}

impl Loader {
    #[allow(clippy::too_many_lines)]
    fn load(
        &mut self,
        path: &Path,
        is_root: bool,
        import_span: Option<&SourceSpan>,
        depth: usize,
    ) -> Result<SourceUnitId> {
        if depth > MAX_IMPORT_DEPTH {
            return Err(Diagnostic::new(
                "E_PROGRAM_IMPORT_DEPTH",
                format!("program import nesting exceeds the supported depth of {MAX_IMPORT_DEPTH}"),
                import_span
                    .cloned()
                    .unwrap_or_else(|| SourceSpan::file_start(path)),
            ));
        }
        require_clipasm_extension(path, import_span)?;
        let canonical =
            fs::canonicalize(path).map_err(|error| Diagnostic::io("E_SOURCE_IO", path, &error))?;
        if let Some(id) = self.loaded.get(&canonical).copied() {
            return Ok(id);
        }
        if let Some(start) = self.visiting_positions.get(&canonical).copied() {
            let mut cycle = self.visiting[start..]
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            cycle.push(canonical.display().to_string());
            return Err(Diagnostic::new(
                "E_PROGRAM_IMPORT_CYCLE",
                format!("program import cycle: {}", cycle.join(" -> ")),
                import_span
                    .cloned()
                    .unwrap_or_else(|| SourceSpan::file_start(&canonical)),
            ));
        }

        let text = fs::read_to_string(&canonical)
            .map_err(|error| Diagnostic::io("E_SOURCE_IO", &canonical, &error))?;
        let source = SourceFile::new(canonical.clone(), text);
        let syntax = parser::parse(source.clone())?;

        self.visiting_positions
            .insert(canonical.clone(), self.visiting.len());
        self.visiting.push(canonical.clone());

        let mut aliases = BTreeSet::new();
        let mut callables = self.builtins.clone();
        let mut import_targets = Vec::new();

        for declaration in &syntax.declarations {
            match declaration {
                Declaration::Import(import) => {
                    self.reserve_alias(&mut aliases, &import.alias.value, &import.alias.span)?;
                    let imported_path = resolve_from(&canonical, Path::new(&import.path.value));
                    let target =
                        self.load(&imported_path, false, Some(&import.path.span), depth + 1)?;
                    let shape = CallableShape::from_source(self.units[target.index()].program());
                    let replaced = callables.insert(import.alias.value.clone(), shape);
                    debug_assert!(replaced.is_none());
                    import_targets.push(target);
                }
                Declaration::Config(_)
                | Declaration::External(_)
                | Declaration::Input(_)
                | Declaration::Parameter(_) => {}
            }
        }

        let unit = lower_source(source, syntax, &callables)?;
        if matches!(
            unit.program.implementation(),
            SourceProgramImplementation::External(_)
        ) && let Some(import) = unit.imports.first()
        {
            return Err(Diagnostic::new(
                "E_EXTERNAL_WITH_IMPORTS",
                "an external program cannot import other programs; use a separate ClipAsm wrapper program for composition",
                import.alias.span.clone(),
            ));
        }
        if !is_root {
            if let Some(project) = &unit.project {
                return Err(Diagnostic::new(
                    "E_IMPORTED_PROJECT_SETTINGS",
                    "imported programs cannot declare video project settings",
                    project.span.clone(),
                ));
            }
            if let Some(output) = &unit.output {
                return Err(Diagnostic::new(
                    "E_IMPORTED_OUTPUT",
                    "imported programs cannot declare `config.output`; publication belongs to the root entrypoint",
                    output.span.clone(),
                ));
            }
        }

        debug_assert_eq!(unit.imports.len(), import_targets.len());
        let imports = unit
            .imports
            .into_iter()
            .zip(import_targets)
            .map(|(import, target)| ResolvedImport {
                alias: import.alias,
                target,
            })
            .collect();
        let popped = self.visiting.pop().expect("active source loading frame");
        debug_assert_eq!(popped, canonical);
        self.visiting_positions.remove(&canonical);

        let id = SourceUnitId(self.units.len());
        self.units.push(SourceUnit {
            source: unit.source,
            imports,
            project: unit.project,
            program: unit.program,
            output: unit.output,
        });
        self.loaded.insert(canonical, id);
        Ok(id)
    }

    fn reserve_alias(
        &self,
        aliases: &mut BTreeSet<String>,
        alias: &str,
        span: &SourceSpan,
    ) -> Result<()> {
        if !aliases.insert(alias.to_owned()) {
            return Err(Diagnostic::new(
                "E_DUPLICATE_PROGRAM_IMPORT",
                format!("duplicate program import alias `{alias}`"),
                span.clone(),
            ));
        }
        if self.builtins.contains_key(alias) {
            return Err(Diagnostic::new(
                "E_PROGRAM_IMPORT_COLLISION",
                format!("program import alias `{alias}` collides with a built-in program"),
                span.clone(),
            ));
        }
        if sugar::resolve(alias).is_some() {
            return Err(Diagnostic::new(
                "E_PROGRAM_IMPORT_COLLISION",
                format!("program import alias `{alias}` collides with language sugar"),
                span.clone(),
            ));
        }
        if !super::parser::accepts_invocation_name(alias) {
            return Err(Diagnostic::new(
                "E_PROGRAM_IMPORT_COLLISION",
                format!("program import alias `{alias}` is reserved by the language"),
                span.clone(),
            ));
        }
        Ok(())
    }
}

fn resolve_from(source: &Path, authored: &Path) -> PathBuf {
    if authored.is_absolute() {
        authored.to_path_buf()
    } else {
        source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(authored)
    }
}

fn require_clipasm_extension(path: &Path, span: Option<&SourceSpan>) -> Result<()> {
    if path.extension() == Some(OsStr::new("clipasm")) {
        return Ok(());
    }
    Err(Diagnostic::new(
        "E_SOURCE_EXTENSION",
        "ClipAsm source files must use the `.clipasm` extension",
        span.cloned()
            .unwrap_or_else(|| SourceSpan::file_start(path)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler;

    fn write(directory: &Path, name: &str, source: &str) -> PathBuf {
        let path = directory.join(name);
        fs::write(&path, source).expect("write source fixture");
        path
    }

    #[test]
    fn loads_and_compiles_transitive_native_imports() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "leaf.clipasm",
            "clipasm 1\ninput video: Video\nparam count: Integer\nrepeat($video, $count)\n",
        );
        write(
            directory.path(),
            "middle.clipasm",
            "clipasm 1\nimport \"leaf.clipasm\" as leaf\ninput video: Video\nparam count: Integer\nleaf($video, $count)\n",
        );
        let root = write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nconfig {\n  video {\n    width = 64\n    height = 64\n    fps = 10\n  }\n}\nimport \"middle.clipasm\" as middle\nimage(\"card.png\", 1s)\nmiddle(2)\n",
        );

        let package = parse_file(&root).expect("native package");
        assert_eq!(package.units().len(), 3);
        let compiled = compiler::compile(&package).expect("compiled package");
        assert_eq!(
            compiled.result_domain().expect("known domain").frames().0,
            20
        );
    }

    #[test]
    fn deduplicates_repeated_clipasm_import_paths() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "effect.clipasm",
            "clipasm 1\ninput video: Video\nrepeat($video, 1)\n",
        );
        write(
            directory.path(),
            "external.clipasm",
            "clipasm 1\ninput video: Video\nparam amount: Integer\nexternal {\n  command = \"./missing-script\"\n  semantic_version = 1\n  preserve = video\n}\n",
        );
        let root = write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"effect.clipasm\" as first\nimport \"./effect.clipasm\" as second\nimport \"external.clipasm\" as external_one\nimport \"./external.clipasm\" as external_two\nimage(\"card.png\", 1s)\nfirst\nsecond\nexternal_one(1)\nexternal_two(2)\n",
        );

        let package = parse_file(&root).expect("deduplicated package");
        assert_eq!(package.units().len(), 3);
        compiler::compile(&package).expect("compiled package");
    }

    #[test]
    fn reports_native_import_cycles_with_the_full_path() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let first = write(
            directory.path(),
            "first.clipasm",
            "clipasm 1\nimport \"second.clipasm\" as second\nsecond\n",
        );
        write(
            directory.path(),
            "second.clipasm",
            "clipasm 1\nimport \"first.clipasm\" as first\nfirst\n",
        );

        let error = parse_file(&first).expect_err("import cycle");
        assert_eq!(error.code, "E_PROGRAM_IMPORT_CYCLE");
        assert!(error.message.contains("first.clipasm"));
        assert!(error.message.contains("second.clipasm"));
    }

    #[test]
    fn rejects_duplicate_and_reserved_import_aliases() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(directory.path(), "empty.clipasm", "clipasm 1\n");
        let duplicate = write(
            directory.path(),
            "duplicate.clipasm",
            "clipasm 1\nimport \"empty.clipasm\" as effect\nimport \"empty.clipasm\" as effect\n",
        );
        let error = parse_file(&duplicate).expect_err("duplicate alias");
        assert_eq!(error.code, "E_DUPLICATE_PROGRAM_IMPORT");

        let builtin = write(
            directory.path(),
            "builtin.clipasm",
            "clipasm 1\nimport \"empty.clipasm\" as image\n",
        );
        let error = parse_file(&builtin).expect_err("built-in collision");
        assert_eq!(error.code, "E_PROGRAM_IMPORT_COLLISION");

        let sugar = write(
            directory.path(),
            "sugar.clipasm",
            "clipasm 1\nimport \"empty.clipasm\" as clip\n",
        );
        let error = parse_file(&sugar).expect_err("sugar collision");
        assert_eq!(error.code, "E_PROGRAM_IMPORT_COLLISION");

        let declaration = write(
            directory.path(),
            "declaration.clipasm",
            "clipasm 1\nimport \"empty.clipasm\" as input\n",
        );
        let error = parse_file(&declaration).expect_err("language reservation");
        assert_eq!(error.code, "E_PROGRAM_IMPORT_COLLISION");
        assert!(error.message.contains("reserved by the language"));
    }

    #[test]
    fn rejects_root_only_settings_in_imported_programs() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "project.clipasm",
            "clipasm 1\nconfig {\n  video {\n    width = 64\n  }\n}\n",
        );
        let root = write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"project.clipasm\" as project\n",
        );
        let error = parse_file(&root).expect_err("imported project settings");
        assert_eq!(error.code, "E_IMPORTED_PROJECT_SETTINGS");

        write(
            directory.path(),
            "output.clipasm",
            "clipasm 1\nconfig {\n  output = \"result.mp4\"\n}\n",
        );
        let root = write(
            directory.path(),
            "output-root.clipasm",
            "clipasm 1\nimport \"output.clipasm\" as output\n",
        );
        let error = parse_file(&root).expect_err("imported output");
        assert_eq!(error.code, "E_IMPORTED_OUTPUT");
    }

    #[test]
    fn loads_external_clipasm_programs_for_native_calls() {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "effect.clipasm",
            "clipasm 1\ninput video: Video\nparam amount: Integer\nexternal {\n  command = \"./missing-script\"\n  semantic_version = 1\n  preserve = video\n}\n",
        );
        let root = write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"effect.clipasm\" as effect\nimage(\"card.png\", 1s)\neffect(12)\n",
        );

        let package = parse_file(&root).expect("external package");
        let compiled = compiler::compile(&package).expect("compiled external package");
        let document: serde_json::Value =
            serde_json::from_str(&compiled.compiled_json().expect("compiled JSON"))
                .expect("JSON document");
        assert_eq!(document["nodes"][1]["kind"]["operation"], "external_video");
        assert_eq!(document["nodes"][1]["kind"]["parameters"]["amount"], 12);
    }

    #[test]
    fn requires_clipasm_extensions_for_root_and_imported_sources() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = write(directory.path(), "root.txt", "clipasm 1\n");
        let error = parse_file(&root).expect_err("root extension");
        assert_eq!(error.code, "E_SOURCE_EXTENSION");

        write(directory.path(), "child.txt", "clipasm 1\n");
        let root = write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"child.txt\" as child\n",
        );
        let error = parse_file(&root).expect_err("import extension");
        assert_eq!(error.code, "E_SOURCE_EXTENSION");
        assert!(error.span.file().ends_with("root.clipasm"));
    }
}
