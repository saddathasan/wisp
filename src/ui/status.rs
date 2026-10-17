//! The menu bar item: a small label over a bold value for CPU, GPU and MEM, drawn by six CATextLayers.
//! A redraw only swaps layer strings, so AppKit never re-renders the status bar button (~14 ms each time it does).

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSAppearance, NSAppearanceCustomization, NSAutoresizingMaskOptions, NSColor,
    NSColorSpace, NSFont, NSFontAttributeName, NSFontWeightMedium, NSFontWeightSemibold,
    NSForegroundColorAttributeName, NSMenu, NSStatusBar, NSStatusItem, NSStringDrawing,
    NSVariableStatusItemLength, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize, NSString,
    ns_string,
};

use crate::format::{self, Values};

const LABELS: [&str; 3] = ["CPU", "GPU", "MEM"];
const LABEL_PT: f64 = 7.5;
const VALUE_PT: f64 = 12.0;
/// Space between columns; Stats uses more than it needs, so this is deliberately tight.
const GAP: f64 = 6.0;
/// Inset at each end, so the item does not butt against its neighbours.
const PAD: f64 = 4.0;
/// Space between the value's cap height and the label's baseline.
const LEADING: f64 = 2.5;

type Attrs = Retained<NSDictionary<NSAttributedStringKey, AnyObject>>;

/// Owns the status item, its view and the values last drawn into it.
pub struct StatusItem {
    // Held only so the item stays in the menu bar; AppKit removes it once released.
    _item: Retained<NSStatusItem>,
    view: Retained<CompactView>,
    shown: Cell<Option<Values>>,
}

impl StatusItem {
    /// Adds the compact item to the menu bar with `menu` as its dropdown.
    pub fn new(mtm: MainThreadMarker, menu: &NSMenu) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setMenu(Some(menu));
        let button = item.button(mtm).expect("a status item has a button");
        // The layers are invisible to VoiceOver, so the button names the item.
        button.setAccessibilityLabel(Some(ns_string!("Wisp")));
        let view = CompactView::new(mtm, button.bounds().size.height);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        item.setLength(view.frame().size.width);
        button.addSubview(&view);
        Self {
            _item: item,
            view,
            shown: Cell::new(None),
        }
    }

    /// Draws `values` on the first call and afterwards only when they have moved from what is on screen.
    pub fn show(&self, values: Values) {
        // Compared with the drawn values, not the previous sample, so slow drift still shows once it adds up.
        if self
            .shown
            .get()
            .is_some_and(|shown| !format::moved(&shown, &values))
        {
            return;
        }
        self.view.set(values.map(format::percent));
        self.shown.set(Some(values));
    }
}

struct Ivars {
    /// CATextLayers: the three labels, then the three values.
    layers: Vec<Retained<AnyObject>>,
    values: RefCell<[String; 3]>,
    /// Value font and resolved colour, rebuilt only by `restyle`.
    value_attrs: RefCell<Option<Attrs>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and CompactView does not implement Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct CompactView;

    // Layer colours are resolved, not dynamic, so the view restyles when it lands in the status bar window
    // (the first frame already has the right colour and scale), when the bar's appearance flips and when the scale changes.
    impl CompactView {
        #[unsafe(method(viewDidMoveToWindow))]
        fn moved_to_window(&self) {
            self.restyle();
        }

        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            self.restyle();
        }

        #[unsafe(method(viewDidChangeBackingProperties))]
        fn backing_changed(&self) {
            self.restyle();
        }

        // Clicks fall through to the status item button, which opens the menu.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

impl CompactView {
    /// A view whose columns fit their widest content ("100%" or the label), so the item width never changes.
    fn new(mtm: MainThreadMarker, height: f64) -> Retained<Self> {
        let (label, value) = fonts();
        let width_of = |s: &str, f: &NSFont| {
            let attrs = attributes(f, &NSColor::labelColor());
            unsafe { NSString::from_str(s).sizeWithAttributes(Some(&attrs)) }.width
        };
        let col = LABELS
            .iter()
            .map(|l| width_of(l, &label))
            .fold(width_of("100%", &value), f64::max)
            .ceil();
        let root: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), layer] };
        let layers = (0..6)
            .map(|i| {
                let tl: Retained<AnyObject> = unsafe { msg_send![class!(CATextLayer), layer] };
                let font = if i < 3 { &label } else { &value };
                // Line box from descender to ascender; CATextLayer draws the first line's ascent at its top.
                let size = NSSize::new(col, (font.ascender() - font.descender()).ceil());
                let x = PAD + (i % 3) as f64 * (col + GAP);
                unsafe {
                    let _: () = msg_send![&*root, addSublayer: &*tl];
                    let _: () = msg_send![&*tl, setBounds: NSRect::new(NSPoint::ZERO, size)];
                    let _: () = msg_send![&*tl, setAnchorPoint: NSPoint::ZERO];
                    let _: () = msg_send![&*tl, setPosition: NSPoint::new(x, 0.0)];
                }
                tl
            })
            .collect();
        let this = Self::alloc(mtm).set_ivars(Ivars {
            layers,
            values: RefCell::default(),
            value_attrs: RefCell::default(),
        });
        let frame = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(2.0 * PAD + 3.0 * col + 2.0 * GAP, height),
        );
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // Layer-hosting (layer set before wantsLayer): we own the tree, so adding sublayers is supported.
        let _: () = unsafe { msg_send![&*this, setLayer: &*root] };
        this.setWantsLayer(true);
        this
    }

    /// Shows new value strings; only the three value layers change.
    fn set(&self, values: [String; 3]) {
        *self.ivars().values.borrow_mut() = values;
        // Not yet styled: restyle sets every string itself.
        let attrs = self.ivars().value_attrs.borrow().clone();
        let Some(attrs) = attrs else {
            return self.restyle();
        };
        let values = self.ivars().values.borrow();
        transaction(|| {
            for (tl, v) in self.ivars().layers[3..].iter().zip(values.iter()) {
                set_string(tl, v, &attrs);
            }
        });
    }

    /// Resolves colour, scale and vertical placement against the current appearance and window, then sets all strings.
    fn restyle(&self) {
        let (label_font, value_font) = fonts();
        let color = resolved(&self.effectiveAppearance());
        let label_attrs = attributes(&label_font, &color);
        let value_attrs = attributes(&value_font, &color);
        let scale = self.window().map_or(2.0, |w| w.backingScaleFactor());
        let snap = |y: f64| (y * scale).round() / scale;
        // Centre the block from the value's baseline to the label's cap height, which is what the eye reads as the text.
        let block = value_font.capHeight() + LEADING + label_font.capHeight();
        let value_base = snap((self.bounds().size.height - block) / 2.0);
        let label_base = snap(value_base + value_font.capHeight() + LEADING);
        let values = self.ivars().values.borrow();
        transaction(|| {
            for (i, tl) in self.ivars().layers.iter().enumerate() {
                let (text, attrs, font, base) = if i < 3 {
                    (LABELS[i], &label_attrs, &label_font, label_base)
                } else {
                    (
                        values[i - 3].as_str(),
                        &value_attrs,
                        &value_font,
                        value_base,
                    )
                };
                unsafe {
                    let pos: NSPoint = msg_send![&**tl, position];
                    let y = base + font.descender();
                    let _: () = msg_send![&**tl, setPosition: NSPoint::new(pos.x, y)];
                    let _: () = msg_send![&**tl, setContentsScale: scale];
                }
                set_string(tl, text, attrs);
            }
        });
        *self.ivars().value_attrs.borrow_mut() = Some(value_attrs);
    }
}

fn fonts() -> (Retained<NSFont>, Retained<NSFont>) {
    unsafe {
        (
            NSFont::systemFontOfSize_weight(LABEL_PT, NSFontWeightMedium),
            NSFont::monospacedDigitSystemFontOfSize_weight(VALUE_PT, NSFontWeightSemibold),
        )
    }
}

fn attributes(font: &NSFont, color: &NSColor) -> Attrs {
    let keys: [&NSAttributedStringKey; 2] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

fn set_string(layer: &AnyObject, text: &str, attrs: &Attrs) {
    // SAFETY: `attrs` maps the font and colour keys to an NSFont and an NSColor, the types AppKit expects.
    let s = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), attrs) };
    let _: () = unsafe { msg_send![layer, setString: &*s] };
}

/// Runs `f` with implicit layer animations off, so a new value snaps in rather than cross-fading.
fn transaction(f: impl FnOnce()) {
    let cat = class!(CATransaction);
    let _: () = unsafe { msg_send![cat, begin] };
    let _: () = unsafe { msg_send![cat, setDisableActions: true] };
    f();
    let _: () = unsafe { msg_send![cat, commit] };
}

/// The primary label colour as a fixed sRGB value for `appearance`; CATextLayer cannot follow dynamic colours.
// The non-deprecated `performAsCurrentDrawingAppearance:` takes a block, which needs another crate.
#[allow(deprecated)]
fn resolved(appearance: &NSAppearance) -> Retained<NSColor> {
    let prev = NSAppearance::currentAppearance();
    unsafe { NSAppearance::setCurrentAppearance(Some(appearance)) };
    let color = NSColor::labelColor();
    let fixed = color
        .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
        .unwrap_or(color);
    unsafe { NSAppearance::setCurrentAppearance(prev.as_deref()) };
    fixed
}
