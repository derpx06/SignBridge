#!/usr/bin/env bash
# Builds ONE release archive of the whole Voicio pipeline for Linux:
#   dist/voicio-<version>-linux-x86_64.tar.gz  (+ .sha256)
# containing the agent, the model worker (voice detection + speech + cleanup), the GNOME extension,
# the launcher entry and an installer that needs no compiler. Models are checksum-pinned and either
# bundled (--with-models DIR, for offline installs) or downloaded by the installer.
#   package-release.sh [--with-models DIR]
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WITH_MODELS=""
if [ "${1:-}" = "--with-models" ]; then WITH_MODELS=${2:?--with-models needs the models folder}; fi

for doc in LICENSE PRIVACY.md; do
  [ -f "$ROOT/$doc" ] || { echo "missing $ROOT/$doc: the release must ship it"; exit 1; }
done

VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/engine/crates/app/Cargo.toml" | head -1)
NAME=voicio-$VERSION-linux-$(uname -m)
OUT=$ROOT/dist/$NAME

(cd "$ROOT/engine" && cargo build --release --locked --workspace)

rm -rf "$OUT" "$OUT.tar.gz" "$OUT.tar.gz.sha256"
mkdir -p "$OUT/bin" "$OUT/share"
for bin in voicio voicio-engine; do
  install -m755 "$ROOT/engine/target/release/$bin" "$OUT/bin/$bin"
  strip --strip-unneeded "$OUT/bin/$bin" 2>/dev/null || true
done
cp "$ROOT/engine/crates/app/io.github.voicio.Voicio.desktop" "$OUT/share/"
cp "$ROOT/engine/crates/app/ui/voicio.svg" "$OUT/share/"
# A syntax error here silently disables the shortcut and typing at the next login, so refuse to ship one.
if command -v node >/dev/null; then
  cp "$ROOT/gnome-extension/voicio@derpx06.github.io/extension.js" "$OUT/extension-check.mjs"
  node --check "$OUT/extension-check.mjs" || { echo "extension.js has a syntax error"; exit 1; }
  rm -f "$OUT/extension-check.mjs"
fi
cp -r "$ROOT/gnome-extension/voicio@derpx06.github.io" "$OUT/share/"
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$ROOT/LICENSE" "$ROOT/PRIVACY.md" "$OUT/"
install -m755 "$ROOT/engine/scripts/release/install.sh" "$OUT/install.sh"
cp "$ROOT/engine/scripts/release/README.txt" "$OUT/README.txt"
if [ -n "$WITH_MODELS" ]; then
  mkdir -p "$OUT/models"
  cp -r "$WITH_MODELS"/. "$OUT/models/"
fi

(cd "$ROOT/dist" && tar -czf "$NAME.tar.gz" "$NAME" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
echo "built $ROOT/dist/$NAME.tar.gz ($(du -h "$ROOT/dist/$NAME.tar.gz" | cut -f1))"
