#!/bin/sh
# Images dyld loads on the first open, plus a forced-light then forced-dark screenshot of the open dropdown and the
# frontmost app while it is open (lsappinfo, read-only). Fresh launch with DYLD_PRINT_LIBRARIES; not a footprint run.
# usage: [APP=..] [WISP_MENU=..] dyld_open.sh <outdir>
set -eu
OUT=$1
HERE=$(cd "$(dirname "$0")" && pwd)
APP=${APP:-$HERE/../../target/Wisp.app}
. "$HERE/lib.sh"
front() { lsappinfo info "$(lsappinfo front)" | head -1 | cut -d'"' -f2; }
FRONT0=$(front)
launch "$OUT" 2,3,3 --env DYLD_PRINT_LIBRARIES=1 --env WISP_LOOKS=light,dark
for i in 1 2; do
    # The i-th visible line: a variant g helper always logs "visible 1".
    until [ "$(grep -c ' visible ' "$L")" -ge $i ]; do sleep 0.1; done; sleep 1.5
    look=$([ $i = 1 ] && echo light || echo dark)
    W=$(grep -F " visible " "$L" | sed -n "${i}p" | sed 's/.* win \([0-9]*\) .*/\1/')
    # Window captures of a just-shown window fail now and then ("could not create image"): retry.
    for _ in 1 2 3; do screencapture -x -o -l "$W" "$OUT/$look.png" 2>/dev/null && break; sleep 0.3; done
    echo "open $i front: $(front)  (before launch: $FRONT0)"
done
wait_for " $P done"; sleep 1
python3 -I "$HERE/dyld_diff.py" "$L" "$P"
