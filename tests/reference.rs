#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::path::Path;

use clipasm::reference::{BuiltinCategory, builtin_programs};

#[test]
fn every_builtin_reference_example_compiles_and_uses_its_program() {
    let programs = builtin_programs();
    assert_eq!(programs.len(), 14);

    let mut names = BTreeSet::new();
    let mut routes = BTreeSet::new();
    let mut categories = BTreeSet::new();
    for program in programs {
        assert!(names.insert(program.name().to_owned()));
        assert!(routes.insert(program.documentation_route()));
        assert!(!program.summary().trim().is_empty());
        categories.insert(program.category());

        let source = format!("clipasm 1\n\n{}\n", program.example().trim());
        let package = clipasm::language::parse_str(
            Path::new(&format!("{}-reference.clipasm", program.name())),
            &source,
        )
        .unwrap_or_else(|error| panic!("{} example did not parse: {error}", program.name()));
        let compiled = clipasm::compiler::compile(&package)
            .unwrap_or_else(|error| panic!("{} example did not compile: {error}", program.name()));
        assert!(
            compiled
                .explain()
                .iter()
                .any(|entry| entry.construct() == program.name()),
            "{} example did not invoke its documented program",
            program.name()
        );
        let expectation = program.example_expectation();
        let actual_outputs = compiled
            .outputs()
            .iter()
            .map(|output| output.value_type())
            .collect::<Vec<_>>();
        assert_eq!(actual_outputs, expectation.outputs(), "{}", program.name());
        if let Some(expected_frames) = expectation.expected_frames() {
            assert_eq!(
                compiled
                    .result_domain()
                    .unwrap_or_else(|| {
                        panic!("{} result domain was unexpectedly deferred", program.name())
                    })
                    .frames()
                    .0,
                expected_frames,
                "{}",
                program.name()
            );
        }
    }

    assert_eq!(
        categories,
        BuiltinCategory::ALL.into_iter().collect::<BTreeSet<_>>()
    );
}
