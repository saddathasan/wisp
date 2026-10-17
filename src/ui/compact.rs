//! EXPERIMENT (exp/render-cost, WISP_COMPACT): the two-line menu bar item, a small label over a bold value per metric.
//! A layer-hosting view with six CATextLayers; a redraw only swaps the three value strings.

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAttributedStringNSStringDrawing, NSColor,
    NSColorSpace, NSFont, NSFontWeightMedium, NSFontWeightSemibold, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect, NSSize, NSString,
};

use super::status::attributes;

const LABELS: [&str; 3] = ["CPU", "GPU", "MEM"];
const LABEL_PT: f64 = 7.5;
const VALUE_PT: f64 = 12.0;
/// Space between columns; Stats uses more than it needs, so this is deliberately tight.
const GAP: f64 = 6.0;
/// Inset at each end, so the item does not butt against its neighbours.
const PAD: f64 = 4.0;
/// Space between the value's cap height and the label's baseline.
const LEADING: f64 = 2.5;

pub struct Ivars {
    layers: OnceCell<Vec<Retained<AnyObject>>>,
    values: RefCell<[String; 3]>,
    /// Value font and resolved primary colour, rebuilt only by `restyle`.
    value_attrs: RefCell<Option<Retained<NSDictionary<NSAttributedStringKey, AnyObject>>>>,
}

define_class!(
    // SAFETY: plain NSView subclass; no Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub struct CompactView;

    impl CompactView {
        // Layer colours are resolved, not dynamic, so they are re-resolved whenever the menu bar's appearance flips.
        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn appearance_changed(&self) {
            self.restyle();
        }

        // Fires when the view lands in the status bar window, and on a scale change, so contentsScale stays crisp.
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

fn fonts() -> (Retained<NSFont>, Retained<NSFont>) {
    unsafe {
        (
            NSFont::systemFontOfSize_weight(LABEL_PT, NSFontWeightMedium),
            NSFont::monospacedDigitSystemFontOfSize_weight(VALUE_PT, NSFontWeightSemibold),
        )
    }
}

impl CompactView {
    /// A view sized to its widest content ("100%" in every column), so the item width never changes.
    pub fn new(mtm: MainThreadMarker, height: f64) -> Retained<Self> {
        let (label, value) = fonts();
        let width_of = |s: &str, f: &NSFont| {
            let a = attributes(f, &NSColor::labelColor());
            unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(s), &a) }.size().width
        };
        let col = LABELS
            .iter()
            .map(|l| width_of(l, &label))
            .fold(width_of("100%", &value), f64::max)
            .ceil();
        let width = 2.0 * PAD + 3.0 * col + 2.0 * GAP;
        let ivars = Ivars { layers: OnceCell::new(), values: RefCell::new(Default::default()), value_attrs: RefCell::new(None) };
        let this = Self::alloc(mtm).set_ivars(ivars);
        let this: Retained<Self> = unsafe {
            msg_send![super(this), initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height))]
        };
        // Layer-hosting (layer set before wantsLayer): we own the tree, so adding sublayers is supported.
        let root: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), layer] };
        let _: () = unsafe { msg_send![&*this, setLayer: &*root] };
        this.setWantsLayer(true);
        let layers = (0..6)
            .map(|i| {
                let tl: Retained<AnyObject> = unsafe { msg_send![class!(CATextLayer), layer] };
                let _: () = unsafe { msg_send![&*root, addSublayer: &*tl] };
                let x = PAD + (i % 3) as f64 * (col + GAP);
                let font = if i < 3 { &label } else { &value };
                // Line box from descender to ascender; CATextLayer draws the first line's ascent at its top.
                let h = (font.ascender() - font.descender()).ceil();
                let _: () = unsafe { msg_send![&*tl, setBounds: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(col, h))] };
                let _: () = unsafe { msg_send![&*tl, setAnchorPoint: NSPoint::new(0.0, 0.0)] };
                let _: () = unsafe { msg_send![&*tl, setPosition: NSPoint::new(x, 0.0)] };
                tl
            })
            .collect();
        let _ = this.ivars().layers.set(layers);
        this
    }

    /// Shows new value strings; only the three value layers change.
    pub fn set(&self, values: [String; 3]) {
        *self.ivars().values.borrow_mut() = values;
        let attrs = self.ivars().value_attrs.borrow().clone();
        // First draw, before any appearance or backing callback has styled the layers.
        let (Some(layers), Some(attrs)) = (self.ivars().layers.get(), attrs) else { return self.restyle() };
        transaction(|| {
            for (tl, v) in layers[3..].iter().zip(self.ivars().values.borrow().iter()) {
                let s = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(v), &attrs) };
                let _: () = unsafe { msg_send![&**tl, setString: &*s] };
            }
        });
    }

    /// Resolves colours, scale and vertical placement against the current appearance and window, then sets all strings.
    fn restyle(&self) {
        let Some(layers) = self.ivars().layers.get() else { return };
        let (label_font, value_font) = fonts();
        let (secondary, primary) = resolved(&self.effectiveAppearance());
        let label_attrs = attributes(&label_font, &secondary);
        let value_attrs = attributes(&value_font, &primary);
        let scale = self.window().map_or(2.0, |w| w.backingScaleFactor());
        // Centre the block from the value's baseline to the label's cap height, which is what the eye reads as the text.
        let block = value_font.capHeight() + LEADING + label_font.capHeight();
        let value_base = ((self.bounds().size.height - block) / 2.0 * scale).round() / scale;
        let label_base = ((value_base + value_font.capHeight() + LEADING) * scale).round() / scale;
        let values = self.ivars().values.borrow();
        transaction(|| for (i, tl) in layers.iter().enumerate() {
            let (text, attrs, font, base) = if i < 3 {
                (LABELS[i], &label_attrs, &label_font, label_base)
            } else {
                (values[i - 3].as_str(), &value_attrs, &value_font, value_base)
            };
            let s = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), attrs) };
            let pos: NSPoint = unsafe { msg_send![&**tl, position] };
            let _: () = unsafe { msg_send![&**tl, setPosition: NSPoint::new(pos.x, base + font.descender())] };
            let _: () = unsafe { msg_send![&**tl, setContentsScale: scale] };
            let _: () = unsafe { msg_send![&**tl, setString: &*s] };
        });
        drop(values);
        *self.ivars().value_attrs.borrow_mut() = Some(value_attrs);
    }
}

/// Runs `f` with implicit layer animations off, so a new value snaps in rather than cross-fading.
fn transaction(f: impl FnOnce()) {
    let cat = class!(CATransaction);
    let _: () = unsafe { msg_send![cat, begin] };
    let _: () = unsafe { msg_send![cat, setDisableActions: true] };
    f();
    let _: () = unsafe { msg_send![cat, commit] };
}

/// Secondary and primary label colours as fixed sRGB values for `appearance`; CATextLayer cannot follow dynamic colours.
fn resolved(appearance: &NSAppearance) -> (Retained<NSColor>, Retained<NSColor>) {
    let space = NSColorSpace::sRGBColorSpace();
    #[allow(deprecated)]
    let prev = NSAppearance::currentAppearance();
    #[allow(deprecated)]
    unsafe { NSAppearance::setCurrentAppearance(Some(appearance)) };
    let fix = |c: Retained<NSColor>| c.colorUsingColorSpace(&space).unwrap_or(c);
    let out = (fix(NSColor::secondaryLabelColor()), fix(NSColor::labelColor()));
    #[allow(deprecated)]
    unsafe { NSAppearance::setCurrentAppearance(prev.as_deref()) };
    out
}
