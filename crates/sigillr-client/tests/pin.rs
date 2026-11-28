//! Server key pinning (FTR-004 R-5), against a loopback TLS server in the test that presents
//! fixture certificate A or B (both for `localhost`, issued by the fixture CA). The fixtures
//! are made by `tests/fixtures/gen-pin-fixtures.sh`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use sigillr_client::{Client, Error, Lookup};

const INSTANCE: &str = "https://sigillr.provr.dev";
const KEY: &str = include_str!("fixtures/key.json");
const FINGERPRINT: &str = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

const CA: &[u8] = include_bytes!("fixtures/pin_ca.cert.pem");
const CERT_A: &[u8] = include_bytes!("fixtures/pin_a.cert.pem");
const KEY_A: &[u8] = include_bytes!("fixtures/pin_a.test-key.pem");
const CERT_B: &[u8] = include_bytes!("fixtures/pin_b.cert.pem");
const KEY_B: &[u8] = include_bytes!("fixtures/pin_b.test-key.pem");

/// `openssl x509 -in tests/fixtures/pin_<n>.cert.pem -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | base64`
const PIN_A: &str = "sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM=";
const PIN_B: &str = "sha256//EuRJqysZkQ2eFqBiqvs4qZA1vhdR59e8yPJ50XByH/U=";

/// A loopback TLS server that answers every request with the key fixture.
struct Server {
    port: u16,
    /// Connections accepted.
    connections: Arc<AtomicUsize>,
    /// HTTP requests read (any plaintext byte after the handshake counts).
    requests: Arc<AtomicUsize>,
}

impl Server {
    fn start(cert: &[u8], key: &[u8]) -> Server {
        let certs = vec![CertificateDer::from_pem_slice(cert).unwrap()];
        let key = PrivateKeyDer::from_pem_slice(key).unwrap();
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls_graviola::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .unwrap();
        let config = Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let connections = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let (c, r) = (Arc::clone(&connections), Arc::clone(&requests));
        std::thread::spawn(move || {
            for sock in listener.incoming() {
                let Ok(sock) = sock else { continue };
                c.fetch_add(1, Ordering::SeqCst);
                let conn = ServerConnection::new(Arc::clone(&config)).unwrap();
                let mut tls = StreamOwned::new(conn, sock);
                let mut req = Vec::new();
                let mut buf = [0u8; 4096];
                // A refused handshake ends here with an error, before any plaintext.
                while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => req.extend_from_slice(&buf[..n]),
                    }
                }
                if req.is_empty() {
                    continue;
                }
                r.fetch_add(1, Ordering::SeqCst);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Cache-Control: public, max-age=0\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n",
                    KEY.len()
                );
                let _ = tls.write_all(head.as_bytes());
                let _ = tls.write_all(KEY.as_bytes());
                let _ = tls.flush();
                tls.conn.send_close_notify();
                let _ = tls.flush();
            }
        });
        Server {
            port,
            connections,
            requests,
        }
    }

    fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// A fresh client for `server`, trusting the fixture CA, with these pins (none if empty).
fn lookup(server: &Server, pins: &[&str]) -> Result<Lookup<sigillr_client::KeyLookup>, Error> {
    let mut b = Client::builder(INSTANCE)
        .online(&format!("https://localhost:{}", server.port))
        .add_root_certificates_pem(CA)
        .unwrap();
    if !pins.is_empty() {
        b = b.pin_server_keys(pins).unwrap();
    }
    b.build().unwrap().key(FINGERPRINT).map(|a| a.result)
}

#[test]
fn client_refuses_server_with_unpinned_key() {
    let a = Server::start(CERT_A, KEY_A);
    let b = Server::start(CERT_B, KEY_B);

    // Pinned to A: A answers; B is refused at the handshake and sees no request.
    let found = lookup(&a, &[PIN_A]);
    assert!(matches!(found, Ok(Lookup::Found(_))), "{found:?}");
    assert_eq!(a.requests(), 1);
    let refused = lookup(&b, &[PIN_A]);
    assert_eq!(refused, Err(Error::ServerKeyNotPinned));
    assert_eq!(b.connections(), 1, "the client did reach B");
    assert_eq!(b.requests(), 0, "B saw an HTTP request");

    // Pinned to both: both answer.
    for s in [&a, &b] {
        let found = lookup(s, &[PIN_A, PIN_B]);
        assert!(matches!(found, Ok(Lookup::Found(_))), "{found:?}");
    }
    assert_eq!((a.requests(), b.requests()), (2, 1));

    // No pins: both answer, as before pinning existed.
    for s in [&a, &b] {
        let found = lookup(s, &[]);
        assert!(matches!(found, Ok(Lookup::Found(_))), "{found:?}");
    }
    assert_eq!((a.requests(), b.requests()), (3, 2));
}

#[test]
fn pinned_client_still_checks_the_chain() {
    // Pinned to A but not trusting the fixture CA: the usual check refuses A first.
    let a = Server::start(CERT_A, KEY_A);
    let client = Client::builder(INSTANCE)
        .online(&format!("https://localhost:{}", a.port))
        .pin_server_keys(&[PIN_A])
        .unwrap()
        .build()
        .unwrap();
    let err = client.key(FINGERPRINT).unwrap_err();
    assert!(matches!(err, Error::Unreachable { .. }), "{err:?}");
    assert_eq!(a.requests(), 0);
}

#[test]
fn pinned_client_refuses_plain_http() {
    // A pinned client refuses a server that presents no TLS key at all.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let client = Client::builder(INSTANCE)
        .online(&format!("http://127.0.0.1:{port}"))
        .pin_server_keys(&[PIN_A])
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        client.key(FINGERPRINT).unwrap_err(),
        Error::ServerKeyNotPinned
    );
}

#[test]
fn pins_apply_to_a_mirror() {
    let b = Server::start(CERT_B, KEY_B);
    let client = Client::builder(INSTANCE)
        .mirror(&format!("https://localhost:{}", b.port))
        .add_root_certificates_pem(CA)
        .unwrap()
        .pin_server_keys(&[PIN_A])
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(
        client.key(FINGERPRINT).unwrap_err(),
        Error::ServerKeyNotPinned
    );
    assert_eq!(b.requests(), 0);
}

#[test]
fn bad_pins_are_config_errors() {
    for pins in [
        &[][..],
        &["kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM="][..],
        &[PIN_A, "sha256//AAAA"][..],
    ] {
        let r = Client::builder(INSTANCE).pin_server_keys(pins);
        assert!(
            matches!(r, Err(sigillr_client::ConfigError::BadPin { .. })),
            "{pins:?}"
        );
    }
}
