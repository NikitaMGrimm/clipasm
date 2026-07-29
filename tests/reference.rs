#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::path::Path;

use clipasm::reference::{
    BuiltinCategory, DiagnosticCategory, builtin_programs, diagnostic, diagnostics,
};

#[test]
fn every_builtin_reference_example_compiles_and_uses_its_program() {
    let programs = builtin_programs();

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

#[test]
fn diagnostic_catalog_is_complete_well_formed_and_searchable() {
    let references = diagnostics();

    let mut codes = BTreeSet::new();
    let mut categories = BTreeSet::new();
    let mut previous_code = None;
    for reference in references {
        let code = reference.code();
        assert!(codes.insert(code), "duplicate diagnostic code {code}");
        assert!(
            code.starts_with("E_")
                && code
                    .bytes()
                    .all(|byte| byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit()),
            "invalid diagnostic code {code}"
        );
        if let Some(previous_code) = previous_code {
            assert!(previous_code < code, "catalog is not ordered at {code}");
        }
        previous_code = Some(code);

        assert!(!reference.title().trim().is_empty(), "{code}");
        assert!(!reference.summary().trim().is_empty(), "{code}");
        assert!(!reference.common_causes().is_empty(), "{code}");
        assert!(!reference.recommended_actions().is_empty(), "{code}");
        assert!(
            !reference.retry_guidance().explanation().is_empty(),
            "{code}"
        );
        assert!(
            reference
                .documentation_route()
                .starts_with("diagnostics/index.html#"),
            "{code}"
        );
        assert_eq!(reference.documentation_anchor(), code.to_ascii_lowercase());
        assert_eq!(diagnostic(code), Some(reference));
        assert_eq!(reference.diagnostic().code(), code);
        categories.insert(reference.category());
    }

    assert_eq!(
        categories,
        DiagnosticCategory::ALL.into_iter().collect::<BTreeSet<_>>()
    );
    assert!(diagnostic("E_NOT_A_CLIPASM_DIAGNOSTIC").is_none());
}

#[test]
fn diagnostic_reference_metadata_does_not_affect_compiled_identity() {
    let source = "clipasm 1\nimage(\"card.ppm\", 1s)\n";
    let package = clipasm::language::parse_str(Path::new("identity.clipasm"), source)
        .expect("reference identity source parses");
    let before = clipasm::compiler::compile(&package).expect("first compile");
    let before_hash = before.structure_hash().to_owned();
    let before_json = before.compiled_json().expect("first compiled JSON");

    let metadata_prose = diagnostics()
        .iter()
        .flat_map(|reference| {
            [
                reference.title(),
                reference.summary(),
                reference.retry_guidance().explanation(),
            ]
        })
        .collect::<Vec<_>>();
    assert!(metadata_prose.iter().all(|prose| !prose.trim().is_empty()));

    let after = clipasm::compiler::compile(&package).expect("second compile");
    let after_json = after.compiled_json().expect("second compiled JSON");
    assert_eq!(after.structure_hash(), before_hash);
    assert_eq!(after_json, before_json);
    for prose in metadata_prose {
        assert!(
            !after_json.contains(prose),
            "diagnostic reference prose leaked into compiled JSON"
        );
    }
}

#[test]
fn machine_contract_catalog_matches_the_documented_versions() {
    use clipasm::reference::{
        MachineContractAudience, MachineContractStability, machine_contracts,
    };

    let contracts = machine_contracts();
    assert_eq!(contracts.len(), 5);
    assert_eq!(contracts[0].title(), "Compiled inspection JSON");
    assert_eq!(contracts[0].versions()[0].value(), 22);
    assert_eq!(contracts[1].versions()[0].value(), 1);
    assert_eq!(contracts[2].versions()[0].value(), 1);
    assert_eq!(contracts[3].versions()[0].value(), 13);
    assert_eq!(contracts[4].versions()[0].value(), 1);
    assert_eq!(contracts[4].versions()[1].value(), 2);

    assert_eq!(
        contracts[0].stability(),
        MachineContractStability::Versioned
    );
    assert_eq!(
        contracts[2].audience(),
        MachineContractAudience::ExternalPrograms
    );
    assert_eq!(
        contracts[3].stability(),
        MachineContractStability::HostInternal
    );

    let page = include_str!("../docs/reference/machine-contracts.md");
    for contract in contracts {
        assert!(
            page.contains(contract.title()),
            "missing {}",
            contract.title()
        );
        for version in contract.versions() {
            assert!(
                page.contains(&format!("{}: {}", version.field(), version.value())),
                "missing documented {} version {}",
                contract.title(),
                version.value()
            );
        }
    }
    assert!(page.contains("Cache entry metadata"));
    assert!(page.contains("Private implementation detail"));
}
