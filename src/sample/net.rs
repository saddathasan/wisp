//! Network throughput from the kernel's 64-bit per-interface byte counters (`NET_RT_IFLIST2`).
//! `getifaddrs` is not used: its counters are 32-bit and wrap within minutes on a fast link.

use std::mem::size_of;
use std::ptr;

/// One interface's cumulative byte counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Iface {
    pub index: u16,
    pub rx: u64,
    pub tx: u64,
}

/// Counters for every interface that is up and not loopback.
pub type Counters = Vec<Iface>;

/// Throughput in bytes per second.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Net {
    pub down: f64,
    pub up: f64,
}

/// Throughput between two readings, counting only interfaces present in both.
/// An interface that just appeared (VPN, Wi-Fi rejoin) carries its whole history in its counters and would show as a spike.
pub fn rate(prev: &Counters, now: &Counters, secs: f64) -> Option<Net> {
    if secs <= 0.0 {
        return None;
    }
    let (mut down, mut up) = (0u64, 0u64);
    for n in now {
        if let Some(p) = prev.iter().find(|p| p.index == n.index) {
            // A counter that went backwards means the interface reset; count 0 rather than a wrap-sized spike.
            down += n.rx.saturating_sub(p.rx);
            up += n.tx.saturating_sub(p.tx);
        }
    }
    Some(Net {
        down: down as f64 / secs,
        up: up as f64 / secs,
    })
}

/// Reads the interface list with `sysctl(CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, 0)`.
pub fn read() -> Option<Counters> {
    let mut mib = [libc::CTL_NET, libc::PF_ROUTE, 0, 0, libc::NET_RT_IFLIST2, 0];
    let mut len = 0usize;
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            6,
            ptr::null_mut(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    // Headroom: an interface appearing between the two calls would otherwise fail the second with ENOMEM.
    let mut buf = vec![0u8; len + len / 8];
    len = buf.len();
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            6,
            buf.as_mut_ptr().cast(),
            &mut len,
            ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then(|| parse(&buf[..len]))
}

/// Walks the routing messages and keeps RTM_IFINFO2 entries for interfaces that are up and not loopback.
fn parse(buf: &[u8]) -> Counters {
    let mut out = Vec::new();
    let mut off = 0;
    // Every routing message starts with u16 length, u8 version, u8 type.
    while off + 4 <= buf.len() {
        let msglen = u16::from_ne_bytes([buf[off], buf[off + 1]]) as usize;
        if msglen == 0 {
            break;
        }
        let kind = buf[off + 3] as i32;
        let whole = msglen >= size_of::<libc::if_msghdr2>()
            && off + size_of::<libc::if_msghdr2>() <= buf.len();
        if kind == libc::RTM_IFINFO2 && whole {
            // The buffer has no alignment guarantee for this packed struct, so read it unaligned.
            let m: libc::if_msghdr2 = unsafe { ptr::read_unaligned(buf[off..].as_ptr().cast()) };
            let flags = m.ifm_flags;
            if flags & libc::IFF_UP != 0 && flags & libc::IFF_LOOPBACK == 0 {
                out.push(Iface {
                    index: m.ifm_index,
                    rx: m.ifm_data.ifi_ibytes,
                    tx: m.ifm_data.ifi_obytes,
                });
            }
        }
        off += msglen;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    fn iface(index: u16, rx: u64, tx: u64) -> Iface {
        Iface { index, rx, tx }
    }

    /// Serialises one RTM_IFINFO2 message the way the kernel lays it out.
    fn msg(index: u16, flags: i32, rx: u64, tx: u64) -> Vec<u8> {
        let mut m: libc::if_msghdr2 = unsafe { std::mem::zeroed() };
        m.ifm_msglen = size_of::<libc::if_msghdr2>() as u16;
        m.ifm_type = libc::RTM_IFINFO2 as u8;
        m.ifm_index = index;
        m.ifm_flags = flags;
        m.ifm_data.ifi_ibytes = rx;
        m.ifm_data.ifi_obytes = tx;
        let bytes = unsafe {
            std::slice::from_raw_parts((&raw const m).cast::<u8>(), size_of::<libc::if_msghdr2>())
        };
        bytes.to_vec()
    }

    #[test]
    fn rate_is_delta_over_time() {
        let prev = vec![iface(4, 1_000, 500)];
        let now = vec![iface(4, 3_000, 1_500)];
        assert_eq!(
            rate(&prev, &now, 2.0),
            Some(Net {
                down: 1_000.0,
                up: 500.0
            })
        );
    }

    #[test]
    fn counter_going_backwards_counts_as_zero() {
        let prev = vec![iface(4, 5_000, 5_000)];
        let now = vec![iface(4, 100, 6_000)];
        assert_eq!(
            rate(&prev, &now, 1.0),
            Some(Net {
                down: 0.0,
                up: 1_000.0
            })
        );
    }

    #[test]
    fn interface_that_just_appeared_adds_nothing() {
        let prev = vec![iface(4, 1_000, 1_000)];
        let now = vec![
            iface(4, 2_000, 1_000),
            iface(9, 9_000_000_000, 9_000_000_000),
        ];
        assert_eq!(
            rate(&prev, &now, 1.0),
            Some(Net {
                down: 1_000.0,
                up: 0.0
            })
        );
    }

    #[test]
    fn zero_elapsed_time_is_none() {
        assert_eq!(rate(&Vec::new(), &Vec::new(), 0.0), None);
    }

    #[test]
    fn parse_keeps_up_non_loopback_interfaces_and_skips_other_messages() {
        let mut buf = msg(4, libc::IFF_UP, 100, 50);
        buf.extend(msg(1, libc::IFF_UP | libc::IFF_LOOPBACK, 999, 999));
        buf.extend(msg(7, 0, 888, 888)); // down
        buf.extend([
            8,
            0,
            libc::RTM_VERSION as u8,
            libc::RTM_NEWADDR as u8,
            0,
            0,
            0,
            0,
        ]); // unrelated 8-byte message
        buf.extend(msg(5, libc::IFF_UP, 10, 20));
        assert_eq!(parse(&buf), vec![iface(4, 100, 50), iface(5, 10, 20)]);
    }

    #[test]
    fn parse_stops_on_zero_length_message() {
        assert_eq!(parse(&[0, 0, 0, 0, 0, 0, 0, 0]), vec![]);
    }

    #[test]
    fn parse_skips_an_ifinfo2_message_shorter_than_the_struct() {
        let mut buf = msg(4, libc::IFF_UP, 100, 50);
        buf[0..2].copy_from_slice(&8u16.to_ne_bytes()); // claims 8 bytes: not a whole if_msghdr2
        buf.truncate(8);
        buf.extend(msg(5, libc::IFF_UP, 10, 20));
        assert_eq!(parse(&buf), vec![iface(5, 10, 20)]);
    }

    #[test]
    fn live_reading_succeeds() {
        assert!(read().is_some());
    }
}
