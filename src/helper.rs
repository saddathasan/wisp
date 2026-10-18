//! The dropdown helper process (ADR 0001): `wisp --dropdown <x> <y> [appearance]` pops the §3.2 menu up at a screen
//! point, keeps its rows live on its own timer and exits when the menu closes, taking the menu's AppKit memory with it.

use std::cell::{Cell, OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSMenuItem,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRunLoop, NSRunLoopCommonModes, NSString,
    NSTimer, NSUserDefaults, ns_string,
};

use crate::sample::Sampler;
use crate::ui::menu::{self, Dropdown};

/// The helper's exit status when the user chose Quit Wisp; the main process terminates on it.
pub const QUIT_STATUS: i32 = 3;

/// `SMAppService.Status.enabled`.
const LOGIN_ENABLED: isize = 1;

struct Ivars {
    /// Top-left of the menu, in screen coordinates.
    at: NSPoint,
    /// The menu bar's appearance name, which a status item's own menu would follow.
    appearance: Option<String>,
    sampler: RefCell<Sampler>,
    dropdown: OnceCell<Dropdown>,
    /// Exit status once the menu has closed.
    status: Cell<i32>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and Helper does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Helper;

    unsafe impl NSObjectProtocol for Helper {}

    unsafe impl NSApplicationDelegate for Helper {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let interval = stored_interval();
            let dropdown = Dropdown::new(self.mtm(), self, interval, login_enabled());
            let look = self.ivars().appearance.as_deref();
            let look = look.and_then(|n| NSAppearance::appearanceNamed(&NSString::from_str(n)));
            dropdown.menu.setAppearance(look.as_deref());
            // The baseline is milliseconds old, and deltas over it are noise (idle CPU ticks are published up to ~1 s
            // apart), so CPU and network wait for the first interval.
            let mut opening = self.ivars().sampler.borrow_mut().sample(true);
            (opening.cpu, opening.net) = (None, None);
            dropdown.update(&opening);
            let _ = self.ivars().dropdown.set(dropdown);
            // Common modes: the menu tracks events in its own run loop mode, where a default-mode timer never fires.
            self.timer(interval as f64, sel!(tick:), true);
            // popUp blocks until the menu closes, so it runs from a run loop callout rather than inside launching.
            self.timer(0.0, sel!(open:), false);
        }
    }

    impl Helper {
        #[unsafe(method(tick:))]
        fn tick_fired(&self, _timer: &NSTimer) {
            self.tick();
        }

        #[unsafe(method(open:))]
        fn open(&self, _timer: &NSTimer) {
            let Some(d) = self.ivars().dropdown.get() else { return };
            // Returns once the menu has closed. A chosen item's action has already run by then (checked on macOS 27
            // by selecting with Up + Return inside tracking), so its status is set before the exit.
            d.menu.popUpMenuPositioningItem_atLocation_inView(None, self.ivars().at, None);
            std::process::exit(self.ivars().status.get());
        }

        // ponytail: inert; Milestone 4 stores the choice in NSUserDefaults.
        #[unsafe(method(setInterval:))]
        fn set_interval(&self, _sender: &NSMenuItem) {}

        // ponytail: inert, so no run can register a login item; Milestone 4 calls SMAppService register/unregister.
        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, _sender: &NSMenuItem) {}

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: &NSMenuItem) {
            self.ivars().status.set(QUIT_STATUS);
        }
    }
);

impl Helper {
    fn new(mtm: MainThreadMarker, at: NSPoint, appearance: Option<String>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            at,
            appearance,
            sampler: RefCell::new(Sampler::new()),
            dropdown: OnceCell::new(),
            status: Cell::new(0),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// Samples everything the dropdown shows and updates its rows.
    fn tick(&self) {
        let snapshot = self.ivars().sampler.borrow_mut().sample(true);
        if let Some(d) = self.ivars().dropdown.get() {
            d.update(&snapshot);
        }
    }

    fn timer(&self, secs: f64, action: objc2::runtime::Sel, repeats: bool) {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                secs, self, action, None, repeats,
            )
        };
        // The 10% tolerance lets macOS batch the wakeup with others.
        timer.setTolerance(secs * 0.1);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    }
}

/// Runs the helper for `args` (what follows `--dropdown`) and never returns.
pub fn run(args: &[String]) -> ! {
    let n = |i: usize| args.get(i).and_then(|s| s.parse::<f64>().ok());
    let (Some(x), Some(y)) = (n(0), n(1)) else {
        eprintln!("usage: wisp --dropdown <x> <y> [appearance]");
        std::process::exit(2);
    };
    let mtm = MainThreadMarker::new().expect("the helper must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let helper = Helper::new(mtm, NSPoint::new(x, y), args.get(2).cloned());
    app.setDelegate(Some(ProtocolObject::from_ref(&*helper)));
    app.run();
    // `open:` exits; the run loop only ends if launching failed.
    std::process::exit(1);
}

/// The update interval in seconds stored as `interval` in NSUserDefaults; both processes read it through here.
pub(crate) fn stored_interval() -> isize {
    menu::interval(NSUserDefaults::standardUserDefaults().integerForKey(ns_string!("interval")))
}

/// Whether `SMAppService.mainApp` is enabled. ServiceManagement is loaded here, in the helper only, so the main
/// process's images stay as they are (spec §4.3).
fn login_enabled() -> bool {
    let path = c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement";
    unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
    let Some(cls) = AnyClass::get(c"SMAppService") else {
        return false;
    };
    let svc: Retained<AnyObject> = unsafe { msg_send![cls, mainAppService] };
    let status: isize = unsafe { msg_send![&*svc, status] };
    status == LOGIN_ENABLED
}
