#!/bin/sh
# Builds the browser host into packages/figure/pkg, which the package ships and isn't checked in.
# Serve the repo root, then open /web/:
#   scripts/serve.py
set -e
cd "$(dirname "$0")/.."

# The CLI must match the wasm-bindgen crate exactly, so it's installed per checkout.
VERSION=0.2.130
TOOLS=target/tools
WASM_BINDGEN=$TOOLS/bin/wasm-bindgen
if ! "$WASM_BINDGEN" --version 2>/dev/null | grep -q "$VERSION"; then
  cargo install wasm-bindgen-cli --version "$VERSION" --root "$TOOLS" --locked
fi

cargo build -p pencil-web --target wasm32-unknown-unknown --release
"$WASM_BINDGEN" target/wasm32-unknown-unknown/release/pencil_web.wasm \
  --target web --no-typescript --out-dir packages/figure/pkg
ls -la packages/figure/pkg
