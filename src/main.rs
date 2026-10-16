//! Wisp entry point: an accessory app (no Dock icon) that owns one menu bar item.
//! All UI setup happens in the delegate once AppKit has finished launching.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, ns_string};

mod sample;

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
    /// NSApplication delegate that builds the status item once launching finishes.
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let item =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
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
