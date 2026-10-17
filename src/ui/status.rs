//! The menu bar item: draws the CPU / GPU / MEM line, redrawing only when a value has moved enough to matter.
//! Every title change costs ~14 ms of AppKit rendering, so ticks that only jitter must cost nothing.

use std::cell::RefCell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSFontWeightRegular, NSForegroundColorAttributeName,
    NSMenu, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSString,
};

use crate::format::{self, StatusLine, Values};

/// Owns the status item and the values last drawn into it.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    shown: RefCell<Option<Values>>,
}

impl StatusItem {
    /// Adds a variable-width item to the menu bar with `menu` as its dropdown.
    pub fn new(_mtm: MainThreadMarker, menu: &NSMenu) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setMenu(Some(menu));
        Self {
            item,
            shown: RefCell::new(None),
        }
    }

    /// Draws `values` on the first call and afterwards only when they have moved from what is on screen.
    pub fn show(&self, mtm: MainThreadMarker, values: Values) {
        // Compared with the drawn values, not the previous sample, so slow drift still shows once it adds up.
        if let Some(shown) = self.shown.borrow().as_ref()
            && !format::moved(shown, &values)
        {
            return;
        }
        if let Some(button) = self.item.button(mtm) {
            button.setAttributedTitle(&attributed(&format::status(&values)));
        }
        *self.shown.borrow_mut() = Some(values);
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
