#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
output_directory="${1:-${repository_root}/target/book/playground}"
wasm_bindgen_version="0.2.126"
web_directory="${repository_root}/playground/web"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
    echo "missing required command: wasm-bindgen ${wasm_bindgen_version}" >&2
    exit 1
fi

installed_version="$(wasm-bindgen --version)"
if [[ "${installed_version}" != "wasm-bindgen ${wasm_bindgen_version}" ]]; then
    echo "expected wasm-bindgen ${wasm_bindgen_version}, found ${installed_version}" >&2
    exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
    echo "missing required command: npm" >&2
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
install -m 0644 \
    "playground/web/clipasm-playground-render-worker.js" \
    "${output_directory}/clipasm-playground-render-worker.js"

npm ci \
    --prefix "${web_directory}" \
    --ignore-scripts \
    --no-audit \
    --no-fund

wrapper_directory="${output_directory}/ffmpeg/wrapper"
core_directory="${output_directory}/ffmpeg/core"
install -d "${wrapper_directory}" "${core_directory}"
for file in classes const errors index types utils worker; do
    install -m 0644 \
        "${web_directory}/node_modules/@ffmpeg/ffmpeg/dist/esm/${file}.js" \
        "${wrapper_directory}/${file}.js"
done
install -m 0644 \
    "${web_directory}/node_modules/@ffmpeg/core/dist/esm/ffmpeg-core.js" \
    "${core_directory}/ffmpeg-core.js"
install -m 0644 \
    "${web_directory}/node_modules/@ffmpeg/core/dist/esm/ffmpeg-core.wasm" \
    "${core_directory}/ffmpeg-core.wasm"
install -m 0644 \
    "${web_directory}/THIRD_PARTY.md" \
    "${output_directory}/THIRD_PARTY.md"
install -m 0644 \
    "${web_directory}/COPYING.GPLv2" \
    "${output_directory}/COPYING.GPLv2"
install -m 0644 \
    "${web_directory}/LICENSE.ffmpeg-wasm-MIT" \
    "${output_directory}/LICENSE.ffmpeg-wasm-MIT"

example_assets="${output_directory}/example-assets/assets"
install -d "${example_assets}"
install -m 0644 examples/assets/*.png "${example_assets}/"
