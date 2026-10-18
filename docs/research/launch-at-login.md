# Launch at login (2026-10-18)

The spike behind feat/menu Milestone 2: does `SMAppService.mainApp` work for Wisp's ad-hoc signed bundle, does it work from a child process of the same binary (the dropdown helper of [ADR 0001](../adr/0001-dropdown-in-helper-process.md)), and what does a rebuild do to a registration. Run on an Apple M5, macOS 27.0.1 (26A434). The spike code is on the branch [`exp/login-spike`](https://github.com/saddathasan/wisp/tree/exp/login-spike) (355b2a3 → 338faca), which is never merged.

**Short answer:** it works, from the main process and from a child, with no approval step and no linking. The registration is keyed on the bundle id and the bundle's path, not the signature, so a rebuild in place keeps it enabled.

## Method

- `scripts/bundle.sh` builds `target/Wisp.app` (bundle id `io.github.saddathasan.wisp`, `Signature=adhoc`, no team).
- The spike adds `wisp --login-spike <status|register|unregister|child <cmd>>`. It `dlopen`s `/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement` (`RTLD_LAZY`), gets `[SMAppService mainAppService]` through `msg_send`, and calls `status`, `registerAndReturnError:` or `unregisterAndReturnError:`. It prints the pid, the parent pid, `NSBundle.mainBundle.bundleIdentifier`, the status before and after, and any NSError (domain, code, description) to stderr. `child <cmd>` spawns `current_exe() --login-spike <cmd>`, waits for it, then reads the status again in the parent, the way the main process will see a change the helper made.
- Run directly as `target/Wisp.app/Contents/MacOS/wisp --login-spike …`, and once through LaunchServices as `open -n --stderr <file> target/Wisp.app --args --login-spike …`.
- Status values: 0 notRegistered, 1 enabled, 2 requiresApproval, 3 notFound.
- `sfltool dumpbtm`, filtered to Wisp's record, before, during and after.

## Results

| Call | From | Result | Status before → after |
|---|---|---|---|
| `status` | unbundled `target/release/wisp` (`bundleIdentifier` nil) | — | 3 → 3 |
| `status` | bundle, exec'd directly | — | 0 → 0 |
| `register` | main process | YES, no error | 0 → 1 |
| `status` | child, after the parent registered | — | 1 → 1 |
| `unregister` | child | YES, no error | 1 → 0; the parent then reads 0 |
| `register` | child | YES, no error | 0 → 1; the parent then reads 1 |
| `register` | main process, already registered | YES, no error | 1 → 1 |
| `unregister` | main process | YES, no error | 1 → 0 |
| `unregister` | main process, already unregistered | YES, no error | 0 → 0 |
| `status`, `register`, `status`, `unregister` | through `open -n` (parent pid 1) | same as the direct exec | 0; 0 → 1; 1; 1 → 0 |

No call returned an error, so there was no NSError to record. Every process, exec'd directly or by a parent, resolved `NSBundle.mainBundle.bundleIdentifier` to `io.github.saddathasan.wisp`; LaunchServices (`open`) is not needed. The status changes take effect at once and both processes see them on their next read.

**Approval:** none. Status never read 2; registering went straight to 1, and the record's disposition is `allowed`. Saddat gets the "Background Items Added" notification (the `notified` flag), which is all.

**`sfltool dumpbtm`** (Wisp's record only; the UUID is stable across all the runs):

```
Name: Wisp            Type: app (0x2)       Developer Name: (null)
Identifier: 2.io.github.saddathasan.wisp
URL: /Users/501/codeRepository/wisp/target/Wisp.app
Bundle Identifier: io.github.saddathasan.wisp
before the spike:   Disposition: [disabled, allowed, notified] (0xa)  Generation: 16
registered:         Disposition: [enabled, allowed, notified]  (0xb)  Generation: 23
after unregister:   Disposition: [disabled, allowed, notified] (0xa)  Generation: 24
```

The record already existed (disabled) from earlier builds; unregistering disables it rather than removing it. `sfltool dumpbtm` asks for an administrator password through a SecurityAgent dialog unless one was entered in the last few minutes, so a scripted check has to be batched after one prompt.

## After a rebuild

- Rebuilding unchanged source gives a byte-identical binary and the same CDHash (`f63f2e10…`), so nothing changes.
- A code change gives a new ad-hoc signature (`f63f2e10…` → `aabe08f8…` → `0e056355…`). After each, `status` still read 1 in the main process and in a child, and the record stayed `enabled` with the same URL and the same generation: BTM's record holds the bundle id and the path, not the signature.
- `register` on the new signature, without an `unregister` first, returned YES and left status at 1.

Not tested: an actual logout and login after a rebuild (the record names the path, so it should launch whatever `Wisp.app` is there). That goes on the hand-check list.

## Conclusion for the design

- The helper can own Launch at Login: it `dlopen`s ServiceManagement, calls `register`/`unregister` on `SMAppService.mainApp`, and shows `status` as the checkmark. Nothing has to be linked, and the main process (which never loads ServiceManagement today) stays as it is.
- An unbundled `cargo run` reads 3 (notFound), so the item only works from `Wisp.app`, as the spec says. The helper should treat 3 as "unavailable".
- `unregister` when not registered succeeds, so the toggle needs no special case.
- README: a rebuild in place keeps Launch at Login on; nothing needs re-enabling. A copy at another path (e.g. `/Applications`) is a different record by the same logic, so turn Launch at Login on from the copy that is used (inferred, not tested).
