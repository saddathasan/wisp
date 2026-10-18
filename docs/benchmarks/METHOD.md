# Benchmark method

**Method v1** · adopted 2026-10-18

How Wisp's cost is compared with Stats. Every public claim that Wisp is cheaper than Stats rests on run
records in [`runs/`](runs/) made with this method. Anyone with both apps can re-run it.

**Versioning.** Any change to the procedure, the tools, the Stats config or the rules below bumps the
method version (v2, v3, ...). Records carry the version they were made with, and only runs of the same
version are compared with each other.

## What is compared

| Measure | How | Why |
|---|---|---|
| **CPU share** over the window | CPU-time delta ÷ wall time, from `ps -o cputime=` at the start and end (`scripts/measure.sh`) | the process's real average cost, independent of sampling luck |
| **Physical footprint** at the end | `footprint -f bytes <pid>` (`scripts/measure.sh`) | what the process costs the system in memory, the figure Activity Monitor's Memory column is based on |

`measure.sh` also prints idle wakeups. **They are not used:** `top`'s IDLEW counter is coarse and
moves with things outside the app, so its deltas are unreliable at these levels.

## Setup

**Wisp.** The bundled release build from the commit under test:

```sh
scripts/bundle.sh          # cargo build --release, wrapped and ad-hoc signed in target/Wisp.app
open target/Wisp.app
```

The default 2 s update interval. No dev builds, no `cargo run`.

**Stats** (version recorded per run; 3.0.21 for the first v1 runs), configured to show what Wisp shows:

- modules **CPU, GPU and RAM** enabled, every other module off;
- each with the **`mini`** widget;
- each with a **2 s** update interval.

`compare.sh` records Stats' `*_state`, `*_widget` and `*_updateInterval` keys for the enabled modules
from `defaults read eu.exelban.Stats`, so a run made with a different config is visible in its record.

## Procedure

```sh
scripts/bundle.sh
pkill -x wisp              # if a Wisp is running, so the run starts from a fresh launch
scripts/compare.sh "idle"  # or "normal use"; optional 2nd argument: seconds (default 600)
```

`compare.sh`:

1. checks that Stats is running and that the running Wisp (if any) is `target/Wisp.app` and was
   launched after the bundle was built;
2. if Wisp is not running, opens `target/Wisp.app` and waits **20 s** to settle;
3. runs `scripts/measure.sh` on **both processes in parallel over the same 600 s window**;
4. writes `runs/<YYYY-MM-DD>-<shortsha>.md` (`-2`, `-3` ... for repeats of the same commit and day).

## Rules during a run

- **Never open either app's menu.** In-process, a first NSMenu open permanently adds 4.5–7.3 MiB of
  footprint ([research](../research/menu-memory-cost.md)); Wisp's dropdown runs in a separate helper
  process that `measure.sh` doesn't count (ADR 0001). A run in which a menu was opened is void.
- **Nothing polls the apps' accessibility tree** (System Events UI scripting, Accessibility Inspector,
  screen readers): answering it allocates and runs code inside the app.
- **Screen awake** and unlocked for the whole window: Wisp pauses on display sleep and lock (spec
  §3.3), so a window that includes either measures nothing comparable.
- **Note** whether the machine was **idle** or in **normal use**; it is the first argument of
  `compare.sh` and is written into the record.

## What a record holds

Written by `compare.sh`, never edited by hand afterwards:

- method version, note, start and end of the window;
- git commit, branch and whether the tree was dirty (with the uncommitted paths);
- Wisp binary path, version and sha256; Stats path and version (`Info.plist`) and its module config;
- chip, RAM, macOS version and build; displays (count, resolutions, `system_profiler` block); power
  source and Low Power Mode (`pmset`);
- both processes' pids and uptimes at the start;
- the results table (CPU share and footprint for each, Stats ÷ Wisp);
- the raw `measure.sh` output, verbatim.

## How to report

- Give **both numbers and the ratio**, with the record's link, method version and machine, e.g.
  "Wisp <cpu>% CPU, <n> MiB; Stats <cpu>%, <n> MiB (<ratio>×) — Method v1, <chip>, macOS <version>,
  [record](runs/...)".
- **Noise:** CPU share at these levels moves by about **±20% between runs** of the same build;
  claims use the range across runs, not the best one.
- **Never cherry-pick.** Every run is committed, including bad, odd and failed ones; a run that broke
  a rule stays in `runs/` with the breach noted under it.

## S8: forced-redraw cost (Wisp only)

How much one redraw of the menu bar item costs (spec §1, S8: ≤ 2 ms). The patch
`scripts/forced-redraw.patch` makes `StatusItem::show` in `src/ui/status.rs` alternate CPU between 11%
and 88% every tick, so every tick changes a value and redraws.

```sh
git apply scripts/forced-redraw.patch
scripts/bundle.sh
pkill -x wisp; open target/Wisp.app
sleep 20                                  # settle
scripts/measure.sh "$(pgrep -x wisp)" 120
git apply -R scripts/forced-redraw.patch  # revert; never commit the patched tree
scripts/bundle.sh                         # rebuild the real app
```

At the 2 s interval, 120 s is 60 redraws: **ms per redraw = CPU seconds ÷ 60 × 1000**, where CPU
seconds = CPU share × 120 s (e.g. `cpu 0.042%` → 0.0504 s → 0.84 ms). The unpatched build measured the
same way gives the per-tick floor (sampling and timer); run them interleaved, forced and unpatched
alternately. The same rules apply: no menu, no accessibility polling, screen awake. Record the raw
output in `runs/` with the patch's commit.

## Dropdown: open latency and footprint (Wisp only)

How fast the dropdown helper (ADR 0001) opens, what it costs while open, and that the main process keeps nothing
after it. The patch `scripts/open-latency.patch` makes the main process, after the 20 s settle, `performClick` its own
button 5 times: the real click path, no synthetic input. Each helper closes its menu itself after 3 s (`cancelTracking`,
as an outside click would) and the next click comes 5 s after it exits. Both processes log `<epoch ms> <pid> <event>`
to the same stderr: `click N` (main), `popUp`, `visible` (the helper's menu window is on screen), `closed` (helper),
`exit N helper <pid> status S` (main) and `done`.

```sh
git apply scripts/open-latency.patch
scripts/bundle.sh
pkill -x wisp; open --stderr /tmp/wisp-open.log target/Wisp.app
# ~17 s in, main before:  footprint -f bytes --noCategories <main pid>; vmmap <main pid> | grep '^__TEXT'
# ~2 s into each open:     footprint -f bytes --noCategories "$(pgrep -P <main pid> -x wisp)"   # the helper
# 3 s after `done`:        footprint and vmmap of the main process again
git apply -R scripts/open-latency.patch   # revert; never commit the patched tree
scripts/bundle.sh                         # rebuild the real app
```

- **Open latency** = `visible` − `click` for each open; `popUp` − `click` is the process start.
- **Helper peak** = the helper's `phys_footprint_peak` while its menu is up.
- **Main Δ** = main footprint after the 5 opens − before; it must stay ≈ 0. Main's `__TEXT` image list must not
  change (no ServiceManagement, no SwiftUI).

The menu shows on screen for 3 s at a time: don't click or type during the run, or the hold ends early. Record the raw
log and footprints in `runs/` with the commit the patch was applied to.
