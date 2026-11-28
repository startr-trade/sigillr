//! Builder, cache and lookup tests with a fake transport and clock: no network.

use std::sync::Arc;
use std::time::Duration;

use sigillr_client::testing::{FakeClock, FakeTransport};
use sigillr_client::{Client, ConfigError, HttpResponse, TransportError, TransportErrorKind};

const INSTANCE: &str = "https://sigillr.provr.dev";

#[test]
fn client_rejects_plain_http_except_loopback() {
    let build = |instance: &str, online: Option<&str>, mirror: Option<&str>| {
        let mut b = Client::builder(instance).transport(FakeTransport::new());
        if let Some(o) = online {
            b = b.online(o);
        }
        if let Some(m) = mirror {
            b = b.mirror(m);
        }
        b.build()
    };

    // https anywhere, http on loopback only.
    assert!(build(INSTANCE, None, None).is_ok());
    assert!(build(INSTANCE, Some("https://lookup.example/prefix/"), None).is_ok());
    for ok in [
        "http://localhost:8080",
        "http://127.0.0.1:1234/",
        "http://[::1]:9",
    ] {
        assert!(build(INSTANCE, None, Some(ok)).is_ok(), "{ok} is loopback");
        assert!(build(ok, None, None).is_ok(), "{ok} as instance");
    }
    for insecure in [
        "http://sigillr.provr.dev",
        "http://10.0.0.1",
        "http://localhost.example.com",
        "ftp://127.0.0.1",
    ] {
        assert!(
            matches!(
                build(INSTANCE, Some(insecure), None),
                Err(ConfigError::InsecureUrl { .. })
            ),
            "{insecure} is refused"
        );
        assert!(
            matches!(
                build(INSTANCE, None, Some(insecure)),
                Err(ConfigError::InsecureUrl { .. })
            ),
            "{insecure} is refused as a mirror"
        );
    }
    assert!(matches!(
        build("http://sigillr.provr.dev", None, None),
        Err(ConfigError::InsecureUrl { .. })
    ));
    for bad in ["", "not a url", "/v1/keys", "sigillr.provr.dev", "https://"] {
        assert!(
            matches!(
                build(INSTANCE, Some(bad), None),
                Err(ConfigError::BadUrl { .. })
            ),
            "{bad:?} is a bad URL"
        );
    }

    // PEM roots: none, or garbage, is refused.
    assert!(matches!(
        Client::builder(INSTANCE).add_root_certificates_pem(b"no pem here"),
        Err(ConfigError::BadRootCertificate(_))
    ));
}

const KEY: &str = include_str!("fixtures/key.json");
const IDENTITY: &str = include_str!("fixtures/identity.json");
const FP: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

/// A 200 key answer for fingerprint `fp` (same length as the fixture's), from `instance`.
fn key_answer(
    fp: &str,
    cache_max_age: u32,
    instance: &str,
) -> Result<HttpResponse, TransportError> {
    let mut v: serde_json::Value = serde_json::from_str(KEY).unwrap();
    v["fingerprint"] = fp.into();
    v["cache_max_age"] = cache_max_age.into();
    v["instance"] = instance.into();
    Ok(HttpResponse::new(
        200,
        vec![(
            "Cache-Control".into(),
            format!("public, max-age={cache_max_age}"),
        )],
        serde_json::to_vec(&v).unwrap(),
    ))
}

fn identity_answer() -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse::new(200, vec![], IDENTITY.as_bytes().to_vec()))
}

fn connect_error() -> Result<HttpResponse, TransportError> {
    Err(TransportError::new(
        TransportErrorKind::Connect,
        "connection refused",
    ))
}

fn unavailable(code: &str) -> Result<HttpResponse, TransportError> {
    let body = serde_json::json!({
        "type": "about:blank", "title": "Service Unavailable", "status": 503,
        "detail": "The index cannot answer now; retry later.", "code": code,
    });
    Ok(HttpResponse::new(
        503,
        vec![
            ("Retry-After".into(), "5".into()),
            ("Cache-Control".into(), "no-store".into()),
        ],
        serde_json::to_vec(&body).unwrap(),
    ))
}

fn client(t: &Arc<FakeTransport>, c: &Arc<FakeClock>) -> sigillr_client::ClientBuilder {
    Client::builder(INSTANCE)
        .transport(t.clone())
        .clock(c.clone())
}

/// R-2.
#[test]
fn client_serves_cached_answer_within_lifetime() {
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(key_answer(FP, 60, INSTANCE));
    let cl = client(&t, &c).build().unwrap();

    let a = cl.key(FP).unwrap();
    assert_eq!(t.calls(), 1);
    assert!(!a.from_cache);
    assert_eq!(a.lifetime, Duration::from_secs(60));
    assert_eq!(a.source, sigillr_client::Source::Online);
    assert_eq!(a.found().unwrap().user_id, "alice");

    c.advance(Duration::from_secs(59));
    let a = cl.key(FP).unwrap();
    assert_eq!(t.calls(), 1, "no request within the lifetime");
    assert!(a.from_cache);
    assert_eq!(a.age, Duration::from_secs(59));
    // The same fingerprint in upper case is the same entry; a clone shares the cache.
    assert!(
        cl.clone()
            .key(&FP.to_uppercase().replace("SHA256", "sha256"))
            .unwrap()
            .from_cache
    );
    assert_eq!(t.calls(), 1);

    c.advance(Duration::from_secs(1));
    let a = cl.key(FP).unwrap();
    assert_eq!(t.calls(), 2, "refetched at the lifetime");
    assert!(!a.from_cache);

    // The caller's cap lowers the lifetime, and cannot raise it past 60 s.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(key_answer(FP, 60, INSTANCE));
    let cl = client(&t, &c)
        .max_lifetime(Duration::from_secs(10))
        .build()
        .unwrap();
    assert_eq!(cl.key(FP).unwrap().lifetime, Duration::from_secs(10));
    c.advance(Duration::from_secs(9));
    assert!(cl.key(FP).unwrap().from_cache);
    c.advance(Duration::from_secs(1));
    assert!(!cl.key(FP).unwrap().from_cache);
    assert_eq!(t.calls(), 2);

    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(key_answer(FP, 300, INSTANCE));
    let cl = client(&t, &c)
        .max_lifetime(Duration::from_secs(600))
        .build()
        .unwrap();
    assert_eq!(cl.key(FP).unwrap().lifetime, Duration::from_secs(60));

    // cache_max_age 0: every lookup calls.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(key_answer(FP, 0, INSTANCE));
    let cl = client(&t, &c).build().unwrap();
    for n in 1..=3 {
        assert!(!cl.key(FP).unwrap().from_cache);
        assert_eq!(t.calls(), n);
    }

    // user_ids are folded to lower case for the cache key.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(identity_answer());
    let cl = client(&t, &c).build().unwrap();
    assert!(!cl.identity("ALICE").unwrap().from_cache);
    assert!(cl.identity("alice").unwrap().from_cache);
    assert_eq!(t.calls(), 1);
    assert!(t.urls()[0].ends_with("/v1/identities/ALICE"));

    // clear_cache drops everything.
    cl.clear_cache();
    assert!(!cl.identity("alice").unwrap().from_cache);
    assert_eq!(t.calls(), 2);

    // A 404 is cached for the body's cache_max_age.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    let body = serde_json::json!({
        "type": "about:blank", "title": "Not Found", "status": 404,
        "detail": "No key with this fingerprint is bound.", "code": "key_not_found",
        "registry_commit": "3f2a9c1e5b7d4f6a8c0e2b4d6f8a1c3e5b7d9f0a", "cache_max_age": 30,
    });
    t.push(Ok(HttpResponse::new(
        404,
        vec![],
        serde_json::to_vec(&body).unwrap(),
    )));
    t.push(Ok(HttpResponse::new(
        404,
        vec![],
        serde_json::to_vec(&body).unwrap(),
    )));
    let cl = client(&t, &c).build().unwrap();
    assert!(!cl.key(FP).unwrap().from_cache);
    c.advance(Duration::from_secs(29));
    assert!(cl.key(FP).unwrap().from_cache);
    assert_eq!(t.calls(), 1);
    c.advance(Duration::from_secs(1));
    assert!(!cl.key(FP).unwrap().from_cache);
    assert_eq!(t.calls(), 2);
}

/// R-3.
#[test]
fn client_refuses_stale_answer_when_unreachable() {
    for failure in [connect_error(), unavailable("index_unavailable")] {
        let (t, c) = (FakeTransport::new(), FakeClock::new());
        t.push(key_answer(FP, 60, INSTANCE));
        let cl = client(&t, &c).build().unwrap();
        cl.key(FP).unwrap();

        c.advance(Duration::from_secs(90));
        t.push(failure.clone());
        let err = cl.key(FP).expect_err("never Ok past the lifetime");
        match &err {
            sigillr_client::Error::Stale {
                age,
                lifetime,
                registry_commit,
                ..
            } => {
                assert_eq!(*age, Duration::from_secs(90));
                assert_eq!(*lifetime, Duration::from_secs(60));
                assert!(registry_commit.is_some());
            }
            other => panic!("expected Stale, got {other:?}"),
        }
        let text = err.to_string();
        assert!(text.contains("90 s"), "{text}");
        assert!(text.contains("60 s"), "{text}");
        assert_eq!(t.calls(), 2);
    }

    // With no cached entry, the failure itself.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(connect_error());
    let cl = client(&t, &c).build().unwrap();
    assert!(matches!(
        cl.key(FP),
        Err(sigillr_client::Error::Unreachable { .. })
    ));
    t.push(unavailable("index_unavailable"));
    assert!(matches!(
        cl.key(FP),
        Err(sigillr_client::Error::Unavailable { retry_after: Some(d), .. }) if d == Duration::from_secs(5)
    ));
}

#[test]
fn client_refuses_answer_from_other_instance() {
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    t.push(key_answer(FP, 60, "https://other.example"));
    let cl = client(&t, &c).build().unwrap();
    assert!(matches!(
        cl.key(FP),
        Err(sigillr_client::Error::WrongInstance { expected, got })
            if expected == INSTANCE && got == "https://other.example"
    ));
    // Nothing was cached: the next lookup calls again.
    t.push(key_answer(FP, 60, &format!("{INSTANCE}/")));
    assert!(!cl.key(FP).unwrap().from_cache, "one trailing / is dropped");
    assert_eq!(t.calls(), 2);
}

fn fp(n: u8) -> String {
    format!("sha256:{}", format!("{n:x}").repeat(64))
}

#[test]
fn client_cache_evicts_within_budget() {
    let size = key_answer(&fp(1), 60, INSTANCE).unwrap().body.len();
    let budget = size * 5 / 2; // room for two entries

    // Expired entries go first, oldest received first.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    let cl = client(&t, &c).max_cache_bytes(budget).build().unwrap();
    t.push(key_answer(&fp(1), 50, INSTANCE)); // X: received 0, expires 50
    cl.key(&fp(1)).unwrap();
    c.advance(Duration::from_secs(10));
    t.push(key_answer(&fp(2), 5, INSTANCE)); // Y: received 10, expires 15
    cl.key(&fp(2)).unwrap();
    c.advance(Duration::from_secs(50)); // both expired
    t.push(key_answer(&fp(3), 60, INSTANCE)); // Z evicts X
    cl.key(&fp(3)).unwrap();
    t.push(connect_error());
    assert!(
        matches!(
            cl.key(&fp(1)),
            Err(sigillr_client::Error::Unreachable { .. })
        ),
        "X was evicted"
    );
    assert!(
        matches!(cl.key(&fp(2)), Err(sigillr_client::Error::Stale { .. })),
        "Y is still held"
    );
    assert!(cl.key(&fp(3)).unwrap().from_cache);

    // With none expired, the earliest received + lifetime goes, not the oldest.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    let cl = client(&t, &c).max_cache_bytes(budget).build().unwrap();
    t.push(key_answer(&fp(1), 60, INSTANCE)); // A: expires 60
    cl.key(&fp(1)).unwrap();
    c.advance(Duration::from_secs(1));
    t.push(key_answer(&fp(2), 30, INSTANCE)); // B: expires 31
    cl.key(&fp(2)).unwrap();
    t.push(key_answer(&fp(3), 60, INSTANCE)); // C evicts B
    cl.key(&fp(3)).unwrap();
    assert_eq!(t.calls(), 3);
    assert!(cl.key(&fp(1)).unwrap().from_cache);
    assert!(cl.key(&fp(3)).unwrap().from_cache);
    assert_eq!(t.calls(), 3);
    assert!(!cl.key(&fp(2)).unwrap().from_cache, "B was evicted");

    // An answer larger than the whole budget is returned and not stored.
    let (t, c) = (FakeTransport::new(), FakeClock::new());
    let cl = client(&t, &c).max_cache_bytes(size - 1).build().unwrap();
    t.push(key_answer(FP, 60, INSTANCE));
    assert!(cl.key(FP).unwrap().found().is_some());
    assert!(!cl.key(FP).unwrap().from_cache);
    assert_eq!(t.calls(), 2);
}
