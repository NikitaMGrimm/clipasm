//! Browser-facing adapter for `ClipAsm`'s media-pure compiler.

use std::path::Path;

use serde::Serialize;
use wasm_bindgen::prelude::wasm_bindgen;

const RESPONSE_VERSION: u32 = 2;
const SOURCE_NAME: &str = "playground.clipasm";

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum CompileResponse {
    Success {
        version: u32,
        value_count: usize,
        outputs: Vec<clipasm::model::ValueType>,
        frames: Option<u64>,
        structure_hash: String,
        compiled_json: String,
        render: RenderAvailability,
    },
    Error {
        version: u32,
        diagnostic: DiagnosticResponse,
    },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum RenderAvailability {
    Ready {
        assets: Vec<clipasm::preflight::browser::BrowserAssetRequest>,
    },
    Unsupported {
        diagnostic: DiagnosticResponse,
    },
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum RenderPlanResponse {
    Success {
        version: u32,
        plan_json: String,
    },
    Error {
        version: u32,
        diagnostic: DiagnosticResponse,
    },
}

#[derive(Serialize)]
struct DiagnosticResponse {
    code: &'static str,
    message: String,
    file: String,
    line: usize,
    column: usize,
    rendered: String,
    notes: Vec<String>,
}

impl From<clipasm::diagnostic::Diagnostic> for DiagnosticResponse {
    fn from(diagnostic: clipasm::diagnostic::Diagnostic) -> Self {
        Self {
            code: diagnostic.code,
            message: diagnostic.message.clone(),
            file: diagnostic.span.file().display().to_string(),
            line: diagnostic.span.line,
            column: diagnostic.span.column,
            rendered: diagnostic.to_string(),
            notes: diagnostic.notes,
        }
    }
}

fn compile(source: &str) -> CompileResponse {
    let result = (|| {
        let compiled = compile_program(source)?;
        let document = compiled.compiled_json()?;
        Ok::<_, clipasm::diagnostic::Diagnostic>((compiled, document))
    })();

    match result {
        Ok((compiled, document)) => {
            let render = match clipasm::preflight::browser::required_assets(&compiled) {
                Ok(assets) => RenderAvailability::Ready { assets },
                Err(diagnostic) => RenderAvailability::Unsupported {
                    diagnostic: diagnostic.into(),
                },
            };
            CompileResponse::Success {
                version: RESPONSE_VERSION,
                value_count: compiled.value_count(),
                outputs: compiled
                    .outputs()
                    .iter()
                    .map(|output| output.value_type())
                    .collect(),
                frames: compiled.result_domain().map(|domain| domain.frames().0),
                structure_hash: compiled.structure_hash().to_owned(),
                compiled_json: document,
                render,
            }
        }
        Err(diagnostic) => CompileResponse::Error {
            version: RESPONSE_VERSION,
            diagnostic: diagnostic.into(),
        },
    }
}

fn compile_program(
    source: &str,
) -> Result<clipasm::compiler::CompiledProgram, clipasm::diagnostic::Diagnostic> {
    let package = clipasm::language::parse_str(Path::new(SOURCE_NAME), source)?;
    clipasm::compiler::compile(&package)
}

/// Compile one in-memory `ClipAsm` source and return a versioned JSON response.
///
/// Compilation does not read media or invoke external processes. File-backed
/// imports are rejected because the browser adapter accepts a single source.
///
/// # Panics
///
/// Panics only if the fixed response schema cannot be serialized.
#[wasm_bindgen(js_name = compileSource)]
#[must_use]
pub fn compile_source(source: &str) -> String {
    serde_json::to_string(&compile(source)).expect("playground response must serialize")
}

/// Prepare versioned browser render recipes for supplied virtual asset facts.
///
/// `assets_json` must be an array of objects containing `path` and
/// `content_hash`. The function does not open media or invoke external code.
///
/// # Panics
///
/// Panics only if the fixed response schema cannot be serialized.
#[wasm_bindgen(js_name = prepareRender)]
#[must_use]
pub fn prepare_render(source: &str, assets_json: &str) -> String {
    let result = (|| {
        let assets =
            serde_json::from_str::<Vec<clipasm::preflight::browser::BrowserAsset>>(assets_json)
                .map_err(|error| {
                    clipasm::diagnostic::Diagnostic::new(
                        "E_BROWSER_ASSET_FACTS",
                        format!("invalid browser asset facts: {error}"),
                        clipasm::source::SourceSpan::file_start(SOURCE_NAME),
                    )
                })?;
        let compiled = compile_program(source)?;
        let prepared = clipasm::preflight::browser::prepare(&compiled, &assets)?;
        prepared.render_json()
    })();
    let response = match result {
        Ok(plan_json) => RenderPlanResponse::Success {
            version: RESPONSE_VERSION,
            plan_json,
        },
        Err(diagnostic) => RenderPlanResponse::Error {
            version: RESPONSE_VERSION,
            diagnostic: diagnostic.into(),
        },
    };
    serde_json::to_string(&response).expect("playground response must serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(source: &str) -> serde_json::Value {
        serde_json::from_str(&compile_source(source)).expect("response JSON")
    }

    #[test]
    fn compiles_the_scenic_sequence_without_reading_assets() {
        let response = response(include_str!("../../examples/scenic-sequence.clipasm"));

        assert_eq!(response["status"], "success");
        assert_eq!(response["value_count"], 4);
        assert_eq!(response["outputs"], serde_json::json!(["video"]));
        assert_eq!(response["frames"], 108);
        assert_eq!(response["render"]["status"], "ready");
        assert_eq!(
            response["render"]["assets"].as_array().map(Vec::len),
            Some(3)
        );
        assert!(response["structure_hash"].as_str().is_some());
        let compiled: serde_json::Value =
            serde_json::from_str(response["compiled_json"].as_str().expect("compiled JSON"))
                .expect("valid compiled JSON");
        assert_eq!(compiled["structure_hash"], response["structure_hash"]);
    }

    #[test]
    fn returns_structured_source_diagnostics() {
        let response = response("clipasm 1\nunknown()\n");

        assert_eq!(response["status"], "error");
        assert_eq!(response["diagnostic"]["code"], "E_UNKNOWN_PROGRAM");
        assert_eq!(response["diagnostic"]["file"], SOURCE_NAME);
        assert_eq!(response["diagnostic"]["line"], 2);
        assert_eq!(response["diagnostic"]["column"], 1);
        assert!(
            response["diagnostic"]["rendered"]
                .as_str()
                .expect("rendered diagnostic")
                .contains("unknown()")
        );
    }

    #[test]
    fn rejects_imports_that_need_a_filesystem() {
        let response = response("clipasm 1\nimport \"helper.clipasm\" as helper\n");

        assert_eq!(response["status"], "error");
        assert_eq!(response["diagnostic"]["code"], "E_IMPORT_REQUIRES_FILE");
    }

    #[test]
    fn prepares_scenic_browser_recipes_from_virtual_asset_hashes() {
        let assets = serde_json::json!([
            {"path": "assets/morning.png", "content_hash": "11".repeat(32)},
            {"path": "assets/meadow.png", "content_hash": "22".repeat(32)},
            {"path": "assets/evening.png", "content_hash": "33".repeat(32)},
        ]);
        let response: serde_json::Value = serde_json::from_str(&prepare_render(
            include_str!("../../examples/scenic-sequence.clipasm"),
            &assets.to_string(),
        ))
        .expect("response JSON");

        assert_eq!(response["status"], "success");
        assert_eq!(response["version"], RESPONSE_VERSION);
        let plan: serde_json::Value =
            serde_json::from_str(response["plan_json"].as_str().expect("plan JSON"))
                .expect("valid plan JSON");
        assert_eq!(plan["version"], 1);
        assert_eq!(plan["steps"].as_array().map(Vec::len), Some(4));
        assert_eq!(plan["export"]["contract"]["frames"], 108);
        assert_eq!(plan["export"]["contract"]["width"], 320);
        assert_eq!(plan["export"]["contract"]["height"], 180);
    }
}
