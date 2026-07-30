use std::fs;
use std::path::Path;

use clipasm::compiler;
use serde_json::Value;
use tempfile::TempDir;

pub(crate) fn compile_file(path: &Path) -> clipasm::diagnostic::Result<compiler::CompiledProgram> {
    let source = clipasm::language::parse_file(path)?;
    compiler::compile(&source)
}

pub(crate) fn project(source: &str) -> (TempDir, clipasm::source::SourcePackage) {
    let directory = tempfile::tempdir().expect("temporary directory");
    fs::write(directory.path().join("a.ppm"), b"P3\n1 1\n255\n255 0 0\n").expect("a image");
    fs::write(directory.path().join("b.ppm"), b"P3\n1 1\n255\n0 255 0\n").expect("b image");
    fs::write(directory.path().join("c.ppm"), b"P3\n1 1\n255\n0 0 255\n").expect("c image");
    fs::write(directory.path().join("x.ppm"), b"P3\n1 1\n255\n255 255 0\n").expect("x image");
    fs::write(directory.path().join("y.ppm"), b"P3\n1 1\n255\n0 255 255\n").expect("y image");
    let path = directory.path().join("workflow.clipasm");
    fs::write(&path, source).expect("workflow");
    let workflow = clipasm::language::parse_file(&path).expect("parse workflow");
    (directory, workflow)
}

pub(crate) struct CompiledDocument {
    value: Value,
}

#[derive(Clone, Copy)]
pub(crate) struct CompiledOperation<'a> {
    node: &'a Value,
}

pub(crate) fn compiled_document(compiled: &compiler::CompiledProgram) -> CompiledDocument {
    CompiledDocument {
        value: serde_json::from_str(&compiled.compiled_json().expect("compiled JSON"))
            .expect("JSON value"),
    }
}

impl CompiledDocument {
    fn nodes(&self) -> &[Value] {
        self.value["nodes"].as_array().expect("compiled nodes")
    }

    pub(crate) fn operation_names(&self) -> Vec<&str> {
        self.nodes()
            .iter()
            .map(|node| {
                node["kind"]["operation"]
                    .as_str()
                    .expect("compiled operation name")
            })
            .collect()
    }

    pub(crate) fn has_operation(&self, operation: &str) -> bool {
        self.operations(operation).next().is_some()
    }

    pub(crate) fn operation_count(&self, operation: &str) -> usize {
        self.operations(operation).count()
    }

    pub(crate) fn typed_operation_count(&self, value_type: &str, operation: &str) -> usize {
        self.operations(operation)
            .filter(|node| node.value_type() == value_type)
            .count()
    }

    pub(crate) fn has_typed_operation_range(
        &self,
        value_type: &str,
        operation: &str,
        range: (u64, u64),
    ) -> bool {
        self.operations(operation)
            .any(|node| node.value_type() == value_type && node.range() == range)
    }

    pub(crate) fn operation(&self, operation: &str) -> CompiledOperation<'_> {
        self.operations(operation)
            .next()
            .unwrap_or_else(|| panic!("missing compiled `{operation}` operation"))
    }

    pub(crate) fn last_operation(&self, operation: &str) -> CompiledOperation<'_> {
        self.operations(operation)
            .next_back()
            .unwrap_or_else(|| panic!("missing compiled `{operation}` operation"))
    }

    pub(crate) fn typed_operation(
        &self,
        value_type: &str,
        operation: &str,
    ) -> CompiledOperation<'_> {
        self.operations(operation)
            .find(|node| node.value_type() == value_type)
            .unwrap_or_else(|| panic!("missing compiled `{value_type}` `{operation}` operation"))
    }

    pub(crate) fn operation_for_construct(&self, construct: &str) -> CompiledOperation<'_> {
        self.nodes()
            .iter()
            .map(|node| CompiledOperation { node })
            .find(|node| node.construct() == construct)
            .unwrap_or_else(|| panic!("missing compiled `{construct}` construct"))
    }

    pub(crate) fn operation_for_construct_named(
        &self,
        construct: &str,
        operation: &str,
    ) -> CompiledOperation<'_> {
        self.nodes()
            .iter()
            .map(|node| CompiledOperation { node })
            .find(|node| node.construct() == construct && node.name() == operation)
            .unwrap_or_else(|| {
                panic!("missing compiled `{construct}` construct with `{operation}` operation")
            })
    }

    pub(crate) fn last_node(&self) -> CompiledOperation<'_> {
        CompiledOperation {
            node: self.nodes().last().expect("compiled result node"),
        }
    }

    pub(crate) fn has_named_value(&self, name: &str) -> bool {
        self.value["named_values"][name].is_object()
    }

    pub(crate) fn named_value(&self, name: &str) -> CompiledOperation<'_> {
        let id = self.value["named_values"][name]["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("missing compiled named value `{name}`"));
        let index = usize::try_from(id).expect("compiled node id fits usize");
        CompiledOperation {
            node: self
                .nodes()
                .get(index)
                .unwrap_or_else(|| panic!("named value `{name}` references missing node {id}")),
        }
    }

    pub(crate) fn operations(
        &self,
        operation: &str,
    ) -> impl DoubleEndedIterator<Item = CompiledOperation<'_>> {
        self.nodes()
            .iter()
            .map(|node| CompiledOperation { node })
            .filter(move |node| node.name() == operation)
    }
}

impl<'a> CompiledOperation<'a> {
    pub(crate) fn name(self) -> &'a str {
        self.node["kind"]["operation"]
            .as_str()
            .expect("compiled operation name")
    }

    pub(crate) fn construct(self) -> &'a str {
        self.node["origin"]["construct"]
            .as_str()
            .expect("compiled origin construct")
    }

    pub(crate) fn value_type(self) -> &'a str {
        self.node["value_type"]
            .as_str()
            .expect("compiled value type")
    }

    pub(crate) fn string_parameter(self, name: &str) -> &'a str {
        self.node["kind"][name].as_str().unwrap_or_else(|| {
            panic!(
                "compiled `{}` operation has no string `{name}`",
                self.name()
            )
        })
    }

    pub(crate) fn integer_parameter(self, name: &str) -> u64 {
        self.node["kind"][name].as_u64().unwrap_or_else(|| {
            panic!(
                "compiled `{}` operation has no integer `{name}`",
                self.name()
            )
        })
    }

    pub(crate) fn has_input(self, name: &str) -> bool {
        self.node["kind"][name].is_object()
    }

    pub(crate) fn input_id(self, name: &str) -> u64 {
        self.node["kind"][name]["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("compiled `{}` operation has no `{name}` input", self.name()))
    }

    pub(crate) fn input_count(self) -> usize {
        self.node["kind"]["inputs"]
            .as_array()
            .unwrap_or_else(|| panic!("compiled `{}` operation has no input list", self.name()))
            .len()
    }

    pub(crate) fn domain_frames(self) -> u64 {
        self.node["domain"]["frames"]
            .as_u64()
            .expect("compiled frame domain")
    }

    pub(crate) fn domain_width(self) -> u64 {
        self.node["domain"]["width"]
            .as_u64()
            .expect("compiled domain width")
    }

    pub(crate) fn domain_height(self) -> u64 {
        self.node["domain"]["height"]
            .as_u64()
            .expect("compiled domain height")
    }

    pub(crate) fn range(self) -> (u64, u64) {
        (
            self.node["kind"]["range"]["start"]
                .as_u64()
                .expect("compiled range start"),
            self.node["kind"]["range"]["end"]
                .as_u64()
                .expect("compiled range end"),
        )
    }

    pub(crate) fn range_project_frame_offsets(self) -> (&'a str, &'a str) {
        (
            self.node["kind"]["range"]["start"]["project_frames"]
                .as_str()
                .expect("compiled start project-frame offset"),
            self.node["kind"]["range"]["end"]["project_frames"]
                .as_str()
                .expect("compiled end project-frame offset"),
        )
    }

    pub(crate) fn range_term_counts(self) -> (usize, usize) {
        (
            self.node["kind"]["range"]["start"]["terms"]
                .as_array()
                .expect("compiled start range terms")
                .len(),
            self.node["kind"]["range"]["end"]["terms"]
                .as_array()
                .expect("compiled end range terms")
                .len(),
        )
    }
}

pub(crate) fn assert_last_slice_range(compiled: &compiler::CompiledProgram, start: u64, end: u64) {
    let document = compiled_document(compiled);
    let slice = document
        .operations("slice")
        .rev()
        .find(|node| node.value_type() == "video" && node.string_parameter("unit") == "frames")
        .expect("trim slice");
    assert_eq!(slice.range(), (start, end));
}

pub(crate) fn assert_last_audio_slice_range(
    compiled: &compiler::CompiledProgram,
    start: u64,
    end: u64,
) {
    let document = compiled_document(compiled);
    let slice = document
        .operations("slice")
        .rev()
        .find(|node| node.value_type() == "audio" && node.string_parameter("unit") == "samples")
        .expect("audio trim slice");
    assert_eq!(slice.range(), (start, end));
}
