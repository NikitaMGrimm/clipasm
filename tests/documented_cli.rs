#![allow(missing_docs)]

#[test]
fn documented_cli_transcripts_are_current() {
    let cases = trycmd::TestCases::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for page in [
        "docs/reference/cli.md",
        "docs/guides/root-inputs-and-parameters.md",
        "docs/guides/import-a-program.md",
    ] {
        cases.case(root.join(page));
    }
    cases.run();
}
