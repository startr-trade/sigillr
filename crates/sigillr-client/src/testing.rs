//! Test fakes for programs that use the client (feature `testing`): a transport that plays back
//! canned answers and a clock that moves only when told, so a dependent crate can test its
//! lookups with no network. They are the fakes this crate's own tests use.
//!
//! ```
//! use std::time::Duration;
//! use sigillr_client::testing::{FakeClock, FakeTransport};
//! use sigillr_client::{Client, HttpResponse};
//!
//! let transport = FakeTransport::new();
//! let clock = FakeClock::new();
//! transport.push(Ok(HttpResponse::new(404, vec![], br#"{"status":404,"code":"key_not_found"}"#.to_vec())));
//! let client = Client::builder("https://sigillr.example")
//!     .transport(transport.clone())
//!     .clock(clock.clone())
//!     .build()
//!     .unwrap();
//! assert!(client.key(&format!("ssh:{}", "ab".repeat(32))).unwrap().found().is_none());
//! assert_eq!(transport.calls(), 1);
//! clock.advance(Duration::from_secs(1));
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use time::OffsetDateTime;

use crate::clock::Clock;
use crate::transport::{HttpResponse, Transport, TransportError};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A transport that records every URL and plays back canned results in order, repeating the
/// last one once the queue is empty. A request before any result was pushed panics.
#[derive(Default)]
pub struct FakeTransport {
    calls: Mutex<Vec<String>>,
    replies: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
    last: Mutex<Option<Result<HttpResponse, TransportError>>>,
}

impl FakeTransport {
    /// An empty fake, ready to hand to [`ClientBuilder::transport`](crate::ClientBuilder::transport).
    pub fn new() -> Arc<Self> {
        Arc::new(FakeTransport::default())
    }

    /// Queue the result of the next request.
    pub fn push(&self, r: Result<HttpResponse, TransportError>) {
        lock(&self.replies).push_back(r);
    }

    /// How many requests were made.
    pub fn calls(&self) -> usize {
        lock(&self.calls).len()
    }

    /// The URLs requested, in order.
    pub fn urls(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }
}

impl std::fmt::Debug for FakeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeTransport")
            .field("calls", &self.calls())
            .finish_non_exhaustive()
    }
}

impl Transport for FakeTransport {
    fn get(
        &self,
        url: &str,
        _timeout: Duration,
        _max_body: usize,
    ) -> Result<HttpResponse, TransportError> {
        lock(&self.calls).push(url.to_owned());
        let next = lock(&self.replies).pop_front();
        let mut last = lock(&self.last);
        if let Some(r) = next {
            *last = Some(r);
        }
        last.clone()
            .expect("FakeTransport: a request came before any reply was pushed")
    }
}

/// A clock that moves only when told. Both readings start at 1000 s: the monotonic one as a
/// duration, the wall clock as 1000 s after the Unix epoch, and [`advance`](Self::advance)
/// moves both.
#[derive(Debug)]
pub struct FakeClock {
    now: Mutex<Duration>,
}

impl FakeClock {
    /// A clock at 1000 s.
    pub fn new() -> Arc<Self> {
        Arc::new(FakeClock {
            now: Mutex::new(Duration::from_secs(1000)),
        })
    }

    /// Move the clock forward by `d`.
    pub fn advance(&self, d: Duration) {
        *lock(&self.now) += d;
    }
}

impl Clock for FakeClock {
    fn monotonic(&self) -> Duration {
        *lock(&self.now)
    }

    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + *lock(&self.now)
    }
}
