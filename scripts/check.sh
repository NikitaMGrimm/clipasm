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
cargo clippy -p clipasm --no-default-features --all-targets -- -D warnings
cargo clippy -p clipasm --no-default-features --features native --all-targets -- -D warnings
cargo clippy -p clipasm-playground --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p clipasm --no-default-features --all-targets
cargo test -p clipasm --no-default-features --doc
cargo test -p clipasm --no-default-features --features native --all-targets
cargo test -p clipasm --no-default-features --features native --doc
cargo test --workspace --all-targets
cargo test --workspace --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-deps --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-deps --no-default-features --features native
cargo run --locked -p clipasm-reference-docs -- check
mdbook build
python3 scripts/check_docs.py
python3 scripts/check_package_contents.py
python3 -m unittest discover -s scripts -p 'test_*.py'
node --check theme/clipasm-playground.js
node --check playground/web/clipasm-playground-worker.js
node --check playground/web/clipasm-playground-render-worker.js
node --check playground/web/browser-smoke.mjs
git diff --check
