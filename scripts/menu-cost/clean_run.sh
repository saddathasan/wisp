#!/bin/sh
# Measures the first-menu-open footprint step: fresh Wisp, footprint before / after one open+close / after 4 more cycles.
# usage: clean_run.sh <outdir> <x> <y>   (x y = the status item's global point, read off `screencapture` of the menu bar)
set -eu
OUT=$1 X=$2 Y=$3
HERE=$(cd "$(dirname "$0")" && pwd)
[ -x "$HERE/menu" ] || swiftc -O -o "$HERE/menu" "$HERE/menu.swift"
mkdir -p "$OUT"
pkill -x wisp || true; sleep 1
open "$HERE/../../target/Wisp.app"; sleep 20
P=$(pgrep -x wisp)
footprint -f bytes "$P" > "$OUT/before.footprint.txt"
"$HERE/menu" open "$P" "$X" "$Y"; sleep 1; "$HERE/menu" close "$P"; sleep 5
footprint -f bytes "$P" > "$OUT/after1.footprint.txt"
"$HERE/menu" cycle "$P" "$X" "$Y" 4; sleep 5
footprint -f bytes "$P" > "$OUT/after5.footprint.txt"
python3 -I "$HERE/fpdiff.py" "$OUT/before.footprint.txt" "$OUT/after1.footprint.txt" "$OUT/after5.footprint.txt"
