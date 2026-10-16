//! System readings for Wisp. Nothing here touches AppKit, so every reader is tested headless.

mod cpu;
mod gpu;
mod mem;
mod net;
mod sys;

pub use mem::{Memory, Pressure, Swap};
pub use net::Net;
