#!/bin/sh
# One fresh launch driven from inside Wisp (WISP_DRIVE; no synthetic input): footprint before / while the first open
# is held / 5 s after the first close / 5 s after the last close. The driver's event log (latencies) is <outdir>/log.txt.
# Variant g also snapshots the helper while it is open (open.helper.footprint.txt).
# usage: [APP=target/Wisp.app] [WISP_MENU=a|b|..|g] clean_run.sh <outdir> [n,hold_s,gap_s (default 5,4,7)]
set -eu
OUT=$1 DRIVE=${2:-5,4,7}
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-$HERE/../../target/Wisp.app}
. "$HERE/lib.sh"
launch "$OUT" "$DRIVE"
sleep 16; fp "$P" "$OUT/before"
wait_for " visible 1 "; sleep 2; fp "$P" "$OUT/open"
H=$(helper); [ -n "$H" ] && fp "$H" "$OUT/open.helper"
wait_for " $P gone 1 "; sleep 5; fp "$P" "$OUT/after1"
wait_for " $P done"; sleep 5; fp "$P" "$OUT/after5"
python3 -I "$HERE/fpdiff.py" "$OUT/before.footprint.txt" "$OUT/open.footprint.txt" "$OUT/after1.footprint.txt" "$OUT/after5.footprint.txt"
