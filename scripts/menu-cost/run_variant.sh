#!/bin/sh
# One variant's full measurement: two fresh-launch clean runs (footprint + open latency), then on the second
# launch 60 s CPU with the menu closed and 60 s held open with live updates.
# usage: [APP=..] [WISP_MENU=..] run_variant.sh <outdir> <x> <y>
set -eu
OUT=$1 X=$2 Y=$3
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$OUT"
for i in 1 2; do "$HERE/clean_run.sh" "$OUT/run$i" "$X" "$Y" > "$OUT/run$i.txt" 2>&1; done
P=$(pgrep -x wisp)
"$HERE/../measure.sh" "$P" 60 > "$OUT/cpu-closed.txt"
"$HERE/menu" open "$P" "$X" "$Y" > "$OUT/cpu-open.txt"
"$HERE/../measure.sh" "$P" 60 >> "$OUT/cpu-open.txt"
"$HERE/menu" close "$P" >> "$OUT/cpu-open.txt"
