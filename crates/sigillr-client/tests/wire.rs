//! Wire-type tests (FTR-004 T1): fixtures round-trip, unknown statuses stay not valid,
//! `status_at` follows FEAT-021's precedence, and no body panics the decoder.

use proptest::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sigillr_client::{
    IdentityLookup, IdentityStatus, Issuer, KeyLookup, KeyStatus, MatchedVia, Problem, Proofing,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const KEY: &str = include_str!("fixtures/key.json");
const IDENTITY: &str = include_str!("fixtures/identity.json");
const IDENTITY_NOT_FOUND: &str = include_str!("fixtures/identity_not_found.json");
const KEY_CA: &str = include_str!("fixtures/key_ca.json");
const MIRROR_KEY: &str = include_str!("fixtures/mirror_key.json");

/// `v` with every `null` object member removed, recursively.
fn drop_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, drop_nulls(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(drop_nulls).collect()),
        v => v,
    }
}

fn round_trip<T: DeserializeOwned + Serialize>(name: &str, json: &str) -> T {
    let decoded: T = serde_json::from_str(json).unwrap_or_else(|e| panic!("{name} decodes: {e}"));
    let again = serde_json::to_value(&decoded).expect("re-encodes");
    let fixture: Value = serde_json::from_str(json).expect("fixture is JSON");
    assert_eq!(drop_nulls(again), drop_nulls(fixture), "{name} round-trips");
    decoded
}

fn at(s: &str) -> OffsetDateTime {
    OffsetDateTime::parse(s, &Rfc3339).expect("RFC 3339")
}

#[test]
fn client_wire_round_trips_fixtures() {
    let key: KeyLookup = round_trip("key.json", KEY);
    assert_eq!(key.user_id, "alice");
    assert_eq!(key.status, KeyStatus::Active);
    assert_eq!(key.identity.proofing, Proofing::SelfAsserted);
    assert_eq!(key.issuer, Issuer::SelfSigned);
    assert_eq!(key.registry_commit.len(), 40);
    assert!(key.mirror.is_none() && key.algorithm.is_none() && key.matched_via.is_none());
    assert!(key.is_valid());

    let ca: KeyLookup = round_trip("key_ca.json", KEY_CA);
    assert_eq!(
        ca.issuer,
        Issuer::Ca {
            dn: "CN=Test Intermediate,O=sigillr test".into()
        }
    );
    // An older server's `serial` is ignored.
    let old: Issuer = serde_json::from_str(r#"{"dn":"CN=x","serial":"0a"}"#).unwrap();
    assert_eq!(old, Issuer::Ca { dn: "CN=x".into() });

    let id: IdentityLookup = round_trip("identity.json", IDENTITY);
    assert_eq!(id.keys.len(), 3);
    assert_eq!(id.keys[0].status, KeyStatus::Revoked);
    assert!(matches!(&id.keys[0].issuer, Issuer::Ca { dn } if dn == "CN=Example CA,O=Example"));
    assert_eq!(id.keys[2].algorithm.as_deref(), Some("ssh-ed25519"));

    let nf: Problem = round_trip("identity_not_found.json", IDENTITY_NOT_FOUND);
    assert_eq!(nf.status, 404);
    assert_eq!(nf.code, "identity_not_found");
    assert!(nf.registry_commit.is_some());
    assert_eq!(nf.cache_max_age, Some(30));

    let m: KeyLookup = round_trip("mirror_key.json", MIRROR_KEY);
    assert_eq!(m.matched_via, Some(MatchedVia::Subkey));
    assert_eq!(m.identity.status, IdentityStatus::Disabling);
    let mirror = m.mirror.as_ref().expect("mirror block");
    assert!(!mirror.stale);
    assert_eq!(mirror.stale_after_seconds, 86400);
    assert!(m.is_valid());

    // A field the server adds later is ignored.
    let mut extra: Value = serde_json::from_str(KEY).unwrap();
    extra["public_key"] = Value::String("MIIB…".into());
    extra["identity"]["future"] = Value::Bool(true);
    let _: KeyLookup = serde_json::from_value(extra).expect("unknown fields are ignored");
}

#[test]
fn client_treats_unknown_status_as_not_valid() {
    let mut v: Value = serde_json::from_str(KEY).unwrap();
    v["status"] = Value::String("quarantined".into());
    let k: KeyLookup = serde_json::from_value(v).expect("decodes");
    assert_eq!(k.status, KeyStatus::Other);
    assert!(!k.is_valid());
    assert_eq!(k.status_at(at("2027-01-01T00:00:00Z")), KeyStatus::Other);

    let mut v: Value = serde_json::from_str(KEY).unwrap();
    v["status"] = Value::String("unknown".into());
    let k: KeyLookup = serde_json::from_value(v).expect("decodes");
    assert_eq!(k.status, KeyStatus::Unknown);
    assert!(!k.is_valid());
    assert_eq!(k.status_at(at("2027-01-01T00:00:00Z")), KeyStatus::Unknown);

    // An unknown identity status is not valid either.
    let mut v: Value = serde_json::from_str(KEY).unwrap();
    v["identity"]["status"] = Value::String("suspended".into());
    let k: KeyLookup = serde_json::from_value(v).expect("decodes");
    assert_eq!(k.identity.status, IdentityStatus::Other);
    assert!(!k.is_valid());

    // An unknown key type decodes as Other.
    let mut v: Value = serde_json::from_str(KEY).unwrap();
    v["key_type"] = Value::String("fido2".into());
    let k: KeyLookup = serde_json::from_value(v).expect("decodes");
    assert_eq!(k.key_type, sigillr_client::KeyType::Other);
}

fn key_with(not_before: &str, not_after: Option<&str>, revoked_at: Option<&str>) -> KeyLookup {
    let mut v: Value = serde_json::from_str(KEY).unwrap();
    v["not_before"] = Value::String(not_before.into());
    v["not_after"] = not_after.map_or(Value::Null, |s| Value::String(s.into()));
    v["revoked"] = revoked_at.map_or(
        Value::Null,
        |s| serde_json::json!({ "at": s, "by": "user", "reason": "user" }),
    );
    serde_json::from_value(v).expect("decodes")
}

#[test]
fn client_status_at_follows_precedence() {
    let nb = "2026-01-01T00:00:00Z";
    let na = "2027-01-01T00:00:00Z";

    // Plain validity window, boundaries inclusive at not_before, exclusive at not_after.
    let k = key_with(nb, Some(na), None);
    assert_eq!(
        k.status_at(at("2025-12-31T23:59:59Z")),
        KeyStatus::NotYetValid
    );
    assert_eq!(k.status_at(at(nb)), KeyStatus::Active);
    assert_eq!(k.status_at(at("2026-12-31T23:59:59Z")), KeyStatus::Active);
    assert_eq!(k.status_at(at(na)), KeyStatus::Expired);

    // Revoked from revoked.at on, and it beats expired.
    let k = key_with(nb, Some(na), Some("2026-06-01T00:00:00Z"));
    assert_eq!(k.status_at(at("2026-05-31T23:59:59Z")), KeyStatus::Active);
    assert_eq!(k.status_at(at("2026-06-01T00:00:00Z")), KeyStatus::Revoked);
    assert_eq!(k.status_at(at("2028-01-01T00:00:00Z")), KeyStatus::Revoked);

    // Revoked after expiry: expired until revoked.at, revoked from then on.
    let k = key_with(nb, Some(na), Some("2027-06-01T00:00:00Z"));
    assert_eq!(k.status_at(at(na)), KeyStatus::Expired);
    assert_eq!(k.status_at(at("2027-06-01T00:00:00Z")), KeyStatus::Revoked);

    // Revoked beats not yet valid.
    let k = key_with(nb, Some(na), Some("2025-06-01T00:00:00Z"));
    assert_eq!(k.status_at(at("2025-12-31T00:00:00Z")), KeyStatus::Revoked);

    // Expired beats not yet valid (a window that ends before it starts).
    let k = key_with(na, Some(nb), None);
    assert_eq!(k.status_at(at("2026-06-01T00:00:00Z")), KeyStatus::Expired);

    // No not_after: never expires.
    let k = key_with(nb, None, None);
    assert_eq!(k.status_at(at("2099-01-01T00:00:00Z")), KeyStatus::Active);

    // KeyEntry follows the same rule.
    let id: IdentityLookup = serde_json::from_str(IDENTITY).unwrap();
    let revoked = &id.keys[0];
    assert_eq!(
        revoked.status_at(at("2026-05-31T00:00:00Z")),
        KeyStatus::Active
    );
    assert_eq!(
        revoked.status_at(at("2026-06-01T12:00:00Z")),
        KeyStatus::Revoked
    );
    assert_eq!(
        revoked.status_at(at("2025-12-31T00:00:00Z")),
        KeyStatus::NotYetValid
    );
}

fn decode_all(bytes: &[u8]) {
    let _ = serde_json::from_slice::<KeyLookup>(bytes);
    let _ = serde_json::from_slice::<IdentityLookup>(bytes);
    let _ = serde_json::from_slice::<Problem>(bytes);
    if let Ok(k) = serde_json::from_slice::<KeyLookup>(bytes) {
        let _ = k.is_valid();
        let _ = k.status_at(OffsetDateTime::UNIX_EPOCH);
    }
}

proptest! {
    #[test]
    fn client_never_panics_on_any_body(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        decode_all(&bytes);
    }

    #[test]
    fn client_never_panics_on_mutated_fixtures(
        which in 0usize..5,
        edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
    ) {
        let mut body = [KEY, IDENTITY, IDENTITY_NOT_FOUND, MIRROR_KEY, KEY_CA][which].as_bytes().to_vec();
        for (i, b) in edits {
            let i = i.index(body.len());
            body[i] = b;
        }
        decode_all(&body);
    }
}

/// T11: `public_key` is decoded when the server sends it (FTR-005 T1) and is `None` when it
/// does not (the server as built today), on a key answer and on an identity's key entries.
#[test]
fn client_decodes_public_key_when_present() {
    let key: KeyLookup = serde_json::from_str(KEY).expect("key.json decodes");
    assert!(key.public_key.is_none(), "absent from the server as built");
    let id: IdentityLookup = serde_json::from_str(IDENTITY).expect("identity.json decodes");
    assert!(id.keys.iter().all(|k| k.public_key.is_none()));

    let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    let mut v: Value = serde_json::from_str(KEY).expect("JSON");
    v["fingerprint"] = Value::from(format!("ssh:{}", "ab".repeat(32)));
    v["key_type"] = Value::from("ssh");
    v["usage"] = Value::from("signing");
    v["algorithm"] = Value::from("ssh-ed25519");
    v["public_key"] = Value::from(line);
    let with = serde_json::to_string(&v).expect("JSON");
    let key: KeyLookup = round_trip("key with public_key", &with);
    assert_eq!(key.public_key.as_deref(), Some(line));

    let mut v: Value = serde_json::from_str(IDENTITY).expect("JSON");
    v["keys"][2]["public_key"] = Value::from(line);
    let with = serde_json::to_string(&v).expect("JSON");
    let id: IdentityLookup = round_trip("identity with public_key", &with);
    assert_eq!(id.keys[2].public_key.as_deref(), Some(line));
    assert!(id.keys[0].public_key.is_none());

    let mut v: Value = serde_json::from_str(KEY).expect("JSON");
    v["public_key"] = Value::Null;
    let key: KeyLookup = serde_json::from_value(v).expect("null decodes");
    assert!(key.public_key.is_none());
}
