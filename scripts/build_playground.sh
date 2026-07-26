#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
output_directory="${1:-${repository_root}/target/book/playground}"
wasm_bindgen_version="0.2.126"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
    echo "missing required command: wasm-bindgen ${wasm_bindgen_version}" >&2
    exit 1
fi

installed_version="$(wasm-bindgen --version)"
if [[ "${installed_version}" != "wasm-bindgen ${wasm_bindgen_version}" ]]; then
    echo "expected wasm-bindgen ${wasm_bindgen_version}, found ${installed_version}" >&2
    exit 1
fi

cd "${repository_root}"
cargo build \
    --locked \
    --release \
    --package clipasm-playground \
    --target wasm32-unknown-unknown

mkdir -p "${output_directory}"
wasm-bindgen \
    "target/wasm32-unknown-unknown/release/clipasm_playground.wasm" \
    --out-dir "${output_directory}" \
    --out-name clipasm_playground \
    --no-typescript \
    --target web
install -m 0644 \
    "playground/web/clipasm-playground-worker.js" \
    "${output_directory}/clipasm-playground-worker.js"
