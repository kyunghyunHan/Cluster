#![allow(dead_code)]
use std::time::{Duration, Instant};

/// Tracks when the circuit was last autosaved and whether a save is due.
pub(crate) struct AutosaveTimer {
    last_save: Instant,
    interval: Duration,
}

impl AutosaveTimer {
    pub(crate) fn new(interval_secs: u64) -> Self {
        Self {
            last_save: Instant::now(),
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// Returns true if the timer has elapsed and resets it.
    pub(crate) fn should_save(&mut self) -> bool {
        if self.last_save.elapsed() >= self.interval {
            self.last_save = Instant::now();
            true
        } else {
            false
        }
    }

    /// Reset the timer (call after a manual save).
    pub(crate) fn reset(&mut self) {
        self.last_save = Instant::now();
    }
}
