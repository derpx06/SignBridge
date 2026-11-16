#!/usr/bin/env bash
# Installs Voicio for the current user on Linux:
#   ~/.local/bin/voicio         the agent (always running, ~20 MB)
#   ~/.local/bin/voicio-engine  the model worker (spawned on demand, unloaded when idle)
#   a systemd user service capping agent + worker at 1 GB, the models, and the GNOME extension.
#   install.sh              install / update
#   install.sh --uninstall  remove everything except downloaded models and settings
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN=$HOME/.local/bin
UNITS=$HOME/.config/systemd/user
UNIT=$UNITS/voicio.service
LEGACY_UNITS="whispr.service whispr-engine.service" # the app was called Whispr before it became Voicio
APP_ID=io.github.voicio.Voicio
DESKTOP=$HOME/.local/share/applications/$APP_ID.desktop
ICON=$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg
UUID=voicio@derpx06.github.io
EXT=$HOME/.local/share/gnome-shell/extensions/$UUID


# How to dictate, from the key and mode the running agent actually bound (the desktop may have given
# it another key, or not answered yet): `voicio status` ends with "shortcut=KEY mode=MODE".
how_to_dictate() {
  local status="" key="" mode=""
  for _ in 1 2 3 4 5 6 7 8; do
    status=$("$BIN/voicio" status 2>/dev/null || true)
    key=$(printf '%s' "$status" | sed -n 's/.* shortcut=\([^ ]*\) mode=.*/\1/p')
    mode=$(printf '%s' "$status" | sed -n 's/.* mode=\([^ ]*\).*/\1/p')
    if [ -n "$key" ] && [ "$key" != none ]; then break; fi
    sleep 1
  done
  [ "$key" = none ] && key=""
  case "${key:+$mode}" in
    hold) echo "Hold $key in any text field while you speak, let go to type." ;;
    toggle) echo "Press $key in any text field to start, press it again to stop." ;;
    hold-or-tap) echo "Hold $key in any text field while you speak (or tap it to start and stop)." ;;
    *) echo "Press your shortcut in any text field (F9 by default; run voicio status to see it)." ;;
  esac
}

remove_legacy() {
  for unit in $LEGACY_UNITS; do
    if [ -f "$UNITS/$unit" ]; then
      systemctl --user disable --now "$unit" 2>/dev/null || true
      rm -f "$UNITS/$unit"
    fi
  done
  command -v gnome-extensions >/dev/null && gnome-extensions disable whispr@whispr.local 2>/dev/null || true
  rm -rf "$HOME/.local/share/gnome-shell/extensions/whispr@whispr.local" \
    "$HOME/.local/bin/whispr" "$HOME/.local/bin/whispr-engine" \
    "$HOME/.local/share/applications/io.github.whispr.Whispr.desktop" \
    "$HOME/.local/share/icons/hicolor/scalable/apps/io.github.whispr.Whispr.svg" \
    "$HOME/.config/autostart/io.github.whispr.desktop"
}

if [ "${1:-}" = "--uninstall" ]; then
  systemctl --user disable --now voicio.service 2>/dev/null || true
  remove_legacy
  command -v gnome-extensions >/dev/null && gnome-extensions disable "$UUID" 2>/dev/null || true
  rm -rf "$BIN/voicio" "$BIN/voicio-engine" "$UNIT" "$EXT" "$DESKTOP" "$ICON" "$HOME/.config/autostart/io.github.voicio.desktop"
  systemctl --user daemon-reload
  echo "uninstalled (models kept in ~/.cache/voicio, settings in ~/.config/voicio)"
  exit 0
fi

(cd "$ROOT/engine" && cargo build --release --locked --workspace)
"$ROOT/engine/scripts/fetch-models.sh"

remove_legacy
systemctl --user stop voicio.service 2>/dev/null || true
install -Dm755 "$ROOT/engine/target/release/voicio" "$BIN/voicio"
install -Dm755 "$ROOT/engine/target/release/voicio-engine" "$BIN/voicio-engine"
# App id for portals (global shortcuts on KDE etc.), notifications and the app grid.
install -Dm644 "$ROOT/engine/crates/app/$APP_ID.desktop" "$DESKTOP"
sed -i "s|^Exec=voicio$|Exec=$BIN/voicio|" "$DESKTOP"
install -Dm644 "$ROOT/engine/crates/app/ui/voicio.svg" "$ICON"
# A stale icon cache makes the launcher show a fallback icon.
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true

mkdir -p "$UNITS"
cat > "$UNIT" <<EOF
[Unit]
Description=Voicio on-device dictation
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=$BIN/voicio
Restart=on-failure
RestartSec=2
# Agent + model worker share this cgroup. This is only a safety net: the app keeps each quality
# level to its own memory budget (Lite and Standard stay under 1 GB).
MemoryHigh=2500M
MemoryMax=3G
OOMPolicy=continue

[Install]
WantedBy=graphical-session.target
EOF
systemctl --user daemon-reload
systemctl --user enable voicio.service
systemctl --user restart voicio.service

if command -v gnome-shell >/dev/null; then
  rm -rf "$EXT"
  mkdir -p "$(dirname "$EXT")"
  cp -r "$ROOT/gnome-extension/$UUID" "$EXT"
  glib-compile-schemas "$EXT/schemas"
  # A freshly copied extension is unknown to the running shell until the next login (Wayland
  # cannot restart it), so enabling can fail now. Say what is true instead of hiding it.
  if gnome-extensions enable "$UUID" 2>/dev/null && gnome-extensions info "$UUID" 2>/dev/null | grep -q "State: ACTIVE"; then
    echo "GNOME extension active (live typing)."
  else
    echo "GNOME extension installed but NOT active yet. Log out and back in, then run:"
    echo "    gnome-extensions enable $UUID"
    echo "Until then Voicio shows its own pill and leaves text on the clipboard (run: voicio doctor)."
  fi
fi
echo "done: $(how_to_dictate)"
echo "      or run: voicio toggle | voicio status | voicio settings"
