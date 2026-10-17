# Wisp v1 — Design

**Status:** Draft for review · **Date:** 2026-10-17 · **Licence:** MIT

## 1. Purpose

Wisp is a macOS menu bar resource monitor whose own cost is close to zero. Existing monitors
(Stats, iStat Menus) show the right numbers but cost 1–11% CPU and tens of MB, mostly from
features and redraws the user never sees. Wisp shows a small, fixed set of readings in one line
of text, plus one plain list on click, and nothing else.

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

One line of text: `CPU 12%  GPU 3%  MEM 61%`.

- Labels in the secondary label colour; values in the primary label colour.
- System font at menu bar size with **monospaced digits**. Each value is padded to three digits
  with figure spaces (U+2007), so the item's width never changes and never nudges neighbouring
  icons. A missing reading shows as a figure dash (U+2012, also digit-wide), e.g. `GPU  ‒%`.
- Light/dark and the menu bar's tinting are handled by using system semantic colours only.

### 3.2 Dropdown (on click)

A native `NSMenu`:

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
- Rows are built once at launch and updated in place, only while the menu is open.
- Network, pressure and swap are read only while the menu is open; with it closed, each tick reads
  just CPU, GPU and memory. The network baseline is dropped while closed, so the first rate after
  opening is never averaged across the closed period.
- Interval and Launch at Login use standard checkmarks; Quit has ⌘Q.

### 3.3 Behaviour

- Default interval **2 s**; options 1, 2, 3, 5, 10 s. The choice persists (`NSUserDefaults`).
- While the menu is open, the timer keeps firing (scheduled in the run loop's common modes,
  because menus track events in a separate mode) and the rows update live.
- Sampling pauses on display sleep, system sleep and screen lock, and resumes on wake/unlock
  with a fresh baseline (the first sample after resume is not shown as a delta across the gap).
- Launch at Login uses `SMAppService.mainApp`. It only works from the bundled `Wisp.app`.
- No Dock icon (`LSUIElement`).

## 4. Architecture

A single Rust binary crate. macOS 14+, `aarch64-apple-darwin` only.

```
src/
  main.rs        app delegate: startup, timer, sleep/lock observers, settings, launch at login
  sample/        reads the system; no AppKit, so it can be unit-tested
    mod.rs       Snapshot type + Sampler that owns per-metric state
    cpu.rs
    mem.rs
    gpu.rs
    net.rs
  format.rs      Snapshot → display strings (pure)
  ui/
    status.rs    menu bar item; sets the title only when the formatted text changed
    menu.rs      dropdown construction and in-place row updates
```

**Data flow:** timer fires → `Sampler::sample()` returns a `Snapshot` → `format` produces strings →
`status` compares with the last string and sets the title only on change → if the menu is open,
`menu` updates its rows.

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
| Memory Used / % | derived | Used = App + Wired + Compressed (Activity Monitor's definition); % = Used / total |
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
- `objc2-service-management` — launch at login
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
  (padding, units, `—` for missing values).
- **Smoke test:** each sampler returns `Some` on the build machine (`cargo test`, run locally).
- **Manual acceptance:** S1–S7 from §1, recorded in the README with the machine and macOS
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
| Custom menu rows cost more than expected | measured in S2; fall back to attributed titles with a no-op action |
| Menu bar text unreadable on some wallpapers / tinted menu bars | semantic colours only; checked in S7 |
| App Nap throttles the timer of a background accessory app, leaving values stale | checked during the S1 run; hold an `NSProcessInfo` activity only if values stall |
| Launch at Login (`SMAppService`) is tied to the code signature, which every ad-hoc rebuild changes | spike it first in `feat/menu`; document re-enabling after a rebuild if needed |
| AppKit's own floor leaves little room under the 15 MB target | footprint of the bare scaffold measured in `feat/core` Task 1, before anything else is built |
