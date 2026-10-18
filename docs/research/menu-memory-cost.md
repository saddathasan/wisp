# Dropdown memory cost (2026-10-18)

The research spike behind [ADR 0001](../adr/0001-dropdown-in-helper-process.md): what the dropdown costs in memory, latency and CPU for each way of building it. The constraint was that the dropdown must not hog memory. Measured on an Apple M5 running macOS 27.0.1, with no synthetic input in the final batch: Wisp opens its own dropdown through `button.performClick`, from a `WISP_DRIVE` test hook. The prototypes are on the branch [`exp/menu-cost`](https://github.com/saddathasan/wisp/tree/exp/menu-cost) (eeb12a6 → 90f7620), which is never merged. Each variant is selected with `WISP_MENU=a|b|ca|cb|d|dv|ds|e|g`, and the measurement scripts are in `scripts/menu-cost/` on that branch. The generated summary tables are reproduced verbatim in the appendix. The full raw captures (vmmap, heap, footprint and dyld image lists) are kept privately, because they include unrelated details of the measuring machine.

## Where the first-open memory goes (NSMenu)

The first NSMenu open keeps **+4.5 to +7.3 MiB** of footprint for the life of the process, and later opens add about nothing. The size depends on the display: +7.3 with the 3440×1440 external display (custom ICC profile), +4.5 on the built-in P3 alone. The menu window itself is freed on close; what stays is process-wide warm-up:

| Bucket | MB | Evidence |
|---|---|---|
| ColorSync transforms and TRC tables for the display's profile, plus vImage converters | ~1.7 (external), ~0.3 (built-in) | 66 × 140 KB `MakeInvertedTRC`/`MakeTRC` tables; Accelerate region +0.26 |
| SwiftUI + MenuBarClientCore + 4 small images, loaded to host the menu's `NSGlassEffectView` | ~1.6–2.2 | dyld diff on first open; dirty `__DATA` +1.48 MB; Swift metadata; AttributeGraph |
| ObjC method caches and class realisation | 0.4–0.7 | ~960 new cache arrays |
| SF Symbols / CoreUI / CoreSVG | 0.1–0.5 | design-library catalog |
| NSMenu objects, strings, fonts | 0.5–1.0 | ~3,000 CFStrings; only ~0.3 MB is NSMenu-specific |
| malloc fragmentation and metadata | 0.8–1.5 | region growth with no live allocations |

**Nothing gives it back.** `malloc_zone_pressure_relief` returns 0 bytes, and `removeAllItems` plus `setMenu(nil)` leave the footprint byte-identical. `UIDesignRequiresCompatibility` has no effect at SDK 26 or SDK 27: the menu keeps the same glass, loads the same images and keeps the same step. Apple drops the key for apps built with the 27 SDK anyway. No documented API purges these caches, and none of the apps checked (Stats, menu bar guides) does anything about it.

## Comparison

Δ = footprint growth over the same launch's "before" (13.2–15.0 MiB; the baseline drifts over a session, so compare Δs). Medians of 2–3 fresh launches each, using the in-process driver. Latency runs from `performClick` to the window being on screen.

| | Variant | Δ open | Δ after close (1×) | Δ after 5× | First open | Later opens | CPU open (closed ≈ 0.03–0.2%) | Loads SwiftUI |
|---|---|---|---|---|---|---|---|---|
| — | Quit-only NSMenu (main today) | +4.4 | +4.3–4.5 | +4.5–4.7 | 47–78 ms | 40 ms | 0.03–0.05% | yes |
| a | NSMenu, text items | +6.8–8.1 | +6.0–6.9 | +5.8–7.9 | 78–98 ms | 53–55 ms | 0.15–0.28% | yes |
| b | NSMenu, layer-drawn row views | +7.1 | +6.2 | +6.4 | 89–98 ms | 52 ms | 0.18% | yes |
| c | a or b built on open, released on close | = a / b | = a / b | = a / b | ≈ a / b | slower (126 vs 110 ms, synthetic clicks) | ≈ | yes |
| f | a + `UIDesignRequiresCompatibility` (SDK 26 / 27) | = a | = a | = a | = a | = a | ≈ | yes |
| d | borderless NSPanel, CALayers, released on close | +1.8–2.3 | +1.0–2.0 | +2.2–2.5 | 48–74 ms | 52 ms | 0.17–0.23% | **no** |
| ds | d with an sRGB window | +1.8 | +1.1 (ext +1.3) | +1.2 | 61–71 ms | 49 ms | 0.25% | no |
| dv | d on an NSVisualEffectView background | +2.0–2.2 | +1.4–1.9 | +2.0–2.4 | 49–70 ms | 40–57 ms | 0.10–0.62% (noisy) | no |
| e | NSPopover with the same layer rows | **+11.8** | **+10.4** | **+11.0** | 83–104 ms | 63 ms | 0.23% | yes, plus GPU memory |
| g | **out-of-process NSMenu**: a helper (`wisp --dropdown x y`) shows it and exits on close | **+0.1** | **0.0** | **0.0** | 138–171 ms | 150 ms | main 0.07%, helper 0.15% | in the helper only |

g's helper peaks at 20.2–20.5 MiB while open and returns all of it on exit (~9 ms after the 242 ms menu fade). 82 ms of its latency is process start. For scale: the earlier synthetic-click runs measured a real NSMenu first open at 150–280 ms from mouse-down, so g is about as fast as a native first open on every open.

**CPU:** holding even the Quit-only menu open costs 0.05–0.4%. Most of the cost while open is the menu being up, not Wisp's live updates. The menu is only open for seconds at a time, so it doesn't affect S1.

## Native behaviour

| | Keyboard | VoiceOver | Dark / light | Dismissal | Spec fit |
|---|---|---|---|---|---|
| a | native: arrows, type-select, Return, Escape, ⌘Q, submenu | native menu items | follows the menu bar | native | ✗ macOS 27 greys disabled items whatever their colour, and enabled items highlight on hover |
| b | native; view rows are skipped, which is right for info rows | rows read as static text ("CPU 42%") | yes | native | ✓ matches §3.2 |
| d / ds / dv | Escape only today; arrows, Return, ⌘Q and the submenu all need hand-building | 16 AXStaticText rows; Quit, Interval and Login aren't buttons, and headers aren't headings, so all of it needs hand-building | own drawing; dv gets the system material | resign-key only; outside clicks on the menu bar or another item likely don't close it, and a global monitor needs `block2` | ✗ the spec lists popover windows as a non-goal; the look isn't the system menu |
| e | Escape | same content view as d | yes | transient | ✗ the worst memory, and a non-goal |
| g | native (it is variant a's or b's NSMenu) | native | yes | native | ✓ if the helper uses b's rows; two processes |

Not yet verified (needs real input; checked when the feature is built): outside-click dismissal for d and e, keyboard focus in d, e and g, and VoiceOver activation.

## Method

- **Footprint:** `footprint -f bytes` at fixed points of a fresh launch: before any open (after the 20 s settle), while open (during the first hold), after-1 (5 s after the first close) and after-5. Each variant ran 2–3 fresh launches.
- **Driver:** `WISP_DRIVE=n,hold,gap` calls `button.performClick(None)`, the real click path, holds each open with live updates, closes it, then waits. Latency is from just before `performClick` to the first 1 ms poll where one of Wisp's windows (or the helper's) is on screen. That is "on screen", not "first frame drawn". The two NSMenu tables in the appendix used real synthetic clicks instead, timed from mouse-down to the window appearing in CGWindowList.
- **CPU:** `scripts/measure.sh <pid> 60`, with the dropdown held open for 60 s and again with it closed. At these levels a 60 s window is noisy (±0.1 points).
- **Attribution:** `vmmap`, `heap`, `footprint -v` diffs, a `MallocStackLogging` run, and dyld image lists before and after the first open.
- **Displays** (all identified by the logged window colour space):
  - the first-open attribution: the built-in 2560×1664 P3 plus a 3440×1440 external display;
  - the first NSMenu table: a 1920×1080 external;
  - the in-process batches: the built-in display alone, then the built-in plus the 1920×1080 external.
- Never open a menu during a Method v1 benchmark run ([METHOD.md](../benchmarks/METHOD.md)). These were separate runs.

## Appendix: generated summaries (verbatim)

Footprint figures are MiB per launch. CPU is a percentage.

NSMenu family, synthetic clicks, 1920×1080 external display (`f26`/`f27` = the key at SDK 26/27, `s26` = SDK 26 without the key; the `ca` launch at 20.3 MiB before any open is an outlier, and `s26-locked` broke when the screen locked):

```
variant            before             open           after1           after5             Δ1    first ms       later ms (n)     cpu closed       cpu open
a          14.2/14.7/14.9   23.5/22.7/22.7   23.3/22.3/22.4   23.1/22.1/22.2    9.1/7.7/7.5 256/116/154  114 [97-136] (12)    0.083/0.100    0.433/0.300
b          14.4/15.5/15.1   23.2/23.4/22.8   24.7/23.1/22.4   23.9/23.3/22.3   10.3/7.6/7.2 280/149/142  110 [76-128] (12)    0.067/0.100    0.350/0.183
ca         20.3/14.7/14.6   24.1/22.8/22.8   23.6/22.4/22.3   23.6/22.4/22.2    3.3/7.7/7.7 241/149/159  118 [97-131] (12)    0.067/0.083    0.467/0.183
cb         15.2/13.9/14.9   23.4/23.1/23.2   23.0/22.7/22.8   23.3/23.0/22.5    7.8/8.8/7.9 244/231/153 126 [114-224] (12)    0.200/0.133    0.350/0.183
default    13.9/13.8/14.7   21.3/21.0/20.7   21.1/21.0/20.7   21.7/21.3/21.0    7.2/7.2/6.0 248/245/134  100 [85-226] (12)    0.633/0.117    0.383/0.050
f26        14.3/15.1/14.2   23.4/23.1/23.3   23.0/22.6/23.0   23.0/22.5/22.8    8.7/7.5/8.8 221/144/218  108 [92-124] (12)    0.133/0.067    0.583/0.450
f27        14.2/15.0/15.0   23.2/22.2/23.0   22.7/21.8/22.8   22.7/21.7/22.4    8.5/6.8/7.8 270/130/145 117 [109-122] (12)    0.100/0.117    0.300/0.317
s26             15.0/14.9        23.0/22.7        22.7/22.3        22.5/22.2        7.7/7.5     152/150  112 [109-120] (8)          0.150          0.300
s26-locked             14.9              nan              nan              nan            nan                              ?              ?              ?
```

In-process driver, batch 1. The built-in display for default, a, b, d and dc; the external connected for dv, e and g:

```
variant   Δopen Δafter1 Δafter5           before             open           after1      after5         first ms   later ms med [range] (n)  close ms cpu closed  cpu open  helper
a           6.8     6.0     5.8   13.4/13.4/13.5   20.1/20.3/20.8   19.4/19.5/19.2   19.5/18.9   98.5/93.5/92.5             55 [44-66] (8)       242      0.033     0.283  
b           7.1     6.2     6.4   13.5/13.5/13.5   20.6/20.6/20.1   19.4/19.7/20.2   19.8/19.9   89.4/98.4/89.4             52 [46-68] (8)         1      0.150     0.183  
d           1.8     1.0     2.5   13.9/13.9/15.3   15.7/15.7/17.2   14.9/15.0/16.4   15.0/17.9   56.4/58.4/48.3             52 [47-58] (8)         5      0.100     0.167  
dc          1.8     1.0     1.7   13.9/13.9/14.0   16.8/15.6/15.7   15.9/14.9/14.8   16.1/15.1   53.3/64.3/46.2             56 [45-72] (8)         3      0.117     0.133  
default     4.4     4.3     4.5   13.2/13.3/13.2   17.6/17.7/17.7   17.6/17.6/17.5   17.7/17.8   72.3/60.3/71.6             42 [35-46] (8)       240      0.033     0.033  
dv          2.0     1.9     2.4   14.7/14.8/14.7   16.6/17.5/16.8   15.9/16.7/18.0   16.9/17.5   57.3/70.3/49.3             40 [37-68] (8)         3      0.083     0.617  
e          11.8    10.4    11.0   14.6/14.7/14.7   26.5/26.5/26.4   25.2/25.1/25.0   25.9/25.6  104.5/95.4/83.3             63 [56-70] (8)        17      0.100     0.233  
g           0.1     0.0     0.0   14.6/14.4/14.7   14.7/14.5/14.8   14.6/14.5/14.7   14.6/14.4 137.8/171.3/151.3          150 [114-158] (8)       397      0.067     0.067  open 20.2/20.5 peak 20.2/20.5 MiB, exit 250.2/252.4/251.0/254.0/254.5/253.7/257.0/249.6/246.8/248.9/245.6 ms, cpu open 0.150
```

In-process driver, batch 2, two displays:

```
variant   Δopen Δafter1 Δafter5           before             open           after1      after5         first ms   later ms med [range] (n)  close ms cpu closed  cpu open  helper
a           8.1     6.9     7.9   14.1/14.9/15.0   20.8/23.0/23.2   20.1/22.3/22.0   20.6/24.2   93.4/94.3/78.2             53 [43-61] (8)       242      0.083     0.150  
d           2.3     2.0     2.2   14.5/14.5/14.5   16.8/16.9/16.8   16.6/16.6/17.0   16.7/16.8   59.4/69.5/74.3             53 [46-65] (8)         7      0.083     0.233  
default     4.5     4.5     4.7   13.6/13.6/13.7   18.2/18.2/18.2   18.2/18.2/18.2   18.3/18.3   52.2/47.2/78.3             40 [29-44] (8)       240      0.200     0.050  
ds          1.8     1.1     1.2   13.7/13.8/13.7   15.5/15.5/15.5   14.8/14.8/16.1   15.0/14.9   61.5/70.3/71.4             49 [46-58] (8)         6      0.083     0.250  
dv          2.2     1.4     2.0   13.6/13.7/13.7   15.9/15.7/15.9   15.0/15.0/15.7   15.2/16.2   68.5/67.6/69.4             57 [48-65] (8)         6      0.100     0.100  
```
