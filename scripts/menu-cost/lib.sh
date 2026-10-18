# Shared by the drive scripts: fresh launch with the in-process driver, log polling, footprint snapshots.
# launch <outdir> <WISP_DRIVE> [extra open(1) args]: kills any Wisp, starts a new one, sets $P (main pid) and $L (log).
launch() {
    mkdir -p "$1"; L=$1/log.txt; : > "$L"
    pkill -x wisp || true; sleep 1
    D=$2; shift 2
    # -n: a new instance, so `--env` is applied even if LaunchServices still remembers the last one.
    open -n --stderr "$L" --env WISP_DRIVE="$D" ${WISP_MENU:+--env WISP_MENU=$WISP_MENU} ${WISP_SCREEN:+--env WISP_SCREEN=$WISP_SCREEN} "$@" "$APP"; sleep 2
    P=$(pgrep -x -o wisp)
}
# Blocks until the log has a line matching the fixed string $1 (120 s cap).
# (Its own counter name: callers loop over $i.)
wait_for() { _w=0; until grep -qF -- "$1" "$L" || [ $_w -ge 1200 ]; do sleep 0.1; _w=$((_w + 1)); done; }
# The live helper's pid (variant g), from the main process's last "spawned" line, if it is still running.
helper() { h=$(sed -n 's/.* spawned [0-9]* .* pid \([0-9]*\)$/\1/p' "$L" | tail -1); [ -n "$h" ] && kill -0 "$h" 2>/dev/null && echo "$h" || true; }
fp() { footprint -f bytes "$1" > "$2.footprint.txt"; }
