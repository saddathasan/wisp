//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer, and starts the
//! dropdown helper on a click; `--dropdown` runs that helper instead (ADR 0001).

mod format;
mod helper;
mod sample;
mod ui;

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate};
use objc2_foundation::{
    NSArray, NSBundle, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol, NSRunLoop,
    NSRunLoopCommonModes, NSString, NSTask, NSTaskDidTerminateNotification,
    NSTaskTerminationReason, NSTimer,
};

use sample::Sampler;
use ui::status::StatusItem;

/// Seconds between samples. Spec §3.3 default; the menu feature makes it user-selectable.
const INTERVAL: f64 = 2.0;

/// State the delegate keeps alive for the app's lifetime.
struct Ivars {
    sampler: RefCell<Sampler>,
    status: OnceCell<StatusItem>,
    /// The running dropdown helper, if any.
    helper: RefCell<Option<Retained<NSTask>>>,
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
            let _ = self
                .ivars()
                .status
                .set(StatusItem::new(mtm, self, sel!(openDropdown:)));
            self.tick();
            self.start_timer();
        }
    }

    impl AppDelegate {
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _timer: &NSTimer) {
            self.tick();
        }

        #[unsafe(method(openDropdown:))]
        fn open_dropdown(&self, _sender: Option<&AnyObject>) {
            // At most one helper; a click while its menu is up is the click that closes it.
            if self.ivars().helper.borrow().is_some() {
                return;
            }
            if let Some(task) = self.spawn_helper() {
                *self.ivars().helper.borrow_mut() = Some(task);
            }
        }

        /// Delivered on the main thread by the run loop, so the exit is seen without polling.
        #[unsafe(method(helperExited:))]
        fn helper_exited(&self, _notification: &NSNotification) {
            let Some(task) = self.ivars().helper.take() else {
                return;
            };
            unsafe {
                NSNotificationCenter::defaultCenter().removeObserver_name_object(
                    self,
                    Some(NSTaskDidTerminateNotification),
                    Some(&task),
                )
            };
            // A signal's number is reported as the status too, so only a normal exit counts.
            if task.terminationReason() == NSTaskTerminationReason::Exit
                && task.terminationStatus() == helper::QUIT_STATUS
            {
                NSApplication::sharedApplication(self.mtm()).terminate(None);
            }
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let ivars = Ivars {
            sampler: RefCell::new(Sampler::new()),
            status: OnceCell::new(),
            helper: RefCell::new(None),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Samples once and updates the menu bar text.
    fn tick(&self) {
        // Menu bar values only; the helper reads the dropdown's own (spec §3.2).
        let snapshot = self.ivars().sampler.borrow_mut().sample(false);
        if let Some(status) = self.ivars().status.get() {
            status.show(format::values(&snapshot));
        }
    }

    /// Starts this binary as `--dropdown <x> <y> <appearance>` at the status item, watching for its exit.
    fn spawn_helper(&self) -> Option<Retained<NSTask>> {
        let (at, look) = self.ivars().status.get()?.anchor();
        let args = [
            NSString::from_str("--dropdown"),
            NSString::from_str(&at.x.to_string()),
            NSString::from_str(&at.y.to_string()),
            look,
        ];
        let task = NSTask::new();
        task.setExecutableURL(NSBundle::mainBundle().executableURL().as_deref());
        task.setArguments(Some(&NSArray::from_retained_slice(&args)));
        let center = NSNotificationCenter::defaultCenter();
        unsafe {
            center.addObserver_selector_name_object(
                self,
                sel!(helperExited:),
                Some(NSTaskDidTerminateNotification),
                Some(&task),
            )
        };
        if task.launchAndReturnError().is_err() {
            unsafe { center.removeObserver_name_object(self, None, Some(&task)) };
            return None;
        }
        Some(task)
    }

    /// One repeating timer for everything. The 10% tolerance lets macOS batch our wakeup with others;
    /// common modes keep it firing while the button tracks a press (event tracking has its own run loop mode).
    fn start_timer(&self) {
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                INTERVAL,
                self,
                sel!(timerFired:),
                None,
                true,
            )
        };
        timer.setTolerance(INTERVAL * 0.1);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--dropdown") {
        helper::run(&args[1..]);
    }
    let mtm = MainThreadMarker::new().expect("Wisp must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // LSUIElement covers the bundled app; this also keeps `cargo run` out of the Dock.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
