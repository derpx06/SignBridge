#!/usr/bin/env bash
# Installs Voicio for the current user. No compiler needed.
#   ./install.sh                 install or update, then download the speech models for this computer
#   ./install.sh --no-models     skip the model download (run `voicio models install` later)
#   ./install.sh --no-service    do not start/enable the background service (for testing)
#   ./install.sh --uninstall     remove Voicio (downloaded models and your settings are kept)
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
BIN=$HOME/.local/bin
UNITS=$HOME/.config/systemd/user
UNIT=$UNITS/voicio.service
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

MODELS=1; SERVICE=1; UNINSTALL=0
for arg in "$@"; do
  case "$arg" in
    --no-models) MODELS=0 ;;
    --no-service) SERVICE=0 ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option $arg (see --help)"; exit 2 ;;
  esac
done

if [ "$UNINSTALL" = 1 ]; then
  if [ "$SERVICE" = 1 ]; then systemctl --user disable --now voicio.service 2>/dev/null || true; fi
  command -v gnome-extensions >/dev/null && gnome-extensions disable "$UUID" 2>/dev/null || true
  rm -rf "$BIN/voicio" "$BIN/voicio-engine" "$UNIT" "$EXT" "$DESKTOP" "$ICON" "$HOME/.config/autostart/io.github.voicio.desktop"
  if [ "$SERVICE" = 1 ]; then systemctl --user daemon-reload 2>/dev/null || true; fi
  echo "Voicio removed. Downloaded models and settings were kept (voicio models folder, ~/.config/voicio)."
  exit 0
fi

# A readable message instead of a crash when the archive is damaged or the system is too old.
for f in bin/voicio bin/voicio-engine share/voicio.svg share/$APP_ID.desktop; do
  [ -f "$HERE/$f" ] || { echo "this package is incomplete: $f is missing. Download it again."; exit 1; }
done
if [ "$(uname -s)" != Linux ]; then echo "this package is for Linux"; exit 1; fi

echo "Installing Voicio..."
if [ "$SERVICE" = 1 ]; then systemctl --user stop voicio.service 2>/dev/null || true; fi
install -Dm755 "$HERE/bin/voicio" "$BIN/voicio"
install -Dm755 "$HERE/bin/voicio-engine" "$BIN/voicio-engine"
install -Dm644 "$HERE/share/$APP_ID.desktop" "$DESKTOP"
sed -i "s|^Exec=voicio$|Exec=$BIN/voicio|" "$DESKTOP"
install -Dm644 "$HERE/share/voicio.svg" "$ICON"
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
# Safety net only: the app keeps Lite and Standard under 1 GB by itself.
MemoryHigh=2500M
MemoryMax=3G
OOMPolicy=continue

[Install]
WantedBy=graphical-session.target
EOF

# Models: bundled (offline package) or downloaded and checksum-verified by the app itself.
if [ "$MODELS" = 1 ]; then
  FOLDER=$("$BIN/voicio" models list 2>/dev/null | sed -n 's/^models folder: //p' | head -1)
  if [ -d "$HERE/models" ] && [ -n "$FOLDER" ]; then
    echo "Copying the bundled models..."
    mkdir -p "$FOLDER" && cp -rn "$HERE/models/." "$FOLDER/"
  fi
  echo "Checking the speech models (downloads only what is missing, each file is verified)..."
  "$BIN/voicio" models install || echo "Model download did not finish. Run: voicio models install"
fi

if [ "$SERVICE" = 1 ]; then
  systemctl --user daemon-reload
  systemctl --user enable voicio.service
  systemctl --user restart voicio.service
fi

if command -v gnome-shell >/dev/null && [ -d "$HERE/share/$UUID" ]; then
  rm -rf "$EXT"; mkdir -p "$(dirname "$EXT")"
  cp -r "$HERE/share/$UUID" "$EXT"
  command -v glib-compile-schemas >/dev/null && glib-compile-schemas "$EXT/schemas" || true
  if [ "$SERVICE" = 1 ] && gnome-extensions enable "$UUID" 2>/dev/null && gnome-extensions info "$UUID" 2>/dev/null | grep -q "State: ACTIVE"; then
    echo "GNOME extension active (live typing)."
  else
    echo "GNOME extension installed. Log out and back in once, then it switches on by itself"
    echo "(or run: gnome-extensions enable $UUID)."
  fi
fi

echo
if [ "$SERVICE" = 1 ]; then HOW=$(how_to_dictate); else HOW="Press your shortcut in any text field (F9 by default; run voicio status to see it)."; fi
echo "Voicio is installed. $HOW"
echo "Settings: run  voicio settings     Check everything: run  voicio doctor"
