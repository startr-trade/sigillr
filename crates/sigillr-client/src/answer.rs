//! What a lookup returns: the decoded answer and where and when it came from.

use std::time::Duration;

use time::OffsetDateTime;

use crate::wire::{IdentityStatus, KeyEntry, KeyLookup, KeyStatus, Revoked};

/// A lookup's answer, with its provenance and cache state.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct Answer<T> {
    /// Found, or a registry-signed "not found".
    pub result: Lookup<T>,
    /// The online service or a mirror.
    pub source: Source,
    /// True when served from the client's cache without a request.
    pub from_cache: bool,
    /// Time since the client received the answer (monotonic clock).
    pub age: Duration,
    /// How long the answer may be served: the smaller of the server's `cache_max_age` and
    /// the caller's `max_lifetime`.
    pub lifetime: Duration,
}

impl<T> Answer<T> {
    /// The found value, if any.
    pub fn found(&self) -> Option<&T> {
        match &self.result {
            Lookup::Found(t) => Some(t),
            Lookup::NotFound(_) => None,
        }
    }

    /// The found value, if any, by value.
    pub fn into_found(self) -> Option<T> {
        match self.result {
            Lookup::Found(t) => Some(t),
            Lookup::NotFound(_) => None,
        }
    }
}

/// Found, or not found.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup<T> {
    /// The registry holds the key or identity.
    Found(T),
    /// The registry does not hold it, at `registry_commit`.
    NotFound(NotFound),
}

/// A "not found" answer (a 404 with `key_not_found` or `identity_not_found`).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    /// `key_not_found` or `identity_not_found`.
    pub code: String,
    /// The registry commit the answer was read at, when the server gave it.
    pub registry_commit: Option<String>,
    /// How many seconds the answer may be cached.
    pub cache_max_age: u32,
}

/// Where an answer came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The online service.
    Online,
    /// A mirror.
    Mirror,
}

/// FEAT-021's precedence: revoked from `revoked.at` on, then expired from `not_after` on,
/// then not yet valid before `not_before`. `Unknown` and `Other` are returned unchanged.
fn status_at(
    current: KeyStatus,
    not_before: OffsetDateTime,
    not_after: Option<OffsetDateTime>,
    revoked: Option<&Revoked>,
    t: OffsetDateTime,
) -> KeyStatus {
    match current {
        KeyStatus::Unknown | KeyStatus::Other => return current,
        _ => {}
    }
    if revoked.is_some_and(|r| r.at <= t) {
        KeyStatus::Revoked
    } else if not_after.is_some_and(|na| na <= t) {
        KeyStatus::Expired
    } else if t < not_before {
        KeyStatus::NotYetValid
    } else {
        KeyStatus::Active
    }
}

impl KeyLookup {
    /// The status at `t`, recomputed from `not_before`, `not_after` and `revoked` with
    /// FEAT-021's precedence (revoked from `revoked.at` on, then expired from `not_after` on,
    /// then not yet valid before `not_before`). Returns `self.status` unchanged when it is
    /// `Unknown` or `Other`.
    pub fn status_at(&self, t: OffsetDateTime) -> KeyStatus {
        status_at(
            self.status,
            self.not_before,
            self.not_after,
            self.revoked.as_ref(),
            t,
        )
    }

    /// `status` is `Active` and the identity is `Active` or `Disabling`.
    pub fn is_valid(&self) -> bool {
        self.status == KeyStatus::Active
            && matches!(
                self.identity.status,
                IdentityStatus::Active | IdentityStatus::Disabling
            )
    }
}

impl KeyEntry {
    /// The status at `t`, by the same rule as [`KeyLookup::status_at`].
    pub fn status_at(&self, t: OffsetDateTime) -> KeyStatus {
        status_at(
            self.status,
            self.not_before,
            self.not_after,
            self.revoked.as_ref(),
            t,
        )
    }
}
