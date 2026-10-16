//! Turns a Snapshot into the exact strings Wisp shows. Pure, so every display rule is unit-tested.

use crate::sample::{Pressure, Snapshot};

/// Shown in the dropdown in place of any reading that failed.
pub const MISSING: &str = "—";

/// U+2007 and U+2012 are as wide as a digit, so padded numbers and missing values keep the menu bar item a constant width.
const FIGURE_SPACE: char = '\u{2007}';
const FIGURE_DASH: char = '\u{2012}';
const GIB: f64 = (1u64 << 30) as f64;

/// The menu bar's (label, value) pairs in display order.
pub type StatusLine = [(&'static str, String); 3];

/// The CPU / GPU / MEM pairs for the menu bar.
pub fn status(s: &Snapshot) -> StatusLine {
    [
        ("CPU", percent(s.cpu)),
        ("GPU", percent(s.gpu)),
        ("MEM", percent(s.mem.map(|m| m.percent()))),
    ]
}

/// A whole-number percentage padded to three digit-widths, e.g. "  3%"; missing is "  ‒%".
pub fn percent(value: Option<f64>) -> String {
    let pad = |s: String| s.replace(' ', &FIGURE_SPACE.to_string());
    match value {
        // NaN clamps to NaN and `as u32` maps it to 0.
        Some(v) => pad(format!("{:>3}%", v.round().clamp(0.0, 100.0) as u32)),
        None => format!("{FIGURE_SPACE}{FIGURE_SPACE}{FIGURE_DASH}%"),
    }
}

/// Bytes as GB (binary, as Activity Monitor counts) with at most one decimal.
pub fn size(bytes: u64) -> String {
    format!("{} GB", one_decimal(bytes as f64 / GIB))
}

/// "used / total GB", e.g. "19.5 / 32 GB".
pub fn size_of_total(used: u64, total: u64) -> String {
    format!(
        "{} / {} GB",
        one_decimal(used as f64 / GIB),
        one_decimal(total as f64 / GIB)
    )
}

/// Throughput with the shortest sensible unit: whole B/s and KB/s, one decimal for MB/s and GB/s.
pub fn rate(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bytes_per_sec.max(0.0);
    let mut unit = 0;
    // Step up at 999.5 rather than 1024 so a value never rounds to a four-digit "1024 B/s".
    while v >= 999.5 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit <= 1 {
        format!("{v:.0} {}", UNITS[unit])
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// The pressure level's label.
pub fn pressure(p: Option<Pressure>) -> &'static str {
    match p {
        Some(Pressure::Normal) => "Normal",
        Some(Pressure::Warning) => "Warning",
        Some(Pressure::Critical) => "Critical",
        None => MISSING,
    }
}

fn one_decimal(x: f64) -> String {
    let s = format!("{x:.1}");
    s.strip_suffix(".0").map(str::to_owned).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{Memory, Snapshot};

    const GIB: u64 = 1 << 30;

    #[test]
    fn percent_pads_to_three_figure_widths() {
        assert_eq!(percent(Some(3.0)), "\u{2007}\u{2007}3%");
        assert_eq!(percent(Some(12.4)), "\u{2007}12%");
        assert_eq!(percent(Some(99.6)), "100%");
        for v in [0.0, 7.0, 42.0, 100.0] {
            assert_eq!(
                percent(Some(v)).chars().count(),
                4,
                "constant width for {v}"
            );
        }
    }

    #[test]
    fn percent_clamps_out_of_range_and_nan() {
        assert_eq!(percent(Some(150.0)), "100%");
        assert_eq!(percent(Some(-2.0)), "\u{2007}\u{2007}0%");
        assert_eq!(percent(Some(f64::NAN)), "\u{2007}\u{2007}0%");
    }

    #[test]
    fn missing_values_show_a_dash() {
        // Figure dash is digit-wide, so a missing reading keeps the menu bar item's width.
        assert_eq!(percent(None), "\u{2007}\u{2007}\u{2012}%");
        assert_eq!(percent(None).chars().count(), 4);
        assert_eq!(pressure(None), MISSING);
    }

    #[test]
    fn sizes_drop_a_trailing_zero_decimal() {
        assert_eq!(size(GIB * 32), "32 GB");
        assert_eq!(size(GIB * 3 + GIB / 10), "3.1 GB");
        assert_eq!(size_of_total(GIB * 39 / 2, GIB * 32), "19.5 / 32 GB");
    }

    #[test]
    fn rates_pick_a_short_unit() {
        assert_eq!(rate(0.0), "0 B/s");
        assert_eq!(rate(500.0), "500 B/s");
        assert_eq!(rate(999.6), "1 KB/s");
        assert_eq!(rate(84.0 * 1024.0), "84 KB/s");
        assert_eq!(rate(1.2 * 1024.0 * 1024.0), "1.2 MB/s");
        assert_eq!(rate(3.0 * 1024.0 * 1024.0 * 1024.0), "3.0 GB/s");
    }

    #[test]
    fn pressure_labels() {
        assert_eq!(pressure(Some(Pressure::Normal)), "Normal");
        assert_eq!(pressure(Some(Pressure::Warning)), "Warning");
        assert_eq!(pressure(Some(Pressure::Critical)), "Critical");
    }

    #[test]
    fn status_line_is_cpu_gpu_mem_and_survives_missing_gpu() {
        let snap = Snapshot {
            cpu: Some(12.0),
            gpu: None,
            mem: Some(Memory {
                total: 100,
                app: 40,
                wired: 11,
                compressed: 10,
            }),
            ..Snapshot::default()
        };
        let line = status(&snap);
        assert_eq!(line[0], ("CPU", "\u{2007}12%".to_string()));
        assert_eq!(line[1], ("GPU", "\u{2007}\u{2007}\u{2012}%".to_string()));
        assert_eq!(line[2], ("MEM", "\u{2007}61%".to_string()));
    }
}
