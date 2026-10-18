# Wisp Menu Plan

**Goal:** The spec §3.2 dropdown, the update interval, launch at login and the sleep/lock pause, with S1/S2 still
holding, then a README with measurements and screenshots. That finishes v1 (spec §7, `feat/menu`).

**Why this shape:** an NSMenu opened in Wisp's own process keeps +4.5–7.3 MiB for life, so the dropdown runs in a
short-lived helper, `wisp --dropdown <x> <y>`, and the main process never opens a menu ([[0001-dropdown-in-helper-process]],
`docs/research/menu-memory-cost.md`).

**Prototype:** `exp/menu-cost` (90f7620): `src/ui/menu.rs` (`rows`, `Style`, `RowView`, variant b) and the helper path
in `src/main.rs` (variant g). Port what is needed; never merge the branch, never port its `WISP_*` switches or driver.

**Spec:** `docs/specs/2026-10-17-wisp-v1-design.md` §1, §3.2, §3.3, §4, §8. **Method:** `docs/benchmarks/METHOD.md`.

## Global Constraints

- Everything in `docs/plans/wisp-core.md` and `docs/plans/wisp-compact-bar.md` Global Constraints still holds.
- **The main process never creates an NSMenu**, and never loads anything only the helper needs (ServiceManagement).
- **No synthetic input** (CGEvent, `osascript` keystrokes or clicks, `cliclick`) while Saddat may be using the machine.
  Drive the app in-process (`performClick`, calling actions directly) through temporary patches; anything that needs
  real keys or clicks goes on the hand-check list below.
- **Never post system-wide notifications** (e.g. a fake `com.apple.screenIsLocked`): other apps act on them.
- **Login items:** anything that registers Wisp unregisters it before the milestone ends, checked with
  `sfltool dumpbtm`. Registering shows Saddat a "Background Items Added" notification; that is expected.
- No env-var switches or experiment code in production. Measurement patches live in `scripts/*.patch` (like
  `forced-redraw.patch`), are applied only for a run and are never committed applied.
- Gate ("green"): `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

## Decisions taken in this plan (logged; Saddat can overturn)

- **ServiceManagement is `dlopen`ed by the helper and called through `msg_send`**, not linked through the
  `objc2-service-management` crate the spec lists. No new dependency, and the main process's loaded images stay as
  they are. Spec §4.3 is updated.
- **Launch at Login state is `SMAppService`'s own** (`status`), never mirrored in `NSUserDefaults`; only the
  interval is a stored setting. The main process never needs the login state.
- **The helper's menu takes the menu bar's appearance**, which a status item's own menu follows: the main process
  passes the button's effective appearance name as a third argument.
- **The main process learns of the helper's exit on the main thread without polling** (e.g. `NSTask` and its
  terminate notification), so Quit and a new interval apply at once.

## Branching

`main ← feat/menu ← ms/2-login-spike, ms/3-helper-dropdown, ms/4-settings, ms/5-pause, ms/6-readme`. Milestone PRs
self-merge on green with merge commits; `feat/menu → main` is opened and left for Saddat. Spike code lives on
`exp/login-spike`, never merged.

---

## Milestone 2 — `ms/2-login-spike`: does `SMAppService.mainApp` work ad-hoc signed, and from a helper?

- [x] **Task 1: spike** on `exp/login-spike`: from the bundled, ad-hoc signed `target/Wisp.app`, call
  `register`/`unregister`/`status` on `SMAppService.mainApp` from the main process and from a child process of the
  same binary (as the helper will be). Record each call's result and error, `status` before and after, what
  `sfltool dumpbtm` shows, whether approval is needed, and what happens to a registration after a rebuild changes
  the ad-hoc signature (status read, re-register needed?). Unregister at the end. *Works from the main process and
  a child, no errors, no approval (0 → 1 → 0); a rebuild keeps status 1 and the record enabled; ends unregistered (338faca).*
- [x] **Task 2: record** in `docs/research/launch-at-login.md`; update spec §3.3 / §8 with what the README must say
  about re-enabling after a rebuild, if anything. *Recorded; nothing to re-enable after a rebuild in place; spec §3.3,
  §4.3 and §8 updated.*

**Acceptance:** each question above answered from a real run; Wisp not registered at the end.

## Milestone 3 — `ms/3-helper-dropdown`: the helper shows the §3.2 dropdown

- [x] **Task 3 (TDD): rows.** `src/ui/menu.rs`: `rows(&Snapshot) -> Vec<Row>` and the section layout, from the
  prototype; tests for the order, the breakdown line and the missing-value dashes. Drop the `dead_code` allows in
  `format.rs` as the formatters get used. *4 tests, red on an empty `rows` then green; every `format.rs` allow
  gone (4426626).*
- [x] **Task 4: helper.** `wisp --dropdown <x> <y> <appearance>` builds the menu (section headers, `RowView` rows,
  Update Interval submenu, Launch at Login, Quit ⌘Q), samples with `detail` on its own timer at the stored
  interval, pops the menu up at the point and exits when it closes. An item's action must run before the exit.
  *`src/helper.rs`. The action runs before `popUp…` returns (Up + Return posted in-process: `quit:` 5.8 ms before
  the return), so it exits straight after; CPU and network show `—` until the first interval (02469d5, eb527b6).*
- [x] **Task 5: main side.** The status item has no menu; the button's action spawns the helper at the button's
  bottom-left in screen coordinates, at most one at a time. Quit in the helper exits with a status that terminates
  the main process. `quit_menu` goes. *`NSTask` + `NSTaskDidTerminateNotification`, delivered on the main thread;
  Quit (status 3) verified to terminate main; no NSMenu in main's heap and its images unchanged (fafc024).*
- [x] **Task 6: measure and look.** Open the dropdown in-process (a `scripts/open-latency.patch` that `performClick`s
  the button after the 20 s settle, holds ~3 s, closes, 5 opens, logs click → menu visible): open latency, the
  helper's peak footprint, the main process's footprint before and after 5 opens (Δ must be ~0). Screenshot the open
  dropdown and compare with spec §3.2. *Open 130–151 ms (median 138.5), helper peak 18.0–18.4 MiB, main Δ +16 and
  +32 KiB over two runs ([record](../benchmarks/runs/2026-10-18-fafc024-dropdown.md)); the screenshot matches §3.2 (eeb768f).*

**Acceptance:** gate green; the dropdown matches §3.2 on screen with live rows; main-process Δ ≈ 0 after opens;
latency and helper footprint recorded in `docs/benchmarks/runs/`.

## Milestone 4 — `ms/4-settings`: interval and launch at login round-trip

- [x] **Task 7 (TDD): interval.** The stored interval is read through one function that accepts only 1/2/3/5/10
  and falls back to 2 s; tested. *`menu::interval`, red on a pass-through stub then green; both processes read
  through `helper::stored_interval`, and main's `INTERVAL` constant is gone (e0c6bdd).*
- [x] **Task 8: round-trip.** Choosing an interval in the helper writes `NSUserDefaults` (flushed before exit); the
  main process re-reads it when the helper exits and restarts its timer only if it changed. Launch at Login calls
  `register`/`unregister` and its checkmark always shows the real `status`. *The helper `synchronize`s after the
  write; the main process needs no sync: a read saw another process's `defaults write` within ~30 ms, before the
  writer had exited. The button is highlighted while the helper runs (d03a87a).*
- [x] **Task 9: verify in-process**: trigger the items' actions directly (a temporary patch), check
  `defaults read io.github.saddathasan.wisp`, the main timer's new interval, and `sfltool dumpbtm` after register and
  after unregister. Leave Wisp unregistered. *`performActionForItemAtIndex` + `cancelTracking`, 5 opens: stored 7
  read as 2 in both processes; choosing 5 gave `defaults` 5 and main ticks 5.0 s apart, back to 2 ticked 2.0 s;
  Launch at Login 0 → 1 → 0 read through `status` in the helper and the next helper, no NSError (no `sfltool`, which
  prompts for a password). Each open brings one burst of 12 `restyle`s in main (the helper's menu window appearing,
  as in Milestone 3); the highlight adds none and never dirties the button. Ends at status 0, interval 2.*

**Acceptance:** gate green; both settings round-trip as above.

## Milestone 5 — `ms/5-pause`: sleep and lock pause (S3)

- [x] **Task 10 (TDD): pause reasons.** Display asleep, system asleep and screen locked are tracked separately;
  sampling resumes only when none is left (e.g. lock → display sleep → display wake stays paused until unlock). Tested.
  *`src/pause.rs`, a bit set; 3 tests (nested reasons, a resume never paused for, duplicates), red on a stub that
  always resumed, then green (7df055e).*
- [x] **Task 11: observers.** `NSWorkspace` screens-did-sleep/wake and will-sleep/did-wake, and the distributed
  `com.apple.screenIsLocked`/`Unlocked`; pausing invalidates the timer; resuming resets the sampler, ticks and restarts it.
  *Lock/unlock observed with `DeliverImmediately`: an accessory app is never active, and the default behaviour holds
  distributed notifications until it is. Pausing drops the timer, so `helperExited:` has none to restart (2099f0e).*
- [x] **Task 12: verify in-process** with a temporary patch that calls the observer methods: no ticks while paused
  (CPU over 120 s paused vs running), a fresh baseline after resume. *`performSelector:withObject:afterDelay:` with
  unposted notifications (lock +20 s, display sleep +30, display wake +150, unlock +160), bundled build: no tick from
  +20.0 to +160.0 (display wake alone left it paused), one tick at unlock, then 2.2 s apart. CPU 0.000% (0.01 idle
  wakeups/s) over 120 s paused vs 0.100% (0.07/s) over 120 s running. The resume tick's CPU came out `None`, and
  the launch tick's 41.7% is noise over the same milliseconds-old baseline, so both fresh ticks now show CPU as a dash
  until the first interval, as the helper does (cd697e1).*

**Acceptance:** gate green; paused CPU ≈ 0; the real lock/sleep check is on the hand-check list.

## Milestone 6 — `ms/6-readme`: measure and document

- [x] **Task 13: S1/S2.** A Method v1 `scripts/compare.sh` run against Stats, committed in `docs/benchmarks/runs/`.
  *Normal use, 600 s at `e29c4c7`: Wisp 0.093% CPU, 13.9 MiB; Stats 3.0.22 2.040%, 120.5 MiB (22×, 8.7×); S1 and S2
  pass ([record](../benchmarks/runs/2026-10-18-e29c4c7.md)). Stats read 3.0.22 (3.0.21 in the earlier runs); it relaunched once just after being opened for the run, before the window (fa47926).*
- [x] **Task 14: screenshots** of the item and the open dropdown, dark and light (appearance forced in-process by a
  temporary patch, never the system setting), cropped to Wisp alone. *`docs/images/`: the dropdown captured as its
  window (`screencapture -l`, helper opened by `performClick`); the item rendered from its own layers at 2× on a
  transparent background (`renderInContext:`), because here it sits behind the notch overflow on the Retina display
  and shows only on the 1× external one. Checked against §3.1/§3.2 and the real 1× item; forced Aqua over this dark
  desktop gives the light menu a grey glass (87dabce).*
- [x] **Task 15: README**: what Wisp is and is not, screenshots, measured cost vs Stats with the record link, helper
  open latency and footprint, build from source, launch at login after a rebuild, licence. Spec §1 Measured updated.
  *README with a `<picture>` per screenshot (dark/light by the reader's theme) and the range across the three
  Method v1 runs; spec §1 has a feat/menu Measured block.*

**Acceptance:** S1 < 0.1% and S2 < 15 MB in the committed run; README complete.

## Hand checks (need real keyboard and clicks; Saddat, when convenient)

1. **Keyboard:** open the dropdown, then ↓/↑ move over the action items and skip the reading rows; → opens the
   interval submenu; Return chooses; Escape closes; ⌘Q quits Wisp.
2. **VoiceOver:** each reading row reads as one line ("CPU 12%"); headers and items are announced; the item reads "Wisp".
3. **Outside click:** the menu closes on a click on the desktop, in another app, and on another menu bar item; a
   click on Wisp's item while open closes it and opens no second menu.
4. **Mouse:** choosing 5 s makes the bar update every ~5 s; Launch at Login on then off shows and removes Wisp in
   System Settings → General → Login Items. With it on, rebuild, log out and in: Wisp starts.
5. **Pause:** lock the screen (and let the display sleep) for 2 min with `scripts/measure.sh <pid> 120` running; CPU ≈ 0.
6. **Mouse choice:** clicking Quit Wisp quits Wisp, and choosing an interval with the mouse closes the menu cleanly
   (action-before-exit was verified in-process through the keyboard path only).
7. **Highlight:** the item shows pressed while the dropdown is open and unpressed once it closes, whichever way it
   closes (item chosen, Escape, outside click).
