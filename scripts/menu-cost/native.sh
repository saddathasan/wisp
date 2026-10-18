#!/bin/bash
# Native-behaviour check of one variant (one-off; uses AX once, so never mixed with measurement runs).
# usage: [X= Y= OUTX=] native.sh <outdir> <app name> <WISP_MENU> [light]   (X Y = status item point, OUTX = empty bar x)
# CAUTION: synthetic keys go wherever the system routes them. Run only on an idle machine: with someone using it,
# a neighbour's menu or the frontmost app got keys meant for Wisp's menu, even with `guard` passing.
set -u
cd /Users/saddat/codeRepository/wisp
HERE=$(pwd)/scripts/menu-cost
S=${TMPDIR:-/tmp}/wisp-menu-tools; mkdir -p "$S"
for t in key mv click haswin guard; do [ -x "$S/$t" ] || swiftc -O -o "$S/$t" "$HERE/$t.swift"; done
N=$1; mkdir -p "$N"; M=scripts/menu-cost/menu; X=${X:-926}; Y=${Y:--1065}; OUTX=${OUTX:-600}
shot() { screencapture -x -R "$((MX - 60)),$((MY - 40)),560,460" "$N/$1.png"; }
# Every key first checks that only Wisp has a menu window on screen; a check, not a guarantee (see CAUTION).
key() { $S/guard $P || { echo "ABORT before key $*"; exit 1; }; $S/key "$@"; sleep 0.35; }
pkill -x wisp; sleep 1; open -n --env WISP_MENU=$3 "target/$2.app"; sleep 6; P=$(pgrep -x wisp)
{
L=$($M open $P $X $Y) || exit 1; echo "$L"; MX=$(echo "$L" | sed -E "s/.*\{\{([-0-9.]+), ([-0-9.]+)\}.*/\1/"); MY=$(echo "$L" | sed -E "s/.*\{\{([-0-9.]+), ([-0-9.]+)\}.*/\2/"); MX=${MX%.*}; MY=${MY%.*}
$S/mv $((MX + 40)) $((MY + 15)); sleep 2.5; shot dark-open
osascript scripts/menu-cost/ax_rows.applescript $P > "$N/ax.txt" 2>&1
# Submenu by hover: Update Interval sits 292 pt below the menu top (-1041).
$S/mv $((MX + 60)) $((MY + 292)); sleep 1.2; shot submenu-hover; key 53; key 53; sleep 0.5; echo "after 2x Escape: menu windows $($S/haswin $P)"
# Keyboard: Down x3 highlights; Up from the top wraps to the bottom; Right enters the submenu; Return picks 3 s.
$M open $P $X $Y; $S/mv $((MX + 40)) $((MY + 15)); sleep 0.8; key 125; key 125; key 125; sleep 0.3; shot kbd-down3
key 126; key 126; key 126; key 126; key 126; key 126; sleep 0.3; shot kbd-up-wrap
$M close $P; $M open $P $X $Y; $S/mv $((MX + 40)) $((MY + 15)); sleep 0.8
key 126; key 126; key 126; key 124; sleep 0.5; key 125; key 125; shot kbd-submenu-3s; key 36; sleep 0.8; echo "after Return: menu windows $($S/haswin $P)"
$M open $P $X $Y; $S/mv $((MX + 60)) $((MY + 292)); sleep 1.2; shot submenu-after-return; key 53; key 53; sleep 0.5
# Outside click on empty menu bar of the same display.
$M open $P $X $Y; sleep 0.8; $S/click $OUTX $Y; sleep 1; echo "after outside click: menu windows $($S/haswin $P)"
if [ "${4:-}" = light ]; then
  osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to false'; sleep 3
  $M open $P $X $Y; $S/mv $((MX + 40)) $((MY + 15)); sleep 2.5; shot light-open; $M close $P
  osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to true'; sleep 3
fi
# Cmd-Q while open quits.
$M open $P $X $Y; sleep 0.8; key 12 cmd 2>/dev/null; sleep 1.5; echo "after cmd-Q: wisp running: $(pgrep -x wisp || echo no)"
} > "$N/log.txt" 2>&1
cat "$N/log.txt"
