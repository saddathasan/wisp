# Wisp v1 — Design

**Status:** Draft for review · **Date:** 2026-10-17 · **Licence:** MIT

## 1. Purpose

Wisp is a macOS menu bar resource monitor whose own cost is close to zero. Existing monitors
(Stats, iStat Menus) show the right numbers but cost 1–11% CPU and tens of MB, mostly from
features and redraws the user never sees. Wisp shows a small, fixed set of readings in one compact
menu bar item, plus one plain list on click, and nothing else.

**Success criteria (v1 is done when all hold on an M-series Mac, macOS 14+):**

| # | Criterion | How it's checked |
|---|-----------|------------------|
| S1 | Average CPU < 0.1% over 10 min at the default 2 s interval, menu closed | `scripts/measure.sh <pid> 600` (CPU-time and idle-wakeup deltas) |
| S2 | Physical footprint < 15 MB after 10 min | `footprint <pid>` |
| S3 | ~Zero activity while the display is asleep or the screen is locked | `top` sample during display sleep |
| S4 | No added WindowServer load vs. no monitor running | `top -pid $(pgrep WindowServer)`, with and without Wisp |
| S5 | Measurably cheaper than Stats tuned to CPU+GPU+RAM (~1.6% CPU) | same `scripts/measure.sh` run on both, plus `top` power |
| S6 | Readings agree with Activity Monitor (CPU %, Memory Used, Swap) within rounding | side-by-side check |
| S7 | Looks native and polished in light and dark mode | screenshots in both, reviewed before release |
| S8 | A forced redraw (a visible value change every tick) costs ≤ 2 ms CPU | temporary patch alternating CPU 11/88, `scripts/measure.sh <pid> 120`, CPU time ÷ redraws |
| S9 | The menu bar item is a constant ~130 pt wide, the same at `0%`, `100%` and missing | width of the item's window or accessibility frame |

**Measured (2026-10-18, compact bar, Apple M5, macOS 27.0.1; built-in 2560×1664 Retina with notch plus a 3440×1440 external display, extended):**

- **Source of record:** [`docs/benchmarks/`](../benchmarks/) — the method ([METHOD.md](../benchmarks/METHOD.md)) and every run, raw output included; the figures below are from [the backfilled 2026-10-18 runs](../benchmarks/runs/2026-10-18-compact-bar-backfill.md).
- **S1, S5:** 10 min side by side, bundled release build after a 20 s settle: Wisp `cpu 0.027%`, Stats `cpu 2.143%`; a clean repeat (no accessibility polling) read Wisp `cpu 0.043%`, Stats `cpu 2.027%`.
- **S2:** Wisp 14.6 MiB after 10 min (15,270,728 B; `footprint` prints `15 MB`), 14.8 MiB on the clean repeat; Stats 135–137 MB. The live heap is ~4.1–4.4 MB; the rest is AppKit's floor. Two one-time costs plateau and never grow back: switching the active display warms up ~1 MB (method caches, ColorSync and vImage converters for the other display, plateau at ~14.9 MiB after five round trips), and **the first time an NSMenu opens in a process, AppKit keeps +4.5 to +7.3 MiB for the life of that process**. Later opens add nothing, and nothing gives it back ([research](../research/menu-memory-cost.md)). The dropdown is therefore shown by a short-lived helper process ([ADR 0001](../adr/0001-dropdown-in-helper-process.md)), and Wisp itself never opens a menu. `leaks` stays at 416 leaks / 19.9 KB from launch.
- **S8:** CPU time ÷ 60 redraws over 120 s, interleaved with the unpatched build. Steady state: forced 0.84, 0.84, 0.84 and 1.00 ms per redraw, unpatched 0.50–0.84 ms per tick (sampling plus noise), so a redraw adds ~0.3–0.5 ms. A re-check read 1.16 and 1.16 ms. The first set read 4.3 and 2.5 ms forced and coincided with the footprint step of a first menu open (19 MB) and the item moving; it never reproduced in six later runs. `sample` in steady state shows only the timer's Core Animation commit.
- **S9:** the accessibility frame is 130 × 24 pt at `0%`, `100%` and `‒`, and on the real build.
- **S7:** dark: white labels and values, no light flash in 24 screenshots over the 2.8 s after launch. The item fades in white. With the system in light mode the bar stays dark over this wallpaper and the item stays white. With the button forced to Aqua, the labels and values turn dark. A genuinely light bar was not available on this setup.
- **Two displays:** the inactive display's copy refreshes every ~30 s while values move (§3.1); before that it sat on stale values for minutes. With the sync on, Wisp read `cpu 0.085%`, 14.4 MiB against Stats' 1.988% ([run](../benchmarks/runs/2026-10-18-7d7bf25.md)); the extra ~0.05% is one ~14 ms button render per 30 s. On the active display the item updates every tick.

**Measured (2026-10-18, feat/menu, same machine; Method v1):**

- **S1, S2, S5:** 10 min in normal use next to Stats 3.0.22, at `e29c4c7` (dropdown helper, settings and pause in): Wisp
  `cpu 0.093%`, 13.9 MiB (14,566,216 B); Stats `cpu 2.040%`, 120.5 MiB (22× CPU, 8.7× footprint) ([run](../benchmarks/runs/2026-10-18-e29c4c7.md)).
  Across the three Method v1 runs Wisp reads 0.038–0.093% and 13.9–14.6 MiB, Stats 1.99–2.23% and 120–137 MiB.
- **S3 (in-process):** with the observers called directly (lock, display sleep, display wake, unlock), no tick from lock
  to unlock, display wake alone left it paused, and the first tick after unlock started a fresh baseline. CPU 0.000%
  (0.01 idle wakeups/s) over 120 s paused against 0.100% (0.07/s) over 120 s running. A real lock and display sleep is
  on the hand-check list.
- **Dropdown (ADR 0001):** opens in 130–151 ms (median 138.5 ms), about half of it process start; the helper peaks at
  18.0–18.4 MiB while open and gives it all back on exit; the main process keeps +16 and +32 KiB after five opens and
  loads no new image ([run](../benchmarks/runs/2026-10-18-fafc024-dropdown.md)).
- **S7:** screenshots in `docs/images/`, the item rendered from its own layers at 2× (it sits behind the notch overflow
  on the built-in display here) and the dropdown captured as a window, each with the appearance forced in-process.

## 2. Scope

**v1:** CPU %, GPU %, RAM % in the menu bar; CPU, GPU, memory breakdown, memory pressure, swap and
network throughput in the dropdown; selectable interval; launch at login.

**v2 (deferred):** CPU/GPU temperature and power. These need private, undocumented interfaces
(IOReport, IOHID sensors, SMC) that break across macOS and chip versions, so they ship only once
the v1 core is proven.

**Non-goals:** graphs, sparklines, history, per-process lists, notifications, popover windows,
preferences windows, themes, Intel Macs, auto-update, App Store distribution.

## 3. What the user sees

### 3.1 Menu bar

A compact two-line item: per metric a small label (`CPU`, `GPU`, `MEM`) over its value, columns
left to right.

- Label 7.5 pt medium system font; value 12 pt semibold system font with monospaced digits.
- Labels and values both in the primary label colour, so the item is white on a dark menu bar and
  adapts on a light one.
- 6 pt between columns and a 4 pt inset at each end. Every column is as wide as its widest possible
  content (`100%` or the label), so the item is a constant ~130 pt and never nudges its neighbours.
- Values are unpadded (`7%`, `42%`, `100%`); a missing reading is a figure dash `‒` (U+2012).
- Drawn by six text layers in a layer-hosting view inside the status item button: a redraw only
  swaps layer strings, so a value change never re-renders the button (~14 ms each time it does).
- With two or more displays, the inactive display's dimmed copy is a snapshot AppKit takes of the
  button; after a value change Wisp dirties the button to refresh it at most once every 30 s, and
  never with one display (decision 2026-10-18, inactive-display sync).
- Layer colours cannot follow dynamic system colours, so they are resolved to sRGB and re-resolved
  when the view enters the menu bar window, when the bar's appearance changes and when the display
  scale changes. Clicks fall through the view to the button, which opens the dropdown.

### 3.2 Dropdown (on click)

A native `NSMenu`, shown by a short-lived helper process (ADR 0001). A press on the status item runs the same
binary as `wisp --dropdown <x> <y> <appearance>` (an `NSTask`): the button's bottom-left in screen coordinates and
the name of the menu bar's appearance, which the menu takes as a status item's own menu would. The helper shows the menu at that point and exits when it closes, so the
menu's ~5–7 MiB of AppKit warm-up goes back to the system every time:

```
SYSTEM
  CPU                           12%
  GPU                            3%
MEMORY
  Used               61%  19.5 / 32 GB
  App 11.2 · Wired 3.1 · Compressed 5.2 GB
  Pressure                  ● Warning
  Swap                   12.2 / 13.3 GB
NETWORK
  Download                  1.2 MB/s
  Upload                     84 KB/s
──────────────────────────────────────
Update Interval                       ▸   1 s · 2 s ✓ · 3 s · 5 s · 10 s
Launch at Login                       ✓
──────────────────────────────────────
Quit Wisp                           ⌘Q
```

- Section headers use the native macOS 14 section-header style (`NSMenuItem.sectionHeader`).
- Reading rows are small custom views (`NSMenuItem.view`). Plain menu items with no action render
  greyed out and highlight on hover; info rows must do neither. Labels sit left, values sit in
  one right-aligned column; the breakdown line uses the secondary label colour.
- The pressure dot is green (Normal), yellow (Warning) or red (Critical), using system colours.
- The helper builds the rows when it starts and updates them in place on its own timer while the menu is open.
- Network, pressure and swap are read only by the helper; the main process reads just CPU, GPU and
  memory each tick. The helper starts its own network baseline, so the first rate after opening is
  never averaged across the closed period. CPU and network show `—` until the helper's first interval: a delta
  over the milliseconds since its baseline is noise.
- At most one helper runs; a click while one is open does nothing more. The item's button stays highlighted while
  the helper runs, as a status item with its own menu does.
- The helper writes the interval to `NSUserDefaults` (same bundle id) and waits for it to reach cfprefsd before it
  exits. The main process re-reads it when the helper exits, learning of the exit from `NSTaskDidTerminateNotification`
  on its main thread (no polling), and restarts its timer only if it changed. Launch at Login is `SMAppService`'s own
  state, never stored. Quit makes the helper exit with a dedicated status (`QUIT_STATUS`) that tells the
  main process to terminate. A chosen item's action runs before `popUpMenuPositioningItem…` returns, so the helper
  exits straight after it.
- Interval and Launch at Login use standard checkmarks; Quit has ⌘Q.

### 3.3 Behaviour

- Default interval **2 s**; options 1, 2, 3, 5, 10 s. The choice persists (`NSUserDefaults`); any other stored
  value reads as 2 s.
- While the menu is open, the helper's timer keeps firing (scheduled in the run loop's common modes,
  because menus track events in a separate mode) and the rows update live. The main process's timer
  keeps the menu bar item current meanwhile.
- Sampling pauses on display sleep, system sleep and screen lock, and resumes on wake/unlock
  with a fresh baseline (the first sample after resume is not shown as a delta across the gap; CPU shows a dash
  until the first interval, as at launch).
- Launch at Login uses `SMAppService.mainApp`, called from the helper; the checkmark shows its `status`. It only
  works from the bundled `Wisp.app` (unbundled, `status` is notFound). A rebuild in place keeps it enabled
  ([launch-at-login.md](../research/launch-at-login.md)).
- No Dock icon (`LSUIElement`).

## 4. Architecture

A single Rust binary crate. macOS 14+, `aarch64-apple-darwin` only.

```
src/
  main.rs        app delegate: startup, timer, sleep/lock observers, settings; spawns the helper on a click;
                 `--dropdown` runs the helper (ADR 0001) instead
  helper.rs      the helper process: pops the menu up, samples on its own timer, launch at login, exits on close
  sample/        reads the system; no AppKit, so it can be unit-tested
    mod.rs       Snapshot type + Sampler that owns per-metric state
    cpu.rs
    mem.rs
    gpu.rs
    net.rs
  format.rs      Snapshot → display strings (pure)
  ui/
    status.rs    menu bar item: a layer-drawn two-line view, redrawn only when a value moves ≥ 2 points
    menu.rs      dropdown construction and in-place row updates (helper process only)
```

**Data flow:** timer fires → `Sampler::sample()` returns a `Snapshot` → `format` produces strings →
`status` compares with the values on screen and swaps the layer strings only when one has moved. A click
spawns the helper, which runs the same sample → format path into `menu` rows while it is open, then exits.

Everything runs on the main thread. One sample is a handful of system calls (microseconds), so a
background queue adds synchronisation without saving anything. If profiling ever shows sampling
on the main thread is measurable, move it to a serial queue.

### 4.1 Metric sources

All work without root or entitlements. The GPU key, pressure level, swap and 16 KB page size were confirmed with `ioreg`/`sysctl` on macOS 27 / M5 on 2026-10-17; the rest are standard, long-stable calls.

| Reading | Source | Formula / notes |
|---|---|---|
| CPU % | `host_statistics(HOST_CPU_LOAD_INFO)` | `(Δuser+Δsystem+Δnice) / Δtotal` between samples |
| Memory total | `sysctl hw.memsize` | read once |
| App / Wired / Compressed | `host_statistics64(HOST_VM_INFO64)` | App = `internal_page_count − purgeable_count`; Wired = `wire_count`; Compressed = `compressor_page_count`; pages × `sysconf(_SC_PAGESIZE)` (16 KB on Apple Silicon — never hard-code 4096) |
| Memory Used / % | `host_statistics64` + `sysctl vm.page_free_count`, `vm.page_free_cpu_count` | Used = total − Free − Cached Files (Activity Monitor's definition), Free = `vm.page_free_count − vm.page_free_cpu_count` (fallback `free_count − speculative_count`), Cached Files = `external_page_count + purgeable_count`; % = Used / total. App + Wired + Compressed falls short by the pages outside every queue (`vm.page_booter_count`, 0.6 GB on an M5), so it is not Used (S6 run) |
| Pressure | `sysctl kern.memorystatus_vm_pressure_level` | 1 Normal, 2 Warning, 4 Critical, other → `—` |
| Swap | `sysctl vm.swapusage` (`xsw_usage`) | used / total |
| GPU % | IOKit `IOAccelerator` → `PerformanceStatistics` → `"Device Utilization %"` | match the service **once** at startup and keep it; per tick only copy the property; release CF objects every tick |
| Network ↓/↑ | `sysctl {CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, 0}` → `if_msghdr2` | 64-bit `ifi_ibytes`/`ifi_obytes` summed over up, non-loopback interfaces; rate = Δbytes / Δt; a counter that goes backwards (interface reset) yields 0 for that tick. `getifaddrs` is not used: its counters are 32-bit and wrap |

### 4.2 Error handling

Any reading that fails returns `None` and displays as `—`. Nothing panics on a failed system
call. If the GPU service is not found at startup, GPU shows `—` for the session. Release builds
use `panic = "abort"`, so a logic bug fails loudly rather than leaving a half-dead menu item.

### 4.3 Dependencies

Only what the design needs; exact versions are pinned in the implementation plan.

- `objc2`, `objc2-foundation`, `objc2-app-kit` — AppKit bindings
- ServiceManagement (launch at login) is not a crate: the helper `dlopen`s the framework and calls
  `SMAppService` through `msg_send`, so there is no new dependency and the main process's loaded images are unchanged
- `libc` — sysctl and Mach counters (its two deprecated Mach calls are allowed locally; no `mach2`)
- IOKit through four hand-declared `extern "C"` functions; the CF dictionary it returns is read through
  toll-free bridging to `NSDictionary`, so no IOKit or CoreFoundation crate

Excluded on purpose: `sysinfo` (broad, refreshes far more than needed), `tray-icon`/`tao`
(an extra event loop and a few MB for nothing we use), `cacao` (unmaintained since 2023).

### 4.4 Build and packaging

- Release profile: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`,
  `panic = "abort"`.
- `scripts/bundle.sh` builds the release binary and assembles `Wisp.app` with a hand-written
  `Info.plist` (`LSUIElement`, `LSMinimumSystemVersion` 14.0, bundle id `io.github.saddathasan.wisp`),
  then ad-hoc signs it (`codesign -s -`). No `cargo-bundle`.
- v1 is built from source. Signed and notarised release downloads are out of scope until there
  are users who need them.

## 5. Testing

- **Unit tests (pure logic):** CPU delta maths, memory derivation from a fixed
  `vm_statistics64`, network rate including counter reset, pressure mapping, and all formatting
  (rounding, units, dashes for missing values).
- **Smoke test:** each sampler returns `Some` on the build machine (`cargo test`, run locally).
- **Manual acceptance:** S1–S9 from §1, recorded in the README with the machine and macOS
  version they were measured on.

## 6. Repository and docs

- `README.md`: what Wisp is and is not, screenshots (light and dark), measured footprint vs.
  Stats, build-from-source steps, licence.
- `docs/specs/` — this document. `docs/plans/` — the implementation plan.
- Every source file opens with a 1–2 line header; every public item has a one-line doc comment.

## 7. Delivery

The repo follows `main ← feat/<slug> ← ms/<n>-<slug>` with a human-reviewed `feat → main` PR
kept to about 400 changed lines. v1 is therefore split into two features:

1. **`feat/core`** — samplers, formatting, the menu bar text, the bundle script, unit tests.
   A usable monitor on its own. Accepted as one ~970-line PR (incl. ~260 test lines) over the
   400-line guideline (decision 2026-10-17).
2. **`feat/menu`** — the dropdown, interval setting, launch at login, sleep/lock pause, polish
   pass with screenshots, footprint measurement, README.

## 8. Risks

| Risk | Mitigation |
|---|---|
| `"Device Utilization %"` key changes in a future macOS | GPU shows `—`; everything else keeps working |
| objc2 API churn between minor versions | pin versions; the AppKit surface used is small |
| An NSMenu permanently costs the process +4.5–7.3 MiB | the menu lives in a short-lived helper (ADR 0001); the main process never opens one |
| The helper's open latency (~150 ms, ~80 ms of it process start) feels slow | measured in feat/menu; fall back to the in-process NSMenu (supersede ADR 0001) if it does |
| Menu bar text unreadable on some wallpapers / tinted menu bars | primary label colour resolved per appearance and re-resolved on change (§3.1); checked in S7 |
| App Nap throttles the timer of a background accessory app, leaving values stale | checked during the S1 run; hold an `NSProcessInfo` activity only if values stall |
| Launch at Login (`SMAppService`) breaks with the ad-hoc signature every rebuild changes, or from the helper | spiked ([launch-at-login.md](../research/launch-at-login.md)): works ad-hoc signed and from a child process, no approval; the record is keyed on bundle id and path, so a rebuild in place stays enabled. A logout/login after a rebuild is a hand check |
| AppKit's own floor leaves little room under the 15 MB target | footprint of the bare scaffold measured in `feat/core` Task 1, before anything else is built |
