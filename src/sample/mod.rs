//! System readings for Wisp. Nothing here touches AppKit, so every reader is tested headless.

mod cpu;
mod gpu;
mod mem;
mod net;
mod sys;

use std::time::Instant;

pub use mem::{Memory, Pressure, Swap};
pub use net::Net;

/// One reading of everything Wisp displays; `None` means that reading failed this tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub mem: Option<Memory>,
    pub pressure: Option<Pressure>,
    pub swap: Option<Swap>,
    pub net: Option<Net>,
}

/// Owns everything that must persist between ticks: the host port, the GPU handle and the last counters.
pub struct Sampler {
    host: libc::mach_port_t,
    page_size: u64,
    total: Option<u64>,
    gpu: Option<gpu::Gpu>,
    cpu_prev: Option<cpu::Ticks>,
    net_prev: Option<(net::Counters, Instant)>,
}

impl Sampler {
    /// Opens every source once and takes a baseline, so the first `sample()` already has deltas.
    pub fn new() -> Self {
        let mut sampler = Self {
            host: sys::host(),
            page_size: sys::page_size(),
            total: mem::read_total(),
            gpu: gpu::Gpu::open(),
            cpu_prev: None,
            net_prev: None,
        };
        sampler.reset();
        sampler
    }

    /// Takes a fresh baseline so the next sample isn't averaged across a pause (sleep, lock).
    pub fn reset(&mut self) {
        self.cpu_prev = cpu::read(self.host);
        self.net_prev = net::read().map(|c| (c, Instant::now()));
    }

    /// Reads the menu bar values; with `detail` (dropdown open) also network, pressure and swap.
    /// Delta-based readings cover the time since the previous call.
    pub fn sample(&mut self, detail: bool) -> Snapshot {
        let cpu_now = cpu::read(self.host);
        let cpu = match (self.cpu_prev, cpu_now) {
            (Some(prev), Some(now)) => cpu::busy_percent(prev, now),
            _ => None,
        };
        self.cpu_prev = cpu_now;

        let mut snap = Snapshot {
            cpu,
            gpu: self.gpu.as_ref().and_then(gpu::Gpu::utilization),
            mem: self
                .total
                .zip(mem::read_pages(self.host))
                .map(|(t, p)| mem::memory(t, self.page_size, p)),
            ..Snapshot::default()
        };
        if !detail {
            // Nobody sees the dropdown readings, so skip them; a stale net baseline would average across the gap.
            self.net_prev = None;
            return snap;
        }

        let at = Instant::now();
        let net_now = net::read();
        snap.net = match (&self.net_prev, &net_now) {
            (Some((prev, then)), Some(now)) => {
                net::rate(prev, now, at.duration_since(*then).as_secs_f64())
            }
            _ => None,
        };
        self.net_prev = net_now.map(|c| (c, at));
        snap.pressure = mem::read_pressure();
        snap.swap = mem::read_swap();
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kernel publishes idle CPU ticks up to ~1 s apart, so a shorter wait can see no elapsed ticks.
    const TICK_WAIT: std::time::Duration = std::time::Duration::from_millis(1100);

    #[test]
    fn detailed_sample_is_a_full_snapshot() {
        let mut sampler = Sampler::new();
        std::thread::sleep(TICK_WAIT);
        let s = sampler.sample(true);
        let cpu = s.cpu.expect("cpu");
        assert!((0.0..=100.0).contains(&cpu));
        assert!(s.gpu.is_some());
        assert!(s.mem.expect("mem").total > 0);
        assert!(s.pressure.is_some());
        assert!(s.swap.is_some());
        assert!(s.net.is_some());
    }

    #[test]
    fn menu_bar_sample_skips_dropdown_readings_and_drops_the_net_baseline() {
        let mut sampler = Sampler::new();
        std::thread::sleep(TICK_WAIT);
        let s = sampler.sample(false);
        assert!(s.cpu.is_some() && s.gpu.is_some() && s.mem.is_some());
        assert_eq!((s.pressure, s.swap, s.net), (None, None, None));
        // No baseline survived, so the first detailed sample has no rate rather than one across the gap.
        assert_eq!(sampler.sample(true).net, None);
        assert!(sampler.sample(true).net.is_some());
    }
}
