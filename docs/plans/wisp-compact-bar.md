# Wisp Compact Bar Plan

**Goal:** Replace the one-line `CPU 12%  GPU 3%  MEM 61%` title with a compact two-line item — a small label over a bold value per metric — drawn by layers so a redraw never re-renders the status bar button ([[2026-10-17-compact-layer-bar]]).

**Why:** any change to the button (title, image, drawn view) makes AppKit re-render and re-snapshot it (`_updateReplicants`, ~12–15 ms per redraw, one display or two). A layer-hosting view inside the button whose CATextLayers change only their strings, with implicit animations off, never dirties the button: the prototype measured 1.2–1.5 ms per redraw and 0.038% CPU / 13 MB over 10 min, against Stats at 1.888% / 138 MB ([[render-cost-findings]]).

**Prototype:** `exp/render-cost:src/ui/compact.rs` (commit 26edaa6). Port it; do not merge the branch.

**Spec:** `docs/specs/2026-10-17-wisp-v1-design.md` §1, §3.1, §4.

## Look (fixed)

- Per metric: label `CPU` / `GPU` / `MEM` on top, value below; columns left to right CPU, GPU, MEM.
- Label 7.5 pt medium system font; value 12 pt semibold monospaced-digit system font.
- **Labels and values both in the primary label colour** (white on a dark bar, adapts on a light one), like Stats.
- 6 pt gap between columns, 4 pt inset at each end, every column as wide as its widest possible content (`100%` or the label), so the item is a constant ~130 pt and never nudges its neighbours.
- Values unpadded (`7%`, `42%`, `100%`); a missing reading is a figure dash `‒` (U+2012).

## Global Constraints

- Everything from `docs/plans/wisp-core.md` Global Constraints still holds (deps, release profile, 2 s interval, comments, Conventional Commits with no attribution, never squash).
- No env-var switches or experiment code in production. Measurement-only tweaks (forced redraw, forced appearance) are temporary uncommitted patches, reverted before commit.
- Keep the 2-point redraw hysteresis (`format::moved`, [[2026-10-17-redraw-hysteresis]]).
- Implicit layer animations off for every layer change (`CATransaction setDisableActions:`).
- Layer colours are resolved (CATextLayer cannot follow dynamic colours), so they are re-resolved on appearance change; the view restyles in `viewDidMoveToWindow`, `viewDidChangeEffectiveAppearance` and `viewDidChangeBackingProperties` so the first frame already has the right colours and scale.
- Clicks fall through the view to the button (`hitTest:` → nil) so the menu still opens.
- The attributed-title path and any format code it alone used are deleted.
- Gate ("green"): `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

## Branching

`main ← feat/compact-bar ← ms/1-layer-bar, ms/2-acceptance`. Milestone PRs self-merge on green with merge commits; `feat/compact-bar → main` is opened and left for Saddat.

---

## Milestone 1 — `ms/1-layer-bar`: the layer-rendered compact item

- [x] **Task 1 (TDD): unpadded percent.** In `src/format.rs` replace `percent` (padded) and `status`/`StatusLine` with an unpadded `percent(Option<f64>) -> String`: `7.4 → "7%"`, `150 → "100%"`, `-2 → "0%"`, NaN → `"0%"`, `None → "‒"`. Update the tests (seen red first); drop `FIGURE_SPACE` if nothing else uses it.
- [x] **Task 2: compact view.** Port `CompactView` into `src/ui/status.rs` (one file for the item; delete the attributed-title code): six CATextLayers in a layer-hosting view, value-only updates in a disabled-actions transaction, one resolved primary colour for labels and values, restyle on move-to-window / appearance / backing changes. `StatusItem::new` sets the item length to the view width and adds the view to the button; `show` keeps the hysteresis and calls the view's `set`. The button carries an accessibility label so VoiceOver still names the item.
- [x] **Task 3: spec.** Rewrite spec §3.1 for the new look, update §4's `status.rs` line, and add the redraw cost and width to §1's criteria (forced redraw ≤ 2 ms; constant ~130 pt width).

**Acceptance:** gate green; the bundled app shows the two-line item; the menu still opens on click.

## Milestone 2 — `ms/2-acceptance`: prove it

- [x] **Task 4: forced redraw cost.** Temporary patch alternating CPU between 11 and 88 every tick; 20 s settle, 120 s `scripts/measure.sh`, report ms CPU per redraw. Target **≤ 2 ms**. *0.84–1.16 ms in six steady-state runs; the two early high runs coincided with a first menu open (spec §1, Measured).*
- [x] **Task 5: real 10-minute run next to Stats.** Bundled release build, 20 s settle, `scripts/measure.sh <pid> 600` on Wisp and Stats over the same window. Targets **≤ 0.1% CPU** and **footprint ≤ 15 MB**.
- [x] **Task 6: looks.** Screenshots of the real item in dark and light (force the item's appearance in a temporary patch if the wallpaper keeps the bar dark); width measured from the item's window, ~130 pt and the same at `0%`/`100%`/missing; values visibly change over a few minutes; no light-colour flash on the first frame (screenshot right after launch).
- [x] **Task 7: record.** Measured numbers and screenshots' findings into spec §1 (and this plan), with machine and macOS version.

**Acceptance:** every target above met. A missed target is profiled (`sample <pid> 20`) and its cause fixed before the milestone merges — targets are never relaxed.

## Milestone 6 — `ms/6-inactive-sync`: keep the inactive display's copy current

- [x] **Task 8 (TDD): throttled copy refresh.** `copy_due(stale, screens, since)` in `src/ui/status.rs`; each tick, once values have changed since the last refresh, with ≥ 2 displays and ≥ 30 s (`COPY_REFRESH`) since it, dirty the button once ([[2026-10-18-inactive-display-sync]]). Dirtying the button was first shown to refresh the layer-hosting view's copy (without it the copy stayed frozen for 60 s of changing values).
- [x] **Task 9: verify and measure.** On two displays the copy changed at 29–30 s intervals for 2 min; four focus switches showed no cross-fade or size jump. Method v1 two-display run: Wisp 0.085% CPU, 14.4 MiB; Stats 1.988% ([run](../benchmarks/runs/2026-10-18-7d7bf25.md)).
