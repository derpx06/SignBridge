#!/usr/bin/env bash
# Keeps the phone reachable when its USB link drops (kernel "device descriptor read, error -71").
# First run, phone on USB:  scripts/adb-keepalive.sh setup   -> switches it to Wi-Fi adb and remembers its IP.
# Later:                    scripts/adb-keepalive.sh          -> reconnects over USB or Wi-Fi, every 10 s, until Ctrl-C.
set -u
SERIAL=${VOICIO_PHONE:-c955b31}
IPFILE=${XDG_CACHE_HOME:-$HOME/.cache}/voicio-phone-ip
PORT=5555

if [ "${1:-}" = setup ]; then
  adb -s "$SERIAL" get-state >/dev/null 2>&1 || { echo "Connect the phone by USB first (serial $SERIAL)."; exit 1; }
  ip=$(adb -s "$SERIAL" shell ip -f inet addr show wlan0 | sed -n 's/.*inet \([0-9.]*\)\/.*/\1/p' | head -1)
  [ -n "$ip" ] || { echo "Phone has no Wi-Fi address. Join the same Wi-Fi as this laptop."; exit 1; }
  adb -s "$SERIAL" tcpip $PORT >/dev/null && sleep 2 && adb connect "$ip:$PORT" && echo "$ip" > "$IPFILE"
  echo "Saved $ip. You can unplug; run this script without arguments to keep it connected."
  exit 0
fi

while true; do
  ip=$(cat "$IPFILE" 2>/dev/null || true)
  if ! adb devices | grep -Eq "^($SERIAL|${ip:-none}:$PORT)[[:space:]]+device$"; then
    [ -n "$ip" ] && adb connect "$ip:$PORT" >/dev/null 2>&1 && echo "$(date +%T) reconnected over Wi-Fi ($ip)"
  fi
  sleep 10
done
