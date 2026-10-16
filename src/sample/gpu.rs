//! GPU utilisation from the IOAccelerator's "PerformanceStatistics" registry property.
//! The service is matched once at startup; each tick copies one property and nothing else.

use std::ffi::{c_char, c_void};
use std::ptr;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString, ns_string};

type IoObject = u32;
const IO_MAIN_PORT_DEFAULT: u32 = 0;
// Keys go through ns_string!, which caches each NSString in a static on first use: none is allocated per tick.
const PERF_STATS: &str = "PerformanceStatistics";
const UTILIZATION: &str = "Device Utilization %";

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> IoObject;
    fn IORegistryEntryCreateCFProperty(
        entry: IoObject,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *mut c_void;
    fn IOObjectRelease(object: IoObject) -> i32;
}

/// A handle on the GPU's registry entry; the IOKit reference is released on drop.
pub struct Gpu(IoObject);

impl Gpu {
    /// Finds the first IOAccelerator (the integrated GPU on Apple Silicon); `None` if there is none.
    pub fn open() -> Option<Self> {
        // IOServiceGetMatchingService consumes the matching dictionary, so it is not released here.
        let service = unsafe {
            IOServiceGetMatchingService(
                IO_MAIN_PORT_DEFAULT,
                IOServiceMatching(c"IOAccelerator".as_ptr()),
            )
        };
        (service != 0).then_some(Self(service))
    }

    /// The GPU's current "Device Utilization %", 0–100.
    pub fn utilization(&self) -> Option<f64> {
        self.property_f64(ns_string!(PERF_STATS), ns_string!(UTILIZATION))
    }

    /// Reads `dict_key` from the dictionary-valued registry property `property` as a number.
    fn property_f64(&self, property: &NSString, dict_key: &NSString) -> Option<f64> {
        let raw = unsafe {
            IORegistryEntryCreateCFProperty(
                self.0,
                (property as *const NSString).cast(),
                ptr::null(),
                0,
            )
        };
        // CF types are toll-free bridged to their NS counterparts, and the Create rule hands us one reference to own.
        let value: Retained<AnyObject> = unsafe { Retained::from_raw(raw.cast()) }?;
        // Check the class before treating it as a dictionary: a wrong guess would raise an ObjC exception and abort.
        let stats = value.downcast::<NSDictionary>().ok()?;
        let number = stats.objectForKey(dict_key)?.downcast::<NSNumber>().ok()?;
        Some(number.doubleValue())
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_gpu_reports_a_percentage() {
        let gpu = Gpu::open().expect("an IOAccelerator should exist on Apple Silicon");
        let pct = gpu
            .utilization()
            .expect("PerformanceStatistics should carry Device Utilization %");
        assert!((0.0..=100.0).contains(&pct));
    }

    #[test]
    fn missing_property_is_none_not_a_panic() {
        let gpu = Gpu::open().unwrap();
        assert_eq!(
            gpu.property_f64(ns_string!(PERF_STATS), ns_string!("No Such Key")),
            None
        );
        assert_eq!(
            gpu.property_f64(ns_string!("NoSuchProperty"), ns_string!(UTILIZATION)),
            None
        );
    }

    #[test]
    fn non_dictionary_property_is_none_not_a_crash() {
        // IOClass is a string-valued property on every registry entry.
        let gpu = Gpu::open().unwrap();
        assert_eq!(
            gpu.property_f64(ns_string!("IOClass"), ns_string!(UTILIZATION)),
            None
        );
    }
}
