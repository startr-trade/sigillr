//! The serde mirror of the `/v1` lookup wire types (FEAT-021, FEAT-041, FTR-002).
//!
//! Forward compatibility: a field the server adds is ignored (no `deny_unknown_fields`), a
//! field that is absent or `null` decodes as `None` where the type is an `Option`, and an
//! enum value the client does not know decodes as `Other`. Every `Other` status is not valid.

use std::fmt;

use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;

/// The answer to `GET /v1/keys/{fingerprint}`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyLookup {
    /// `sha256:<64 hex>` (X.509), `ssh:<hex>` or `gpg:<hex>`.
    pub fingerprint: String,
    /// x509, ssh or openpgp.
    pub key_type: KeyType,
    /// The key algorithm, e.g. `ssh-ed25519`; `None` from a server without it.
    #[serde(default)]
    pub algorithm: Option<String>,
    /// login or signing.
    pub usage: KeyUsage,
    /// The status at `checked_at`.
    pub status: KeyStatus,
    /// Start of validity.
    #[serde(with = "time::serde::rfc3339")]
    pub not_before: OffsetDateTime,
    /// End of validity; `None` when the key does not expire.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub not_after: Option<OffsetDateTime>,
    /// The revocation, if any.
    #[serde(default)]
    pub revoked: Option<Revoked>,
    /// The certificate's issuer.
    pub issuer: Issuer,
    /// Whether the query matched the primary key or a subkey (OpenPGP).
    #[serde(default)]
    pub matched_via: Option<MatchedVia>,
    /// The public key itself; `None` from a server that does not send it (FEAT-021 as built,
    /// before FTR-005 T1).
    #[serde(default)]
    pub public_key: Option<String>,
    /// The user_id the key is bound to.
    pub user_id: String,
    /// The identity the key is bound to.
    pub identity: Identity,
    /// The origin of the instance that answered.
    pub instance: String,
    /// The registry commit the answer was read at (40 or 64 lowercase hex).
    pub registry_commit: String,
    /// When the status was computed.
    #[serde(with = "time::serde::rfc3339")]
    pub checked_at: OffsetDateTime,
    /// How many seconds the answer may be cached.
    pub cache_max_age: u32,
    /// The mirror's freshness; present only on answers from a mirror.
    #[serde(default)]
    pub mirror: Option<MirrorFreshness>,
}

/// A key, as listed in an [`IdentityLookup`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyEntry {
    /// `sha256:<64 hex>` (X.509), `ssh:<hex>` or `gpg:<hex>`.
    pub fingerprint: String,
    /// x509, ssh or openpgp.
    pub key_type: KeyType,
    /// The key algorithm; `None` from a server without it.
    #[serde(default)]
    pub algorithm: Option<String>,
    /// login or signing.
    pub usage: KeyUsage,
    /// The status at `checked_at`.
    pub status: KeyStatus,
    /// Start of validity.
    #[serde(with = "time::serde::rfc3339")]
    pub not_before: OffsetDateTime,
    /// End of validity; `None` when the key does not expire.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub not_after: Option<OffsetDateTime>,
    /// The revocation, if any.
    #[serde(default)]
    pub revoked: Option<Revoked>,
    /// The certificate's issuer.
    pub issuer: Issuer,
    /// Whether the key is a primary key or a subkey (OpenPGP).
    #[serde(default)]
    pub matched_via: Option<MatchedVia>,
    /// The public key itself; `None` from a server that does not send it.
    #[serde(default)]
    pub public_key: Option<String>,
}

/// The answer to `GET /v1/identities/{user_id}`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentityLookup {
    /// The identity.
    pub identity: Identity,
    /// Its keys, sorted by fingerprint, revoked and expired ones included.
    pub keys: Vec<KeyEntry>,
    /// The origin of the instance that answered.
    pub instance: String,
    /// The registry commit the answer was read at.
    pub registry_commit: String,
    /// When the statuses were computed.
    #[serde(with = "time::serde::rfc3339")]
    pub checked_at: OffsetDateTime,
    /// How many seconds the answer may be cached.
    pub cache_max_age: u32,
    /// The mirror's freshness; present only on answers from a mirror.
    #[serde(default)]
    pub mirror: Option<MirrorFreshness>,
}

/// An identity: a person or an organisation.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// The user_id.
    pub user_id: String,
    /// A person or an organisation.
    pub kind: IdentityKind,
    /// The pseudonym; `None` once blanked by a retire or disable.
    #[serde(default)]
    pub pseudonym: Option<String>,
    /// The proofing level.
    pub proofing: Proofing,
    /// The lifecycle status.
    pub status: IdentityStatus,
    /// A pending or recent recovery; `None` until recovery exists.
    #[serde(default)]
    pub pending_recovery: Option<serde_json::Value>,
}

/// Whether an identity is a person or an organisation.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    /// A person.
    Person,
    /// An organisation.
    Organisation,
    /// A kind this client does not know.
    #[serde(other)]
    Other,
}

/// How a key was revoked.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revoked {
    /// When.
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    /// Who.
    pub by: RevokedBy,
    /// A literal code: `user`, `key_compromise`, `ca:<crl reason>` or `identity_disabled`.
    pub reason: String,
}

/// A mirror's freshness (FTR-002).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorFreshness {
    /// The upstream the mirror syncs from.
    pub upstream: String,
    /// When the mirror last synced.
    #[serde(with = "time::serde::rfc3339")]
    pub synced_at: OffsetDateTime,
    /// True when the mirror is older than `stale_after_seconds`.
    pub stale: bool,
    /// The mirror's staleness limit.
    pub stale_after_seconds: u64,
}

/// An error body, as `application/problem+json` (RFC 9457).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    /// The problem type (`about:blank`).
    #[serde(rename = "type", default)]
    pub type_: Option<String>,
    /// The HTTP status phrase.
    #[serde(default)]
    pub title: Option<String>,
    /// The HTTP status.
    pub status: u16,
    /// The stable code, e.g. `key_not_found` or `index_behind`.
    pub code: String,
    /// What went wrong, for people.
    #[serde(default)]
    pub detail: Option<String>,
    /// The registry commit, when the index answered.
    #[serde(default)]
    pub registry_commit: Option<String>,
    /// How many seconds a not-found answer may be cached; `None` when the body gives none.
    #[serde(default)]
    pub cache_max_age: Option<u32>,
}

/// The issuer of a key's certificate: `"self-signed"` or `{"dn": …}`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issuer {
    /// `"self-signed"`.
    SelfSigned,
    /// A certificate authority.
    Ca {
        /// The issuer distinguished name.
        dn: String,
    },
}

const SELF_SIGNED: &str = "self-signed";

impl Serialize for Issuer {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Issuer::SelfSigned => s.serialize_str(SELF_SIGNED),
            Issuer::Ca { dn } => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("dn", dn)?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Issuer {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct IssuerVisitor;

        impl<'de> Visitor<'de> for IssuerVisitor {
            type Value = Issuer;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"self-signed\" or an object with \"dn\"")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Issuer, E> {
                if v == SELF_SIGNED {
                    Ok(Issuer::SelfSigned)
                } else {
                    Err(E::invalid_value(de::Unexpected::Str(v), &self))
                }
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Issuer, A::Error> {
                let mut dn: Option<String> = None;
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "dn" => dn = Some(map.next_value()?),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(Issuer::Ca {
                    dn: dn.ok_or_else(|| de::Error::missing_field("dn"))?,
                })
            }
        }

        d.deserialize_any(IssuerVisitor)
    }
}

/// The kind of public key.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    /// An X.509 certificate's key.
    X509,
    /// An SSH key.
    Ssh,
    /// An OpenPGP key.
    Openpgp,
    /// A key type this client does not know.
    #[serde(other)]
    Other,
}

/// What the key is used for.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyUsage {
    /// Logging in to the site.
    Login,
    /// Signing.
    Signing,
    /// A usage this client does not know.
    #[serde(other)]
    Other,
}

/// A key's status. Any value this client does not know is `Other`, which is never valid.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    /// Bound, within its validity, not revoked.
    Active,
    /// `not_before` is still in the future.
    NotYetValid,
    /// `not_after` has passed.
    Expired,
    /// Revoked.
    Revoked,
    /// The registry cannot tell (FEAT-017).
    Unknown,
    /// A status this client does not know: not valid.
    #[serde(other)]
    Other,
}

/// An identity's lifecycle status.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityStatus {
    /// Active.
    Active,
    /// A disable request is in its cancellation window.
    Disabling,
    /// Irrevocably disabled.
    Disabled,
    /// Retired.
    Retired,
    /// A status this client does not know.
    #[serde(other)]
    Other,
}

/// How far an identity has been proofed.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Proofing {
    /// Self-asserted: every identity starts here.
    SelfAsserted,
    /// Verified.
    Verified,
    /// A level this client does not know.
    #[serde(other)]
    Other,
}

/// Who revoked a key.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevokedBy {
    /// The key's owner.
    User,
    /// The certificate's issuing CA (CRL or OCSP).
    Ca,
    /// The service (for example when the identity is disabled).
    Service,
    /// A revoker this client does not know.
    #[serde(other)]
    Other,
}

/// Whether a query matched a primary key or a subkey (OpenPGP, FEAT-041).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchedVia {
    /// The primary key.
    Primary,
    /// A subkey.
    Subkey,
    /// A value this client does not know.
    #[serde(other)]
    Other,
}
