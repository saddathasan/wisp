#!/bin/sh
# One variant's full measurement, all driven from inside Wisp: two fresh-launch clean runs (footprint + latency), then a
# third launch that holds one open 65 s (60 s CPU with live updates; footprint before/open/after1 as run3) and
# measures 60 s closed after it. Variant g also measures the helper's CPU while it is open.
# usage: [APP=..] [WISP_MENU=..] run_variant.sh <outdir>
set -eu
OUT=$1
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-$HERE/../../target/Wisp.app}
. "$HERE/lib.sh"
mkdir -p "$OUT"
for i in 1 2; do "$HERE/clean_run.sh" "$OUT/run$i" > "$OUT/run$i.txt" 2>&1; done
R=$OUT/run3
launch "$R" 1,65,70
sleep 16; fp "$P" "$R/before"
wait_for " visible 1 "; sleep 2; fp "$P" "$R/open"
H=$(helper)
if [ -n "$H" ]; then "$HERE/../measure.sh" "$H" 60 > "$OUT/cpu-open-helper.txt" & fi
"$HERE/../measure.sh" "$P" 60 > "$OUT/cpu-open.txt"
wait
wait_for " $P gone 1 "; sleep 3; fp "$P" "$R/after1"
"$HERE/../measure.sh" "$P" 60 > "$OUT/cpu-closed.txt"
