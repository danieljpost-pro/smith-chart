#!/usr/bin/env bash
# Build the wasm module and JS bindings into www/pkg/.
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli (matching
# the wasm-bindgen version pinned in Cargo.toml).
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript \
  --out-dir www/pkg \
  target/wasm32-unknown-unknown/release/smith_chart.wasm

echo "Built. Serve www/ with any static file server, e.g.:"
echo "  ./serve.sh"
