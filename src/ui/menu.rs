//! The spec §3.2 dropdown, built and updated by the helper process only (ADR 0001): reading rows, their layer-drawn
//! views and the menu around them.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityStaticTextRole, NSAppearance, NSAppearanceCustomization,
    NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSFontAttributeName,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSMenu, NSMenuItem, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSPoint,
    NSRect, NSSize, NSString, ns_string,
};

use super::status::{resolve, transaction};
use crate::format::{self, MISSING};
use crate::sample::{Pressure, Snapshot};

type Attrs = Retained<NSDictionary<NSAttributedStringKey, AnyObject>>;

/// The interval options of spec §3.3, in seconds; an item's tag is its value.
pub const INTERVALS: [isize; 5] = [1, 2, 3, 5, 10];
/// The §3.3 default interval in seconds, used when nothing valid is stored.
const DEFAULT_INTERVAL: isize = 2;

/// The update interval in seconds for a stored `interval` value: one of `INTERVALS`, else the §3.3 default.
pub fn interval(stored: isize) -> isize {
    if INTERVALS.contains(&stored) {
        stored
    } else {
        DEFAULT_INTERVAL
    }
}

/// Right edge of the value column.
const VALUE_X: f64 = 276.0;
const ROW_H: f64 = 24.0;
/// Inset of a row's label and value, matching native item text (measured off screenshots).
const ROW_INSET: f64 = 16.0;
const ROW_W: f64 = VALUE_X + ROW_INSET;

/// One reading row: a label left, a value right, and the pressure dot where it applies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub label: String,
    pub value: String,
    /// The memory breakdown line, drawn in the secondary label colour.
    pub secondary: bool,
    pub dot: Option<Pressure>,
}

/// Section headers and how many of `rows`' rows each owns, in display order.
pub const SECTIONS: [(&str, usize); 3] = [("System", 2), ("Memory", 4), ("Network", 2)];

/// The eight reading rows for `s`, in display order; a failed reading shows `MISSING`.
pub fn rows(s: &Snapshot) -> Vec<Row> {
    let row = |label: &str, value: Option<String>| Row {
        label: label.to_owned(),
        value: value.unwrap_or_else(|| MISSING.to_owned()),
        ..Row::default()
    };
    // The breakdown carries its unit once, at the end.
    let gb = |b: u64| format::size(b).trim_end_matches(" GB").to_owned();
    vec![
        row("CPU", s.cpu.map(|v| format::percent(Some(v)))),
        row("GPU", s.gpu.map(|v| format::percent(Some(v)))),
        row(
            "Used",
            s.mem.map(|m| {
                let pct = format::percent(Some(m.percent()));
                format!("{pct}  {}", format::size_of_total(m.used(), m.total))
            }),
        ),
        Row {
            label: s.mem.map_or(MISSING.to_owned(), |m| {
                format!(
                    "App {} · Wired {} · Compressed {} GB",
                    gb(m.app),
                    gb(m.wired),
                    gb(m.compressed)
                )
            }),
            secondary: true,
            ..Row::default()
        },
        Row {
            dot: s.pressure,
            ..row("Pressure", Some(format::pressure(s.pressure).to_owned()))
        },
        row(
            "Swap",
            s.swap.map(|w| format::size_of_total(w.used, w.total)),
        ),
        row("Download", s.net.map(|n| format::rate(n.down))),
        row("Upload", s.net.map(|n| format::rate(n.up))),
    ]
}

/// The built menu and the views of its reading rows, which `update` rewrites in place.
pub struct Dropdown {
    /// The menu the helper pops up.
    pub menu: Retained<NSMenu>,
    views: Vec<Retained<RowView>>,
}

impl Dropdown {
    /// Builds every §3.2 item, with `interval` and `login` checked and `target` receiving their actions.
    pub fn new(mtm: MainThreadMarker, target: &AnyObject, interval: isize, login: bool) -> Self {
        // `target` gets `setInterval:` (tag = seconds), `toggleLogin:` and `quit:`.
        let menu = NSMenu::new(mtm);
        // Every item's enabled state is set here; autoenabling would grey the action-less reading rows.
        menu.setAutoenablesItems(false);
        let content = rows(&Snapshot::default());
        let mut content = content.into_iter();
        let mut views = Vec::new();
        for (title, n) in SECTIONS {
            menu.addItem(&NSMenuItem::sectionHeaderWithTitle(
                &NSString::from_str(title),
                mtm,
            ));
            for row in content.by_ref().take(n) {
                let item = NSMenuItem::new(mtm);
                let view = RowView::new(mtm, row);
                item.setView(Some(&view));
                menu.addItem(&item);
                views.push(view);
            }
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let sub = NSMenu::new(mtm);
        sub.setAutoenablesItems(false);
        for secs in INTERVALS {
            let item = action_item(mtm, &format!("{secs} s"), sel!(setInterval:), target, "");
            item.setTag(secs);
            item.setState(state(secs == interval));
            sub.addItem(&item);
        }
        let interval_item = NSMenuItem::new(mtm);
        interval_item.setTitle(ns_string!("Update Interval"));
        interval_item.setSubmenu(Some(&sub));
        menu.addItem(&interval_item);
        let login_item = action_item(mtm, "Launch at Login", sel!(toggleLogin:), target, "");
        login_item.setState(state(login));
        menu.addItem(&login_item);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&action_item(mtm, "Quit Wisp", sel!(quit:), target, "q"));
        Self { menu, views }
    }

    /// Shows `s` in the reading rows.
    pub fn update(&self, s: &Snapshot) {
        for (view, row) in self.views.iter().zip(rows(s)) {
            view.set(row);
        }
    }
}

fn state(on: bool) -> isize {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

fn action_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
    target: &AnyObject,
    key: &str,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item
}

/// Fonts and appearance-resolved colours for row strings; CATextLayer cannot follow dynamic colours.
struct Style {
    label: Attrs,
    secondary: Attrs,
    value: Attrs,
    /// Pressure dot colours, indexed by `Pressure as usize`.
    dots: [Attrs; 3],
}

impl Style {
    fn new(appearance: &NSAppearance) -> Self {
        let font = NSFont::menuFontOfSize(0.0);
        // Monospaced digits keep the right-aligned value column from jittering as numbers change.
        let digits = unsafe {
            NSFont::monospacedDigitSystemFontOfSize_weight(font.pointSize(), NSFontWeightRegular)
        };
        let attrs = |f: &NSFont, c: Retained<NSColor>| {
            let c = resolve(appearance, &c);
            let keys: [&NSAttributedStringKey; 2] =
                unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
            let vals: [&AnyObject; 2] = [f.as_ref(), c.as_ref()];
            NSDictionary::from_slices(&keys, &vals)
        };
        Self {
            label: attrs(&font, NSColor::labelColor()),
            secondary: attrs(&font, NSColor::secondaryLabelColor()),
            value: attrs(&digits, NSColor::labelColor()),
            dots: [
                attrs(&font, NSColor::systemGreenColor()),
                attrs(&font, NSColor::systemYellowColor()),
                attrs(&font, NSColor::systemRedColor()),
            ],
        }
    }

    fn label(&self, row: &Row) -> Retained<NSMutableAttributedString> {
        let s = NSMutableAttributedString::new();
        let attrs = if row.secondary {
            &self.secondary
        } else {
            &self.label
        };
        append(&s, &row.label, attrs);
        s
    }

    /// The value, led by the pressure dot in its level's colour.
    fn value(&self, row: &Row) -> Retained<NSMutableAttributedString> {
        let s = NSMutableAttributedString::new();
        if let Some(p) = row.dot {
            append(&s, "● ", &self.dots[p as usize]);
        }
        append(&s, &row.value, &self.value);
        s
    }
}

fn append(s: &NSMutableAttributedString, text: &str, attrs: &Attrs) {
    // SAFETY: `attrs` maps the font and colour keys to an NSFont and an NSColor, the types AppKit expects.
    let part = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), attrs) };
    s.appendAttributedString(&part);
}

struct RowIvars {
    label: Retained<AnyObject>,
    value: Retained<AnyObject>,
    row: RefCell<Row>,
    /// None until the view is in a window and its appearance is known.
    style: RefCell<Option<Style>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and RowView does not implement Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RowIvars]
    struct RowView;

    impl RowView {
        // Layer colours are resolved, so the row restyles once it is in the menu's window, against the final
        // appearance and scale. That is the only hook it needs: the helper sets the menu's appearance once before
        // showing it and lives only while the menu is open, so neither can change afterwards.
        #[unsafe(method(viewDidMoveToWindow))]
        fn moved_to_window(&self) {
            self.restyle();
        }
    }
);

impl RowView {
    /// A layer-hosting row, so it neither greys out nor highlights like a plain item: label CATextLayer left,
    /// value CATextLayer right-aligned at `VALUE_X`.
    fn new(mtm: MainThreadMarker, row: Row) -> Retained<Self> {
        let font = NSFont::menuFontOfSize(0.0);
        let line = (font.ascender() - font.descender()).ceil();
        let y = ((ROW_H - line) / 2.0).round();
        let root: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), layer] };
        let layer = |right: bool| {
            let tl: Retained<AnyObject> = unsafe { msg_send![class!(CATextLayer), layer] };
            let frame = NSRect::new(
                NSPoint::new(ROW_INSET, y),
                NSSize::new(ROW_W - 2.0 * ROW_INSET, line),
            );
            unsafe {
                let _: () = msg_send![&*root, addSublayer: &*tl];
                let _: () = msg_send![&*tl, setFrame: frame];
                if right {
                    let _: () = msg_send![&*tl, setAlignmentMode: ns_string!("right")];
                }
            }
            tl
        };
        let this = Self::alloc(mtm).set_ivars(RowIvars {
            label: layer(false),
            value: layer(true),
            row: RefCell::new(row),
            style: RefCell::new(None),
        });
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(ROW_W, ROW_H));
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // Layer-hosting (layer set before wantsLayer): we own the tree, so adding sublayers is supported.
        let _: () = unsafe { msg_send![&*this, setLayer: &*root] };
        this.setWantsLayer(true);
        // The layers are invisible to VoiceOver, so the view is one static-text element carrying the row's text.
        this.setAccessibilityElement(true);
        this.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        this.draw();
        this
    }

    fn set(&self, row: Row) {
        if *self.ivars().row.borrow() == row {
            return;
        }
        *self.ivars().row.borrow_mut() = row;
        self.draw();
    }

    fn restyle(&self) {
        *self.ivars().style.borrow_mut() = Some(Style::new(&self.effectiveAppearance()));
        let scale = self.window().map_or(2.0, |w| w.backingScaleFactor());
        for tl in [&self.ivars().label, &self.ivars().value] {
            let _: () = unsafe { msg_send![&**tl, setContentsScale: scale] };
        }
        self.draw();
    }

    fn draw(&self) {
        let row = self.ivars().row.borrow();
        let spoken = format!("{} {}", row.label, row.value);
        self.setAccessibilityLabel(Some(&NSString::from_str(spoken.trim_end())));
        let style = self.ivars().style.borrow();
        let Some(style) = style.as_ref() else { return };
        transaction(|| unsafe {
            let _: () = msg_send![&*self.ivars().label, setString: &*style.label(&row)];
            let _: () = msg_send![&*self.ivars().value, setString: &*style.value(&row)];
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{Memory, Net, Swap};

    const GIB: u64 = 1 << 30;

    fn full() -> Snapshot {
        Snapshot {
            cpu: Some(12.0),
            gpu: Some(3.0),
            mem: Some(Memory {
                total: 32 * GIB,
                app: GIB * 112 / 10,
                wired: GIB * 31 / 10,
                compressed: GIB * 52 / 10,
                free: GIB * 2,
                cached: GIB * 105 / 10,
            }),
            pressure: Some(Pressure::Warning),
            swap: Some(Swap {
                used: GIB * 122 / 10,
                total: GIB * 133 / 10,
            }),
            net: Some(Net {
                down: 1.2 * 1024.0 * 1024.0,
                up: 84.0 * 1024.0,
            }),
        }
    }

    #[test]
    fn rows_follow_the_spec_order() {
        let r = rows(&full());
        let labels: Vec<_> = r.iter().map(|r| r.label.as_str()).collect();
        let values: Vec<_> = r.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(
            labels,
            [
                "CPU",
                "GPU",
                "Used",
                "App 11.2 · Wired 3.1 · Compressed 5.2 GB",
                "Pressure",
                "Swap",
                "Download",
                "Upload"
            ]
        );
        assert_eq!(
            values,
            [
                "12%",
                "3%",
                "61%  19.5 / 32 GB",
                "",
                "Warning",
                "12.2 / 13.3 GB",
                "1.2 MB/s",
                "84 KB/s"
            ]
        );
    }

    #[test]
    fn only_the_breakdown_is_secondary_and_only_pressure_has_a_dot() {
        let r = rows(&full());
        let secondary: Vec<_> = r.iter().map(|r| r.secondary).collect();
        let dots: Vec<_> = r.iter().map(|r| r.dot).collect();
        assert_eq!(
            secondary,
            [false, false, false, true, false, false, false, false]
        );
        assert_eq!(
            dots,
            [
                None,
                None,
                None,
                None,
                Some(Pressure::Warning),
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn missing_readings_show_dashes() {
        let r = rows(&Snapshot::default());
        assert_eq!(r.len(), 8);
        for (i, row) in r.iter().enumerate() {
            if i == 3 {
                assert_eq!((row.label.as_str(), row.value.as_str()), (MISSING, ""));
            } else {
                assert_eq!(row.value, MISSING, "row {i}");
            }
            assert_eq!(row.dot, None);
        }
    }

    #[test]
    fn only_listed_intervals_are_kept_and_anything_else_is_2_s() {
        for secs in INTERVALS {
            assert_eq!(interval(secs), secs);
        }
        for garbage in [0, -1, 4, 7, 11, 60, isize::MAX] {
            assert_eq!(interval(garbage), 2, "stored {garbage}");
        }
    }

    #[test]
    fn sections_own_every_row() {
        let owned: usize = SECTIONS.iter().map(|s| s.1).sum();
        assert_eq!(owned, rows(&full()).len());
    }
}
