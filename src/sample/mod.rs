//! System readings for Wisp. Nothing here touches AppKit, so every reader is tested headless.

mod cpu;
mod mem;
mod sys;

pub use mem::{Memory, Pressure, Swap};
