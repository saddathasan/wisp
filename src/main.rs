//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item and one timer.
//! All UI setup happens in the delegate once AppKit has finished launching.

mod format;
mod sample;
mod ui;

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSTimer, ns_string,
};

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
        let ivars = Ivars {
            sampler: RefCell::new(Sampler::new()),
            status: OnceCell::new(),
        };
        let this = Self::alloc(mtm).set_ivars(ivars);
        unsafe { msg_send![super(this), init] }
    }

    /// Samples once and updates the menu bar text.
    fn tick(&self) {
        // Menu bar values only; the menu feature passes `true` while the dropdown is open.
        let snapshot = self.ivars().sampler.borrow_mut().sample(false);
        if let Some(status) = self.ivars().status.get() {
            status.show(format::values(&snapshot));
        }
    }

    /// One repeating timer for everything. The 10% tolerance lets macOS batch our wakeup with others;
    /// common modes keep it firing while a menu is open (menus track events in their own run loop mode).
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

/// Spike only (exp/login-spike): `--login-spike <status|register|unregister|child <cmd>>` calls
/// `SMAppService.mainApp` through a dlopened ServiceManagement and prints the result to stderr.
fn login_spike(args: &[String]) {
    use objc2::{class, runtime::AnyObject};
    use objc2_foundation::{NSBundle, NSError};
    let path = c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement";
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
    let tag = format!("[pid {} ppid {}]", std::process::id(), unsafe { libc::getppid() });
    let bundle_id = NSBundle::mainBundle().bundleIdentifier();
    eprintln!("{tag} dlopen={:?} bundleIdentifier={bundle_id:?}", handle);
    let svc: Retained<AnyObject> = unsafe { msg_send![class!(SMAppService), mainAppService] };
    let status = |when: &str| {
        let s: isize = unsafe { msg_send![&svc, status] };
        eprintln!("{tag} status {when} = {s}");
    };
    let report = |name: &str, r: Result<(), Retained<NSError>>| match r {
        Ok(()) => eprintln!("{tag} {name} = YES"),
        Err(e) => eprintln!(
            "{tag} {name} = NO domain={} code={} desc={}",
            e.domain(),
            e.code(),
            e.localizedDescription()
        ),
    };
    status("before");
    match args.first().map(String::as_str) {
        Some("register") => report("register", unsafe { msg_send![&svc, registerAndReturnError: _] }),
        Some("unregister") => {
            report("unregister", unsafe { msg_send![&svc, unregisterAndReturnError: _] })
        }
        Some("child") => {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--login-spike")
                .args(&args[1..])
                .status();
            eprintln!("{tag} child exited {out:?}");
        }
        _ => {}
    }
    status("after");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--login-spike") {
        login_spike(&args[i + 1..]);
        return;
    }
    let mtm = MainThreadMarker::new().expect("Wisp must start on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // LSUIElement covers the bundled app; this also keeps `cargo run` out of the Dock.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
