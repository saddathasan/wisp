//! Throwaway (exp/menu-cost, never merged): the spec §3.2 dropdown built several NSMenu ways, picked by `WISP_MENU`,
//! so each one's footprint, open latency and CPU can be measured. Unset = main's Quit-only menu.
//! The non-NSMenu variants (d panel, e popover, g out-of-process helper) live in panel.rs and main.rs.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, class, define_class,
    msg_send, sel,
};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityStaticTextRole, NSAppearanceCustomization, NSColor,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSFontAttributeName,
    NSFontWeightRegular, NSForegroundColorAttributeName, NSMenu, NSMenuItem,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment, NSTextTab, NSView,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString,
    NSPoint, NSRect, NSSize, NSString, ns_string,
};

use super::status::{resolve, transaction};
use crate::format::{self, MISSING};
use crate::sample::{Pressure, Snapshot};

type Attrs = Retained<NSDictionary<NSAttributedStringKey, AnyObject>>;

/// The interval options of spec §3.3, in seconds; an item's tag is its value.
pub const INTERVALS: [isize; 5] = [1, 2, 3, 5, 10];
/// Right edge of the value column: the text variant's tab stop and the view variant's row width.
const VALUE_X: f64 = 276.0;
pub(crate) const ROW_H: f64 = 24.0;
/// A row view's width: the value column plus its inset.
pub(crate) const ROW_W: f64 = VALUE_X + ROW_INSET;
/// Inset of a row view's label and value, matching native item text (measured off screenshots).
const ROW_INSET: f64 = 16.0;

/// How the dropdown is built. `WISP_MENU`: unset = Quit only (main), `a` = text items, `b` = custom row views,
/// `ca`/`cb` (`c` = `ca`) = the same built on open and released on close. `f` is `a` plus a bundle-level change.
/// `d` = layer-drawn borderless panel (`dv` vibrant background, `dc`/`ds` window colour space = screen's / sRGB),
/// `e` = NSPopover, `g` = variant a's menu shown by a short-lived helper process.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Quit,
    Text,
    View,
    Panel,
    Popover,
    Helper,
}

/// The window colour space a panel variant forces, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    Default,
    Screen,
    Srgb,
}

/// The `WISP_MENU` choice: the row kind and whether rows exist only while the menu is open.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub kind: Kind,
    pub lazy: bool,
    /// Panel only: an NSVisualEffectView background instead of a plain layer colour.
    pub vibrant: bool,
    pub space: Space,
}

impl Variant {
    /// Reads `WISP_MENU`; anything unknown falls back to main's behaviour.
    pub fn from_env() -> Self {
        let v = std::env::var("WISP_MENU").unwrap_or_default();
        let (kind, lazy) = match v.as_str() {
            "a" | "f" => (Kind::Text, false),
            "b" => (Kind::View, false),
            "c" | "ca" => (Kind::Text, true),
            "cb" => (Kind::View, true),
            "d" | "dv" | "dc" | "ds" => (Kind::Panel, true),
            "e" => (Kind::Popover, true),
            "g" => (Kind::Helper, true),
            _ => (Kind::Quit, false),
        };
        let space = match v.as_str() {
            "dc" => Space::Screen,
            "ds" => Space::Srgb,
            _ => Space::Default,
        };
        Self { kind, lazy, vibrant: v == "dv", space }
    }

    /// Variant a, the content the out-of-process helper shows.
    pub fn text() -> Self {
        Self { kind: Kind::Text, lazy: false, vibrant: false, space: Space::Default }
    }

    /// Whether the status item opens an NSMenu (the button's own click path) rather than calling the delegate.
    pub fn is_menu(&self) -> bool {
        matches!(self.kind, Kind::Quit | Kind::Text | Kind::View)
    }
}

/// One reading row of the dropdown, shared by every variant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub label: String,
    pub value: String,
    /// The breakdown line, drawn in the secondary label colour.
    pub secondary: bool,
    pub dot: Option<Pressure>,
}

/// Section titles and how many rows each owns, in order.
pub const SECTIONS: [(&str, usize); 3] = [("System", 2), ("Memory", 4), ("Network", 2)];

/// The eight reading rows for `s`, in display order.
pub fn rows(s: &Snapshot) -> Vec<Row> {
    let or_missing = |v: Option<String>| v.unwrap_or_else(|| MISSING.to_owned());
    let gb = |b: u64| format::size(b).trim_end_matches(" GB").to_owned();
    let row = |label: &str, value: Option<String>| Row {
        label: label.to_owned(),
        value: or_missing(value),
        ..Row::default()
    };
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

/// Fonts and colours for row strings; `fix` turns dynamic colours into resolved ones for layers (identity for text items).
struct Style {
    label: Attrs,
    secondary: Attrs,
    value: Attrs,
    dots: [Attrs; 3],
}

impl Style {
    fn new(fix: impl Fn(&NSColor) -> Retained<NSColor>, tab: bool) -> Self {
        let font = NSFont::menuFontOfSize(0.0);
        let digits = unsafe {
            NSFont::monospacedDigitSystemFontOfSize_weight(font.pointSize(), NSFontWeightRegular)
        };
        let para = tab.then(|| {
            let p = NSMutableParagraphStyle::new();
            let stop = unsafe {
                NSTextTab::initWithTextAlignment_location_options(
                    NSTextTab::alloc(),
                    NSTextAlignment::Right,
                    VALUE_X,
                    &NSDictionary::new(),
                )
            };
            p.setTabStops(Some(&NSArray::from_retained_slice(&[stop])));
            p
        });
        let attrs = |f: &NSFont, c: Retained<NSColor>| {
            let c = fix(&c);
            let mut keys: Vec<&NSAttributedStringKey> =
                unsafe { vec![NSFontAttributeName, NSForegroundColorAttributeName] };
            let mut vals: Vec<&AnyObject> = vec![f.as_ref(), c.as_ref()];
            if let Some(p) = &para {
                keys.push(unsafe { NSParagraphStyleAttributeName });
                vals.push(p.as_ref());
            }
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
        append(
            &s,
            &row.label,
            if row.secondary {
                &self.secondary
            } else {
                &self.label
            },
        );
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

    /// Label, tab, value: the text variant's whole title.
    fn title(&self, row: &Row) -> Retained<NSMutableAttributedString> {
        let s = self.label(row);
        if !row.value.is_empty() {
            append(&s, "\t", &self.label);
            s.appendAttributedString(&self.value(row));
        }
        s
    }
}

fn append(s: &NSMutableAttributedString, text: &str, attrs: &Attrs) {
    let part = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(text), attrs) };
    s.appendAttributedString(&part);
}

enum Rows {
    Text(Vec<Retained<NSMenuItem>>, Style),
    View(Vec<Retained<RowView>>),
}

/// The dropdown menu and, while built, handles to its reading rows.
pub struct Dropdown {
    pub menu: Retained<NSMenu>,
    pub variant: Variant,
    rows: RefCell<Option<Rows>>,
    last: RefCell<Vec<Row>>,
}

impl Dropdown {
    /// An empty menu for the full variants (filled by `build`), or main's Quit-only menu.
    pub fn new(mtm: MainThreadMarker, variant: Variant) -> Self {
        let menu = NSMenu::new(mtm);
        // Quit-only keeps main's menu exactly; the full variants set every item's enabled state themselves.
        menu.setAutoenablesItems(variant.kind == Kind::Quit);
        if variant.kind == Kind::Quit {
            menu.addItem(&quit_item(mtm));
        }
        Self {
            menu,
            variant,
            rows: RefCell::new(None),
            last: RefCell::new(Vec::new()),
        }
    }

    pub fn is_built(&self) -> bool {
        self.rows.borrow().is_some()
    }

    /// Adds every §3.2 item; `target` receives the interval and login actions.
    pub fn build(&self, target: &AnyObject, interval: isize, login: bool) {
        let mtm = MainThreadMarker::from(&*self.menu);
        let content = rows(&Snapshot::default());
        let mut it = content.iter();
        let rows = match self.variant.kind {
            Kind::Text => {
                let style = Style::new(|c| c.retain(), true);
                let mut items = Vec::new();
                for (title, n) in SECTIONS {
                    self.menu.addItem(&NSMenuItem::sectionHeaderWithTitle(
                        &NSString::from_str(title),
                        mtm,
                    ));
                    for row in it.by_ref().take(n) {
                        let item = NSMenuItem::new(mtm);
                        // Disabled items draw greyed even with explicit colours in the attributed title (checked on
                        // macOS 27), so the row stays enabled with no action: it draws normally but highlights on hover.
                        item.setEnabled(true);
                        item.setAttributedTitle(Some(&style.title(row)));
                        self.menu.addItem(&item);
                        items.push(item);
                    }
                }
                Rows::Text(items, style)
            }
            Kind::View => {
                let mut views = Vec::new();
                for (title, n) in SECTIONS {
                    self.menu.addItem(&NSMenuItem::sectionHeaderWithTitle(
                        &NSString::from_str(title),
                        mtm,
                    ));
                    for row in it.by_ref().take(n) {
                        let item = NSMenuItem::new(mtm);
                        let view = RowView::new(mtm, row.clone());
                        item.setView(Some(&view));
                        self.menu.addItem(&item);
                        views.push(view);
                    }
                }
                Rows::View(views)
            }
            _ => return,
        };
        self.menu.addItem(&NSMenuItem::separatorItem(mtm));
        let sub = NSMenu::new(mtm);
        sub.setAutoenablesItems(false);
        for secs in INTERVALS {
            let item = action_item(mtm, &format!("{secs} s"), sel!(setInterval:), target);
            item.setTag(secs);
            item.setState(if secs == interval {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            sub.addItem(&item);
        }
        let interval_item = NSMenuItem::new(mtm);
        interval_item.setTitle(ns_string!("Update Interval"));
        interval_item.setSubmenu(Some(&sub));
        self.menu.addItem(&interval_item);
        let login_item = action_item(mtm, "Launch at Login", sel!(toggleLogin:), target);
        login_item.setState(if login {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        self.menu.addItem(&login_item);
        self.menu.addItem(&NSMenuItem::separatorItem(mtm));
        self.menu.addItem(&quit_item(mtm));
        *self.rows.borrow_mut() = Some(rows);
        self.last.borrow_mut().clear();
    }

    /// Drops every item, row view and the submenu (variant c).
    pub fn release(&self) {
        self.menu.removeAllItems();
        *self.rows.borrow_mut() = None;
    }

    /// Updates rows whose text changed since the last update.
    pub fn update(&self, s: &Snapshot) {
        let new = rows(s);
        let mut last = self.last.borrow_mut();
        let rows = self.rows.borrow();
        let Some(rows) = rows.as_ref() else { return };
        for (i, row) in new.iter().enumerate() {
            if last.get(i) == Some(row) {
                continue;
            }
            match rows {
                Rows::Text(items, style) => items[i].setAttributedTitle(Some(&style.title(row))),
                Rows::View(views) => views[i].set(row.clone()),
            }
        }
        *last = new;
    }
}

fn action_item(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    target: &AnyObject,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            ns_string!(""),
        )
    };
    unsafe { item.setTarget(Some(target)) };
    item
}

/// Quit Wisp ⌘Q; a nil target routes it to NSApplication's `terminate:`.
fn quit_item(mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit Wisp"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    }
}

pub(crate) struct RowIvars {
    label: Retained<AnyObject>,
    value: Retained<AnyObject>,
    row: RefCell<Row>,
    style: RefCell<Option<Style>>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements and RowView does not implement Drop.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RowIvars]
    pub(crate) struct RowView;

    // Layer colours are resolved, so restyle whenever the appearance or scale can have changed (as status.rs does).
    impl RowView {
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
    }
);

impl RowView {
    /// A layer-hosting row: label CATextLayer left, value CATextLayer right-aligned at `VALUE_X`.
    pub(crate) fn new(mtm: MainThreadMarker, row: Row) -> Retained<Self> {
        let font = NSFont::menuFontOfSize(0.0);
        let line = (font.ascender() - font.descender()).ceil();
        let y = ((ROW_H - line) / 2.0).round();
        let root: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), layer] };
        let layer = |x: f64, w: f64, right: bool| {
            let tl: Retained<AnyObject> = unsafe { msg_send![class!(CATextLayer), layer] };
            unsafe {
                let _: () = msg_send![&*root, addSublayer: &*tl];
                let _: () = msg_send![&*tl, setFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(w, line))];
                if right {
                    let _: () = msg_send![&*tl, setAlignmentMode: ns_string!("right")];
                }
            }
            tl
        };
        let width = ROW_W;
        let label = layer(ROW_INSET, width - 2.0 * ROW_INSET, false);
        let value = layer(ROW_INSET, width - 2.0 * ROW_INSET, true);
        let this = Self::alloc(mtm).set_ivars(RowIvars {
            label,
            value,
            row: RefCell::new(row),
            style: RefCell::new(None),
        });
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(width, ROW_H));
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let _: () = unsafe { msg_send![&*this, setLayer: &*root] };
        this.setWantsLayer(true);
        // The layers are invisible to VoiceOver, so the view is one static-text element carrying the row's text.
        this.setAccessibilityElement(true);
        this.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        this.draw();
        this
    }

    pub(crate) fn set(&self, row: Row) {
        *self.ivars().row.borrow_mut() = row;
        self.draw();
    }

    fn restyle(&self) {
        let appearance = self.effectiveAppearance();
        *self.ivars().style.borrow_mut() = Some(Style::new(|c| resolve(&appearance, c), false));
        let scale = self.window().map_or(2.0, |w| w.backingScaleFactor());
        for tl in [&self.ivars().label, &self.ivars().value] {
            let _: () = unsafe { msg_send![&**tl, setContentsScale: scale] };
        }
        self.draw();
    }

    fn draw(&self) {
        let row = self.ivars().row.borrow();
        let label = format!("{} {}", row.label, row.value);
        self.setAccessibilityLabel(Some(&NSString::from_str(label.trim_end())));
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

    #[test]
    fn sections_cover_every_row_and_missing_reads_dash() {
        let r = rows(&Snapshot::default());
        assert_eq!(SECTIONS.iter().map(|s| s.1).sum::<usize>(), r.len());
        assert_eq!(r[0].value, MISSING);
        assert!(r[3].secondary && r[3].value.is_empty());
    }
}
