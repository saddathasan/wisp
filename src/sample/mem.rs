//! Memory readings: Activity Monitor's "Memory Used" breakdown, the pressure level and swap.

use std::mem::MaybeUninit;

use super::sys;

/// Memory in bytes, split the way Activity Monitor splits "Memory Used".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub app: u64,
    pub wired: u64,
    pub compressed: u64,
}

impl Memory {
    /// App + Wired + Compressed — Activity Monitor's definition of used memory.
    pub fn used(&self) -> u64 {
        self.app + self.wired + self.compressed
    }

    /// Used memory as a percentage of physical RAM.
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.used() as f64 * 100.0 / self.total as f64
    }
}

/// The kernel's memory pressure level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressure {
    Normal,
    Warning,
    Critical,
}

impl Pressure {
    /// Maps `kern.memorystatus_vm_pressure_level` (1, 2, 4) to a level; other values are unknown.
    pub fn from_level(level: i32) -> Option<Self> {
        match level {
            1 => Some(Self::Normal),
            2 => Some(Self::Warning),
            4 => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Swap usage in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swap {
    pub used: u64,
    pub total: u64,
}

/// The `vm_statistics64` page counts the breakdown needs.
#[derive(Debug, Clone, Copy)]
pub struct Pages {
    pub internal: u64,
    pub purgeable: u64,
    pub wired: u64,
    pub compressed: u64,
}

/// Converts page counts to bytes. Purgeable pages are cache the system can drop, so they don't count as app memory.
pub fn memory(total: u64, page_size: u64, p: Pages) -> Memory {
    Memory {
        total,
        app: p.internal.saturating_sub(p.purgeable) * page_size,
        wired: p.wired * page_size,
        compressed: p.compressed * page_size,
    }
}

/// Reads the page counts from `host_statistics64(HOST_VM_INFO64)`.
pub fn read_pages(host: libc::mach_port_t) -> Option<Pages> {
    let mut stats = MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    let rc = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            stats.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return None;
    }
    // The struct is packed; fields are copied out by value, never borrowed.
    let s = unsafe { stats.assume_init() };
    Some(Pages {
        internal: s.internal_page_count as u64,
        purgeable: s.purgeable_count as u64,
        wired: s.wire_count as u64,
        compressed: s.compressor_page_count as u64,
    })
}

/// Physical RAM in bytes.
pub fn read_total() -> Option<u64> {
    sys::by_name::<u64>(c"hw.memsize")
}

/// The current memory pressure level.
pub fn read_pressure() -> Option<Pressure> {
    sys::by_name::<i32>(c"kern.memorystatus_vm_pressure_level").and_then(Pressure::from_level)
}

/// Swap used and total.
pub fn read_swap() -> Option<Swap> {
    sys::by_name::<libc::xsw_usage>(c"vm.swapusage").map(|x| Swap {
        used: x.xsu_used,
        total: x.xsu_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: u64 = 16_384;

    #[test]
    fn breakdown_matches_activity_monitor() {
        let pages = Pages {
            internal: 800_000,
            purgeable: 50_000,
            wired: 200_000,
            compressed: 300_000,
        };
        let m = memory(32 << 30, PAGE, pages);
        assert_eq!(m.app, 750_000 * PAGE);
        assert_eq!(m.wired, 200_000 * PAGE);
        assert_eq!(m.compressed, 300_000 * PAGE);
        assert_eq!(m.used(), 1_250_000 * PAGE);
        let expected = (1_250_000 * PAGE) as f64 * 100.0 / (32u64 << 30) as f64;
        assert!((m.percent() - expected).abs() < 1e-9);
    }

    #[test]
    fn purgeable_above_internal_does_not_underflow() {
        let pages = Pages {
            internal: 10,
            purgeable: 20,
            wired: 0,
            compressed: 0,
        };
        assert_eq!(memory(1 << 30, PAGE, pages).app, 0);
    }

    #[test]
    fn zero_total_gives_zero_percent() {
        let pages = Pages {
            internal: 10,
            purgeable: 0,
            wired: 0,
            compressed: 0,
        };
        assert_eq!(memory(0, PAGE, pages).percent(), 0.0);
    }

    #[test]
    fn pressure_levels_map_and_unknown_is_none() {
        assert_eq!(Pressure::from_level(1), Some(Pressure::Normal));
        assert_eq!(Pressure::from_level(2), Some(Pressure::Warning));
        assert_eq!(Pressure::from_level(4), Some(Pressure::Critical));
        assert_eq!(Pressure::from_level(0), None);
        assert_eq!(Pressure::from_level(3), None);
    }

    #[test]
    fn live_readings_succeed() {
        let host = sys::host();
        assert!(read_pages(host).is_some());
        assert!(read_total().unwrap() > 0);
        assert!(read_pressure().is_some());
        let swap = read_swap().unwrap();
        assert!(swap.used <= swap.total);
    }
}
