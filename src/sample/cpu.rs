//! CPU busy percentage from the kernel's cumulative per-state tick counters.
#![allow(deprecated)] // libc's host_statistics is marked deprecated in favour of mach2; it is stable.

use std::mem::MaybeUninit;

const USER: usize = libc::CPU_STATE_USER as usize;
const SYSTEM: usize = libc::CPU_STATE_SYSTEM as usize;
const IDLE: usize = libc::CPU_STATE_IDLE as usize;
const NICE: usize = libc::CPU_STATE_NICE as usize;

/// Cumulative ticks since boot, indexed by `CPU_STATE_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticks(pub [u32; 4]);

/// Reads the all-core tick counters with `host_statistics(HOST_CPU_LOAD_INFO)`.
pub fn read(host: libc::mach_port_t) -> Option<Ticks> {
    let mut info = MaybeUninit::<libc::host_cpu_load_info>::zeroed();
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    let rc = unsafe {
        libc::host_statistics(
            host,
            libc::HOST_CPU_LOAD_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    (rc == libc::KERN_SUCCESS).then(|| Ticks(unsafe { info.assume_init() }.cpu_ticks))
}

/// Busy share (user + system + nice) of the ticks elapsed between two readings; `None` if none elapsed.
pub fn busy_percent(prev: Ticks, now: Ticks) -> Option<f64> {
    // Counters are u32 and wrap on long uptimes; wrapping_sub keeps the delta right across a wrap.
    let d = |i: usize| now.0[i].wrapping_sub(prev.0[i]) as u64;
    let busy = d(USER) + d(SYSTEM) + d(NICE);
    let total = busy + d(IDLE);
    (total > 0).then(|| busy as f64 * 100.0 / total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_share_of_elapsed_ticks() {
        // user 30 + system 20 + nice 0 busy, idle 50 → 50%.
        assert_eq!(
            busy_percent(Ticks([0, 0, 0, 0]), Ticks([30, 20, 50, 0])),
            Some(50.0)
        );
    }

    #[test]
    fn nice_counts_as_busy() {
        assert_eq!(
            busy_percent(Ticks([0, 0, 0, 0]), Ticks([0, 0, 75, 25])),
            Some(25.0)
        );
    }

    #[test]
    fn wrapped_counter_still_gives_the_right_delta() {
        let prev = Ticks([u32::MAX - 9, 0, 0, 0]);
        let now = Ticks([10, 0, 20, 0]); // user advanced 20 across the wrap
        assert_eq!(busy_percent(prev, now), Some(50.0));
    }

    #[test]
    fn no_elapsed_ticks_is_none() {
        let t = Ticks([5, 5, 5, 5]);
        assert_eq!(busy_percent(t, t), None);
    }

    #[test]
    fn live_reading_succeeds() {
        assert!(read(crate::sample::sys::host()).is_some());
    }
}
