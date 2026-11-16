#!/usr/bin/env bash
# Downloads the sherpa-onnx 1.13.8 Android native libraries (arm64) into app/src/main/jniLibs.
# The Kotlin API (app/src/main/kotlin/com/k2fsa/sherpa/onnx, Apache-2.0) is committed; the .so files are not.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=1.13.8
SHA256=2ff63469a71cb6009aa2e3ed5f4a670f8abdcbe4bb9ffd23776afc792a6b4f44
OUT=$ROOT/app/src/main/jniLibs/arm64-v8a
if [ -f "$OUT/libonnxruntime.so" ] && [ -f "$OUT/libsherpa-onnx-jni.so" ]; then echo "sherpa-onnx libs present"; exit 0; fi
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
curl -fL -o "$TMP/a.tar.bz2" "https://github.com/k2-fsa/sherpa-onnx/releases/download/v$VERSION/sherpa-onnx-v$VERSION-android.tar.bz2"
echo "$SHA256  $TMP/a.tar.bz2" | sha256sum -c -
tar -xjf "$TMP/a.tar.bz2" -C "$TMP" jniLibs/arm64-v8a/libonnxruntime.so jniLibs/arm64-v8a/libsherpa-onnx-jni.so
mkdir -p "$OUT"; cp "$TMP"/jniLibs/arm64-v8a/*.so "$OUT/"
echo "installed to $OUT"
