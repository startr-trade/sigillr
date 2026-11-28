//! Time sources: a monotonic clock for cache ages and a wall clock for `status_at`.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use time::OffsetDateTime;

/// The client's time source. Tests inject a fake one with [`crate::ClientBuilder::clock`].
pub trait Clock: Send + Sync {
    /// Monotonic time since an arbitrary fixed start. Cache ages are counted on it, so
    /// wall-clock skew cannot stretch a cached answer's life.
    fn monotonic(&self) -> Duration;
    /// The current wall-clock time in UTC.
    fn now_utc(&self) -> OffsetDateTime;
}

/// The system clock: monotonic from a process-start `Instant`, wall time from
/// `OffsetDateTime::now_utc`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

impl Clock for SystemClock {
    fn monotonic(&self) -> Duration {
        start().elapsed()
    }

    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}
