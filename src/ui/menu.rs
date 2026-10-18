//! The spec §3.2 dropdown, built and updated by the helper process only (ADR 0001): reading rows, their layer-drawn
//! views and the menu around them.

use crate::format::{self, MISSING};
use crate::sample::{Pressure, Snapshot};

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
    fn sections_own_every_row() {
        let owned: usize = SECTIONS.iter().map(|s| s.1).sum();
        assert_eq!(owned, rows(&full()).len());
    }
}
