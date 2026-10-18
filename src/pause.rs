//! Why sampling is paused (spec §3.3, S3): display sleep, system sleep and screen lock come and go independently,
//! so each is tracked on its own and sampling resumes only once none is left.

/// One cause of a pause, set and cleared by its own pair of notifications.
#[derive(Clone, Copy, Debug)]
pub enum Reason {
    DisplayAsleep,
    SystemAsleep,
    Locked,
}

/// The set of reasons currently holding sampling paused; empty means running.
#[derive(Default)]
pub struct Paused(u8);

impl Paused {
    /// Adds `reason`; repeating one already set changes nothing.
    pub fn pause(&mut self, reason: Reason) {
        self.0 |= 1 << reason as u8;
    }

    /// Clears `reason`; true only when it was the last one left, i.e. sampling should resume now.
    pub fn resume(&mut self, reason: Reason) -> bool {
        let was = self.0;
        self.0 &= !(1 << reason as u8);
        was != 0 && self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Reason::*;
    use super::*;

    #[test]
    fn stays_paused_until_the_last_reason_clears() {
        let mut p = Paused::default();
        p.pause(Locked);
        p.pause(DisplayAsleep);
        assert!(!p.resume(DisplayAsleep), "display wake while still locked");
        assert!(p.resume(Locked));
    }

    #[test]
    fn a_reason_never_set_does_not_resume() {
        let mut p = Paused::default();
        assert!(
            !p.resume(SystemAsleep),
            "a wake while running is not a resume"
        );
        p.pause(Locked);
        assert!(!p.resume(SystemAsleep));
        assert!(p.resume(Locked));
    }

    #[test]
    fn duplicate_notifications_are_harmless() {
        let mut p = Paused::default();
        p.pause(Locked);
        p.pause(Locked);
        assert!(p.resume(Locked));
        assert!(!p.resume(Locked), "a second unlock while running");
    }
}
