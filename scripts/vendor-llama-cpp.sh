#!/usr/bin/env bash
# Copies the llama.cpp sources the Android app compiles into app/src/main/cpp/llama.cpp.
# It uses the exact copy the desktop engine already builds (the llama-cpp-sys-2 crate in the cargo
# registry), so both apps run the same llama.cpp. Run `cargo fetch` in engine/ first if it is missing.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=$(ls -d "$HOME"/.cargo/registry/src/*/llama-cpp-sys-2-*/llama.cpp 2>/dev/null | tail -1 || true)
[ -n "$SRC" ] || { echo "llama.cpp source not found: run 'cd engine && cargo fetch'"; exit 1; }
DST=$ROOT/app/src/main/cpp/llama.cpp
if [ -f "$DST/CMakeLists.txt" ]; then echo "llama.cpp already vendored"; exit 0; fi
mkdir -p "$DST"
for d in include src ggml cmake vendor; do cp -r "$SRC/$d" "$DST/"; done
cp "$SRC/CMakeLists.txt" "$SRC/LICENSE" "$DST/"
# the sources need no tests, tools or examples; drop the subdirectories the top-level CMake would look for
mkdir -p "$DST/common" "$DST/tools" "$DST/examples" "$DST/tests" "$DST/pocs"
echo "vendored $(basename "$(dirname "$SRC")") into $DST"
