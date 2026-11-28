//! The bounded in-memory cache. It is never written to disk.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::answer::{NotFound, Source};
use crate::wire::{IdentityLookup, KeyLookup};

/// Which route an entry answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Key,
    Identity,
}

/// The cache key: the route and the folded query.
pub(crate) type CacheKey = (Kind, String);

/// What an entry holds.
#[derive(Debug, Clone)]
pub(crate) enum Stored {
    Key(Arc<KeyLookup>),
    Identity(Arc<IdentityLookup>),
    NotFound(NotFound),
}

impl Stored {
    pub(crate) fn registry_commit(&self) -> Option<String> {
        match self {
            Stored::Key(k) => Some(k.registry_commit.clone()),
            Stored::Identity(i) => Some(i.registry_commit.clone()),
            Stored::NotFound(n) => n.registry_commit.clone(),
        }
    }
}

/// One cached answer.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) outcome: Stored,
    pub(crate) source: Source,
    /// `Clock::monotonic` at receipt.
    pub(crate) received: Duration,
    /// `min(cache_max_age, max_lifetime)`.
    pub(crate) lifetime: Duration,
    /// The body's byte length.
    pub(crate) size: usize,
}

impl Entry {
    /// The entry's age at `now` (monotonic).
    pub(crate) fn age(&self, now: Duration) -> Duration {
        now.saturating_sub(self.received)
    }

    /// True while the entry may be served.
    pub(crate) fn is_fresh(&self, now: Duration) -> bool {
        self.age(now) < self.lifetime
    }
}

/// Entries bounded by the sum of their body sizes.
#[derive(Debug)]
pub(crate) struct Cache {
    map: HashMap<CacheKey, Entry>,
    total: usize,
    max: usize,
}

impl Cache {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Cache {
            map: HashMap::new(),
            total: 0,
            max: max_bytes,
        }
    }

    pub(crate) fn get(&self, key: &CacheKey) -> Option<&Entry> {
        self.map.get(key)
    }

    fn remove(&mut self, key: &CacheKey) {
        if let Some(e) = self.map.remove(key) {
            self.total -= e.size;
        }
    }

    /// Store `entry` under `key`, evicting until it fits: expired entries first, oldest
    /// `received` first, then the entry with the earliest `received + lifetime`. An entry
    /// larger than the whole budget, or with a zero lifetime, is not stored.
    pub(crate) fn insert(&mut self, key: CacheKey, entry: Entry, now: Duration) {
        self.remove(&key);
        if entry.lifetime.is_zero() || entry.size > self.max {
            return;
        }
        while self.total + entry.size > self.max {
            let victim = self
                .map
                .iter()
                .filter(|(_, e)| !e.is_fresh(now))
                .min_by_key(|(_, e)| e.received)
                .or_else(|| {
                    self.map
                        .iter()
                        .min_by_key(|(_, e)| e.received.saturating_add(e.lifetime))
                })
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => self.remove(&k),
                None => break,
            }
        }
        self.total += entry.size;
        self.map.insert(key, entry);
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.total = 0;
    }
}
