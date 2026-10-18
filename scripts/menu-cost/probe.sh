#!/bin/sh
# One driven open per variant with a screenshot of the dropdown window, to check each variant shows at all.
# usage: probe.sh <outdir> <variant>...   ("-" = default)
set -eu
OUT=$1; shift
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-$HERE/../../target/Wisp.app}
mkdir -p "$OUT"
for V in "$@"; do
    L=$OUT/$V.log; : > "$L"
    pkill -x wisp || true; sleep 1
    if [ "$V" = - ]; then open -n --stderr "$L" --env WISP_DRIVE=1,3,1 "$APP"; else open -n --stderr "$L" --env WISP_DRIVE=1,3,1 --env WISP_MENU=$V "$APP"; fi
    i=0; until grep -q ' visible 1 ' "$L" || [ $i -gt 300 ]; do sleep 0.1; i=$((i+1)); done
    sleep 1
    W=$(sed -n 's/.* visible 1 .* win \([0-9]*\) .*/\1/p' "$L" | head -1)
    [ -n "$W" ] && screencapture -x -o -l "$W" "$OUT/$V.png" || true
    sleep 4; echo "== $V"; cat "$L"
done
