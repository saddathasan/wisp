# ADR 0001: Show the dropdown from a short-lived helper process

**Status:** Accepted, 2026-10-18 (decided by Saddat) · **Confidence:** medium

## Context

Wisp's menu bar item costs ~14.6 MiB footprint and ~0.04% CPU (spec §1). The first time an NSMenu opens
in the process, AppKit keeps **+4.5 to +7.3 MiB** for the life of the process. That is process-wide
warm-up: ColorSync tables for the display profile, the SwiftUI stack macOS 27 loads to host the menu's
glass background, method caches, and malloc fragmentation. Nothing gives it back: not releasing the menu,
not `malloc_zone_pressure_relief`, not `UIDesignRequiresCompatibility`. With the full dropdown, Wisp would sit at
~21–23 MiB after its first use and break S2 (< 15 MB) for good. Measurements are in
[docs/research/menu-memory-cost.md](../research/menu-memory-cost.md).

## Drivers

In priority order:
1. **Memory.** The dropdown must not hog memory, and S2 must hold after the menu has been used.
2. **Native behaviour and accessibility.** Keyboard, VoiceOver and dismissal must work as in a system menu (S7, spec §3.2).
3. **Simplicity.** The least code to keep correct.
4. **Open latency.**

CPU while open doesn't decide anything: every option costs 0.1–0.3% for the seconds the menu is up, and S1 is
measured with the menu closed.

## Options considered

| Option | Kept after use | Opens in | Native behaviour | Verdict |
|---|---|---|---|---|
| NSMenu with custom row views, in process | +6–6.4 MiB, for life | 52–98 ms | full | breaks S2 after first use |
| Borderless layer-drawn NSPanel | +1.2–2.5 MiB | 50–74 ms | keyboard, VoiceOver actions and outside-click dismissal all hand-built; spec non-goal | most code, and still over 15 MB |
| NSPopover | +10–11 MiB | 63–104 ms | transient popover | worst on memory |
| **NSMenu in a short-lived helper process** | **0.0 MiB** (helper peaks ~20 MiB while open, all freed on exit) | ~150 ms | full | **chosen** |

Building the menu on open and releasing it on close was also measured: it gives back nothing.

## Decision

A click on the status item spawns a helper: the same binary, run as `wisp --dropdown <x> <y>`. The helper shows the
§3.2 NSMenu with custom row views, samples by itself while open, and exits when the menu closes. The main
process never creates an NSMenu, so it never pays the warm-up.

The contract between the two processes is kept minimal:
- **Position:** the main process passes the status item's screen point as arguments.
- **Settings** (interval, launch at login): the helper writes `NSUserDefaults` (same bundle id). The main process re-reads them when the helper exits.
- **Quit:** the helper's exit status tells the main process to terminate.
- **One at a time:** a click while a helper is running does not start a second.

## Consequences

- The main process stays at its menu-closed footprint whether or not the dropdown has been used. S2 holds.
- While the menu is open, the system carries a second ~20 MiB Wisp process. Activity Monitor shows it briefly.
- Every open costs a process start (~80 ms of the ~150 ms latency). That is close to a native NSMenu's first open, and slower than later in-process opens (~50 ms).
- The helper's first network rate needs one interval, as an in-process menu's would (spec §3.2).
- `SMAppService.mainApp` must work when called from the helper. That is folded into the launch-at-login spike (spec §8).
- Not yet verified by hand: keyboard focus in the helper's menu, VoiceOver, and outside-click dismissal. These are part of feat/menu acceptance.
- If a future macOS makes in-process menus cheap, or the helper causes problems, supersede this ADR with the in-process NSMenu and view rows. The row code is the same.
