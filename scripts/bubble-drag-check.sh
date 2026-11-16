#!/usr/bin/env bash
# Debug-only measurement of bubble jumps. Needs the debug APK, accessibility on, a focused text field (not a payment app).
# Usage: scripts/bubble-drag-check.sh [serial] [orbX orbY]   (logical px; default = right edge, lower third)
# Reads the VoicioPlace frame log (src=drag|snap|place). Reports:
#   jumps  = instant relocations (src=place) larger than 4 px
#   reverse= drag steps that move opposite to the finger
set -euo pipefail
S=${1:-$(adb devices | awk 'NR>1&&$2=="device"{print $1; exit}')}
A="adb -s $S"
read W H < <($A shell wm size | awk '/Override/{split($3,a,"x");print a[1],a[2];exit} /Physical/{split($3,a,"x");p=a[1]" "a[2]} END{if(!p)exit; print p}' | head -1)
X=${2:-$((W-108))}; Y=${3:-$((H*9/10))}
$A logcat -c
run() { $A shell input swipe "$@"; sleep 1.2; }
run $X $Y $((W/4)) $((H/2)) 350        # drag across and up
run $((W/4)) $((H/2)) $((W*3/4)) $((H/3)) 350  # drag to the other side
run $((W*3/4)) $((H/3)) $((W/4)) $((H/2+300)) 300 # fast flick
$A logcat -d -s VoicioPlace | tee /tmp/claude-1000/voicioplace.log | awk '
  { for(i=1;i<=NF;i++){split($i,k,"="); v[k[1]]=k[2]}
    n++; s[v["src"]]++
    d=sqrt(v["dx"]*v["dx"]+v["dy"]*v["dy"])
    if(v["src"]=="place" && d>4) j++
    if(v["src"]=="drag" && d>mx) mx=d }
  END { printf "moves=%d drag=%d snap=%d place=%d  jumps>4px=%d  max drag step=%.0f px\n", n, s["drag"], s["snap"], s["place"], j, mx }'
