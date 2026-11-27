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

# The names of the WASM's functions are 0.73 MB of it, a seventh of what a reader fetches, and
# only show in a panic's stack. They're left out unless asked for, to read one:
#   PENCIL_WASM_NAMES=1 scripts/build-web.sh
NAMES="--remove-name-section --remove-producers-section"
if [ -n "$PENCIL_WASM_NAMES" ]; then NAMES=""; fi

cargo build -p pencil-web --target wasm32-unknown-unknown --release
# shellcheck disable=SC2086
"$WASM_BINDGEN" target/wasm32-unknown-unknown/release/pencil_web.wasm \
  --target web --no-typescript $NAMES --out-dir packages/figure/pkg
ls -la packages/figure/pkg

# The minified bundle: what a site would ship, for the pages in web/ and the tests to load with
# `?min` (web/imports.js). It exports what src/element.js and src/host.js do, from one module
# instance of each, so the WASM starts once, and what src/controls.js does, for a page's own controls. It sits in pkg/ because the glue finds its WASM with
# `new URL('pencil_web_bg.wasm', import.meta.url)`, which esbuild leaves alone in an ES module.
FIGURE=packages/figure
if [ ! -d "$FIGURE/node_modules/esbuild" ]; then
  echo "esbuild isn't installed: run 'pnpm install' at the repo root first." >&2
  exit 1
fi
printf "export * from '%s/src/element.js';\nexport * from '%s/src/host.js';\nexport * from '%s/src/controls.js';\n" "$PWD/$FIGURE" "$PWD/$FIGURE" "$PWD/$FIGURE" |
  pnpm --dir "$FIGURE" exec esbuild --bundle --minify --format=esm --target=es2022 \
    --outfile=pkg/pencil-figure.min.js --log-level=warning
grep -q "pencil_web_bg.wasm" "$FIGURE/pkg/pencil-figure.min.js" || {
  echo "the bundle lost the path of its WASM" >&2
  exit 1
}
echo "sizes, raw and gzipped (bytes):"
sources="$FIGURE/src/element.js $FIGURE/src/host.js $FIGURE/src/labels.js $FIGURE/src/fit.js $FIGURE/src/controls.js $FIGURE/src/wiring.js $FIGURE/src/scheme.js $FIGURE/pkg/pencil_web.js"
for set in "sources:$sources" "bundle:$FIGURE/pkg/pencil-figure.min.js"; do
  files=${set#*:}
  raw=$(cat $files | wc -c)
  gz=$(cat $files | gzip -9 | wc -c)
  printf "  %-8s %8d raw %8d gzip\n" "${set%%:*}" "$raw" "$gz"
done
