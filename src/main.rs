//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer.
//! All UI setup happens in the delegate once AppKit has finished launching.

mod format;
mod sample;
mod ui;

use std::cell::{Cell, OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSControlStateValueOff,
    NSControlStateValueOn, NSMenu, NSMenuDelegate, NSMenuItem,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSTimer,
};

use sample::Sampler;
use ui::menu::{Dropdown, Kind, Variant};
use ui::status::StatusItem;

// Launch at Login (spec §3.3) uses SMAppService; linked directly so no new crate is needed for the throwaway.
#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// Seconds between samples. Spec §3.3 default; the menu feature makes it user-selectable.
const INTERVAL: f64 = 2.0;

/// State the delegate keeps alive for the app's lifetime.
struct Ivars {
    sampler: RefCell<Sampler>,
    status: OnceCell<StatusItem>,
    dropdown: OnceCell<Dropdown>,
    open: Cell<bool>,
    interval: Cell<isize>,
    timer: RefCell<Option<Retained<NSTimer>>>,
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
            let dropdown = Dropdown::new(mtm, Variant::from_env());
            if dropdown.variant.kind != Kind::Quit {
                dropdown.menu.setDelegate(Some(ProtocolObject::from_ref(self)));
                if !dropdown.variant.lazy {
                    dropdown.build(self, self.ivars().interval.get(), login_enabled());
                }
            }
            let _ = self.ivars().status.set(StatusItem::new(mtm, &dropdown.menu));
            let _ = self.ivars().dropdown.set(dropdown);
            self.tick();
            self.start_timer();
        }
    }

    impl AppDelegate {
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _timer: &NSTimer) {
            self.tick();
        }

        #[unsafe(method(setInterval:))]
        fn set_interval(&self, sender: &NSMenuItem) {
            self.ivars().interval.set(sender.tag());
            if let Some(t) = self.ivars().timer.take() {
                t.invalidate();
            }
            self.start_timer();
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
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = Ivars {
            sampler: RefCell::new(Sampler::new()),
            status: OnceCell::new(),
            dropdown: OnceCell::new(),
            open: Cell::new(false),
            interval: Cell::new(INTERVAL as isize),
            timer: RefCell::new(None),
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
        }
    }

    /// One repeating timer for everything. The 10% tolerance lets macOS batch our wakeup with others;
    /// common modes keep it firing while a menu is open (menus track events in their own run loop mode).
    fn start_timer(&self) {
        let interval = self.ivars().interval.get() as f64;
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                interval,
                self,
                sel!(timerFired:),
                None,
                true,
            )
        };
        timer.setTolerance(interval * 0.1);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        *self.ivars().timer.borrow_mut() = Some(timer);
    }
}

/// SMAppService.mainApp's status; 1 = enabled.
fn login_enabled() -> bool {
    let svc: Retained<AnyObject> = unsafe { msg_send![class!(SMAppService), mainAppService] };
    let status: isize = unsafe { msg_send![&*svc, status] };
    status == 1
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
