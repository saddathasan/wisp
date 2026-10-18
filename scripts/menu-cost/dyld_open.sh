#!/bin/sh
# Lists the images dyld loads on the first menu open: fresh launch with DYLD_PRINT_LIBRARIES, open+close once.
# Not a footprint run (the env var changes startup). usage: [APP=..] [WISP_MENU=..] dyld_open.sh <outdir> <x> <y>
set -eu
OUT=$1 X=$2 Y=$3
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-$HERE/../../target/Wisp.app}
mkdir -p "$OUT"; L=$OUT/dyld.txt; : > "$L"
pkill -x wisp || true; sleep 1
open -n --stderr "$L" --env DYLD_PRINT_LIBRARIES=1 ${WISP_MENU:+--env WISP_MENU=$WISP_MENU} "$APP"; sleep 10
P=$(pgrep -x wisp); N=$(wc -l < "$L")
"$HERE/menu" open "$P" "$X" "$Y"; sleep 3; screencapture -x -R "$((X - 170)),$((Y - 15)),500,450" "$OUT/menu.png"; "$HERE/menu" close "$P"; sleep 2
echo "launch: $N images; first open loaded $(($(wc -l < "$L") - N)):"
tail -n +$((N + 1)) "$L" | sed 's/.*dyld_shared_cache.*//; s/^dyld\[[0-9]*\]: <[^>]*> //' | sed 's#.*/##' | grep -v '^$' | tr '\n' ' '; echo
