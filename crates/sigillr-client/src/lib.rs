//! Look up a key fingerprint or a user_id in a Sigillr identity registry, online or from a
//! mirror, with a bounded in-memory cache.
//!
//! # Semver promise
//!
//! **Covered:**
//! - every public item of this crate;
//! - the behaviour: never an answer past its lifetime, never a stale answer on failure, never
//!   the online service when a mirror is set, never an answer from another instance, and
//!   unknown statuses never valid;
//! - the `/v1` wire contract the client reads.
//!
//! **Not covered:** the wording of `Display`, `Debug` output, and the versions of `ureq` and
//! `rustls`, which do not appear in the public API.
//!
//! **Compatibility rules:**
//! - Structs and enums are `#[non_exhaustive]`, so a new wire field or status is a minor
//!   release.
//! - Removing or renaming a public item, or reading `/v2`, is a breaking release. Before 1.0
//!   that means `0.x` to `0.(x+1)`, and the CHANGELOG names it.
//! - Depend on `sigillr-client = "0.x"` to get every compatible fix.
//!
//! # Forward compatibility of the wire types
//!
//! A field the server adds is ignored until a client release adds it. A field that is absent
//! or `null` decodes as `None` where the type is an `Option`. An enum value the client does
//! not know decodes as `Other`, and every `Other` status counts as not valid.
//!
//! # Example
//!
//! Pin the instance you trust and read from a mirror of it. Here the `testing` fakes stand in
//! for the network; a real program leaves out `transport` and `clock`.
//!
//! ```
//! # #[cfg(feature = "testing")] {
//! use sigillr_client::testing::{FakeClock, FakeTransport};
//! use sigillr_client::{Client, HttpResponse, Source};
//!
//! let mirror = FakeTransport::new();
//! mirror.push(Ok(HttpResponse::new(
//!     200,
//!     vec![],
//!     include_bytes!("../tests/fixtures/mirror_key.json").to_vec(),
//! )));
//! let client = Client::builder("https://sigillr.provr.dev")
//!     .mirror("https://sigillr-mirror.example.org")
//!     .accept_stale_mirror(false)
//!     .transport(mirror.clone())
//!     .clock(FakeClock::new())
//!     .build()
//!     .expect("a valid configuration");
//!
//! let answer = client
//!     .key("gpg:0123456789abcdef0123456789abcdef01234567")
//!     .expect("an answer");
//! assert_eq!(answer.source, Source::Mirror);
//! let key = answer.found().expect("registered");
//! assert_eq!(key.user_id, "alice");
//! assert!(key.is_valid());
//! assert!(mirror.urls()[0].starts_with("https://sigillr-mirror.example.org/v1/keys/"));
//! # }
//! ```
//!
//! # Supported targets
//!
//! Linux, macOS and Windows on x86_64 and aarch64 (the TLS provider, graviola, builds on
//! those two architectures only).

use std::sync::{Arc, Mutex};

pub mod answer;
mod builder;
mod cache;
pub mod clock;
mod error;
mod fetch;
mod pin;
#[cfg(feature = "testing")]
pub mod testing;
pub mod transport;
pub mod wire;

pub use answer::{Answer, Lookup, NotFound, Source};
pub use builder::{ClientBuilder, ConfigError};
pub use clock::{Clock, SystemClock};
pub use error::Error;
pub use transport::{HttpResponse, HttpTransport, Transport, TransportError, TransportErrorKind};
pub use wire::{
    Identity, IdentityLookup, IdentityStatus, Issuer, KeyEntry, KeyLookup, KeyStatus, KeyType,
    KeyUsage, MatchedVia, MirrorFreshness, Problem, Proofing, Revoked, RevokedBy,
};

/// A lookup client pinned to one instance. Cheap to clone; clones share one cache.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("instance", &self.inner.cfg.instance)
            .field("base", &self.inner.cfg.base)
            .field("source", &self.inner.cfg.source)
            .finish_non_exhaustive()
    }
}

struct Inner {
    cfg: builder::Config,
    cache: Mutex<cache::Cache>,
    transport: Arc<dyn Transport>,
    clock: Arc<dyn Clock>,
}

impl Client {
    /// `instance` is the origin of the instance the caller trusts, e.g.
    /// `"https://sigillr.provr.dev"`. There is no default: a verifier pins its instance.
    pub fn builder(instance: &str) -> ClientBuilder {
        ClientBuilder::new(instance)
    }

    pub(crate) fn from_parts(
        cfg: builder::Config,
        transport: Arc<dyn Transport>,
        clock: Arc<dyn Clock>,
    ) -> Client {
        Client {
            inner: Arc::new(Inner {
                cache: Mutex::new(cache::Cache::new(cfg.max_cache_bytes)),
                cfg,
                transport,
                clock,
            }),
        }
    }

    /// Look up a key fingerprint in any form the server accepts. Serves a fresh cached
    /// answer with no request (R-2). Never returns an answer older than its lifetime (R-3).
    pub fn key(&self, fingerprint: &str) -> Result<Answer<KeyLookup>, Error> {
        fetch::lookup(&self.inner, fingerprint)
    }

    /// Look up a user_id (folded to lower case for the cache key; the server folds it too).
    pub fn identity(&self, user_id: &str) -> Result<Answer<IdentityLookup>, Error> {
        fetch::lookup(&self.inner, user_id)
    }

    /// Drop every cached answer.
    pub fn clear_cache(&self) {
        self.inner
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}
