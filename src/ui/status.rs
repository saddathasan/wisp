//! The menu bar item: draws the CPU / GPU / MEM line, redrawing only when a value has moved enough to matter.
//! Every title change costs ~14 ms of AppKit rendering, so ticks that only jitter must cost nothing.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAttributedStringNSStringDrawing, NSAutoresizingMaskOptions, NSBitmapImageRep, NSColor, NSDeviceRGBColorSpace,
    NSFont, NSFontAttributeName, NSFontWeightRegular, NSForegroundColorAttributeName,
    NSGraphicsContext, NSImage, NSMenu, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSPoint,
    NSRect, NSSize, NSString,
};

// EXPERIMENT (exp/render-cost): env-var switches, read once at startup.
fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

define_class!(
    // SAFETY: plain NSView subclass; no Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<Retained<NSAttributedString>>>]
    struct TextView;

    impl TextView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if let Some(s) = self.ivars().borrow().as_ref() {
                let b = self.bounds();
                let sz = s.size();
                s.drawAtPoint(NSPoint::new(((b.size.width - sz.width) / 2.0).round(), ((b.size.height - sz.height) / 2.0).round()));
            }
        }
    }
);

use crate::format::{self, StatusLine, Values};

/// Owns the status item and the values last drawn into it.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    shown: Cell<Option<Values>>,
    tick: Cell<u32>,
    force: bool,
    catx: bool,
    image: bool,
    layer: Option<Retained<AnyObject>>,
    view: Option<Retained<TextView>>,
}

impl StatusItem {
    /// Adds a variable-width item to the menu bar with `menu` as its dropdown.
    pub fn new(_mtm: MainThreadMarker, menu: &NSMenu) -> Self {
        let (fixed, layer, view) = (flag("WISP_FIXED"), flag("WISP_LAYER"), flag("WISP_VIEW"));
        // Worst-case width plus the inset AppKit adds around a variable-length title (WISP_PAD, measured).
        let pad: f64 = std::env::var("WISP_PAD").ok().and_then(|v| v.parse().ok()).unwrap_or(20.0);
        let worst = attributed(&format::status(&[Some(100.0); 3])).size().width.ceil() + pad;
        let length = if fixed || layer || view { worst } else { NSVariableStatusItemLength };
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(length);
        item.setMenu(Some(menu));
        let button = item.button(_mtm).unwrap();
        let layer = layer.then(|| {
            button.setWantsLayer(true);
            let tl: Retained<AnyObject> = unsafe { msg_send![class!(CATextLayer), layer] };
            let center = NSString::from_str("center");
            let _: () = unsafe { msg_send![&*tl, setAlignmentMode: &*center] };
            let host = button.layer().unwrap();
            let _: () = unsafe { msg_send![&*host, addSublayer: &*tl] };
            tl
        });
        let view = view.then(|| {
            let v: Retained<TextView> = unsafe {
                msg_send![super(TextView::alloc(_mtm).set_ivars(RefCell::new(None))), initWithFrame: button.bounds()]
            };
            v.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
            button.addSubview(&v);
            v
        });
        Self {
            item,
            shown: Cell::new(None),
            tick: Cell::new(0),
            force: flag("WISP_FORCE"),
            catx: flag("WISP_CATX"),
            image: flag("WISP_IMAGE"),
            layer,
            view,
        }
    }

    /// Draws `values` on the first call and afterwards only when they have moved from what is on screen.
    pub fn show(&self, mtm: MainThreadMarker, mut values: Values) {
        if self.force {
            // Alternate CPU far apart so every tick is a visible change.
            let t = self.tick.get();
            values[0] = Some(if t % 2 == 0 { 11.0 } else { 88.0 });
            self.tick.set(t + 1);
        }
        // Compared with the drawn values, not the previous sample, so slow drift still shows once it adds up.
        if self
            .shown
            .get()
            .is_some_and(|shown| !format::moved(&shown, &values))
        {
            return;
        }
        if let Some(button) = self.item.button(mtm) {
            let text = attributed(&format::status(&values));
            let cat = class!(CATransaction);
            if self.catx {
                let _: () = unsafe { msg_send![cat, begin] };
                let _: () = unsafe { msg_send![cat, setDisableActions: true] };
            }
            if let Some(tl) = &self.layer {
                let b = button.bounds();
                let h = text.size().height.ceil();
                let want = NSRect::new(NSPoint::new(0.0, ((b.size.height - h) / 2.0).round()), NSSize::new(b.size.width, h));
                let cur: NSRect = unsafe { msg_send![&**tl, frame] };
                if cur != want {
                    let scale = button.window().map_or(2.0, |w| w.backingScaleFactor());
                    let _: () = unsafe { msg_send![&**tl, setContentsScale: scale] };
                    let _: () = unsafe { msg_send![&**tl, setFrame: want] };
                }
                let _: () = unsafe { msg_send![&**tl, setString: &*text] };
                if flag("WISP_REPL") {
                    // Dirty the button so AppKit refreshes the replica snapshot on other displays.
                    NSView::setNeedsDisplay(&button, true);
                }
            } else if let Some(v) = &self.view {
                *v.ivars().borrow_mut() = Some(text.clone());
                v.setNeedsDisplay(true);
            } else if self.image {
                button.setImage(Some(&image(&text, &button.effectiveAppearance())));
            } else {
                button.setAttributedTitle(&text);
            }
            if self.catx {
                let _: () = unsafe { msg_send![cat, commit] };
            }
            if flag("WISP_DEBUG") {
                eprintln!("button {:?} text {:?}", button.frame(), text.size());
            }
        }
        self.shown.set(Some(values));
    }
}

/// Labels in the secondary label colour, values in the primary one, all with monospaced digits.
fn attributed(line: &StatusLine) -> Retained<NSAttributedString> {
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(NSFont::systemFontSize(), unsafe {
        NSFontWeightRegular
    });
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

/// EXPERIMENT: the line drawn into a 2x bitmap, resolved against the button's appearance.
fn image(text: &NSAttributedString, appearance: &NSAppearance) -> Retained<NSImage> {
    let sz = text.size();
    let size = NSSize::new(sz.width.ceil(), sz.height.ceil());
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(), std::ptr::null_mut(), (size.width * 2.0) as isize, (size.height * 2.0) as isize, 8, 4, true, false, NSDeviceRGBColorSpace, 0, 0,
        )
    }
    .unwrap();
    rep.setSize(size);
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep).as_deref());
    #[allow(deprecated)]
    unsafe { NSAppearance::setCurrentAppearance(Some(appearance)) };
    text.drawAtPoint(NSPoint::new(0.0, 0.0));
    NSGraphicsContext::restoreGraphicsState_class();
    let img = NSImage::initWithSize(NSImage::alloc(), size);
    img.addRepresentation(&rep);
    img
}

fn attributes(
    font: &NSFont,
    color: &NSColor,
) -> Retained<NSDictionary<NSAttributedStringKey, AnyObject>> {
    let keys: [&NSAttributedStringKey; 2] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

fn append(
    out: &NSMutableAttributedString,
    text: &str,
    attrs: &NSDictionary<NSAttributedStringKey, AnyObject>,
) {
    // SAFETY: `attrs` maps the font and colour keys to an NSFont and an NSColor, the types AppKit expects.
    let piece =
        unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), attrs) };
    out.appendAttributedString(&piece);
}
