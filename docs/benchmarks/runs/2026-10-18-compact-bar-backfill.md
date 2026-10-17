# Benchmark runs 2026-10-18 (compact bar, backfilled)

**Backfilled, pre-Method v1, metadata partly reconstructed.** These runs were made before
[METHOD.md](../METHOD.md) existed, by ad-hoc scripts, and recorded only as scratch files. They are the
source of the numbers in spec §1 "Measured (2026-10-18)". Raw lines below are copied verbatim from those
files; anything no file shows is marked *not recorded*. They are not comparable with Method v1 runs.

## Shared context

| | Value | Source |
|---|---|---|
| Code under test | the ms/1 layer view: `src/` identical at `5f74629` (merge of ms/1-layer-bar) and `e14577c` / `4351328` (docs only on top) | reconstructed; `git diff --stat 5f74629 4351328 -- src` is empty |
| Chip, OS | Apple M5, macOS 27.0.1 | reconstructed (same machine as today's records) |
| Displays | built-in 2560×1664 Retina + one external 3440×1440, extended | reconstructed |
| Stats | 3.0.21, CPU / GPU / RAM modules, `mini` widgets, 2 s interval; pid 23689 in every run, launched Fri Oct 16 11:28:09 2026 | config reconstructed; pid from the logs; launch time from `ps -o lstart=` of the same pid today |
| `measure.sh` | the version before `eecd5c4`/`110cbba`: footprint rounded to MB; cputime printed with awk's `%.6g` (Stats was below 10,000 s CPU, so no precision was lost) | git history |
| Power source, Low Power Mode, RAM, git dirty state | not recorded | — |
| Idle or normal use | not recorded | — |

## S1 / S2 / S5: 10-minute runs, Wisp next to Stats

### Run A — first real run, 03:03:21 → 03:13:2x

- Script: `real_run.sh`: `open target/Wisp.app`, `sleep 20`, then `scripts/measure.sh <pid> 600` on both
  in parallel. Bundled release build; sha256 not recorded (the bundle was rebuilt since).
- **Rule breaches against Method v1:** one System Events accessibility query of the item at the start
  (the `875, 7, 130, 24` line), and `screencapture` of both menu bars every 150 s during the window.
- The spec's `15,270,728 B` for this run's footprint is in no surviving file: not recorded here.

```
wisp 12380 stats 23689 start 03:03:21
875, 7, 130, 24
end 03:15:57
WISP
cpu 0.027%  idle wakeups 0.12/s  over 600 s
wisp [12380]: 64-bit    Footprint: 15 MB (16384 bytes per page)
STATS
cpu 2.143%  idle wakeups 0.87/s  over 600 s
Stats [23689]: 64-bit    Footprint: 135 MB (16384 bytes per page)
DONE
```

| | Wisp | Stats | Stats ÷ Wisp |
|---|---|---|---|
| CPU share | 0.027% | 2.143% | 79× |
| Footprint (rounded by `footprint`) | 15 MB | 135 MB | 9× |

### Run B — clean repeat, ended 04:03:08

- No accessibility polling, no screenshots (per the spec). Script not kept; window start not recorded
  (the result files were written at 04:03:08, so with 600 s it began about 03:53). Build and how
  Wisp was launched: not recorded.
- The two `phys_footprint` lines follow the measurements in the task output; which process they belong
  to is not printed. The spec reads them as Wisp's (14.8 MiB).

```
wisp 71227 stats 23689
cpu 0.043%  idle wakeups 0.17/s  over 600 s
wisp [71227]: 64-bit    Footprint: 15 MB (16384 bytes per page)
cpu 2.027%  idle wakeups 0.69/s  over 600 s
Stats [23689]: 64-bit    Footprint: 137 MB (16384 bytes per page)
    phys_footprint: 15516488 B
    phys_footprint_peak: 16433992 B
```

| | Wisp | Stats | Stats ÷ Wisp |
|---|---|---|---|
| CPU share | 0.043% | 2.027% | 47× |
| Footprint (rounded by `footprint`) | 15 MB | 137 MB | 9× |

## S8: forced-redraw cost, 120 s runs

Builds: unbundled release binaries run directly (`nohup <dir>/wisp`), 20 s settle, then
`scripts/measure.sh <pid> 120`, forced and unpatched (`base`) alternated, one Wisp at a time. Both
binaries are still on disk, unchanged since 02:16, before the first run:

| Build | sha256 |
|---|---|
| `forced` (CPU alternating 11/88 each tick; the exact patch text was not kept) | `d02e3dbc3582d55f9717a29150cc9ac1f53d26cade10f620cec6b82e7b1f8847` |
| `base` (unpatched) | `ed36be3b4da51abb5afe600d3fe819e89393338777f1466420ff09cac9666bfe` |

ms per redraw (forced) or per tick (base) = CPU share × 120 s ÷ 60 ticks.

| Set | Time | Build | pid | CPU | ms | Footprint | Notes |
|---|---|---|---|---|---|---|---|
| 1 | 02:17:16 | forced | 56052 | 0.217% | 4.34 | 19 MB | screenshots at +60 s; 19 MB matches a first menu open |
| 1 | 02:19:40 | base | 59045 | 0.033% | 0.66 | 13 MB | screenshots |
| 1 | 02:22:05 | forced | 61875 | 0.125% | 2.50 | 14 MB | screenshots |
| 1 | 02:24:30 | base | 64648 | 0.025% | 0.50 | 13 MB | screenshots |
| 2 | 02:44:15 | forced | 88834 | 0.042% | 0.84 | 13 MB | accessibility position query before and after |
| 2 | 02:55:09 | base | 1931 | 0.042% | 0.84 | 14 MB | the start query failed (`Application isn't running`) |
| 2 | 02:57:34 | forced | 4941 | 0.042% | 0.84 | 13 MB | accessibility query before and after |
| 2 | 02:59:59 | base | 7891 | 0.025% | 0.50 | 14 MB | accessibility query before and after |
| 3 | 03:21:02 | forced | 33632 | 0.042% | 0.84 | 13 MB | `sample <pid> 120` running alongside; accessibility queries |
| 3 | 03:23:27 | base | 36244 | 0.025% | 0.50 | 13 MB | accessibility queries |
| 3 | 03:25:52 | forced | 38816 | 0.050% | 1.00 | 13 MB | `sample` alongside; accessibility queries |
| 4 | ended 04:18:17 | forced (per spec; the output does not name the build) | 83782 | 0.058% | 1.16 | 14 MB | two back-to-back windows on the same process |
| 4 | | | 83782 | 0.058% | 1.16 | 14 MB | |

Set 1 — `forced_runs.log` (the shell's `Terminated: 15` lines, logged as the script killed each Wisp, are left out):

```
== forced pid 56052 02:17:16
cpu 0.217%  idle wakeups 0.60/s  over 120 s
wisp [56052]: 64-bit    Footprint: 19 MB (16384 bytes per page)
  0:00.38
== base pid 59045 02:19:40
cpu 0.033%  idle wakeups 0.28/s  over 120 s
wisp [59045]: 64-bit    Footprint: 13 MB (16384 bytes per page)
  0:00.14
== forced pid 61875 02:22:05
cpu 0.125%  idle wakeups 0.35/s  over 120 s
wisp [61875]: 64-bit    Footprint: 14 MB (16384 bytes per page)
  0:00.26
== base pid 64648 02:24:30
cpu 0.025%  idle wakeups 0.33/s  over 120 s
wisp [64648]: 64-bit    Footprint: 13 MB (16384 bytes per page)
  0:00.13
DONE
```

Set 2 — `forced_runs2_part1.log`, then `forced_runs2.log` (the `0:00.xx` row is `ps -o cputime=` every 10 s):

```
== forced pid 88834 02:44:15 at 1629, -1437
0:00.11 0:00.12 0:00.12 0:00.13 0:00.13 0:00.13 0:00.14 0:00.14 0:00.15 0:00.15 0:00.16 0:00.16 
cpu 0.042%  idle wakeups 0.24/s  over 120 s
wisp [88834]: 64-bit    Footprint: 13 MB (16384 bytes per page)
```

```
63:73: execution error: System Events got an error: Application isn’t running. (-600)
== base pid 1931 02:55:09 at 
0:00.09 0:00.09 0:00.09 0:00.09 0:00.10 0:00.10 0:00.10 0:00.10 0:00.10 0:00.11 0:00.13 0:00.14 
cpu 0.042%  idle wakeups 0.26/s  over 120 s
wisp [1931]: 64-bit    Footprint: 14 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.15
== forced pid 4941 02:57:34 at 1629, -1437
0:00.11 0:00.12 0:00.12 0:00.13 0:00.13 0:00.13 0:00.14 0:00.14 0:00.15 0:00.15 0:00.15 0:00.16 
cpu 0.042%  idle wakeups 0.27/s  over 120 s
wisp [4941]: 64-bit    Footprint: 13 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.16
== base pid 7891 02:59:59 at 1629, -1437
0:00.11 0:00.11 0:00.11 0:00.12 0:00.12 0:00.12 0:00.12 0:00.12 0:00.13 0:00.13 0:00.13 0:00.13 
cpu 0.025%  idle wakeups 0.16/s  over 120 s
wisp [7891]: 64-bit    Footprint: 14 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.14
DONE
```

Set 3 — `forced_runs3.log`:

```
== forced pid 33632 03:21:02 at 1629, -1437
0:00.12 0:00.12 0:00.13 0:00.13 0:00.14 0:00.14 0:00.15 0:00.15 0:00.16 0:00.16 0:00.16 0:00.17 
cpu 0.042%  idle wakeups 0.12/s  over 120 s
wisp [33632]: 64-bit    Footprint: 13 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.18
== base pid 36244 03:23:27 at 1629, -1437
0:00.13 0:00.13 0:00.14 0:00.14 0:00.14 0:00.15 0:00.15 0:00.15 0:00.15 0:00.16 0:00.16 0:00.16 
cpu 0.025%  idle wakeups 0.13/s  over 120 s
wisp [36244]: 64-bit    Footprint: 13 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.16
== forced pid 38816 03:25:52 at 1629, -1437
0:00.08 0:00.09 0:00.09 0:00.09 0:00.10 0:00.10 0:00.11 0:00.11 0:00.12 0:00.12 0:00.13 0:00.13 
cpu 0.050%  idle wakeups 0.17/s  over 120 s
wisp [38816]: 64-bit    Footprint: 13 MB (16384 bytes per page)
end at 1629, -1437 cputime   0:00.14
DONE
```

Set 4 — background task output, ended 04:18:17. The `t=` lines are a heap / footprint trace of the
same process around the first menu open (spec §1, S2: the +5.4 MB step); their exact timing relative to
the two windows is not recorded.

```
cpu 0.058%  idle wakeups 0.24/s  over 120 s
wisp [83782]: 64-bit    Footprint: 14 MB (16384 bytes per page)
cpu 0.058%  idle wakeups 0.17/s  over 120 s
wisp [83782]: 64-bit    Footprint: 14 MB (16384 bytes per page)
t=04:26 All zones: 28783 nodes (4210144 bytes)     phys_footprint: 14385944 B
t=09:27 All zones: 40902 nodes (6091712 bytes)     phys_footprint: 19956672 B
t=14:27 All zones: 40913 nodes (6083568 bytes)     phys_footprint: 19743680 B
```
