#!/bin/sh
# Measures a running process's CPU share, idle wakeups/s and footprint over a window (default 60 s).
# Uses counter deltas: top's IDLEW column is cumulative since launch, so averaging it is meaningless.
set -eu
PID=${1:?usage: measure.sh <pid> [seconds]}
SECS=${2:-60}
# macOS prints cputime as M:SS.cc with unbounded minutes (2763:15.73, never H:MM:SS); printf, because
# awk's default %.6g output drops the centiseconds once a long-running process passes 10,000 s.
cpu() { ps -o cputime= -p "$PID" | awk -F'[:.]' '{ printf "%.2f", $1 * 60 + $2 + $3 / 100 }'; }
wakeups() { top -l 1 -pid "$PID" -stats idlew | tail -n 1 | tr -d '+ '; }
c0=$(cpu); w0=$(wakeups)
sleep "$SECS"
c1=$(cpu); w1=$(wakeups)
awk -v c0="$c0" -v c1="$c1" -v w0="$w0" -v w1="$w1" -v s="$SECS" \
  'BEGIN { printf "cpu %.3f%%  idle wakeups %.2f/s  over %d s\n", (c1 - c0) / s * 100, (w1 - w0) / s, s }'
footprint "$PID" | grep 'Footprint:'
