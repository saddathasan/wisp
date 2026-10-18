//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer, and starts the
//! dropdown helper on a click; `--dropdown` runs that helper instead (ADR 0001).

mod format;
mod helper;
mod pause;
mod sample;
mod ui;

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSWorkspace,
    NSWorkspaceDidWakeNotification, NSWorkspaceScreensDidSleepNotification,
    NSWorkspaceScreensDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{
    NSArray, NSBundle, NSDistributedNotificationCenter, NSNotification, NSNotificationCenter,
    NSNotificationSuspensionBehavior, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes,
    NSString, NSTask, NSTaskDidTerminateNotification, NSTaskTerminationReason, NSTimer,
};

use pause::{Paused, Reason};
use sample::Sampler;
use ui::status::StatusItem;

/// State the delegate keeps alive for the app's lifetime.
struct Ivars {
    sampler: RefCell<Sampler>,
    status: OnceCell<StatusItem>,
    /// The running dropdown helper, if any.
    helper: RefCell<Option<Retained<NSTask>>>,
    /// None while paused, so nothing restarts it before the pause ends.
    timer: RefCell<Option<Retained<NSTimer>>>,
    paused: RefCell<Paused>,
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
            self.tick(true);
            self.start_timer();
            self.observe_sleep_and_lock();
        }
    }

    impl AppDelegate {
        #[unsafe(method(displaySlept:))]
        fn display_slept(&self, _notification: &NSNotification) {
            self.pause(Reason::DisplayAsleep);
        }

        #[unsafe(method(displayWoke:))]
        fn display_woke(&self, _notification: &NSNotification) {
            self.resume(Reason::DisplayAsleep);
        }

        #[unsafe(method(systemWillSleep:))]
        fn system_will_sleep(&self, _notification: &NSNotification) {
            self.pause(Reason::SystemAsleep);
        }

        #[unsafe(method(systemWoke:))]
        fn system_woke(&self, _notification: &NSNotification) {
            self.resume(Reason::SystemAsleep);
        }

        #[unsafe(method(screenLocked:))]
        fn screen_locked(&self, _notification: &NSNotification) {
            self.pause(Reason::Locked);
        }

        #[unsafe(method(screenUnlocked:))]
        fn screen_unlocked(&self, _notification: &NSNotification) {
            self.resume(Reason::Locked);
        }

        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _timer: &NSTimer) {
            self.tick(false);
        }

        #[unsafe(method(openDropdown:))]
        fn open_dropdown(&self, _sender: Option<&AnyObject>) {
            // At most one helper; a click while its menu is up is the click that closes it.
            if self.ivars().helper.borrow().is_some() {
                return;
            }
            if let Some(task) = self.spawn_helper() {
                *self.ivars().helper.borrow_mut() = Some(task);
                self.highlight(true);
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
            self.highlight(false);
            // A signal's number is reported as the status too, so only a normal exit counts.
            if task.terminationReason() == NSTaskTerminationReason::Exit
                && task.terminationStatus() == helper::QUIT_STATUS
            {
                NSApplication::sharedApplication(self.mtm()).terminate(None);
                return;
            }
            // The helper may have stored a new interval. No sync needed: a read sees another process's write as soon
            // as cfprefsd has it, and the helper waits for that before exiting. While paused there is no timer, and resuming
            // starts one at the stored interval anyway.
            let timer = self.ivars().timer.borrow().clone();
            if let Some(timer) = timer.filter(|t| t.timeInterval() != helper::stored_interval() as f64) {
                timer.invalidate();
                self.start_timer();
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
            timer: RefCell::new(None),
            paused: RefCell::new(Paused::default()),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Samples once and updates the menu bar text; `fresh` when the baseline was just taken (launch, resume).
    fn tick(&self, fresh: bool) {
        // Menu bar values only; the helper reads the dropdown's own (spec §3.2).
        let mut snapshot = self.ivars().sampler.borrow_mut().sample(false);
        // A milliseconds-old baseline holds zero or a few CPU ticks (idle ticks are published up to ~1 s apart), so
        // the delta is a dash or noise either way (41.7% seen at launch); CPU waits for the first interval, as in the
        // helper.
        if fresh {
            snapshot.cpu = None;
        }
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

    /// One repeating timer for everything, at the stored interval. The 10% tolerance lets macOS batch our wakeup with
    /// others; common modes keep it firing while the button tracks a press (event tracking has its own run loop mode).
    fn start_timer(&self) {
        let secs = helper::stored_interval() as f64;
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                secs,
                self,
                sel!(timerFired:),
                None,
                true,
            )
        };
        timer.setTolerance(secs * 0.1);
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        *self.ivars().timer.borrow_mut() = Some(timer);
    }

    /// Stops sampling for `reason`: no timer means no wakeups at all while paused (S3).
    fn pause(&self, reason: Reason) {
        self.ivars().paused.borrow_mut().pause(reason);
        if let Some(timer) = self.ivars().timer.take() {
            timer.invalidate();
        }
    }

    /// Clears `reason`; once none is left, samples against a fresh baseline so the gap isn't averaged in (spec §3.3).
    fn resume(&self, reason: Reason) {
        if self.ivars().paused.borrow_mut().resume(reason) {
            self.ivars().sampler.borrow_mut().reset();
            self.tick(true);
            self.start_timer();
        }
    }

    /// Workspace notifications cover display and system sleep; lock and unlock only come as distributed notifications.
    fn observe_sleep_and_lock(&self) {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        let local = [
            (sel!(displaySlept:), unsafe {
                NSWorkspaceScreensDidSleepNotification
            }),
            (sel!(displayWoke:), unsafe {
                NSWorkspaceScreensDidWakeNotification
            }),
            (sel!(systemWillSleep:), unsafe {
                NSWorkspaceWillSleepNotification
            }),
            (sel!(systemWoke:), unsafe { NSWorkspaceDidWakeNotification }),
        ];
        for (action, name) in local {
            unsafe { workspace.addObserver_selector_name_object(self, action, Some(name), None) };
        }
        let distributed = NSDistributedNotificationCenter::defaultCenter();
        let lock = [
            (sel!(screenLocked:), "com.apple.screenIsLocked"),
            (sel!(screenUnlocked:), "com.apple.screenIsUnlocked"),
        ];
        for (action, name) in lock {
            // An accessory app is never active, and the default behaviour holds distributed notifications until it is.
            unsafe {
                distributed.addObserver_selector_name_object_suspensionBehavior(
                    self,
                    action,
                    Some(&NSString::from_str(name)),
                    None,
                    NSNotificationSuspensionBehavior::DeliverImmediately,
                )
            };
        }
    }

    fn highlight(&self, on: bool) {
        if let Some(status) = self.ivars().status.get() {
            status.highlight(on);
        }
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
