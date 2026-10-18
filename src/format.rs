//! Turns a Snapshot into the exact strings Wisp shows. Pure, so every display rule is unit-tested.

use crate::sample::{Pressure, Snapshot};

/// Shown in the dropdown in place of any reading that failed.
pub const MISSING: &str = "—";

/// A digit-wide dash, the menu bar's stand-in for a missing reading.
const FIGURE_DASH: &str = "\u{2012}";
const GIB: f64 = (1u64 << 30) as f64;

/// CPU, GPU and MEM percentages, the numbers behind the menu bar item.
pub type Values = [Option<f64>; 3];

/// Points a value must move from the one on screen before the bar is redrawn (decision 2026-10-17 / plan Task 10).
/// CPU % jitters a point or two every tick; redrawing on that noise is wasted work.
pub const HYSTERESIS: f64 = 2.0;

/// The menu bar's values from a sample.
pub fn values(s: &Snapshot) -> Values {
    [s.cpu, s.gpu, s.mem.map(|m| m.percent())]
}

/// Whether `now` differs enough from the `shown` values to be worth a redraw.
pub fn moved(shown: &Values, now: &Values) -> bool {
    shown.iter().zip(now).any(|pair| match pair {
        (Some(a), Some(b)) => (a - b).abs() >= HYSTERESIS,
        (a, b) => a.is_some() != b.is_some(),
    })
}

/// A whole-number percentage, e.g. "7%"; missing is "‒". The bar's columns are sized for "100%", so no padding.
pub fn percent(value: Option<f64>) -> String {
    match value {
        // NaN clamps to NaN and `as u32` maps it to 0.
        Some(v) => format!("{}%", v.round().clamp(0.0, 100.0) as u32),
        None => FIGURE_DASH.to_owned(),
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
    fn percent_is_unpadded() {
        assert_eq!(percent(Some(7.4)), "7%");
        assert_eq!(percent(Some(42.0)), "42%");
        assert_eq!(percent(Some(99.6)), "100%");
    }

    #[test]
    fn percent_clamps_out_of_range_and_nan() {
        assert_eq!(percent(Some(150.0)), "100%");
        assert_eq!(percent(Some(-2.0)), "0%");
        assert_eq!(percent(Some(f64::NAN)), "0%");
    }

    #[test]
    fn missing_values_show_a_dash() {
        assert_eq!(percent(None), "\u{2012}");
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
    fn bar_values_are_cpu_gpu_mem_and_survive_missing_gpu() {
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
        assert_eq!(values(&snap).map(percent), ["12%", "\u{2012}", "61%"]);
    }

    #[test]
    fn no_move_below_two_points() {
        assert!(!moved(&[Some(12.0); 3], &[Some(13.9); 3]));
    }

    #[test]
    fn moving_two_points_either_way_counts() {
        assert!(moved(&[Some(12.0); 3], &[Some(14.0); 3]));
        assert!(moved(&[Some(12.0); 3], &[Some(10.0); 3]));
    }

    #[test]
    fn any_single_field_crossing_counts() {
        let shown = [Some(12.0), Some(3.0), Some(61.0)];
        assert!(moved(&shown, &[Some(14.5), Some(3.0), Some(61.0)]));
        assert!(moved(&shown, &[Some(12.0), Some(5.0), Some(61.0)]));
        assert!(moved(&shown, &[Some(12.0), Some(3.0), Some(58.0)]));
    }

    #[test]
    fn a_reading_appearing_or_disappearing_counts() {
        let shown = [Some(12.0), Some(3.0), Some(61.0)];
        for i in 0..3 {
            let mut gone = shown;
            gone[i] = None;
            assert!(moved(&shown, &gone), "field {i} disappearing");
            assert!(moved(&gone, &shown), "field {i} appearing");
        }
        assert!(!moved(&[None; 3], &[None; 3]));
    }

    #[test]
    fn drift_is_measured_from_the_shown_value() {
        let shown = [Some(12.0); 3];
        assert!(!moved(&shown, &[Some(13.0); 3]));
        assert!(moved(&shown, &[Some(14.1); 3]));
    }
}
