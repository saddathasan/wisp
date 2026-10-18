//! Throwaway (exp/menu-cost, never merged): the §3.2 dropdown without NSMenu, as a borderless non-activating panel
//! (variant d) or an NSPopover (e). Rows are menu.rs's layer-drawn RowViews; everything exists only while open.

use std::cell::RefCell;
use std::ffi::c_void;

use objc2::encode::{Encode, Encoding, RefEncode};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSColorSpace, NSEvent,
    NSPanel, NSPopover, NSPopoverBehavior, NSScreen, NSStatusBarButton, NSView,
    NSViewController, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSRectEdge, NSSize};

use super::menu::{ROW_H, ROW_W, Row, RowView, SECTIONS, Space, Variant, rows};
use super::status::resolve;
use crate::sample::Snapshot;

const PAD: f64 = 5.0;
const RADIUS: f64 = 10.0;
/// NSPopUpMenuWindowLevel: the panel sits where a menu would.
const MENU_LEVEL: isize = 101;
/// Action tags a click or key sends to the app delegate's `panelAction:`.
pub const CLOSE: isize = -1;
pub const INTERVAL: isize = 0;
pub const LOGIN: isize = 1;
pub const QUIT: isize = 2;
const NONE: isize = -2;

/// What hosts the content: our own panel or AppKit's popover.
pub enum Host {
    Panel(Retained<DropPanel>),
    Popover(Retained<NSPopover>),
}

/// An open dropdown: its host, the eight reading rows and the two settings rows.
pub struct Panel {
    pub host: Host,
    space: Space,
    readings: Vec<Retained<RowView>>,
    interval: Retained<RowView>,
    login: Retained<RowView>,
    last: RefCell<Vec<Row>>,
}

impl Panel {
    /// Builds the content and its host; `show` puts it on screen.
    pub fn new(mtm: MainThreadMarker, variant: Variant, popover: bool, interval: isize, login: bool) -> Self {
        let content = Content::new(mtm);
        let mut y = PAD;
        let add = |y: &mut f64, row: Row, tag: isize| {
            let v = RowView::new(mtm, row);
            v.setFrameOrigin(NSPoint::new(0.0, *y));
            content.addSubview(&v);
            content.ivars().hits.borrow_mut().push((*y, *y + ROW_H, tag));
            *y += ROW_H;
            v
        };
        let sep = |y: &mut f64| {
            content.addSubview(&separator(mtm, *y + 4.0));
            *y += 9.0;
        };
        let mut readings = Vec::new();
        let mut it = rows(&Snapshot::default()).into_iter();
        for (title, n) in SECTIONS {
            add(&mut y, Row { label: title.to_owned(), secondary: true, ..Row::default() }, NONE);
            for row in it.by_ref().take(n) {
                readings.push(add(&mut y, row, NONE));
            }
        }
        sep(&mut y);
        let interval = add(&mut y, interval_row(interval), INTERVAL);
        let login = add(&mut y, login_row(login), LOGIN);
        sep(&mut y);
        add(&mut y, action_row("Quit Wisp", "⌘Q"), QUIT);
        y += PAD;
        let size = NSSize::new(ROW_W, y);
        content.setFrameSize(size);

        let host = if popover {
            let vc = NSViewController::new(mtm);
            vc.setView(&content);
            let p = NSPopover::new(mtm);
            p.setBehavior(NSPopoverBehavior::Transient);
            p.setContentSize(size);
            p.setContentViewController(Some(&vc));
            Host::Popover(p)
        } else {
            let frame = NSRect::new(NSPoint::ZERO, size);
            let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
            let w: Retained<DropPanel> = unsafe {
                msg_send![DropPanel::alloc(mtm), initWithContentRect: frame, styleMask: style,
                    backing: NSBackingStoreType::Buffered, defer: false]
            };
            unsafe { w.setReleasedWhenClosed(false) };
            w.setLevel(MENU_LEVEL);
            w.setOpaque(false);
            w.setBackgroundColor(Some(&NSColor::clearColor()));
            w.setHasShadow(true);
            let appearance = NSApplication::sharedApplication(mtm).effectiveAppearance();
            if variant.vibrant {
                let fx = NSVisualEffectView::new(mtm);
                fx.setMaterial(NSVisualEffectMaterial::Menu);
                fx.setState(NSVisualEffectState::Active);
                fx.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                fx.setFrame(frame);
                fx.setWantsLayer(true);
                round(&fx, None);
                fx.addSubview(&content);
                w.setContentView(Some(&fx));
            } else {
                content.setWantsLayer(true);
                round(&content, Some(&resolve(&appearance, &NSColor::windowBackgroundColor())));
                w.setContentView(Some(&content));
            }
            w.makeFirstResponder(Some(&content));
            Host::Panel(w)
        };
        Self { host, space: variant.space, readings, interval, login, last: RefCell::new(Vec::new()) }
    }

    /// Shows the dropdown under `button`, whose on-screen frame is `at`.
    pub fn show(&self, button: &NSStatusBarButton, at: NSRect) {
        match &self.host {
            Host::Panel(w) => {
                let size = w.frame().size;
                // The screen holding the button, not the panel's own (it has none until shown).
                let mid = NSPoint::new(at.origin.x + at.size.width / 2.0, at.origin.y + at.size.height / 2.0);
                let mtm = w.mtm();
                let screen = NSScreen::screens(mtm)
                    .iter()
                    .find(|s| {
                        let f = s.frame();
                        mid.x >= f.origin.x && mid.x < f.origin.x + f.size.width && mid.y >= f.origin.y && mid.y <= f.origin.y + f.size.height
                    })
                    .or_else(|| NSScreen::mainScreen(mtm));
                match self.space {
                    Space::Default => {}
                    Space::Screen => w.setColorSpace(screen.as_ref().and_then(|s| s.colorSpace()).as_deref()),
                    Space::Srgb => w.setColorSpace(Some(&NSColorSpace::sRGBColorSpace())),
                }
                let mut x = at.origin.x;
                let mut top = at.origin.y;
                // Keep the panel on that screen, also when the item sits in the notch overflow.
                if let Some(s) = screen.map(|s| s.visibleFrame()) {
                    x = x.clamp(s.origin.x, s.origin.x + s.size.width - size.width);
                    top = top.min(s.origin.y + s.size.height);
                }
                w.setFrameTopLeftPoint(NSPoint::new(x, top - 1.0));
                w.makeKeyAndOrderFront(None);
            }
            Host::Popover(p) => p.showRelativeToRect_ofView_preferredEdge(button.bounds(), button, NSRectEdge::MinY),
        }
    }

    /// Takes the dropdown off screen; dropping `self` afterwards releases everything.
    pub fn close(&self) {
        match &self.host {
            Host::Panel(w) => w.close(),
            Host::Popover(p) => p.close(),
        }
    }

    /// Updates reading rows whose text changed since the last update.
    pub fn update(&self, s: &Snapshot) {
        let new = rows(s);
        let mut last = self.last.borrow_mut();
        for (i, row) in new.iter().enumerate() {
            if last.get(i) != Some(row) {
                self.readings[i].set(row.clone());
            }
        }
        *last = new;
    }

    pub fn set_interval(&self, secs: isize) {
        self.interval.set(interval_row(secs));
    }

    pub fn set_login(&self, on: bool) {
        self.login.set(login_row(on));
    }
}

// ponytail: the interval "submenu" is a row that cycles 1→2→3→5→10 s per click; a real one would expand.
fn interval_row(secs: isize) -> Row {
    action_row("Update Interval", &format!("{secs} s ▸"))
}

fn login_row(on: bool) -> Row {
    action_row("Launch at Login", if on { "✓" } else { "" })
}

fn action_row(label: &str, value: &str) -> Row {
    Row { label: label.to_owned(), value: value.to_owned(), ..Row::default() }
}

/// CGColorRef for `msg_send!`; no objc2-core-graphics dependency for one property.
#[repr(transparent)]
struct CgColor(*const c_void);
unsafe impl Encode for CgColor {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}
unsafe impl RefEncode for CgColor {
    const ENCODING_REF: Encoding = Encoding::Pointer(&<Self as Encode>::ENCODING);
}

fn set_background(view: &NSView, color: &NSColor) {
    let cg: CgColor = unsafe { msg_send![color, CGColor] };
    if let Some(layer) = view.layer() {
        let _: () = unsafe { msg_send![&*layer, setBackgroundColor: cg] };
    }
}

/// Rounds a layer-backed view's corners and, if given, fills it.
fn round(view: &NSView, fill: Option<&NSColor>) {
    if let Some(layer) = view.layer() {
        unsafe {
            let _: () = msg_send![&*layer, setCornerRadius: RADIUS];
            let _: () = msg_send![&*layer, setMasksToBounds: true];
        }
    }
    if let Some(c) = fill {
        set_background(view, c);
    }
}

fn separator(mtm: MainThreadMarker, y: f64) -> Retained<NSView> {
    let v = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(PAD + 5.0, y), NSSize::new(ROW_W - 2.0 * (PAD + 5.0), 1.0)),
    );
    v.setWantsLayer(true);
    let appearance = NSApplication::sharedApplication(mtm).effectiveAppearance();
    set_background(&v, &resolve(&appearance, &NSColor::separatorColor()));
    v
}

define_class!(
    // SAFETY: NSPanel has no subclassing requirements and DropPanel does not implement Drop.
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    pub struct DropPanel;

    // Borderless windows refuse key by default; key is what makes Escape and resign-on-outside-click work.
    impl DropPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }
    }
);

struct ContentIvars {
    /// (top, bottom, tag) of each row, in flipped coordinates.
    hits: RefCell<Vec<(f64, f64, isize)>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and Content does not implement Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ContentIvars]
    struct Content;

    impl Content {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        // The panel never activates Wisp, so the first click must act rather than only focus.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let p = self.convertPoint_fromView(event.locationInWindow(), None);
            let tag = self.ivars().hits.borrow().iter().find(|h| p.y >= h.0 && p.y < h.1).map(|h| h.2);
            if let Some(tag) = tag.filter(|t| *t != NONE) {
                send(self.mtm(), tag);
            }
        }

        // ponytail: Escape only; arrow-key highlight and Return are what a real build would add.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == 53 {
                send(self.mtm(), CLOSE);
            }
        }
    }
);

impl Content {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ContentIvars { hits: RefCell::new(Vec::new()) });
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }
}

/// Forwards a row action to the app delegate's `panelAction:`.
fn send(mtm: MainThreadMarker, tag: isize) {
    if let Some(d) = NSApplication::sharedApplication(mtm).delegate() {
        let d: &AnyObject = d.as_ref();
        let _: () = unsafe { msg_send![d, panelAction: tag] };
    }
}
