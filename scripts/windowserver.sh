#!/bin/sh
# Measures WindowServer's CPU over interleaved windows with Wisp running and quit (spec S4; METHOD.md, "S4").
# Usage: windowserver.sh [rounds (3)] [seconds per window (180)]; ends with Wisp running.
set -eu
cd "$(dirname "$0")/.."
export LC_ALL=C
ROUNDS=${1:-3}
SECS=${2:-180}
APP=target/Wisp.app
BIN=$PWD/$APP/Contents/MacOS/wisp
fail() { echo "windowserver.sh: $*" >&2; exit 1; }

[ -x "$BIN" ] || fail "no $APP; run scripts/bundle.sh first"
! pgrep -x Stats >/dev/null || fail "Stats is running; quit it first"
WS=$(pgrep -x WindowServer)
# A display-sleep assertion for this run only: Wisp pauses on display sleep (spec §3.3).
caffeinate -d -i -w $$ &

# Same parsing as measure.sh: cputime is M:SS.cc with unbounded minutes.
cpu() { ps -o cputime= -p "$1" | awk -F'[:.]' '{ printf "%.2f", $1 * 60 + $2 + $3 / 100 }'; }
idle_ns() { ioreg -c IOHIDSystem | awk '/HIDIdleTime/ { print $NF; exit }'; }
# The key is only present while the session is locked.
locked() { ioreg -n Root -d1 | grep -q '"CGSSessionScreenIsLocked" = Yes' && echo yes || echo no; }
quit_wisp() { pkill -x wisp || true; while pgrep -x wisp >/dev/null; do sleep 1; done; }

echo "date $(date '+%Y-%m-%d %H:%M:%S %z')"
echo "git $(git rev-parse --short HEAD)$([ -n "$(git status --porcelain)" ] && echo ' (dirty)')"
echo "macOS $(sw_vers -productVersion) ($(sw_vers -buildVersion)), $(sysctl -n machdep.cpu.brand_string)"
system_profiler SPDisplaysDataType | sed -n 's/^ *Resolution: /display /p'
echo "WindowServer pid $WS, up $(ps -o etime= -p "$WS" | tr -d ' ')"
echo "monitors running: $(ps -axo pid=,comm= | grep -E '/(Stats|Activity Monitor|iStat Menus|MenuMeters|eul)$' || echo none)"
echo "rounds $ROUNDS, $SECS s per window, 20 s settle"

tmp=$(mktemp -d)
window() { # $1: on | off
  w=$([ "$1" = on ] && pgrep -x wisp || true)
  c0=$(cpu "$WS"); w0=$([ -n "$w" ] && cpu "$w" || echo 0); t0=$(date +%s)
  top -l $((SECS / 2 + 1)) -s 2 -pid "$WS" -stats cpu >"$tmp/top" &
  top_pid=$!
  # Sampled from 10 s in: an input before the window then shows idle >= 10 s, one inside it < 10 s.
  (k=0; while [ "$k" -lt $((SECS / 10)) ]; do sleep 10; echo "$(idle_ns) $(locked)"; k=$((k + 1)); done) >"$tmp/hid" &
  hid_pid=$!
  wait "$top_pid" "$hid_pid"
  c1=$(cpu "$WS"); w1=$([ -n "$w" ] && cpu "$w" || echo 0); t1=$(date +%s)
  # top's first sample is an average since launch, not a delta over the interval.
  top_avg=$(awk '/^%CPU/ { getline; if (n++) { s += $1; k++ } } END { printf "%.2f", s / k }' "$tmp/top")
  awk -v arm="$1" -v c0="$c0" -v c1="$c1" -v w0="$w0" -v w1="$w1" -v t0="$t0" -v t1="$t1" -v top="$top_avg" -v w="$w" '
    { if (min == "" || $1 < min) min = $1; if ($2 == "yes") lock = 1 }
    END {
      s = t1 - t0; idle = min / 1e9
      printf "wisp %-3s  ws %.3f%%  ws_top %s%%  wisp %s  over %d s  min_idle %.1f s %s  locked %s\n",
        arm, (c1 - c0) / s * 100, top, (w == "" ? "-" : sprintf("%.3f%%", (w1 - w0) / s * 100)), s,
        idle, (idle < 10 ? "active" : "idle"), (lock ? "yes" : "no")
    }' "$tmp/hid"
}

quit_wisp
i=1
while [ "$i" -le "$ROUNDS" ]; do
  open "$APP"
  sleep 20
  w=$(pgrep -x wisp) || fail "Wisp did not start"
  [ "$(ps -o comm= -p "$w")" = "$BIN" ] || fail "the running wisp is not $BIN"
  printf 'round %d  %s  ' "$i" "$(date +%T)"; window on
  quit_wisp
  sleep 20
  printf 'round %d  %s  ' "$i" "$(date +%T)"; window off
  i=$((i + 1))
done
open "$APP"
rm -r "$tmp"
echo "end $(date '+%Y-%m-%d %H:%M:%S %z')"
