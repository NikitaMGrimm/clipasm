use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::language::Language;
use crate::diagnostic::{Diagnostic, Result};
use crate::external::{ExternalProgram, ExternalProgramId, load_manifest};
const MAX_IMPORT_DEPTH: usize = 128;

use crate::source::{
    ResolvedExternalImport, ResolvedImport, SourceFile, SourcePackage, SourceSpan, SourceUnit,
    SourceUnitId,
};

/// Parse and link a YAML source package rooted at `path`.
///
/// # Errors
///
/// Returns a source-located diagnostic when a source cannot be read, violates
/// the YAML frontend grammar, uses root-only settings in an import, or forms an
/// import cycle.
pub fn parse_file(path: &Path) -> Result<SourcePackage> {
    let language = Language::default();
    let mut loader = Loader {
        language: &language,
        units: Vec::new(),
        external_programs: Vec::new(),
        loaded_externals: BTreeMap::new(),
        loaded: BTreeMap::new(),
        visiting: Vec::new(),
        visiting_positions: BTreeMap::new(),
    };
    let root = loader.load(path, true, None, 0)?;
    Ok(SourcePackage {
        root,
        units: loader.units,
        external_programs: loader.external_programs,
    })
}

struct Loader<'a> {
    language: &'a Language,
    units: Vec<SourceUnit>,
    external_programs: Vec<ExternalProgram>,
    loaded_externals: BTreeMap<PathBuf, ExternalProgramId>,
    loaded: BTreeMap<PathBuf, SourceUnitId>,
    visiting: Vec<PathBuf>,
    visiting_positions: BTreeMap<PathBuf, usize>,
}

impl Loader<'_> {
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
        let canonical = fs::canonicalize(path)
            .map_err(|error| Diagnostic::io("E_WORKFLOW_IO", path, &error))?;
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
            .map_err(|error| Diagnostic::io("E_WORKFLOW_IO", &canonical, &error))?;
        let source = SourceFile::new(canonical.clone(), text);
        let unit = super::lower::parse_source_with_language(source, self.language)?;
        if !is_root {
            if let Some(project) = &unit.project {
                return Err(Diagnostic::new(
                    "E_IMPORTED_PROJECT_SETTINGS",
                    "imported programs cannot declare `project`; project settings belong to the root entrypoint",
                    project.span.clone(),
                ));
            }
            if let Some(output) = &unit.output {
                return Err(Diagnostic::new(
                    "E_IMPORTED_OUTPUT",
                    "imported programs cannot declare `output`; publication belongs to the root entrypoint",
                    output.span.clone(),
                ));
            }
        }

        self.visiting_positions
            .insert(canonical.clone(), self.visiting.len());
        self.visiting.push(canonical.clone());

        let mut aliases = BTreeSet::new();
        let mut externals = Vec::with_capacity(unit.externals.len());
        for external in &unit.externals {
            if !aliases.insert(external.alias.value.clone()) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_PROGRAM_IMPORT",
                    format!("duplicate program import alias `{}`", external.alias.value),
                    external.alias.span.clone(),
                ));
            }
            if self.language.programs.get(&external.alias.value).is_some() {
                return Err(Diagnostic::new(
                    "E_PROGRAM_IMPORT_COLLISION",
                    format!(
                        "external program alias `{}` collides with a built-in program",
                        external.alias.value
                    ),
                    external.alias.span.clone(),
                ));
            }
            let manifest_path = if external.path.value.is_absolute() {
                external.path.value.clone()
            } else {
                canonical
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(&external.path.value)
            };
            let manifest_path = fs::canonicalize(&manifest_path).map_err(|error| {
                Diagnostic::io("E_EXTERNAL_MANIFEST_IO", &manifest_path, &error)
            })?;
            let target = if let Some(id) = self.loaded_externals.get(&manifest_path).copied() {
                id
            } else {
                let program = load_manifest(&manifest_path)?;
                let id = ExternalProgramId::new(
                    u32::try_from(self.external_programs.len())
                        .expect("external program catalog fits in u32"),
                );
                self.external_programs.push(program);
                self.loaded_externals.insert(manifest_path, id);
                id
            };
            externals.push(ResolvedExternalImport {
                alias: external.alias.clone(),
                target,
            });
        }

        let mut imports = Vec::with_capacity(unit.imports.len());
        for import in &unit.imports {
            if !aliases.insert(import.alias.value.clone()) {
                return Err(Diagnostic::new(
                    "E_DUPLICATE_PROGRAM_IMPORT",
                    format!("duplicate program import alias `{}`", import.alias.value),
                    import.alias.span.clone(),
                ));
            }
            if self.language.programs.get(&import.alias.value).is_some() {
                return Err(Diagnostic::new(
                    "E_PROGRAM_IMPORT_COLLISION",
                    format!(
                        "program import alias `{}` collides with a built-in program",
                        import.alias.value
                    ),
                    import.alias.span.clone(),
                ));
            }
            let imported_path = if import.path.value.is_absolute() {
                import.path.value.clone()
            } else {
                canonical
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(&import.path.value)
            };
            let target = self.load(&imported_path, false, Some(&import.path.span), depth + 1)?;
            imports.push(ResolvedImport {
                alias: import.alias.clone(),
                target,
            });
        }

        let popped = self.visiting.pop().expect("active source loading frame");
        debug_assert_eq!(popped, canonical);
        self.visiting_positions.remove(&canonical);

        let id = SourceUnitId(self.units.len());
        self.units.push(SourceUnit {
            source: unit.source,
            imports,
            externals,
            project: unit.project,
            program: Arc::new(unit.program),
            output: unit.output,
        });
        self.loaded.insert(canonical, id);
        Ok(id)
    }
}
