#!/bin/sh
# Runs benchmark Method v1 (docs/benchmarks/METHOD.md): Wisp and Stats measured over one shared window,
# recorded with the build, machine and Stats config in docs/benchmarks/runs/<date>-<sha>.md for committing.
set -eu
cd "$(dirname "$0")/.."
export LC_ALL=C
NOTE=${1:?usage: compare.sh \"<idle | normal use | ...>\" [seconds]}
SECS=${2:-600}
METHOD=v1
APP=target/Wisp.app
BIN=$PWD/$APP/Contents/MacOS/wisp
fail() { echo "compare.sh: $*" >&2; exit 1; }

[ -x "$BIN" ] || fail "no $APP; run scripts/bundle.sh first"
s=$(pgrep -x Stats) || fail "Stats is not running"
if ! w=$(pgrep -x wisp); then
  open "$APP"
  sleep 20 # Method v1 settle after launch.
  w=$(pgrep -x wisp) || fail "Wisp did not start"
fi
[ "$(ps -o comm= -p "$w")" = "$BIN" ] || fail "the running wisp is not $BIN; quit it and rerun"
# A bundle rebuilt after launch would make the recorded sha256 describe a binary that is not running.
started=$(date -j -f '%a %b %e %T %Y' "$(ps -o lstart= -p "$w" | sed 's/ *$//')" +%s)
[ "$(stat -f %m "$BIN")" -le "$started" ] || fail "$BIN was rebuilt after Wisp launched; quit it and rerun"

sha=$(git rev-parse --short HEAD)
dirty=$(git status --porcelain)
out=docs/benchmarks/runs/$(date +%F)-$sha.md
n=2
while [ -e "$out" ]; do out=docs/benchmarks/runs/$(date +%F)-$sha-$n.md; n=$((n + 1)); done
stats_bin=$(ps -o comm= -p "$s")
stats_plist=${stats_bin%/MacOS/*}/Info.plist
stats_defaults=$(defaults read eu.exelban.Stats)
enabled=$(echo "$stats_defaults" | sed -n 's/^ *"\{0,1\}\([A-Za-z]*\)_state"\{0,1\} = 1;$/\1/p' | paste -sd '|' -)
wisp_up=$(ps -o etime= -p "$w" | tr -d ' ')
stats_up=$(ps -o etime= -p "$s" | tr -d ' ')
displays=$(system_profiler SPDisplaysDataType)
start=$(date '+%Y-%m-%d %H:%M:%S %z')

echo "measuring wisp $w and Stats $s for $SECS s; open neither menu" >&2
tmp=$(mktemp -d)
scripts/measure.sh "$w" "$SECS" >"$tmp/wisp" 2>&1 &
scripts/measure.sh "$s" "$SECS" >"$tmp/stats" 2>&1 &
wait
end=$(date '+%Y-%m-%d %H:%M:%S %z')

cpu() { awk '/^cpu/ { sub("%", "", $2); print $2 }' "$1"; }
bytes() { awk '{ for (i = 1; i < NF; i++) if ($i == "Footprint:") print $(i + 1) }' "$1"; }
wc_=$(cpu "$tmp/wisp"); sc=$(cpu "$tmp/stats"); wb=$(bytes "$tmp/wisp"); sb=$(bytes "$tmp/stats")
[ -n "$wc_" ] && [ -n "$sc" ] && [ -n "$wb" ] && [ -n "$sb" ] || fail "measure.sh failed: $(cat "$tmp/wisp" "$tmp/stats")"
table=$(awk -v wc="$wc_" -v sc="$sc" -v wb="$wb" -v sb="$sb" 'BEGIN {
  print "| | Wisp | Stats | Stats ÷ Wisp |"
  print "|---|---|---|---|"
  printf "| CPU share | %s%% | %s%% | %s |\n", wc, sc, (wc > 0 ? sprintf("%.0f×", sc / wc) : "n/a (Wisp 0)")
  printf "| Physical footprint | %.1f MiB (%d B) | %.1f MiB (%d B) | %.1f× |\n", wb / 1048576, wb, sb / 1048576, sb, sb / wb
}')

mkdir -p "$(dirname "$out")"
cat >"$out" <<EOF
# Benchmark run $(basename "$out" .md)

**Method $METHOD** ([METHOD.md](../METHOD.md)) · note: **$NOTE**

- Window: $start → $end ($SECS s)
- Git: \`$(git rev-parse HEAD)\` on \`$(git branch --show-current)\`, tree $([ -n "$dirty" ] && echo "**dirty**" || echo clean)
- Wisp: \`$BIN\`, version $(plutil -extract CFBundleShortVersionString raw -o - "$APP/Contents/Info.plist"), sha256 \`$(shasum -a 256 "$BIN" | cut -d ' ' -f 1)\`, pid $w, up $wisp_up at start
- Stats: \`$stats_bin\`, version $(plutil -extract CFBundleShortVersionString raw -o - "$stats_plist") ($(plutil -extract CFBundleVersion raw -o - "$stats_plist")), pid $s, up $stats_up at start
- Machine: $(sysctl -n machdep.cpu.brand_string), $(($(sysctl -n hw.memsize) / 1073741824)) GB RAM, macOS $(sw_vers -productVersion) ($(sw_vers -buildVersion))
- Displays: $(echo "$displays" | grep -c 'Resolution:') ($(echo "$displays" | sed -n 's/^ *Resolution: //p' | paste -sd ';' - | sed 's/;/; /g'))
- Power: $(pmset -g batt | head -n 1 | sed "s/^Now drawing from //"), Low Power Mode $(pmset -g | awk '/lowpowermode/ { print ($2 ? "on" : "off") }')

## Results

$table

## Raw

\`\`\`
\$ scripts/measure.sh $w $SECS   # wisp
$(cat "$tmp/wisp")
\$ scripts/measure.sh $s $SECS   # Stats
$(cat "$tmp/stats")
\`\`\`

## Environment

Stats config (\`defaults read eu.exelban.Stats\`, enabled modules):

\`\`\`
$(echo "$stats_defaults" | grep -E "^ *\"?($enabled)_(state|widget|updateInterval)\"? = ")
\`\`\`

Displays (\`system_profiler SPDisplaysDataType\`):

\`\`\`
$(echo "$displays" | sed -n '/^ *Displays:/,$p')
\`\`\`

Power (\`pmset -g batt\`):

\`\`\`
$(pmset -g batt)
\`\`\`
$([ -n "$dirty" ] && printf '\nUncommitted changes at start (`git status --porcelain`):\n\n```\n%s\n```\n' "$dirty")
EOF
rm -r "$tmp"
echo "$out"
