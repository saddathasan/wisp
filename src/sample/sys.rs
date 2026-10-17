//! Thin safe wrappers over the raw sysctl and Mach calls the readers share.
// libc marks mach_host_self deprecated in favour of mach2; it is a stable Mach call.
#![allow(deprecated)]

use std::ffi::CStr;
use std::mem::{MaybeUninit, size_of};
use std::ptr;

/// Reads a fixed-size sysctl value by name; `None` if the call fails or the size doesn't match `T`.
pub fn by_name<T: Copy>(name: &CStr) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut len = size_of::<T>();
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && len == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

/// The host port. Every `mach_host_self` call adds a port reference, so fetch it once and keep it.
pub fn host() -> libc::mach_port_t {
    unsafe { libc::mach_host_self() }
}

/// The VM page size in bytes (16 KB on Apple Silicon).
pub fn page_size() -> u64 {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as u64 }
}
