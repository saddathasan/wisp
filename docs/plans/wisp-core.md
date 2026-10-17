# Wisp Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working menu bar monitor that shows live `CPU 12%  GPU 3%  MEM 61%` text and a Quit item, built as `Wisp.app` — feature 1 of 2 from the spec ([[2026-10-17-wisp-v1-design]] §7).

**Architecture:** One Rust binary. `src/sample/` reads the system through raw sysctl, Mach and IOKit calls and has no AppKit code, so it is unit-tested headless. `src/format.rs` turns a `Snapshot` into strings (pure). `src/ui/status.rs` owns the `NSStatusItem` and redraws only when the text changes. `src/main.rs` is the AppKit delegate with a single repeating timer.

**Tech Stack:** Rust (edition 2024), `objc2` 0.6.5, `objc2-foundation` 0.3.2, `objc2-app-kit` 0.3.2, `libc` 0.2.190. macOS 14+, Apple Silicon.

**Spec:** `docs/specs/2026-10-17-wisp-v1-design.md`

## Global Constraints

- Target `aarch64-apple-darwin`, `LSMinimumSystemVersion` 14.0, bundle id `io.github.saddathasan.wisp`.
- Dependencies limited to the four crates above (the menu feature adds `objc2-service-management`). No `sysinfo`, `tray-icon`, `tao`, `cacao`, `mach2`, `core-foundation`.
- Never hard-code a 4096-byte page; use `sysconf(_SC_PAGESIZE)` (16 KB on Apple Silicon).
- Network counters come from `NET_RT_IFLIST2` (64-bit), never `getifaddrs` (32-bit, wraps).
- A failed reading is `None` and displays as `—`; no panics on failed system calls.
- Release profile: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
- Default interval 2 s, timer tolerance 10% of the interval, timer in the run loop's common modes.
- Read only what is on screen: the menu bar path reads CPU, GPU and memory; network, pressure and swap are read only when the dropdown is open (`Sampler::sample(detail)`). A hidden reading still costs a wakeup's worth of syscalls.
- Scope decision (2026-10-17): `feat/core` ships as one `feat → main` PR of ~970 lines (~710 code, ~260 tests), accepted over the 400-line gate.
- Every source file opens with a 1–2 line `//!` header; every `pub` item has a one-line `///` doc.
- Conventional Commits, no AI attribution, one task = one commit, never squash.
- objc2 signatures below are written against objc2 0.6.5 / objc2-app-kit 0.3.2. If the compiler disagrees on a signature, look the method up on docs.rs for that exact version and adjust the call — keep the behaviour.

## Review Focus

1. **An interface appears mid-session** (VPN connects, Wi-Fi rejoins) with large existing counters → users expect no spike; the rate counts only interfaces present in both samples. Test in Task 4.
2. **An interface's counter goes backwards** (reset) → expect 0 for that tick, not a huge number. Test in Task 4.
3. **CPU tick counters wrap** (`u32`) after long uptime → expect a sane percentage. Test in Task 3.
4. **GPU service missing or key renamed** on a future macOS → expect `—` for GPU, everything else live. Test in Task 6 (format) and Task 5 (`Gpu::utilization` returns `None` instead of panicking when the property is absent).
5. **Purgeable pages exceed internal pages** briefly → expect App = 0, not an underflow panic. Test in Task 2.
6. **Menu closed** → network, pressure and swap are not read at all, and the network baseline is dropped so the first detailed sample is never a rate across a long gap. Test in Task 6.
7. **A reading goes missing** → the menu bar item keeps its exact width (figure dash, U+2012, is digit-wide). Test in Task 6.

---

## Branching

```bash
cd /Users/saddat/codeRepository/wisp
git switch main && git pull
git switch -c feat/core
```

Milestones branch off `feat/core`: `ms/1-scaffold` (Task 1), `ms/2-samplers` (Tasks 2–5), `ms/3-live-title` (Tasks 6–9). Each milestone: commit per task → PR `ms → feat/core` → self-merge on green. After milestone 3, open PR `feat/core → main` with the Task 9 measurements, review it, and merge (the user granted merge access on 2026-10-17).

"Green" means, run locally: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (milestone 2 adds `-A dead_code`, since the readers are unused until Task 6). There is no CI: several tests read live hardware counters that a CI VM may not have.

---

### Task 0: Install the Rust toolchain (needs the user's OK)

Approved by the user on 2026-10-17.

- [x] **Step 1: Install rustup and stable Rust** — done 2026-10-17 (rustc 1.99.1). Homebrew's rustup is keg-only and ships no `rustup-init`:

```bash
brew install rustup
export PATH=/opt/homebrew/opt/rustup/bin:$PATH   # also appended to ~/.zshrc
rustup set profile minimal && rustup default stable
rustup component add clippy rustfmt
rustc --version && cargo --version
```

Expected: `rustc 1.9x.x` (any stable ≥ 1.85, needed for edition 2024).

---

### Task 1: App scaffold — status item with a Quit menu, bundled as Wisp.app

**Branch:** `ms/1-scaffold` off `feat/core`.

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `src/main.rs`, `bundle/Info.plist`, `scripts/bundle.sh`

**Interfaces:**
- Produces: `AppDelegate` (in `main.rs`) with ivars struct `Ivars`; later tasks add fields. `quit_menu(mtm) -> Retained<NSMenu>`.

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "wisp"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT"
description = "A macOS menu bar resource monitor that costs almost nothing to run."
repository = "https://github.com/saddathasan/wisp"

[dependencies]
libc = "0.2.190"
objc2 = "0.6.5"
objc2-foundation = "0.3.2"
objc2-app-kit = "0.3.2"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

- [ ] **Step 2: Create `.gitignore`**

```gitignore
/target
```

- [ ] **Step 3: Create `src/main.rs`**

```rust
//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item.
//! All UI setup happens in the delegate once AppKit has finished launching.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{ns_string, NSNotification, NSObject, NSObjectProtocol};

/// State the delegate keeps alive for the app's lifetime.
#[derive(Default)]
struct Ivars {
    item: OnceCell<Retained<NSStatusItem>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and AppDelegate does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            if let Some(button) = item.button(mtm) {
                button.setTitle(ns_string!("Wisp"));
            }
            item.setMenu(Some(&quit_menu(mtm)));
            let _ = self.ivars().item.set(item);
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }
}

/// The dropdown menu; for now only Quit, which a nil target routes to NSApplication's `terminate:`.
fn quit_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit Wisp"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    };
    menu.addItem(&quit);
    menu
}

fn main() {
    let mtm = MainThreadMarker::new().expect("Wisp must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // LSUIElement covers the bundled app; this also keeps `cargo run` out of the Dock.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
```

- [ ] **Step 4: Create `bundle/Info.plist`**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Wisp</string>
	<key>CFBundleDisplayName</key>
	<string>Wisp</string>
	<key>CFBundleIdentifier</key>
	<string>io.github.saddathasan.wisp</string>
	<key>CFBundleExecutable</key>
	<string>wisp</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.0.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSMinimumSystemVersion</key>
	<string>14.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHumanReadableCopyright</key>
	<string>© 2026 Saddat Hasan. MIT Licence.</string>
</dict>
</plist>
```

- [ ] **Step 5: Create `scripts/bundle.sh` and make it executable**

```sh
#!/bin/sh
# Builds the release binary and wraps it in an ad-hoc signed target/Wisp.app.
set -eu
cd "$(dirname "$0")/.."

cargo build --release

APP=target/Wisp.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/wisp "$APP/Contents/MacOS/wisp"
cp bundle/Info.plist "$APP/Contents/Info.plist"
# Cargo.toml is the single source of the version number.
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
echo "$APP"
```

```bash
chmod +x scripts/bundle.sh
```

- [ ] **Step 6: Build, bundle and launch**

```bash
./scripts/bundle.sh && open target/Wisp.app
```

Expected: build succeeds with no warnings; "Wisp" appears in the menu bar; no Dock icon; clicking shows "Quit Wisp ⌘Q"; Quit (and ⌘Q while the menu is open) exits; `pgrep -x wisp` prints nothing afterwards.

- [ ] **Step 7: Measure the AppKit floor**

The scaffold does no sampling, so its footprint is the floor every later task builds on. If it is already near 15 MB, S2 is at risk and we learn that now, not at Task 9. Relaunch, wait 60 s, then:

```bash
PID=$(pgrep -x wisp)
footprint "$PID" | head -n 5
./scripts/measure.sh "$PID" 60
```

Record the numbers in the `ms/1-scaffold` PR description. `scripts/measure.sh` (added in ms/3) uses counter deltas: `top`'s IDLEW column is cumulative since launch, so averaging it is meaningless.

Measured 2026-10-17 with `scripts/measure.sh` (M5, macOS 27, settled, 60 s): footprint 13–17 MB, CPU 0.17%, 0.28 idle wakeups/s. An earlier "1.0/s" figure averaged the cumulative IDLEW column and was wrong.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs bundle/Info.plist scripts/bundle.sh
git commit -m "feat: add menu bar app scaffold and app bundle script"
```

---

### Task 2: System call helpers and memory readings

**Branch:** `ms/2-samplers` off `feat/core` (after ms/1 merged).

**Files:**
- Create: `src/sample/mod.rs`, `src/sample/sys.rs`, `src/sample/mem.rs`
- Modify: `src/main.rs` (add `mod sample;` under the `use` block)

**Interfaces:**
- Produces:
  - `sys::by_name<T: Copy>(name: &CStr) -> Option<T>`, `sys::host() -> libc::mach_port_t`, `sys::page_size() -> u64`
  - `mem::Memory { total, app, wired, compressed: u64 }` with `used() -> u64`, `percent() -> f64`
  - `mem::Pressure { Normal, Warning, Critical }` with `from_level(i32) -> Option<Pressure>`
  - `mem::Swap { used, total: u64 }`
  - `mem::Pages { internal, purgeable, wired, compressed: u64 }`, `mem::memory(total, page_size, Pages) -> Memory`
  - `mem::read_pages(host) -> Option<Pages>`, `mem::read_total() -> Option<u64>`, `mem::read_pressure() -> Option<Pressure>`, `mem::read_swap() -> Option<Swap>`

- [ ] **Step 1: Create `src/sample/mod.rs`**

```rust
//! System readings for Wisp. Nothing here touches AppKit, so every reader is tested headless.

mod mem;
mod sys;

pub use mem::{Memory, Pressure, Swap};
```

Add to `src/main.rs`, after the `use` block:

```rust
mod sample;
```

- [ ] **Step 2: Create `src/sample/sys.rs`**

```rust
//! Thin safe wrappers over the raw sysctl and Mach calls the readers share.
// libc marks its Mach calls deprecated in favour of mach2; we need only two, and they are stable.
#![allow(deprecated)]

use std::ffi::CStr;
use std::mem::{size_of, MaybeUninit};
use std::ptr;

/// Reads a fixed-size sysctl value by name; `None` if the call fails or the size doesn't match `T`.
pub fn by_name<T: Copy>(name: &CStr) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut len = size_of::<T>();
    let rc = unsafe {
        libc::sysctlbyname(name.as_ptr(), value.as_mut_ptr().cast(), &mut len, ptr::null_mut(), 0)
    };
    (rc == 0 && len == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

/// The host port. Every `mach_host_self` call adds a port reference, so fetch it once and keep it.
pub fn host() -> libc::mach_port_t {
    unsafe { libc::mach_host_self() }
}

/// The VM page size in bytes (16 KB on Apple Silicon).
pub fn page_size() -> u64 {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
}
```

- [ ] **Step 3: Write the failing tests at the bottom of a new `src/sample/mem.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: u64 = 16_384;

    #[test]
    fn breakdown_matches_activity_monitor() {
        let pages = Pages { internal: 800_000, purgeable: 50_000, wired: 200_000, compressed: 300_000 };
        let m = memory(32 << 30, PAGE, pages);
        assert_eq!(m.app, 750_000 * PAGE);
        assert_eq!(m.wired, 200_000 * PAGE);
        assert_eq!(m.compressed, 300_000 * PAGE);
        assert_eq!(m.used(), 1_250_000 * PAGE);
        let expected = (1_250_000 * PAGE) as f64 * 100.0 / (32u64 << 30) as f64;
        assert!((m.percent() - expected).abs() < 1e-9);
    }

    #[test]
    fn purgeable_above_internal_does_not_underflow() {
        let pages = Pages { internal: 10, purgeable: 20, wired: 0, compressed: 0 };
        assert_eq!(memory(1 << 30, PAGE, pages).app, 0);
    }

    #[test]
    fn zero_total_gives_zero_percent() {
        let pages = Pages { internal: 10, purgeable: 0, wired: 0, compressed: 0 };
        assert_eq!(memory(0, PAGE, pages).percent(), 0.0);
    }

    #[test]
    fn pressure_levels_map_and_unknown_is_none() {
        assert_eq!(Pressure::from_level(1), Some(Pressure::Normal));
        assert_eq!(Pressure::from_level(2), Some(Pressure::Warning));
        assert_eq!(Pressure::from_level(4), Some(Pressure::Critical));
        assert_eq!(Pressure::from_level(0), None);
        assert_eq!(Pressure::from_level(3), None);
    }

    #[test]
    fn live_readings_succeed() {
        let host = sys::host();
        assert!(read_pages(host).is_some());
        assert!(read_total().unwrap() > 0);
        assert!(read_pressure().is_some());
        let swap = read_swap().unwrap();
        assert!(swap.used <= swap.total);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test mem::`
Expected: compile errors — `Pages`, `memory`, `Pressure`, `read_pages` etc. not found.

- [ ] **Step 5: Implement `src/sample/mem.rs` above the tests**

```rust
//! Memory readings: Activity Monitor's "Memory Used" breakdown, the pressure level and swap.

use std::mem::MaybeUninit;

use super::sys;

/// Memory in bytes, split the way Activity Monitor splits "Memory Used".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub app: u64,
    pub wired: u64,
    pub compressed: u64,
}

impl Memory {
    /// App + Wired + Compressed — Activity Monitor's definition of used memory.
    pub fn used(&self) -> u64 {
        self.app + self.wired + self.compressed
    }

    /// Used memory as a percentage of physical RAM.
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.used() as f64 * 100.0 / self.total as f64
    }
}

/// The kernel's memory pressure level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressure {
    Normal,
    Warning,
    Critical,
}

impl Pressure {
    /// Maps `kern.memorystatus_vm_pressure_level` (1, 2, 4) to a level; other values are unknown.
    pub fn from_level(level: i32) -> Option<Self> {
        match level {
            1 => Some(Self::Normal),
            2 => Some(Self::Warning),
            4 => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Swap usage in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swap {
    pub used: u64,
    pub total: u64,
}

/// The `vm_statistics64` page counts the breakdown needs.
#[derive(Debug, Clone, Copy)]
pub struct Pages {
    pub internal: u64,
    pub purgeable: u64,
    pub wired: u64,
    pub compressed: u64,
}

/// Converts page counts to bytes. Purgeable pages are cache the system can drop, so they don't count as app memory.
pub fn memory(total: u64, page_size: u64, p: Pages) -> Memory {
    Memory {
        total,
        app: p.internal.saturating_sub(p.purgeable) * page_size,
        wired: p.wired * page_size,
        compressed: p.compressed * page_size,
    }
}

/// Reads the page counts from `host_statistics64(HOST_VM_INFO64)`.
pub fn read_pages(host: libc::mach_port_t) -> Option<Pages> {
    let mut stats = MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let rc = unsafe {
        libc::host_statistics64(host, libc::HOST_VM_INFO64, stats.as_mut_ptr().cast(), &mut count)
    };
    if rc != libc::KERN_SUCCESS {
        return None;
    }
    // The struct is packed; fields are copied out by value, never borrowed.
    let s = unsafe { stats.assume_init() };
    Some(Pages {
        internal: s.internal_page_count as u64,
        purgeable: s.purgeable_count as u64,
        wired: s.wire_count as u64,
        compressed: s.compressor_page_count as u64,
    })
}

/// Physical RAM in bytes.
pub fn read_total() -> Option<u64> {
    sys::by_name::<u64>(c"hw.memsize")
}

/// The current memory pressure level.
pub fn read_pressure() -> Option<Pressure> {
    sys::by_name::<i32>(c"kern.memorystatus_vm_pressure_level").and_then(Pressure::from_level)
}

/// Swap used and total.
pub fn read_swap() -> Option<Swap> {
    sys::by_name::<libc::xsw_usage>(c"vm.swapusage").map(|x| Swap { used: x.xsu_used, total: x.xsu_total })
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test mem::`
Expected: 5 passed. (`dead_code` warnings for unused readers are expected until Task 6.)

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/sample/mod.rs src/sample/sys.rs src/sample/mem.rs
git commit -m "feat(sample): read memory breakdown, pressure and swap"
```

---

### Task 3: CPU busy percentage

**Files:**
- Create: `src/sample/cpu.rs`
- Modify: `src/sample/mod.rs` (add `mod cpu;`)

**Interfaces:**
- Consumes: `sys::host()`
- Produces: `cpu::Ticks` (Copy), `cpu::read(host) -> Option<Ticks>`, `cpu::busy_percent(prev: Ticks, now: Ticks) -> Option<f64>`

- [ ] **Step 1: Write the failing tests in a new `src/sample/cpu.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_share_of_elapsed_ticks() {
        // user 30 + system 20 + nice 0 busy, idle 50 → 50%.
        assert_eq!(busy_percent(Ticks([0, 0, 0, 0]), Ticks([30, 20, 50, 0])), Some(50.0));
    }

    #[test]
    fn nice_counts_as_busy() {
        assert_eq!(busy_percent(Ticks([0, 0, 0, 0]), Ticks([0, 0, 75, 25])), Some(25.0));
    }

    #[test]
    fn wrapped_counter_still_gives_the_right_delta() {
        let prev = Ticks([u32::MAX - 9, 0, 0, 0]);
        let now = Ticks([10, 0, 20, 0]); // user advanced 20 across the wrap
        assert_eq!(busy_percent(prev, now), Some(50.0));
    }

    #[test]
    fn no_elapsed_ticks_is_none() {
        let t = Ticks([5, 5, 5, 5]);
        assert_eq!(busy_percent(t, t), None);
    }

    #[test]
    fn live_reading_succeeds() {
        assert!(read(crate::sample::sys::host()).is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod cpu;` to `src/sample/mod.rs`, then run: `cargo test cpu::`
Expected: compile errors — `Ticks`, `busy_percent`, `read` not found.

- [ ] **Step 3: Implement above the tests**

```rust
//! CPU busy percentage from the kernel's cumulative per-state tick counters.
#![allow(deprecated)] // libc's host_statistics is marked deprecated in favour of mach2; it is stable.

use std::mem::MaybeUninit;

const USER: usize = libc::CPU_STATE_USER as usize;
const SYSTEM: usize = libc::CPU_STATE_SYSTEM as usize;
const IDLE: usize = libc::CPU_STATE_IDLE as usize;
const NICE: usize = libc::CPU_STATE_NICE as usize;

/// Cumulative ticks since boot, indexed by `CPU_STATE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticks(pub [u32; 4]);

/// Reads the all-core tick counters with `host_statistics(HOST_CPU_LOAD_INFO)`.
pub fn read(host: libc::mach_port_t) -> Option<Ticks> {
    let mut info = MaybeUninit::<libc::host_cpu_load_info>::zeroed();
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    let rc = unsafe {
        libc::host_statistics(host, libc::HOST_CPU_LOAD_INFO, info.as_mut_ptr().cast(), &mut count)
    };
    (rc == libc::KERN_SUCCESS).then(|| Ticks(unsafe { info.assume_init() }.cpu_ticks))
}

/// Busy share (user + system + nice) of the ticks elapsed between two readings; `None` if none elapsed.
pub fn busy_percent(prev: Ticks, now: Ticks) -> Option<f64> {
    // Counters are u32 and wrap on long uptimes; wrapping_sub keeps the delta right across a wrap.
    let d = |i: usize| now.0[i].wrapping_sub(prev.0[i]) as u64;
    let busy = d(USER) + d(SYSTEM) + d(NICE);
    let total = busy + d(IDLE);
    (total > 0).then(|| busy as f64 * 100.0 / total as f64)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test cpu::`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/sample/mod.rs src/sample/cpu.rs
git commit -m "feat(sample): compute CPU busy percentage from tick deltas"
```

---

### Task 4: Network throughput

**Files:**
- Create: `src/sample/net.rs`
- Modify: `src/sample/mod.rs` (add `mod net;` and `pub use net::Net;`)

**Interfaces:**
- Produces: `net::Counters` (a `Vec<Iface>`), `net::Iface { index: u16, rx: u64, tx: u64 }`, `net::Net { down: f64, up: f64 }` (bytes/s), `net::read() -> Option<Counters>`, `net::rate(prev: &Counters, now: &Counters, secs: f64) -> Option<Net>`

- [ ] **Step 1: Write the failing tests in a new `src/sample/net.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    fn iface(index: u16, rx: u64, tx: u64) -> Iface {
        Iface { index, rx, tx }
    }

    /// Serialises one RTM_IFINFO2 message the way the kernel lays it out.
    fn msg(index: u16, flags: i32, rx: u64, tx: u64) -> Vec<u8> {
        let mut m: libc::if_msghdr2 = unsafe { std::mem::zeroed() };
        m.ifm_msglen = size_of::<libc::if_msghdr2>() as u16;
        m.ifm_type = libc::RTM_IFINFO2 as u8;
        m.ifm_index = index;
        m.ifm_flags = flags;
        m.ifm_data.ifi_ibytes = rx;
        m.ifm_data.ifi_obytes = tx;
        let bytes = unsafe {
            std::slice::from_raw_parts((&raw const m).cast::<u8>(), size_of::<libc::if_msghdr2>())
        };
        bytes.to_vec()
    }

    #[test]
    fn rate_is_delta_over_time() {
        let prev = vec![iface(4, 1_000, 500)];
        let now = vec![iface(4, 3_000, 1_500)];
        assert_eq!(rate(&prev, &now, 2.0), Some(Net { down: 1_000.0, up: 500.0 }));
    }

    #[test]
    fn counter_going_backwards_counts_as_zero() {
        let prev = vec![iface(4, 5_000, 5_000)];
        let now = vec![iface(4, 100, 6_000)];
        assert_eq!(rate(&prev, &now, 1.0), Some(Net { down: 0.0, up: 1_000.0 }));
    }

    #[test]
    fn interface_that_just_appeared_adds_nothing() {
        let prev = vec![iface(4, 1_000, 1_000)];
        let now = vec![iface(4, 2_000, 1_000), iface(9, 9_000_000_000, 9_000_000_000)];
        assert_eq!(rate(&prev, &now, 1.0), Some(Net { down: 1_000.0, up: 0.0 }));
    }

    #[test]
    fn zero_elapsed_time_is_none() {
        assert_eq!(rate(&Vec::new(), &Vec::new(), 0.0), None);
    }

    #[test]
    fn parse_keeps_up_non_loopback_interfaces_and_skips_other_messages() {
        let mut buf = msg(4, libc::IFF_UP, 100, 50);
        buf.extend(msg(1, libc::IFF_UP | libc::IFF_LOOPBACK, 999, 999));
        buf.extend(msg(7, 0, 888, 888)); // down
        buf.extend([8, 0, libc::RTM_VERSION as u8, libc::RTM_NEWADDR as u8, 0, 0, 0, 0]); // unrelated 8-byte message
        buf.extend(msg(5, libc::IFF_UP, 10, 20));
        assert_eq!(parse(&buf), vec![iface(4, 100, 50), iface(5, 10, 20)]);
    }

    #[test]
    fn parse_stops_on_zero_length_message() {
        assert_eq!(parse(&[0, 0, 0, 0, 0, 0, 0, 0]), vec![]);
    }

    #[test]
    fn parse_skips_an_ifinfo2_message_shorter_than_the_struct() {
        let mut buf = msg(4, libc::IFF_UP, 100, 50);
        buf[0..2].copy_from_slice(&8u16.to_ne_bytes()); // claims 8 bytes: not a whole if_msghdr2
        buf.truncate(8);
        buf.extend(msg(5, libc::IFF_UP, 10, 20));
        assert_eq!(parse(&buf), vec![iface(5, 10, 20)]);
    }

    #[test]
    fn live_reading_succeeds() {
        assert!(read().is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod net;` and `pub use net::Net;` to `src/sample/mod.rs`, then run: `cargo test net::`
Expected: compile errors — `Iface`, `Net`, `rate`, `parse`, `read` not found.

- [ ] **Step 3: Implement above the tests**

```rust
//! Network throughput from the kernel's 64-bit per-interface byte counters (`NET_RT_IFLIST2`).
//! `getifaddrs` is not used: its counters are 32-bit and wrap within minutes on a fast link.

use std::mem::size_of;
use std::ptr;

/// One interface's cumulative byte counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Iface {
    pub index: u16,
    pub rx: u64,
    pub tx: u64,
}

/// Counters for every interface that is up and not loopback.
pub type Counters = Vec<Iface>;

/// Throughput in bytes per second.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Net {
    pub down: f64,
    pub up: f64,
}

/// Throughput between two readings, counting only interfaces present in both.
/// An interface that just appeared (VPN, Wi-Fi rejoin) carries its whole history in its counters and would show as a spike.
pub fn rate(prev: &Counters, now: &Counters, secs: f64) -> Option<Net> {
    if secs <= 0.0 {
        return None;
    }
    let (mut down, mut up) = (0u64, 0u64);
    for n in now {
        if let Some(p) = prev.iter().find(|p| p.index == n.index) {
            // A counter that went backwards means the interface reset; count 0 rather than a wrap-sized spike.
            down += n.rx.checked_sub(p.rx).unwrap_or(0);
            up += n.tx.checked_sub(p.tx).unwrap_or(0);
        }
    }
    Some(Net { down: down as f64 / secs, up: up as f64 / secs })
}

/// Reads the interface list with `sysctl(CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, 0)`.
pub fn read() -> Option<Counters> {
    let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    let mut len = 0usize;
    let rc = unsafe { libc::sysctl(mib.as_mut_ptr(), 6, ptr::null_mut(), &mut len, ptr::null_mut(), 0) };
    if rc != 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let rc = unsafe {
        libc::sysctl(mib.as_mut_ptr(), 6, buf.as_mut_ptr().cast(), &mut len, ptr::null_mut(), 0)
    };
    (rc == 0).then(|| parse(&buf[..len]))
}

/// Walks the routing messages and keeps RTM_IFINFO2 entries for interfaces that are up and not loopback.
fn parse(buf: &[u8]) -> Counters {
    let mut out = Vec::new();
    let mut off = 0;
    // Every routing message starts with u16 length, u8 version, u8 type.
    while off + 4 <= buf.len() {
        let msglen = u16::from_ne_bytes([buf[off], buf[off + 1]]) as usize;
        if msglen == 0 {
            break;
        }
        let kind = buf[off + 3] as i32;
        let whole = msglen >= size_of::<libc::if_msghdr2>() && off + size_of::<libc::if_msghdr2>() <= buf.len();
        if kind == libc::RTM_IFINFO2 && whole {
            // The buffer has no alignment guarantee for this packed struct, so read it unaligned.
            let m: libc::if_msghdr2 = unsafe { ptr::read_unaligned(buf[off..].as_ptr().cast()) };
            let flags = m.ifm_flags;
            if flags & libc::IFF_UP != 0 && flags & libc::IFF_LOOPBACK == 0 {
                out.push(Iface { index: m.ifm_index, rx: m.ifm_data.ifi_ibytes, tx: m.ifm_data.ifi_obytes });
            }
        }
        off += msglen;
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test net::`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add src/sample/mod.rs src/sample/net.rs
git commit -m "feat(sample): measure network throughput from 64-bit interface counters"
```

---

### Task 5: GPU utilisation

**Files:**
- Create: `src/sample/gpu.rs`
- Modify: `src/sample/mod.rs` (add `mod gpu;`)

**Interfaces:**
- Produces: `gpu::Gpu` (owns an IOKit handle, released on drop), `Gpu::open() -> Option<Gpu>`, `Gpu::utilization(&self) -> Option<f64>`

- [ ] **Step 1: Write the failing tests in a new `src/sample/gpu.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_gpu_reports_a_percentage() {
        let gpu = Gpu::open().expect("an IOAccelerator should exist on Apple Silicon");
        let pct = gpu.utilization().expect("PerformanceStatistics should carry Device Utilization %");
        assert!((0.0..=100.0).contains(&pct));
    }

    #[test]
    fn missing_property_is_none_not_a_panic() {
        let gpu = Gpu::open().unwrap();
        assert_eq!(gpu.property_f64(PERF_STATS, ns_string!("No Such Key")), None);
        assert_eq!(gpu.property_f64(ns_string!("NoSuchProperty"), UTILIZATION), None);
    }

    #[test]
    fn non_dictionary_property_is_none_not_a_crash() {
        // IOClass is a string-valued property on every registry entry.
        let gpu = Gpu::open().unwrap();
        assert_eq!(gpu.property_f64(ns_string!("IOClass"), UTILIZATION), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `mod gpu;` to `src/sample/mod.rs`, then run: `cargo test gpu::`
Expected: compile errors — `Gpu` not found.

- [ ] **Step 3: Implement above the tests**

```rust
//! GPU utilisation from the IOAccelerator's "PerformanceStatistics" registry property.
//! The service is matched once at startup; each tick copies one property and nothing else.

use std::ffi::{c_char, c_void};
use std::ptr;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{ns_string, NSDictionary, NSNumber, NSString};

type IoObject = u32;
const IO_MAIN_PORT_DEFAULT: u32 = 0;
// Static strings: no NSString is allocated per tick.
const PERF_STATS: &NSString = ns_string!("PerformanceStatistics");
const UTILIZATION: &NSString = ns_string!("Device Utilization %");

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> IoObject;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *mut c_void;
    fn IOObjectRelease(object: IoObject) -> i32;
}

/// A handle on the GPU's registry entry; the IOKit reference is released on drop.
pub struct Gpu(IoObject);

impl Gpu {
    /// Finds the first IOAccelerator (the integrated GPU on Apple Silicon); `None` if there is none.
    pub fn open() -> Option<Self> {
        // IOServiceGetMatchingService consumes the matching dictionary, so it is not released here.
        let service = unsafe {
            IOServiceGetMatchingService(IO_MAIN_PORT_DEFAULT, IOServiceMatching(c"IOAccelerator".as_ptr()))
        };
        (service != 0).then_some(Self(service))
    }

    /// The GPU's current "Device Utilization %", 0–100.
    pub fn utilization(&self) -> Option<f64> {
        self.property_f64(PERF_STATS, UTILIZATION)
    }

    /// Reads `dict_key` from the dictionary-valued registry property `property` as a number.
    fn property_f64(&self, property: &NSString, dict_key: &NSString) -> Option<f64> {
        let raw = unsafe {
            IORegistryEntryCreateCFProperty(self.0, (property as *const NSString).cast(), ptr::null(), 0)
        };
        // CF types are toll-free bridged to their NS counterparts, and the Create rule hands us one reference to own.
        let value: Retained<AnyObject> = unsafe { Retained::from_raw(raw.cast()) }?;
        // Check the class before treating it as a dictionary: a wrong guess would raise an ObjC exception and abort.
        let stats = value.downcast::<NSDictionary>().ok()?;
        let number = stats.objectForKey(dict_key)?.downcast::<NSNumber>().ok()?;
        Some(number.doubleValue())
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.0) };
    }
}
```

Note: `PerformanceStatistics` is a dictionary on the IOAccelerator (`ioreg -r -c IOAccelerator -d 1`, macOS 27 / M5); the class check keeps a future change from crashing. A `"NoSuchProperty"` lookup returns null, which `from_raw` maps to `None`. objc2 0.6's `downcast` is a class check; if the compiler rejects `downcast::<NSDictionary>()` (generic class) or the `objectForKey` key type, use `value.isKindOfClass(NSDictionary::class())` and then cast — keep the check.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test gpu::`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src/sample/mod.rs src/sample/gpu.rs
git commit -m "feat(sample): read GPU utilisation from the cached IOAccelerator"
```

Then open PR `ms/2-samplers → feat/core`; self-merge once `cargo test` and `cargo clippy -- -D warnings -A dead_code -A unused_imports` are green (the readers are unused until Task 6).

---

### Task 6: Snapshot, Sampler and display formatting

**Branch:** `ms/3-live-title` off `feat/core` (after ms/2 merged).

**Files:**
- Modify: `src/sample/mod.rs` (add `Snapshot`, `Sampler`)
- Create: `src/format.rs`
- Modify: `src/main.rs` (add `mod format;`)

**Interfaces:**
- Consumes: everything produced in Tasks 2–5.
- Produces:
  - `sample::Snapshot { cpu: Option<f64>, gpu: Option<f64>, mem: Option<Memory>, pressure: Option<Pressure>, swap: Option<Swap>, net: Option<Net> }` (Clone, Default, PartialEq)
  - `sample::Sampler::new() -> Sampler`, `Sampler::sample(&mut self, detail: bool) -> Snapshot`, `Sampler::reset(&mut self)`
  - `format::StatusLine = [(&'static str, String); 3]`, `format::status(&Snapshot) -> StatusLine`, `format::percent(Option<f64>) -> String`, `format::size(u64) -> String`, `format::size_of_total(u64, u64) -> String`, `format::rate(f64) -> String`, `format::pressure(Option<Pressure>) -> &'static str`, `format::MISSING`

- [ ] **Step 1: Write the failing tests at the bottom of `src/sample/mod.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_sample_is_a_full_snapshot() {
        let mut sampler = Sampler::new();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let s = sampler.sample(true);
        let cpu = s.cpu.expect("cpu");
        assert!((0.0..=100.0).contains(&cpu));
        assert!(s.gpu.is_some());
        assert!(s.mem.expect("mem").total > 0);
        assert!(s.pressure.is_some());
        assert!(s.swap.is_some());
        assert!(s.net.is_some());
    }

    #[test]
    fn menu_bar_sample_skips_dropdown_readings_and_drops_the_net_baseline() {
        let mut sampler = Sampler::new();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let s = sampler.sample(false);
        assert!(s.cpu.is_some() && s.gpu.is_some() && s.mem.is_some());
        assert_eq!((s.pressure, s.swap, s.net), (None, None, None));
        // No baseline survived, so the first detailed sample has no rate rather than one across the gap.
        assert_eq!(sampler.sample(true).net, None);
        assert!(sampler.sample(true).net.is_some());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test sample::tests`
Expected: compile error — `Sampler` not found.

- [ ] **Step 3: Implement `Snapshot` and `Sampler` in `src/sample/mod.rs`**

Replace the file's non-test content with:

```rust
//! System readings for Wisp. Nothing here touches AppKit, so every reader is tested headless.

mod cpu;
mod gpu;
mod mem;
mod net;
mod sys;

use std::time::Instant;

pub use mem::{Memory, Pressure, Swap};
pub use net::Net;

/// One reading of everything Wisp displays; `None` means that reading failed this tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub mem: Option<Memory>,
    pub pressure: Option<Pressure>,
    pub swap: Option<Swap>,
    pub net: Option<Net>,
}

/// Owns everything that must persist between ticks: the host port, the GPU handle and the last counters.
pub struct Sampler {
    host: libc::mach_port_t,
    page_size: u64,
    total: Option<u64>,
    gpu: Option<gpu::Gpu>,
    cpu_prev: Option<cpu::Ticks>,
    net_prev: Option<(net::Counters, Instant)>,
}

impl Sampler {
    /// Opens every source once and takes a baseline, so the first `sample()` already has deltas.
    pub fn new() -> Self {
        let mut sampler = Self {
            host: sys::host(),
            page_size: sys::page_size(),
            total: mem::read_total(),
            gpu: gpu::Gpu::open(),
            cpu_prev: None,
            net_prev: None,
        };
        sampler.reset();
        sampler
    }

    /// Takes a fresh baseline so the next sample isn't averaged across a pause (sleep, lock).
    pub fn reset(&mut self) {
        self.cpu_prev = cpu::read(self.host);
        self.net_prev = net::read().map(|c| (c, Instant::now()));
    }

    /// Reads the menu bar values; with `detail` (dropdown open) also network, pressure and swap.
    /// Delta-based readings cover the time since the previous call.
    pub fn sample(&mut self, detail: bool) -> Snapshot {
        let cpu_now = cpu::read(self.host);
        let cpu = match (self.cpu_prev, cpu_now) {
            (Some(prev), Some(now)) => cpu::busy_percent(prev, now),
            _ => None,
        };
        self.cpu_prev = cpu_now;

        let mut snap = Snapshot {
            cpu,
            gpu: self.gpu.as_ref().and_then(gpu::Gpu::utilization),
            mem: self.total.zip(mem::read_pages(self.host)).map(|(t, p)| mem::memory(t, self.page_size, p)),
            ..Snapshot::default()
        };
        if !detail {
            // Nobody sees the dropdown readings, so skip them; a stale net baseline would average across the gap.
            self.net_prev = None;
            return snap;
        }

        let at = Instant::now();
        let net_now = net::read();
        snap.net = match (&self.net_prev, &net_now) {
            (Some((prev, then)), Some(now)) => net::rate(prev, now, at.duration_since(*then).as_secs_f64()),
            _ => None,
        };
        self.net_prev = net_now.map(|c| (c, at));
        snap.pressure = mem::read_pressure();
        snap.swap = mem::read_swap();
        snap
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test sample::tests`
Expected: 2 passed.

- [ ] **Step 5: Write the failing format tests in a new `src/format.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{Memory, Snapshot};

    const GIB: u64 = 1 << 30;

    #[test]
    fn percent_pads_to_three_figure_widths() {
        assert_eq!(percent(Some(3.0)), "\u{2007}\u{2007}3%");
        assert_eq!(percent(Some(12.4)), "\u{2007}12%");
        assert_eq!(percent(Some(99.6)), "100%");
        for v in [0.0, 7.0, 42.0, 100.0] {
            assert_eq!(percent(Some(v)).chars().count(), 4, "constant width for {v}");
        }
    }

    #[test]
    fn percent_clamps_out_of_range_and_nan() {
        assert_eq!(percent(Some(150.0)), "100%");
        assert_eq!(percent(Some(-2.0)), "\u{2007}\u{2007}0%");
        assert_eq!(percent(Some(f64::NAN)), "\u{2007}\u{2007}0%");
    }

    #[test]
    fn missing_values_show_a_dash() {
        // Figure dash is digit-wide, so a missing reading keeps the menu bar item's width.
        assert_eq!(percent(None), "\u{2007}\u{2007}\u{2012}%");
        assert_eq!(percent(None).chars().count(), 4);
        assert_eq!(pressure(None), MISSING);
    }

    #[test]
    fn sizes_drop_a_trailing_zero_decimal() {
        assert_eq!(size(GIB * 32), "32 GB");
        assert_eq!(size(GIB * 3 + GIB / 10), "3.1 GB");
        assert_eq!(size_of_total(GIB * 39 / 2, GIB * 32), "19.5 / 32 GB");
    }

    #[test]
    fn rates_pick_a_short_unit() {
        assert_eq!(rate(0.0), "0 B/s");
        assert_eq!(rate(500.0), "500 B/s");
        assert_eq!(rate(999.6), "1 KB/s");
        assert_eq!(rate(84.0 * 1024.0), "84 KB/s");
        assert_eq!(rate(1.2 * 1024.0 * 1024.0), "1.2 MB/s");
        assert_eq!(rate(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB/s");
    }

    #[test]
    fn pressure_labels() {
        assert_eq!(pressure(Some(Pressure::Normal)), "Normal");
        assert_eq!(pressure(Some(Pressure::Warning)), "Warning");
        assert_eq!(pressure(Some(Pressure::Critical)), "Critical");
    }

    #[test]
    fn status_line_is_cpu_gpu_mem_and_survives_missing_gpu() {
        let snap = Snapshot {
            cpu: Some(12.0),
            gpu: None,
            mem: Some(Memory { total: 100, app: 40, wired: 11, compressed: 10 }),
            ..Snapshot::default()
        };
        let line = status(&snap);
        assert_eq!(line[0], ("CPU", "\u{2007}12%".to_string()));
        assert_eq!(line[1], ("GPU", "\u{2007}\u{2007}\u{2012}%".to_string()));
        assert_eq!(line[2], ("MEM", "\u{2007}61%".to_string()));
    }
}
```

- [ ] **Step 6: Run tests to verify they fail**

Add `mod format;` to `src/main.rs` next to `mod sample;`, then run: `cargo test format::`
Expected: compile errors — `percent`, `size`, `rate`, etc. not found.

- [ ] **Step 7: Implement `src/format.rs` above the tests**

```rust
//! Turns a Snapshot into the exact strings Wisp shows. Pure, so every display rule is unit-tested.

use crate::sample::{Pressure, Snapshot};

/// Shown in the dropdown in place of any reading that failed.
pub const MISSING: &str = "—";

/// U+2007 and U+2012 are as wide as a digit, so padded numbers and missing values keep the menu bar item a constant width.
const FIGURE_SPACE: char = '\u{2007}';
const FIGURE_DASH: char = '\u{2012}';
const GIB: f64 = (1u64 << 30) as f64;

/// The menu bar's (label, value) pairs in display order.
pub type StatusLine = [(&'static str, String); 3];

/// The CPU / GPU / MEM pairs for the menu bar.
pub fn status(s: &Snapshot) -> StatusLine {
    [
        ("CPU", percent(s.cpu)),
        ("GPU", percent(s.gpu)),
        ("MEM", percent(s.mem.map(|m| m.percent()))),
    ]
}

/// A whole-number percentage padded to three digit-widths, e.g. "  3%"; missing is "  ‒%".
pub fn percent(value: Option<f64>) -> String {
    let pad = |s: String| s.replace(' ', &FIGURE_SPACE.to_string());
    match value {
        // NaN clamps to NaN and `as u32` maps it to 0.
        Some(v) => pad(format!("{:>3}%", v.round().clamp(0.0, 100.0) as u32)),
        None => format!("{FIGURE_SPACE}{FIGURE_SPACE}{FIGURE_DASH}%"),
    }
}

/// Bytes as GB (binary, as Activity Monitor counts) with at most one decimal.
pub fn size(bytes: u64) -> String {
    format!("{} GB", one_decimal(bytes as f64 / GIB))
}

/// "used / total GB", e.g. "19.5 / 32 GB".
pub fn size_of_total(used: u64, total: u64) -> String {
    format!("{} / {} GB", one_decimal(used as f64 / GIB), one_decimal(total as f64 / GIB))
}

/// Throughput with the shortest sensible unit: whole B/s and KB/s, one decimal for MB/s and GB/s.
pub fn rate(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bytes_per_sec.max(0.0);
    let mut unit = 0;
    // Step up at 999.5 rather than 1024 so a value never rounds to a four-digit "1024 B/s".
    while v >= 999.5 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit <= 1 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// The pressure level's label.
pub fn pressure(p: Option<Pressure>) -> &'static str {
    match p {
        Some(Pressure::Normal) => "Normal",
        Some(Pressure::Warning) => "Warning",
        Some(Pressure::Critical) => "Critical",
        None => MISSING,
    }
}

fn one_decimal(x: f64) -> String {
    let s = format!("{x:.1}");
    s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test`
Expected: all tests pass (mem 5, cpu 5, net 8, gpu 3, sample 2, format 7).

- [ ] **Step 9: Commit**

```bash
git add src/main.rs src/sample/mod.rs src/format.rs
git commit -m "feat: add sampler snapshot and display formatting"
```

---

### Task 7: Menu bar status item that redraws only on change

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/status.rs`
- Modify: `src/main.rs` (add `mod ui;`)

**Interfaces:**
- Consumes: `format::StatusLine`
- Produces: `ui::status::StatusItem::new(mtm, menu: &NSMenu) -> StatusItem`, `StatusItem::show(&self, mtm, line: StatusLine)`

- [ ] **Step 1: Create `src/ui/mod.rs`**

```rust
//! AppKit-facing pieces of Wisp: the menu bar item (and, in the menu feature, the dropdown).

pub mod status;
```

- [ ] **Step 2: Create `src/ui/status.rs`**

```rust
//! The menu bar item: draws the CPU / GPU / MEM line and skips the redraw when the text is unchanged.
//! Every title change costs WindowServer a re-composite, so unchanged ticks must cost nothing.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontWeightRegular, NSForegroundColorAttributeName, NSMenu,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSString};

use crate::format::StatusLine;

/// Owns the status item and the last line drawn into it.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    last: RefCell<Option<StatusLine>>,
}

impl StatusItem {
    /// Adds a variable-width item to the menu bar with `menu` as its dropdown.
    pub fn new(_mtm: MainThreadMarker, menu: &NSMenu) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setMenu(Some(menu));
        Self { item, last: RefCell::new(None) }
    }

    /// Draws `line` unless it is identical to what is already shown.
    pub fn show(&self, mtm: MainThreadMarker, line: StatusLine) {
        if self.last.borrow().as_ref() == Some(&line) {
            return;
        }
        if let Some(button) = self.item.button(mtm) {
            button.setAttributedTitle(&attributed(&line));
        }
        *self.last.borrow_mut() = Some(line);
    }
}

/// Labels in the secondary label colour, values in the primary one, all with monospaced digits.
fn attributed(line: &StatusLine) -> Retained<NSAttributedString> {
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(NSFont::systemFontSize(), unsafe { NSFontWeightRegular });
    let label = attributes(&font, &NSColor::secondaryLabelColor());
    let value = attributes(&font, &NSColor::labelColor());
    let out = NSMutableAttributedString::new();
    for (i, (name, val)) in line.iter().enumerate() {
        let gap = if i == 0 { "" } else { "  " };
        append(&out, &format!("{gap}{name} "), &label);
        append(&out, val, &value);
    }
    Retained::into_super(out)
}

fn attributes(font: &NSFont, color: &NSColor) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    let keys: [&NSAttributedStringKey; 2] = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

fn append(out: &NSMutableAttributedString, text: &str, attrs: &NSDictionary<NSAttributedStringKey, AnyObject>) {
    let piece = unsafe {
        NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(text), Some(attrs))
    };
    out.appendAttributedString(&piece);
}
```

Add `mod ui;` to `src/main.rs` next to the other `mod` lines.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles (an unused warning for `StatusItem` until Task 8 is fine).

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/ui/mod.rs src/ui/status.rs
git commit -m "feat(ui): draw the status line with monospaced digits and skip unchanged redraws"
```

---

### Task 8: Wire the sampler, status item and timer together

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `Sampler::new/sample`, `format::status`, `StatusItem::new/show`, `quit_menu`

- [ ] **Step 1: Replace `src/main.rs`**

```rust
//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer.
//! All UI setup happens in the delegate once AppKit has finished launching.

mod format;
mod sample;
mod ui;

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem};
use objc2_foundation::{ns_string, NSNotification, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSTimer};

use sample::Sampler;
use ui::status::StatusItem;

/// Seconds between samples. Spec §3.3 default; the menu feature makes it user-selectable.
const INTERVAL: f64 = 2.0;

/// State the delegate keeps alive for the app's lifetime.
struct Ivars {
    sampler: RefCell<Sampler>,
    status: OnceCell<StatusItem>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and AppDelegate does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let _ = self.ivars().status.set(StatusItem::new(mtm, &quit_menu(mtm)));
            self.tick();
            self.start_timer();
        }
    }

    impl AppDelegate {
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _timer: &NSTimer) {
            self.tick();
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = Ivars { sampler: RefCell::new(Sampler::new()), status: OnceCell::new() };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Samples once and updates the menu bar text.
    fn tick(&self) {
        // Menu bar values only; the menu feature passes `true` while the dropdown is open.
        let snapshot = self.ivars().sampler.borrow_mut().sample(false);
        if let Some(status) = self.ivars().status.get() {
            status.show(self.mtm(), format::status(&snapshot));
        }
    }

    /// One repeating timer for everything. The 10% tolerance lets macOS batch our wakeup with others;
    /// common modes keep it firing while a menu is open (menus track events in their own run loop mode).
    fn start_timer(&self) {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(INTERVAL, self, sel!(timerFired:), None, true)
        };
        timer.setTolerance(INTERVAL * 0.1);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    }
}

/// The dropdown menu; for now only Quit, which a nil target routes to NSApplication's `terminate:`.
fn quit_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit Wisp"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    };
    menu.addItem(&quit);
    menu
}

fn main() {
    let mtm = MainThreadMarker::new().expect("Wisp must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // LSUIElement covers the bundled app; this also keeps `cargo run` out of the Dock.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
```

- [ ] **Step 2: Lint, test, bundle, launch**

```bash
cargo clippy --release -- -D warnings && cargo test && ./scripts/bundle.sh && open target/Wisp.app
```

Expected: no warnings; all tests pass; the menu bar shows e.g. `CPU  12%  GPU   3%  MEM  61%` with dimmer labels.

- [ ] **Step 3: Manual checks**

1. Values change every ~2 s; the item never changes width as digits change (watch the icons to its left).
2. Open the menu and hold it open for 10 s: the menu bar values keep updating (timer is in common modes).
3. Switch light ↔ dark (System Settings → Appearance): text stays legible in both.
4. ⌘Q from the open menu quits.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: show live CPU, GPU and memory in the menu bar"
```

---

### Task 9: Measure against the success criteria

**Files:** none (results go in the `feat/core → main` PR description; the README table is the menu feature's job).

- [ ] **Step 1: Footprint, CPU, wakeups and power (S1, S2, S5)** — with Wisp running from `target/Wisp.app` for 10 minutes, menu closed:

```bash
PID=$(pgrep -x wisp)
./scripts/measure.sh "$PID" 600
top -l 300 -s 2 -pid "$PID" -stats pid,power | awk '$1==pid && seen++ {p+=$2; n++} END {printf "avg power %.3f (%d samples)\n", p/n, n}' pid="$PID"
footprint "$PID" | head -n 5
```

Expected: average CPU < 0.1%; phys footprint < 15 MB; compare with the Task 1 floor. Run the same commands against Stats (`pgrep -x Stats`) for S5.

Note: the kernel publishes idle CPU ticks up to ~1 s apart on macOS 27, so the sampler tests wait 1.1 s (a 200 ms window often saw no elapsed ticks).

- [ ] **Step 1b: App Nap** — during the same 10 minutes, keep another app frontmost and glance at the menu bar: values must still change every ~2 s. If they stall, App Nap is throttling the timer; only then hold an `NSProcessInfo` activity (`beginActivityWithOptions_reason`) for the app's lifetime and re-measure, since it can raise wakeups.

- [ ] **Step 2: WindowServer (S4)** — 60 s of `top -l 30 -s 2 -pid $(pgrep -x WindowServer) -stats cpu` with Wisp running, then again with Wisp quit. Expected: averages within noise of each other.

- [ ] **Step 3: Accuracy (S6)** — open Activity Monitor next to Wisp's numbers: CPU (CPU tab, "User + System"), Memory Used and Swap Used (Memory tab) agree within rounding.

- [ ] **Step 4: Record** — paste the numbers, machine (M5 MacBook Air, 32 GB) and macOS version into the PR description.

- [ ] **Step 5: Open PRs**

PR `ms/3-live-title → feat/core` (self-merge on green), then PR `feat/core → main` with the measurements; review it and merge (merge access granted 2026-10-17).


---

### Task 10: Redraw hysteresis (decision 2026-10-17)

**Why:** Task 9 measured ~14 ms of CPU per title change — AppKit re-renders the status item for each display — while sampling and the timer cost 0.017%. CPU % jitters by a point or two almost every tick, so "redraw only on change" still redrew nearly every tick (~0.7% CPU). The user chose hysteresis at the 2 s interval over a longer interval.

**Branch:** `ms/4-redraw-hysteresis` off `feat/core`.

**Rule:** redraw only when a reading appears or disappears, or any value has moved **≥ 2 points from the value currently shown** (not from the last sample — slow drift still shows once it adds up to 2 points).

**Files:** Modify `src/format.rs`, `src/ui/status.rs`, `src/main.rs`.

**Interfaces:**
- `format::Values = [Option<f64>; 3]` — CPU, GPU, MEM percentages as shown on the bar.
- `format::values(&Snapshot) -> Values`
- `format::moved(shown: &Values, now: &Values) -> bool` — the rule above; `HYSTERESIS: f64 = 2.0`.
- `format::status(&Values) -> StatusLine` (was `&Snapshot`).
- `StatusItem::show(&self, mtm, values: Values)` keeps the last *drawn* `Values`; first call always draws.
- `tick()` passes `format::values(&snapshot)`.

- [ ] **Step 1: Failing tests in `format.rs`:** no move below 2 points (12.0 → 13.9); move at exactly 2 (12.0 → 14.0) and downward (12.0 → 10.0); any single field crossing triggers; `Some` ↔ `None` on any field triggers; drift is measured from the shown value (shown 12.0, now 13.0 → false; shown still 12.0, now 14.1 → true); update the existing `status_line_...` test for the new signature.
- [ ] **Step 2:** Run, see them fail. **Step 3:** Implement. **Step 4:** Gate green.
- [ ] **Step 5: Commit** `feat(ui): redraw the menu bar only when a value moves two points`
- [ ] **Step 6: Re-measure** with `scripts/measure.sh "$(pgrep -x wisp)" 600` (menu closed) and the same on Stats; record in the PR.
