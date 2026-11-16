#!/bin/bash
# Runs gnome-extension/ in an isolated headless GNOME Shell and screenshots every capsule state.
# Nothing appears on your screen and your real session is untouched. usage: run.sh [OUTDIR]
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${1:-/tmp/voicio-extension-shots}
W=$(mktemp -d /tmp/vhw.XXXX)
mkdir -p "$OUT" "$W/data/gnome-shell/extensions" "$W/config" "$W/home"
EXT=$W/data/gnome-shell/extensions/voicio@derpx06.github.io
cp -r "$HERE/../../../gnome-extension/voicio@derpx06.github.io" "$EXT"
python3 "$HERE/patch_hook.py" "$EXT/extension.js" "$OUT"
RUN=/tmp/vh-run; mkdir -p $RUN; chmod 700 $RUN
cat > "$W/session.sh" <<EOS
#!/bin/bash
export XDG_RUNTIME_DIR=$RUN XDG_DATA_HOME=$W/data XDG_CONFIG_HOME=$W/config GSETTINGS_BACKEND=keyfile HOME=$W/home
gsettings set org.gnome.shell enabled-extensions "['voicio@derpx06.github.io']"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '99'
gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x720 > $W/shell.log 2>&1 &
SH=\$!
sleep 6
timeout 30 python3 $HERE/fake_agent.py > $W/agent.log 2>&1
kill \$SH
EOS
chmod +x "$W/session.sh"
rm -f /tmp/vh-shot
timeout 90 dbus-run-session -- "$W/session.sh" >/dev/null 2>&1 || true
ls "$OUT"; grep -i "JS ERROR\|shot failed" "$W/shell.log" || echo "no JS errors"
