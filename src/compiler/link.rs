use crate::diagnostic::{Diagnostic, Result};
use crate::source::{SourcePackage, SourceSpan, SourceUnitId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Complete,
}

struct VisitFrame {
    unit: SourceUnitId,
    next_import: usize,
}

pub(super) fn source_unit_order(package: &SourcePackage) -> Result<Vec<SourceUnitId>> {
    let unit_count = package.units().len();
    if package.root.index() >= unit_count {
        return Err(invalid_root(package));
    }

    let mut states = vec![VisitState::Unvisited; unit_count];
    let mut positions = vec![None; unit_count];
    let mut path = Vec::<SourceUnitId>::new();
    let mut order = Vec::with_capacity(unit_count);

    for index in 0..unit_count {
        let root = SourceUnitId(index);
        if states[index] != VisitState::Unvisited {
            continue;
        }

        states[index] = VisitState::Visiting;
        positions[index] = Some(path.len());
        path.push(root);
        let mut stack = vec![VisitFrame {
            unit: root,
            next_import: 0,
        }];

        while let Some(frame) = stack.last_mut() {
            let unit = &package.units()[frame.unit.index()];
            let Some(import) = unit.imports.get(frame.next_import) else {
                let completed = stack.pop().expect("active source-unit visit frame");
                let popped = path.pop().expect("active source-unit visit path");
                debug_assert_eq!(popped, completed.unit);
                positions[completed.unit.index()] = None;
                states[completed.unit.index()] = VisitState::Complete;
                order.push(completed.unit);
                continue;
            };
            frame.next_import += 1;

            let target = import.target;
            if target.index() >= unit_count {
                return Err(Diagnostic::new(
                    "E_INTERNAL_PROGRAM_LINK",
                    format!(
                        "import `{}` refers to missing source unit {}",
                        import.alias.value,
                        target.index()
                    ),
                    import.alias.span.clone(),
                ));
            }

            match states[target.index()] {
                VisitState::Unvisited => {
                    states[target.index()] = VisitState::Visiting;
                    positions[target.index()] = Some(path.len());
                    path.push(target);
                    stack.push(VisitFrame {
                        unit: target,
                        next_import: 0,
                    });
                }
                VisitState::Visiting => {
                    let start = positions[target.index()]
                        .expect("visiting source unit has an active path position");
                    let mut cycle = path[start..]
                        .iter()
                        .map(|unit| unit_label(package, *unit))
                        .collect::<Vec<_>>();
                    cycle.push(unit_label(package, target));
                    return Err(Diagnostic::new(
                        "E_PROGRAM_IMPORT_CYCLE",
                        format!("program import cycle: {}", cycle.join(" -> ")),
                        import.alias.span.clone(),
                    ));
                }
                VisitState::Complete => {}
            }
        }
    }

    Ok(order)
}

fn invalid_root(package: &SourcePackage) -> Diagnostic {
    let span = package.units().first().map_or_else(
        || SourceSpan::file_start("<source-package>"),
        |unit| SourceSpan::source_start(unit.source().clone()),
    );
    Diagnostic::new(
        "E_INTERNAL_PROGRAM_LINK",
        format!(
            "source package root refers to missing source unit {}",
            package.root.index()
        ),
        span,
    )
}

fn unit_label(package: &SourcePackage, unit: SourceUnitId) -> String {
    package.units()[unit.index()]
        .source()
        .display_path()
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::source::{ResolvedImport, Spanned};

    fn write(directory: &Path, name: &str, source: &str) {
        fs::write(directory.join(name), source).expect("write source fixture");
    }

    fn chain_package() -> SourcePackage {
        let directory = tempfile::tempdir().expect("temporary directory");
        write(
            directory.path(),
            "leaf.clipasm",
            "clipasm 1\ninput video: Video\nrepeat($video, 2)\n",
        );
        write(
            directory.path(),
            "middle.clipasm",
            "clipasm 1\nimport \"leaf.clipasm\" as leaf\ninput video: Video\nleaf($video)\n",
        );
        write(
            directory.path(),
            "root.clipasm",
            "clipasm 1\nimport \"middle.clipasm\" as middle\nimage(\"missing.ppm\", 1s)\nmiddle\n",
        );
        crate::language::parse_file(&directory.path().join("root.clipasm"))
            .expect("linked source package")
    }

    fn reorder(package: &SourcePackage, old_indices: &[usize]) -> SourcePackage {
        assert_eq!(old_indices.len(), package.units.len());
        let mut old_to_new = vec![usize::MAX; package.units.len()];
        for (new, old) in old_indices.iter().copied().enumerate() {
            assert!(old < package.units.len());
            assert_eq!(old_to_new[old], usize::MAX);
            old_to_new[old] = new;
        }

        let units = old_indices
            .iter()
            .map(|old| {
                let mut unit = package.units[*old].clone();
                for import in &mut unit.imports {
                    import.target = SourceUnitId(old_to_new[import.target.index()]);
                }
                unit
            })
            .collect();
        SourcePackage {
            root: SourceUnitId(old_to_new[package.root.index()]),
            units,
            external_programs: package.external_programs.clone(),
        }
    }

    #[test]
    fn dependency_order_is_independent_of_unit_storage() {
        let ordered = chain_package();
        assert_eq!(
            source_unit_order(&ordered).expect("ordered package"),
            vec![SourceUnitId(0), SourceUnitId(1), SourceUnitId(2)]
        );

        let root_first = reorder(&ordered, &[2, 1, 0]);
        assert_eq!(
            source_unit_order(&root_first).expect("root-first package"),
            vec![SourceUnitId(2), SourceUnitId(1), SourceUnitId(0)]
        );

        let ordered = crate::compiler::compile(&ordered).expect("ordered compile");
        let root_first = crate::compiler::compile(&root_first).expect("root-first compile");
        assert_eq!(ordered.structure_hash(), root_first.structure_hash());
        assert_eq!(
            ordered.canonical_json().expect("ordered JSON"),
            root_first.canonical_json().expect("root-first JSON")
        );
    }

    #[test]
    fn compiler_checks_disconnected_source_units() {
        let mut package = chain_package();
        let disconnected = crate::language::parse_str(
            Path::new("disconnected.clipasm"),
            "clipasm 1\nparam count: Integer = nope\nimage(\"unused.ppm\", 1s)\n",
        )
        .expect("disconnected source unit");
        package.units.push(disconnected.root().clone());

        let error = crate::compiler::compile(&package).expect_err("invalid disconnected program");
        assert_eq!(error.code, "E_INVALID_ARGUMENT_TYPE");
        assert!(error.span.file().ends_with("disconnected.clipasm"));
    }

    #[test]
    fn compiler_rejects_a_linked_package_cycle() {
        let mut package = chain_package();
        let root = package.root;
        let leaf = SourceUnitId(0);
        let span = SourceSpan::source_start(package.units[leaf.index()].source().clone());
        package.units[leaf.index()].imports.push(ResolvedImport {
            alias: Spanned::new("root".to_owned(), span),
            target: root,
        });

        let error = crate::compiler::compile(&package).expect_err("cyclic linked package");
        assert_eq!(error.code, "E_PROGRAM_IMPORT_CYCLE");
        assert!(error.message.contains("leaf.clipasm"));
        assert!(error.message.contains("middle.clipasm"));
        assert!(error.message.contains("root.clipasm"));
    }

    #[test]
    fn compiler_rejects_missing_source_unit_targets() {
        let mut package = chain_package();
        package.units[package.root.index()].imports[0].target = SourceUnitId(99);

        let error = crate::compiler::compile(&package).expect_err("missing import target");
        assert_eq!(error.code, "E_INTERNAL_PROGRAM_LINK");
        assert!(error.message.contains("missing source unit 99"));
    }

    #[test]
    fn compiler_rejects_a_missing_root_source_unit() {
        let mut package = chain_package();
        package.root = SourceUnitId(99);

        let error = crate::compiler::compile(&package).expect_err("missing package root");
        assert_eq!(error.code, "E_INTERNAL_PROGRAM_LINK");
        assert!(error.message.contains("missing source unit 99"));
    }
}
