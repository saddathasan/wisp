//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer.
//! All UI setup happens in the delegate once AppKit has finished launching.
//! exp/menu-cost adds the `WISP_DRIVE` test driver and, with `--dropdown x y`, the out-of-process helper (variant g).

mod format;
mod sample;
mod ui;

use std::cell::{Cell, OnceCell, RefCell};
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSApplicationActivationPolicy, NSApplicationDelegate, NSControlStateValueOff,
    NSControlStateValueOn, NSMenu, NSMenuDelegate, NSMenuItem, NSPopoverDelegate, NSView,
    NSWindow, NSWindowDelegate, NSWindowOcclusionState,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes,
    NSTimer,
};

use sample::Sampler;
use ui::menu::{Dropdown, INTERVALS, Kind, Variant};
use ui::panel::{self, Panel};
use ui::status::StatusItem;

/// Seconds between samples. Spec §3.3 default; the menu feature makes it user-selectable.
const INTERVAL: f64 = 2.0;
/// Seconds after launch before the driver's first open, so startup has settled.
const SETTLE: f64 = 20.0;

/// The live helper's pid (variant g), 0 when none; its waiter thread clears it.
static HELPER: AtomicI32 = AtomicI32::new(0);

/// `WISP_DRIVE=<n>[,<hold_s>,<gap_s>]`: open the dropdown n times through the button's click path after `SETTLE`,
/// hold each open hold_s (live updates on), close it, wait gap_s. `WISP_LOOKS=light,dark,..` forces the appearance per open.
struct Drive {
    n: usize,
    hold: f64,
    gap: f64,
    looks: Vec<String>,
    i: Cell<usize>,
    /// When the current open or close was requested, ms since the epoch.
    t: Cell<f64>,
    closing: Cell<bool>,
    poll: RefCell<Option<Retained<NSTimer>>>,
}

impl Drive {
    /// A negative `hold` never closes: the helper's interactive open still logs its latency.
    fn new(n: usize, hold: f64, gap: f64, looks: Vec<String>) -> Self {
        Self { n, hold, gap, looks, i: Cell::new(0), t: Cell::new(0.0), closing: Cell::new(false), poll: RefCell::new(None) }
    }

    fn from_env() -> Option<Self> {
        let v = std::env::var("WISP_DRIVE").ok()?;
        let f: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        let looks = std::env::var("WISP_LOOKS").unwrap_or_default();
        let looks = looks.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect();
        Some(Self::new(*f.first()? as usize, f.get(1).copied().unwrap_or(4.0), f.get(2).copied().unwrap_or(7.0), looks))
    }
}

/// State the delegate keeps alive for the app's lifetime.
struct Ivars {
    sampler: RefCell<Sampler>,
    status: OnceCell<StatusItem>,
    dropdown: OnceCell<Dropdown>,
    /// Variants d and e: the dropdown while it is open.
    panel: RefCell<Option<Panel>>,
    open: Cell<bool>,
    interval: Cell<isize>,
    login: Cell<bool>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    drive: Option<Drive>,
    /// Helper mode (variant g): where to pop the menu up, in screen coordinates.
    helper_at: Option<NSPoint>,
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
            force_look(mtm, std::env::var("WISP_FORCE_APPEARANCE").ok().as_deref());
            if self.ivars().helper_at.is_some() {
                let dropdown = Dropdown::new(mtm, Variant::text());
                dropdown.menu.setDelegate(Some(ProtocolObject::from_ref(self)));
                dropdown.build(self, self.ivars().interval.get(), login_enabled());
                let _ = self.ivars().dropdown.set(dropdown);
                self.tick();
                self.start_timer();
                // popUp blocks until the menu closes, so it runs from a timer callout, as a click's tracking loop would.
                self.after(0.0, sel!(driveOpen:));
                return;
            }
            let dropdown = Dropdown::new(mtm, Variant::from_env());
            let v = dropdown.variant;
            if matches!(v.kind, Kind::Text | Kind::View) {
                dropdown.menu.setDelegate(Some(ProtocolObject::from_ref(self)));
                if !v.lazy {
                    dropdown.build(self, self.ivars().interval.get(), login_enabled());
                }
            }
            let status = StatusItem::new(mtm, v.is_menu().then_some(&*dropdown.menu));
            if !v.is_menu() {
                let button = status.button();
                unsafe {
                    button.setTarget(Some(self));
                    button.setAction(Some(sel!(toggleDropdown:)));
                }
            }
            let _ = self.ivars().status.set(status);
            let _ = self.ivars().dropdown.set(dropdown);
            self.tick();
            self.start_timer();
            if self.ivars().drive.is_some() {
                self.after(SETTLE, sel!(driveOpen:));
            }
        }
    }

    impl AppDelegate {
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _timer: &NSTimer) {
            self.tick();
        }

        #[unsafe(method(setInterval:))]
        fn set_interval(&self, sender: &NSMenuItem) {
            self.apply_interval(sender.tag());
            if let Some(sub) = unsafe { sender.menu() } {
                for item in sub.itemArray() {
                    item.setState(if item.tag() == sender.tag() { NSControlStateValueOn } else { NSControlStateValueOff });
                }
            }
        }

        // Only flips the checkmark: registering for real would change the user's login items during measurement runs
        // (an earlier build here did, and was unregistered again). The status read at build time stays, as it is part of the cost.
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, sender: &NSMenuItem) {
            let on = sender.state() == NSControlStateValueOn;
            sender.setState(if on { NSControlStateValueOff } else { NSControlStateValueOn });
        }

        /// Variants d, e, g: the button's action, in place of an attached NSMenu.
        #[unsafe(method(toggleDropdown:))]
        fn toggle_dropdown(&self, _sender: Option<&AnyObject>) {
            let kind = self.ivars().dropdown.get().map(|d| d.variant.kind);
            if kind == Some(Kind::Helper) {
                return self.toggle_helper();
            }
            if self.ivars().panel.borrow().is_some() {
                return self.close_dropdown();
            }
            let Some(d) = self.ivars().dropdown.get() else { return };
            let status = self.ivars().status.get().expect("status item exists after launch");
            let button = status.button();
            let at = screen_frame(&button);
            self.ivars().login.set(login_enabled());
            let p = Panel::new(self.mtm(), d.variant, kind == Some(Kind::Popover), self.ivars().interval.get(), self.ivars().login.get());
            match &p.host {
                panel::Host::Panel(w) => w.setDelegate(Some(ProtocolObject::from_ref(self))),
                panel::Host::Popover(po) => {
                    po.setDelegate(Some(ProtocolObject::from_ref(self)));
                    // Like the menu, the popover follows its button's (the menu bar's) appearance unless forced.
                    if let Some(a) = NSApplication::sharedApplication(self.mtm()).appearance() {
                        po.setAppearance(Some(&a));
                    }
                }
            }
            *self.ivars().panel.borrow_mut() = Some(p);
            self.ivars().open.set(true);
            self.tick();
            if let Some(p) = self.ivars().panel.borrow().as_ref() {
                p.show(&button, at);
            }
        }

        /// A row click or Escape in the panel/popover content.
        #[unsafe(method(panelAction:))]
        fn panel_action(&self, tag: isize) {
            match tag {
                panel::INTERVAL => {
                    let cur = self.ivars().interval.get();
                    let next = INTERVALS[(INTERVALS.iter().position(|&s| s == cur).unwrap_or(0) + 1) % INTERVALS.len()];
                    self.apply_interval(next);
                    if let Some(p) = self.ivars().panel.borrow().as_ref() {
                        p.set_interval(next);
                    }
                }
                panel::LOGIN => {
                    // Checkmark only, as toggleLogin: does.
                    self.ivars().login.set(!self.ivars().login.get());
                    if let Some(p) = self.ivars().panel.borrow().as_ref() {
                        p.set_login(self.ivars().login.get());
                    }
                }
                panel::QUIT => NSApplication::sharedApplication(self.mtm()).terminate(None),
                _ => self.close_dropdown(),
            }
        }

        #[unsafe(method(driveOpen:))]
        fn drive_open(&self, _timer: Option<&NSTimer>) {
            let Some(d) = &self.ivars().drive else { return };
            let i = d.i.get() + 1;
            d.i.set(i);
            if let Some(look) = d.looks.get(i - 1) {
                force_look(self.mtm(), Some(look));
                // A status item's menu follows the menu bar's appearance, not the app's.
                if let Some(dd) = self.ivars().dropdown.get() {
                    dd.menu.setAppearance(NSApplication::sharedApplication(self.mtm()).appearance().as_deref());
                }
            }
            d.closing.set(false);
            d.t.set(now_ms());
            log(&format!("open {i}"));
            self.start_poll();
            if let Some(at) = self.ivars().helper_at {
                let Some(dd) = self.ivars().dropdown.get() else { return };
                // Returns once the menu has closed.
                dd.menu.popUpMenuPositioningItem_atLocation_inView(None, at, None);
                log("helper exit");
                std::process::exit(0);
            }
            let button = self.ivars().status.get().expect("status item exists after launch").button();
            // The real click path: an attached NSMenu opens in a nested tracking loop that returns on close;
            // otherwise the button sends toggleDropdown:.
            unsafe { button.performClick(None) };
        }

        #[unsafe(method(drivePoll:))]
        fn drive_poll(&self, _timer: &NSTimer) {
            let Some(d) = &self.ivars().drive else { return };
            let helper = self.ivars().dropdown.get().map(|x| x.variant.kind) == Some(Kind::Helper)
                && self.ivars().helper_at.is_none();
            let shown = if helper { (HELPER.load(Ordering::SeqCst) != 0).then(|| None) } else { dropdown_window(self.mtm()).map(Some) };
            let i = d.i.get();
            let dt = now_ms() - d.t.get();
            match (d.closing.get(), shown) {
                (false, Some(w)) => {
                    self.stop_poll();
                    match w {
                        Some(w) => {
                            let f = w.frame();
                            let (o, z) = (f.origin, f.size);
                            let name = w.class().name().to_string_lossy().into_owned();
                            let active = NSApplication::sharedApplication(self.mtm()).isActive();
                            log(&format!(
                                "visible {i} {dt:.1} ms win {} {name} at {},{} {}x{} key {} active {active} space {}",
                                w.windowNumber(), o.x, o.y, z.width, z.height, w.isKeyWindow(),
                                w.colorSpace().and_then(|c| c.localizedName()).map_or("-".into(), |n| n.to_string())
                            ));
                        }
                        None => log(&format!("spawned {i} {dt:.1} ms pid {}", HELPER.load(Ordering::SeqCst))),
                    }
                    if d.hold >= 0.0 {
                        self.after(d.hold, sel!(driveClose:));
                    }
                }
                (true, None) => {
                    self.stop_poll();
                    log(&format!("gone {i} {dt:.1} ms"));
                    if i < d.n {
                        self.after(d.gap, sel!(driveOpen:));
                    } else {
                        log("done");
                    }
                }
                _ => {}
            }
        }

        #[unsafe(method(driveClose:))]
        fn drive_close(&self, _timer: &NSTimer) {
            let Some(d) = &self.ivars().drive else { return };
            d.closing.set(true);
            d.t.set(now_ms());
            log(&format!("close {}", d.i.get()));
            match self.ivars().dropdown.get().map(|x| x.variant.kind) {
                Some(Kind::Panel | Kind::Popover) => self.close_dropdown(),
                // The helper closes itself after the same hold (WISP_HOLD); here we only wait for it to exit.
                Some(Kind::Helper) if self.ivars().helper_at.is_none() => {}
                _ => self.ivars().dropdown.get().expect("dropdown exists").menu.cancelTracking(),
            }
            self.start_poll();
        }
    }

    unsafe impl NSMenuDelegate for AppDelegate {
        // Variant c: rows exist only between here and menuDidClose.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, _menu: &NSMenu) {
            if let Some(d) = self.ivars().dropdown.get() {
                if d.variant.lazy && !d.is_built() {
                    d.build(self, self.ivars().interval.get(), login_enabled());
                }
            }
        }

        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            self.ivars().open.set(true);
            self.tick();
        }

        #[unsafe(method(menuDidClose:))]
        fn menu_did_close(&self, _menu: &NSMenu) {
            self.ivars().open.set(false);
            if let Some(d) = self.ivars().dropdown.get() {
                if d.variant.lazy {
                    d.release();
                }
            }
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        // Variant d's outside-click dismissal: any click that makes another window key (another app's, or the desktop's).
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.close_dropdown();
        }
    }

    unsafe impl NSPopoverDelegate for AppDelegate {
        // Variant e: a transient popover closes itself on outside clicks; release it then too.
        #[unsafe(method(popoverDidClose:))]
        fn popover_did_close(&self, _notification: &NSNotification) {
            self.close_dropdown();
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker, helper_at: Option<NSPoint>) -> Retained<Self> {
        let drive = match helper_at {
            // The helper drives its own single open; it closes itself only when the main driver passed WISP_HOLD.
            Some(_) => {
                let hold = std::env::var("WISP_HOLD").ok().and_then(|s| s.parse().ok()).unwrap_or(-1.0);
                Some(Drive::new(1, hold, 0.0, Vec::new()))
            }
            None => Drive::from_env(),
        };
        let ivars = Ivars {
            sampler: RefCell::new(Sampler::new()),
            status: OnceCell::new(),
            dropdown: OnceCell::new(),
            panel: RefCell::new(None),
            open: Cell::new(false),
            interval: Cell::new(INTERVAL as isize),
            login: Cell::new(false),
            timer: RefCell::new(None),
            drive,
            helper_at,
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Samples once and updates the menu bar text.
    fn tick(&self) {
        // Dropdown readings only while it is open.
        let open = self.ivars().open.get();
        let snapshot = self.ivars().sampler.borrow_mut().sample(open);
        if let Some(status) = self.ivars().status.get() {
            status.show(format::values(&snapshot));
        }
        if open {
            if let Some(d) = self.ivars().dropdown.get() {
                d.update(&snapshot);
            }
            if let Some(p) = self.ivars().panel.borrow().as_ref() {
                p.update(&snapshot);
            }
        }
    }

    fn apply_interval(&self, secs: isize) {
        self.ivars().interval.set(secs);
        if let Some(t) = self.ivars().timer.take() {
            t.invalidate();
        }
        self.start_timer();
    }

    /// Variants d and e: takes the dropdown off screen and releases it.
    fn close_dropdown(&self) {
        self.ivars().open.set(false);
        // Taken first: closing re-enters through windowDidResignKey: / popoverDidClose:.
        let p = self.ivars().panel.take();
        if let Some(p) = p {
            p.close();
        }
    }

    /// Variant g: spawns the helper with the button's screen frame, or ends the live one.
    fn toggle_helper(&self) {
        let pid = HELPER.load(Ordering::SeqCst);
        if pid != 0 {
            unsafe { libc::kill(pid, libc::SIGTERM) };
            return;
        }
        let button = self.ivars().status.get().expect("status item exists after launch").button();
        let at = screen_frame(&button);
        let mut cmd = Command::new(std::env::current_exe().expect("own path"));
        cmd.args(["--dropdown", &at.origin.x.to_string(), &at.origin.y.to_string()]).env_remove("WISP_DRIVE");
        if let Some(d) = &self.ivars().drive {
            cmd.env("WISP_HOLD", d.hold.to_string());
        }
        if let Some(look) = current_look(self.mtm()) {
            cmd.env("WISP_FORCE_APPEARANCE", look);
        }
        let Ok(mut child) = cmd.spawn() else { return log("helper spawn failed") };
        HELPER.store(child.id() as i32, Ordering::SeqCst);
        // Reaps the helper the moment it exits; a blocking wait off the main thread costs nothing while it runs.
        std::thread::spawn(move || {
            let _ = child.wait();
            log(&format!("helper {} reaped", child.id()));
            HELPER.store(0, Ordering::SeqCst);
        });
    }

    /// One repeating timer for everything. The 10% tolerance lets macOS batch our wakeup with others;
    /// common modes keep it firing while a menu is open (menus track events in their own run loop mode).
    fn start_timer(&self) {
        let interval = self.ivars().interval.get() as f64;
        let timer = self.timer(interval, sel!(timerFired:), true);
        timer.setTolerance(interval * 0.1);
        *self.ivars().timer.borrow_mut() = Some(timer);
    }

    fn timer(&self, secs: f64, action: Sel, repeats: bool) -> Retained<NSTimer> {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(secs, self, action, None, repeats)
        };
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        timer
    }

    /// A one-shot driver step, in common modes so it also fires inside a menu's tracking loop.
    fn after(&self, secs: f64, action: Sel) {
        self.timer(secs, action, false);
    }

    /// Polls every 1 ms for the dropdown to appear or go; only while a driver step waits on it.
    fn start_poll(&self) {
        let t = self.timer(0.001, sel!(drivePoll:), true);
        if let Some(d) = &self.ivars().drive {
            *d.poll.borrow_mut() = Some(t);
        }
    }

    fn stop_poll(&self) {
        if let Some(t) = self.ivars().drive.as_ref().and_then(|d| d.poll.take()) {
            t.invalidate();
        }
    }
}

/// The dropdown's window once it is on screen: any of our windows but the status bar's that the window server
/// reports visible (occlusion state). Open latency is measured to this moment for every variant.
fn dropdown_window(mtm: MainThreadMarker) -> Option<Retained<NSWindow>> {
    NSApplication::sharedApplication(mtm).windows().iter().find(|w| {
        !w.class().name().to_string_lossy().contains("StatusBar")
            && w.isVisible()
            && w.occlusionState().contains(NSWindowOcclusionState::Visible)
    })
}

/// The button's frame in screen coordinates.
fn screen_frame(button: &NSView) -> NSRect {
    let local = button.convertRect_toView(button.bounds(), None);
    button.window().map_or(local, |w| w.convertRectToScreen(local))
}

/// Forces the app's appearance (`light`/`dark`) in-process, for screenshots; the system appearance is untouched.
fn force_look(mtm: MainThreadMarker, look: Option<&str>) {
    let name = match look {
        Some("dark") => unsafe { NSAppearanceNameDarkAqua },
        Some("light") => unsafe { NSAppearanceNameAqua },
        _ => return,
    };
    NSApplication::sharedApplication(mtm).setAppearance(NSAppearance::appearanceNamed(name).as_deref());
}

fn current_look(mtm: MainThreadMarker) -> Option<&'static str> {
    let a = NSApplication::sharedApplication(mtm).appearance()?;
    Some(if a.name().to_string().contains("Dark") { "dark" } else { "light" })
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Driver log line on stderr: epoch ms, pid, event. Helper and main share the file and the clock.
fn log(msg: &str) {
    eprintln!("{:.1} {} {msg}", now_ms(), std::process::id());
}

/// SMAppService.mainApp's status; 1 = enabled.
fn login_enabled() -> bool {
    // dlopen'd, not linked, so the default variant loads exactly main's images; no new crate for the throwaway.
    let path = c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement";
    unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
    let svc: Retained<AnyObject> = unsafe { msg_send![class!(SMAppService), mainAppService] };
    let status: isize = unsafe { msg_send![&*svc, status] };
    status == 1
}

fn main() {
    let mtm = MainThreadMarker::new().expect("Wisp must start on the main thread");
    let args: Vec<String> = std::env::args().collect();
    let helper_at = (args.get(1).map(String::as_str) == Some("--dropdown")).then(|| {
        let n = |i: usize| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        NSPoint::new(n(2), n(3))
    });
    let app = NSApplication::sharedApplication(mtm);
    // LSUIElement covers the bundled app; this also keeps `cargo run` out of the Dock.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm, helper_at);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
