# WindowServer run 2026-10-19-bf55b5e

WindowServer load with and without Wisp, spec S4 ([METHOD.md](../METHOD.md), "S4: WindowServer load") · note:
**activity unknown: started by an agent; the owner's use of the machine was not observed, only `ioreg`'s HID idle time**

- Runs: two sets of 3 rounds (Wisp on, then Wisp off), 180 s of `top` samples per window (about 200 s of wall time
  with sampling overhead; the shares divide by the measured wall time), 20 s settle after each launch and quit
- Git: run 1 at `bf55b5e`, run 2 at `1502e25` on `ms/1-windowserver`, tree clean for both; the two differ only in
  `docs/benchmarks/METHOD.md`, so both measure the same Wisp source. Bundled with `scripts/bundle.sh` before each run
- Wisp: `target/Wisp.app`, version 0.1.0, sha256 `f90c75f099de961a05b7971a046c3f4c9108da82d54bae1d634a9970ff8e684a`
  (run 2's bundle), default 2 s interval, menu never opened
- Machine: Apple M5, 32 GB RAM, macOS 27.0.1 (26A434); 2 displays, extended (2560 × 1664 built-in, main;
  3440 × 1440 external), untouched; on battery at the end of run 2 (`pmset -g batt`)
- Other monitors: Stats not running. **Activity Monitor was running** (since 2026-09-29, the owner's); it stayed
  untouched through both runs, so it weighs on both arms alike, but it breaks the rule that other monitors are quit
- WindowServer pid 609 throughout

## Results

WS = WindowServer CPU share from its `ps` CPU-time delta; top = its mean `top` %CPU over the same window.

| Run | Round | Wisp | WS | WS top | Wisp's own | Min HID idle | Activity | Locked |
|---|---|---|---|---|---|---|---|---|
| 1 | 1 | on | 1.675% | 1.61% | 0.090% | 219.4 s | idle | no |
| 1 | 1 | off | 1.572% | 1.52% | – | 440.1 s | idle | no |
| 1 | 2 | on | 1.388% | 1.34% | 0.080% | 661.0 s | idle | no |
| 1 | 2 | off | 1.328% | 1.28% | – | 881.9 s | idle | no |
| 1 | 3 | on | **2.835%** | 2.78% | 0.090% | 0.1 s | **active** | no |
| 1 | 3 | off | 1.540% | 1.49% | – | 60.6 s | idle | no |
| 2 | 1 | on | 1.652% | 1.60% | 0.090% | 320.2 s | idle | no |
| 2 | 1 | off | 1.731% | 1.67% | – | 540.9 s | idle | no |
| 2 | 2 | on | 1.781% | 1.74% | 0.095% | 762.1 s | idle | no |
| 2 | 2 | off | 1.770% | 1.72% | – | 982.8 s | idle | no |
| 2 | 3 | on | 2.010% | 1.96% | 0.059% | 1203.4 s | idle | no |
| 2 | 3 | off | 1.174% | 1.12% | – | 1424.9 s | idle | no |

One window of twelve was active (run 1, round 3, Wisp on: input during the window, and its WS reading is the
highest by 0.8 pp); none was locked. Per METHOD.md it stays in the table but does not decide the verdict, and
because it left run 1's Wisp-on arm with two idle windows, a second set was run.

| Arm (WS %, idle windows) | n | Mean | Min–max | Spread |
|---|---|---|---|---|
| Run 1, Wisp on | 2 | 1.531% | 1.388–1.675% | 0.287 pp |
| Run 1, Wisp off | 3 | 1.480% | 1.328–1.572% | 0.244 pp |
| Run 2, Wisp on | 3 | 1.814% | 1.652–2.010% | 0.358 pp |
| Run 2, Wisp off | 3 | 1.558% | 1.174–1.770% | 0.596 pp |
| **Both, Wisp on** | 5 | **1.701%** | 1.388–2.010% | 0.622 pp |
| **Both, Wisp off** | 6 | **1.519%** | 1.174–1.770% | 0.596 pp |

With the active window counted, run 1's Wisp-on mean is 1.966% (both runs: 1.890%, n = 6).

**Difference between arms vs. spread within them:** Wisp on − off = **+0.18 pp** over the idle windows (run 1
+0.05, run 2 +0.26), against **0.60 pp** spread within the Wisp-off arm alone. The Wisp-on mean (1.70%) lies inside
the Wisp-off range (1.17–1.77%), and so do three of the five idle Wisp-on windows; the other two (1.781%, 2.010%)
are 0.01 and 0.24 pp above it, while back-to-back Wisp-off windows moved by 0.60 pp (run 2, 1.770% → 1.174%)
with Wisp quit in both.
Paired by round, on − off is +0.10, +0.06, −0.08, +0.01 and +0.84 pp (median +0.06). `top`'s means track the
`ps` figures to within 0.07 pp in every window.

**Verdict: S4 pass.** Wisp running adds no WindowServer load that rises above the noise between Wisp-off windows;
an offset of the order of Wisp's own share (~0.08%) cannot be resolved at this resolution either way.

Two short smoke tests while writing the script (12 s and 20 s windows, not runs of this method) read 8.9–11.5%
for WindowServer, right after a cargo build and with the agent's terminal active; the cause was not isolated, but
it shows that what else is on screen moves WindowServer by far more than the difference measured here.

## Raw

### Run 1

```
$ scripts/windowserver.sh
date 2026-10-19 00:28:49 +0600
git bf55b5e
macOS 27.0.1 (26A434), Apple M5
display 2560 x 1664 Retina
display 3440 x 1440 (UWQHD - Ultra-Wide Quad HD)
WindowServer pid 609, up 19-09:24:21
monitors running:  2926 /System/Applications/Utilities/Activity Monitor.app/Contents/MacOS/Activity Monitor
rounds 3, 180 s per window, 20 s settle
round 1  00:29:10  wisp on   ws 1.675%  ws_top 1.61%  wisp 0.090%  over 200 s  min_idle 219.4 s idle  locked no
round 1  00:32:50  wisp off  ws 1.572%  ws_top 1.52%  wisp -  over 201 s  min_idle 440.1 s idle  locked no
round 2  00:36:31  wisp on   ws 1.388%  ws_top 1.34%  wisp 0.080%  over 201 s  min_idle 661.0 s idle  locked no
round 2  00:40:12  wisp off  ws 1.328%  ws_top 1.28%  wisp -  over 201 s  min_idle 881.9 s idle  locked no
round 3  00:43:53  wisp on   ws 2.835%  ws_top 2.78%  wisp 0.090%  over 200 s  min_idle 0.1 s active  locked no
round 3  00:47:34  wisp off  ws 1.540%  ws_top 1.49%  wisp -  over 200 s  min_idle 60.6 s idle  locked no
end 2026-10-19 00:50:54 +0600
```

### Run 2

```
$ scripts/windowserver.sh
date 2026-10-19 00:51:33 +0600
git 1502e25
macOS 27.0.1 (26A434), Apple M5
display 2560 x 1664 Retina
display 3440 x 1440 (UWQHD - Ultra-Wide Quad HD)
WindowServer pid 609, up 19-09:47:05
monitors running:  2926 /System/Applications/Utilities/Activity Monitor.app/Contents/MacOS/Activity Monitor
rounds 3, 180 s per window, 20 s settle
round 1  00:51:53  wisp on   ws 1.652%  ws_top 1.60%  wisp 0.090%  over 201 s  min_idle 320.2 s idle  locked no
round 1  00:55:34  wisp off  ws 1.731%  ws_top 1.67%  wisp -  over 201 s  min_idle 540.9 s idle  locked no
round 2  00:59:15  wisp on   ws 1.781%  ws_top 1.74%  wisp 0.095%  over 201 s  min_idle 762.1 s idle  locked no
round 2  01:02:56  wisp off  ws 1.770%  ws_top 1.72%  wisp -  over 200 s  min_idle 982.8 s idle  locked no
round 3  01:06:36  wisp on   ws 2.010%  ws_top 1.96%  wisp 0.059%  over 202 s  min_idle 1203.4 s idle  locked no
round 3  01:10:18  wisp off  ws 1.174%  ws_top 1.12%  wisp -  over 201 s  min_idle 1424.9 s idle  locked no
end 2026-10-19 01:13:39 +0600
```
