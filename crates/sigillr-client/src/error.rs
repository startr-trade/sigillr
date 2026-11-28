//! [`Error`]: every way a lookup can fail. An error never carries an answer.

use std::time::Duration;

use time::OffsetDateTime;

use crate::transport::TransportError;

/// Why a lookup returned no answer.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The query failed the local pre-check, or the server answered 400.
    #[error("invalid query ({code}): {detail}")]
    InvalidQuery {
        /// `invalid_fingerprint`, `invalid_user_id`, or the server's code.
        code: String,
        /// What is wrong, for people.
        detail: String,
    },
    /// No response, and no cached answer to speak of.
    #[error("the registry could not be reached ({cause})")]
    Unreachable {
        /// The transport's failure.
        cause: TransportError,
    },
    /// The server answered 503, and there is no cached answer to speak of.
    #[error("the registry is unavailable ({code}){}", retry_text(.retry_after))]
    Unavailable {
        /// The problem's code, e.g. `index_unavailable`.
        code: String,
        /// The server's `Retry-After`, when given.
        retry_after: Option<Duration>,
    },
    /// The registry could not answer and the cached answer is past its lifetime (R-3). The
    /// answer itself is never returned.
    #[error("{} and the cached answer is {} s old, past its {} s lifetime",
        stale_reason(.cause), .age.as_secs(), .lifetime.as_secs())]
    Stale {
        /// The cached answer's age.
        age: Duration,
        /// Its lifetime.
        lifetime: Duration,
        /// The commit it was read at, when it named one.
        registry_commit: Option<String>,
        /// Why the registry could not answer: `Unreachable` or `Unavailable`.
        cause: Box<Error>,
    },
    /// The answer names another instance than the pinned one.
    #[error("the answer is from instance {got:?}, not the pinned instance {expected:?}")]
    WrongInstance {
        /// The pinned instance origin.
        expected: String,
        /// The answer's `instance`.
        got: String,
    },
    /// Mirror mode, but the answer has no `mirror` block.
    #[error("{url} answered without a mirror block, so it is not a mirror")]
    NotAMirror {
        /// The mirror's base URL.
        url: String,
    },
    /// The mirror is stale and `accept_stale_mirror(false)` was set.
    #[error("the mirror is stale: it last synced at {synced_at}")]
    MirrorStale {
        /// When the mirror last synced.
        synced_at: OffsetDateTime,
    },
    /// Server keys are pinned and the server's TLS key is not one of them (R-5). No lookup was
    /// sent, and there is no fallback.
    #[error("the server's TLS key is not one of the pinned keys")]
    ServerKeyNotPinned,
    /// Bad JSON, a bad commit id, or an unexpected status.
    #[error("protocol error: {detail}")]
    Protocol {
        /// What was wrong.
        detail: String,
    },
}

fn retry_text(retry_after: &Option<Duration>) -> String {
    match retry_after {
        Some(d) => format!(", retry after {} s", d.as_secs()),
        None => String::new(),
    }
}

fn stale_reason(cause: &Error) -> String {
    match cause {
        Error::Unreachable { cause } => format!("the registry could not be reached ({cause})"),
        Error::Unavailable { code, .. } => format!("the registry could not answer ({code})"),
        other => format!("the registry could not answer ({other})"),
    }
}
