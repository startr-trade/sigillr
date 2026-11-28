//! One lookup, from the query to a decoded answer: pre-check, cache, request, status mapping,
//! the instance, mirror and commit checks, and the Stale rule.

use std::sync::{Arc, MutexGuard};
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;

use crate::Inner;
use crate::answer::{Answer, Lookup, NotFound, Source};
use crate::builder::{MAX_BODY, trim_slash};
use crate::cache::{Cache, CacheKey, Entry, Kind, Stored};
use crate::error::Error;
use crate::transport::HttpResponse;
use crate::wire::{IdentityLookup, KeyLookup, MirrorFreshness, Problem};

/// The longest query sent.
const MAX_QUERY: usize = 200;
/// The longest `Retry-After` an `index_behind` 503 is retried after.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(2);

/// Everything but the unreserved characters and `:` is percent-encoded in the path segment.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b':');

/// A 200 answer type: a key or an identity lookup.
pub(crate) trait Wire: DeserializeOwned + Clone {
    const KIND: Kind;
    const ROUTE: &'static str;
    const INVALID: &'static str;
    fn instance(&self) -> &str;
    fn registry_commit(&self) -> &str;
    fn cache_max_age(&self) -> u32;
    fn mirror(&self) -> Option<&MirrorFreshness>;
    fn store(self: Arc<Self>) -> Stored;
    fn load(s: &Stored) -> Option<Arc<Self>>;
}

impl Wire for KeyLookup {
    const KIND: Kind = Kind::Key;
    const ROUTE: &'static str = "keys";
    const INVALID: &'static str = "invalid_fingerprint";
    fn instance(&self) -> &str {
        &self.instance
    }
    fn registry_commit(&self) -> &str {
        &self.registry_commit
    }
    fn cache_max_age(&self) -> u32 {
        self.cache_max_age
    }
    fn mirror(&self) -> Option<&MirrorFreshness> {
        self.mirror.as_ref()
    }
    fn store(self: Arc<Self>) -> Stored {
        Stored::Key(self)
    }
    fn load(s: &Stored) -> Option<Arc<Self>> {
        match s {
            Stored::Key(k) => Some(Arc::clone(k)),
            _ => None,
        }
    }
}

impl Wire for IdentityLookup {
    const KIND: Kind = Kind::Identity;
    const ROUTE: &'static str = "identities";
    const INVALID: &'static str = "invalid_user_id";
    fn instance(&self) -> &str {
        &self.instance
    }
    fn registry_commit(&self) -> &str {
        &self.registry_commit
    }
    fn cache_max_age(&self) -> u32 {
        self.cache_max_age
    }
    fn mirror(&self) -> Option<&MirrorFreshness> {
        self.mirror.as_ref()
    }
    fn store(self: Arc<Self>) -> Stored {
        Stored::Identity(self)
    }
    fn load(s: &Stored) -> Option<Arc<Self>> {
        match s {
            Stored::Identity(i) => Some(Arc::clone(i)),
            _ => None,
        }
    }
}

/// A decoded success, before it is cached.
enum Fetched<T> {
    Found(Arc<T>),
    NotFound(NotFound),
}

/// An expired entry's facts, kept for the Stale rule.
struct Expired {
    age: Duration,
    lifetime: Duration,
    registry_commit: Option<String>,
}

fn lock(inner: &Inner) -> MutexGuard<'_, Cache> {
    // The cache holds only plain values, so a poisoned lock is safe to recover.
    inner.cache.lock().unwrap_or_else(|e| e.into_inner())
}

/// The local pre-check: trimmed, non-empty, at most 200 bytes of printable ASCII, and no
/// `/`, `?`, `#` or `%`.
fn pre_check<'q>(raw: &'q str, invalid: &str) -> Result<&'q str, Error> {
    let q = raw.trim();
    let why = if q.is_empty() {
        Some("the query is empty")
    } else if q.len() > MAX_QUERY {
        Some("the query is longer than 200 bytes")
    } else if !q.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        Some("the query is not printable ASCII")
    } else if q.contains(['/', '?', '#', '%']) {
        Some("the query contains '/', '?', '#' or '%'")
    } else {
        None
    };
    match why {
        Some(detail) => Err(Error::InvalidQuery {
            code: invalid.to_owned(),
            detail: detail.to_owned(),
        }),
        None => Ok(q),
    }
}

/// `(Kind::Key, q)` keeps its case after `SHA256:` and is lower-cased otherwise;
/// `(Kind::Identity, user_id)` is lower-cased.
fn cache_key(kind: Kind, q: &str) -> CacheKey {
    match kind {
        Kind::Key if q.starts_with("SHA256:") => (kind, q.to_owned()),
        _ => (kind, q.to_ascii_lowercase()),
    }
}

/// 40 or 64 lowercase hex digits.
fn is_commit_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `Retry-After` in seconds, if given in that form.
fn retry_after(resp: &HttpResponse) -> Option<Duration> {
    resp.header("retry-after")?
        .trim()
        .parse()
        .ok()
        .map(Duration::from_secs)
}

fn protocol(detail: impl Into<String>) -> Error {
    Error::Protocol {
        detail: detail.into(),
    }
}

fn problem(resp: &HttpResponse) -> Option<Problem> {
    serde_json::from_slice(&resp.body).ok()
}

/// Look `raw` up as a `T`: serve a fresh cached answer, or fetch, map and cache one.
pub(crate) fn lookup<T: Wire>(inner: &Inner, raw: &str) -> Result<Answer<T>, Error> {
    let q = pre_check(raw, T::INVALID)?;
    let key = cache_key(T::KIND, q);
    let clock = &inner.clock;

    // Step 2: the cache, never locked across I/O.
    let expired = {
        let cache = lock(inner);
        match cache.get(&key) {
            Some(e) if e.is_fresh(clock.monotonic()) => {
                let now = clock.monotonic();
                let result = match &e.outcome {
                    Stored::NotFound(nf) => Lookup::NotFound(nf.clone()),
                    s => match T::load(s) {
                        Some(t) => Lookup::Found((*t).clone()),
                        None => return Err(protocol("cache entry of the wrong kind")),
                    },
                };
                return Ok(Answer {
                    result,
                    source: e.source,
                    from_cache: true,
                    age: e.age(now),
                    lifetime: e.lifetime,
                });
            }
            Some(e) => Some(Expired {
                age: e.age(clock.monotonic()),
                lifetime: e.lifetime,
                registry_commit: e.outcome.registry_commit(),
            }),
            None => None,
        }
    };

    // Step 3: the request.
    let url = format!(
        "{}/v1/{}/{}",
        inner.cfg.base,
        T::ROUTE,
        utf8_percent_encode(q, SEGMENT)
    );
    match fetch::<T>(inner, &url) {
        // Step 4: success is cached when its lifetime is above 0.
        Ok((fetched, size, lifetime)) => {
            let source = inner.cfg.source;
            let (stored, result) = match fetched {
                Fetched::Found(t) => (Arc::clone(&t).store(), Lookup::Found((*t).clone())),
                Fetched::NotFound(nf) => (Stored::NotFound(nf.clone()), Lookup::NotFound(nf)),
            };
            if !lifetime.is_zero() {
                let received = clock.monotonic();
                let entry = Entry {
                    outcome: stored,
                    source,
                    received,
                    lifetime,
                    size,
                };
                lock(inner).insert(key, entry, received);
            }
            Ok(Answer {
                result,
                source,
                from_cache: false,
                age: Duration::ZERO,
                lifetime,
            })
        }
        // Step 5: "the registry could not answer" turns an expired entry into Stale.
        Err(e @ (Error::Unreachable { .. } | Error::Unavailable { .. })) => match expired {
            Some(x) => Err(Error::Stale {
                age: x.age,
                lifetime: x.lifetime,
                registry_commit: x.registry_commit,
                cause: Box::new(e),
            }),
            None => Err(e),
        },
        Err(e) => Err(e),
    }
}

/// GET `url` and map the response; one retry for an `index_behind` 503. Returns the decoded
/// success, the body's size and its lifetime.
fn fetch<T: Wire>(inner: &Inner, url: &str) -> Result<(Fetched<T>, usize, Duration), Error> {
    let start = inner.clock.monotonic();
    let total = inner.cfg.total_timeout;
    let mut retried = false;
    loop {
        let remaining = total.saturating_sub(inner.clock.monotonic().saturating_sub(start));
        if remaining.is_zero() {
            return Err(Error::Unreachable {
                cause: crate::transport::TransportError::new(
                    crate::transport::TransportErrorKind::Timeout,
                    "the total timeout passed",
                ),
            });
        }
        let resp = inner
            .transport
            .get(url, remaining, MAX_BODY)
            .map_err(|cause| match cause.kind {
                crate::transport::TransportErrorKind::ServerKeyNotPinned => {
                    Error::ServerKeyNotPinned
                }
                _ => Error::Unreachable { cause },
            })?;
        if resp.status == 503 {
            let Some(p) = problem(&resp) else {
                return Err(protocol("503 without a problem body"));
            };
            let wait = retry_after(&resp);
            let elapsed = inner.clock.monotonic().saturating_sub(start);
            if !retried
                && p.code == "index_behind"
                && let Some(w) = wait
                && w <= MAX_RETRY_AFTER
                && elapsed.saturating_add(w) < total
            {
                retried = true;
                std::thread::sleep(w);
                continue;
            }
            return Err(Error::Unavailable {
                code: p.code,
                retry_after: wait,
            });
        }
        return map::<T>(inner, &resp);
    }
}

/// Map a non-503 response.
fn map<T: Wire>(
    inner: &Inner,
    resp: &HttpResponse,
) -> Result<(Fetched<T>, usize, Duration), Error> {
    let cfg = &inner.cfg;
    let cap = |secs: u32| Duration::from_secs(u64::from(secs)).min(cfg.max_lifetime);
    let check_instance = |got: &str| {
        if trim_slash(got) == cfg.instance {
            Ok(())
        } else {
            Err(Error::WrongInstance {
                expected: cfg.instance.clone(),
                got: got.to_owned(),
            })
        }
    };
    match resp.status {
        200 => {
            let t: T = serde_json::from_slice(&resp.body)
                .map_err(|e| protocol(format!("the answer does not decode: {e}")))?;
            check_instance(t.instance())?;
            if cfg.source == Source::Mirror {
                let Some(m) = t.mirror() else {
                    return Err(Error::NotAMirror {
                        url: cfg.base.clone(),
                    });
                };
                if m.stale && !cfg.accept_stale_mirror {
                    return Err(Error::MirrorStale {
                        synced_at: m.synced_at,
                    });
                }
            }
            if !is_commit_id(t.registry_commit()) {
                return Err(protocol(format!(
                    "registry_commit {:?} is not 40 or 64 lowercase hex",
                    t.registry_commit()
                )));
            }
            let lifetime = cap(t.cache_max_age());
            Ok((Fetched::Found(Arc::new(t)), resp.body.len(), lifetime))
        }
        404 => match problem(resp) {
            Some(p) if p.code == "key_not_found" || p.code == "identity_not_found" => {
                let server = p.cache_max_age.unwrap_or(0);
                let nf = NotFound {
                    code: p.code,
                    registry_commit: p.registry_commit,
                    cache_max_age: server,
                };
                Ok((Fetched::NotFound(nf), resp.body.len(), cap(server)))
            }
            _ => Err(protocol("404 without a not-found problem body")),
        },
        400 => match problem(resp) {
            Some(p) => Err(Error::InvalidQuery {
                code: p.code,
                detail: p.detail.unwrap_or_default(),
            }),
            None => Err(protocol("400 without a problem body")),
        },
        s => Err(protocol(format!("unexpected status {s}"))),
    }
}
