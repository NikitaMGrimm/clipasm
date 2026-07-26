#!/usr/bin/env bash
set -euo pipefail

if [[ -d "${HOME}/.cargo/bin" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
fi

for command in cargo mdbook ffmpeg ffprobe node; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        echo "missing required command: ${command}" >&2
        exit 1
    fi
done

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
mdbook build
python3 scripts/check_docs.py
node --check theme/clipasm-playground.js
node --check playground/web/clipasm-playground-worker.js
git diff --check
