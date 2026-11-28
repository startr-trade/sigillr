//! One HTTP GET: the [`Transport`] trait and [`HttpTransport`], its `ureq` implementation.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use rustls_pki_types::CertificateDer;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{Connector, TcpConnector};

use crate::builder::ConfigError;
use crate::pin::{Pin, PinnedTlsConnector, is_not_pinned, pinned_client_config};

/// Something that can make one GET request. Tests inject a fake one with
/// [`crate::ClientBuilder::transport`].
pub trait Transport: Send + Sync {
    /// One GET. No redirect is followed. The body is read up to `max_body` bytes; longer is
    /// [`TransportErrorKind::TooLarge`].
    fn get(
        &self,
        url: &str,
        timeout: Duration,
        max_body: usize,
    ) -> Result<HttpResponse, TransportError>;
}

/// A response, whatever its status.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// The HTTP status.
    pub status: u16,
    /// The headers, names in lower case, in the order received.
    pub headers: Vec<(String, String)>,
    /// The body, at most `max_body` bytes.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// A response with this status, headers and body.
    pub fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        let headers = headers
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        HttpResponse {
            status,
            headers,
            body,
        }
    }

    /// The first value of the header `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Why a request got no response.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    /// The kind of failure.
    pub kind: TransportErrorKind,
    /// A description, for people.
    pub detail: String,
}

impl TransportError {
    /// A transport error of this kind.
    pub fn new(kind: TransportErrorKind, detail: impl Into<String>) -> Self {
        TransportError {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for TransportError {}

/// The kinds of [`TransportError`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// The host could not be resolved or connected to.
    Connect,
    /// The connect or total timeout passed.
    Timeout,
    /// The TLS handshake or certificate check failed.
    Tls,
    /// Reading or writing failed.
    Io,
    /// The body is longer than the limit.
    TooLarge,
    /// Server keys are pinned and the server's TLS key is not one of them (R-5). The
    /// handshake was refused, so no request was sent.
    ServerKeyNotPinned,
    /// The server answered with a redirect that would have to be followed.
    Redirect,
}

impl fmt::Display for TransportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TransportErrorKind::Connect => "connect",
            TransportErrorKind::Timeout => "timeout",
            TransportErrorKind::Tls => "tls",
            TransportErrorKind::Io => "io",
            TransportErrorKind::TooLarge => "too large",
            TransportErrorKind::ServerKeyNotPinned => "server key not pinned",
            TransportErrorKind::Redirect => "redirect",
        })
    }
}

/// The User-Agent every request carries: the crate name and version, nothing about the caller.
pub(crate) const USER_AGENT: &str = concat!("sigillr-client/", env!("CARGO_PKG_VERSION"));

/// [`Transport`] over a `ureq::Agent`: rustls with the graviola provider, the operating
/// system's trust roots plus the caller's, no redirects, no proxy, no cookies.
pub struct HttpTransport {
    agent: ureq::Agent,
}

impl fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpTransport").finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// An agent with this connect timeout, trusting the OS store plus `extra_roots`. With
    /// `pins` set, a server whose TLS key is not one of them is refused at the handshake.
    pub(crate) fn new(
        connect_timeout: Duration,
        extra_roots: &[CertificateDer<'static>],
        pins: &[Pin],
    ) -> Result<Self, ConfigError> {
        // Certificates the OS store cannot load are skipped; a store that yields none leaves
        // only the caller's roots, and TLS then fails at connect time, closed.
        let native = rustls_native_certs::load_native_certs();
        let roots: Vec<CertificateDer<'static>> = native
            .certs
            .into_iter()
            .chain(extra_roots.iter().cloned())
            .collect();
        let provider = Arc::new(rustls_graviola::default_provider());
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .unversioned_rustls_crypto_provider(Arc::clone(&provider))
            .root_certs(RootCerts::Specific(Arc::new(
                roots
                    .iter()
                    .map(|der| Certificate::from_der(der.as_ref()).to_owned())
                    .collect(),
            )))
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .max_redirects(0)
            .http_status_as_error(false)
            .proxy(None)
            .user_agent(USER_AGENT)
            .accept("application/json")
            .timeout_connect(Some(connect_timeout))
            .build();
        let agent = if pins.is_empty() {
            ureq::Agent::new_with_config(config)
        } else {
            // ureq takes no custom certificate verifier, so a pinned client has its own TLS
            // step in the connector chain, with the pinning verifier (crate::pin).
            let tls = pinned_client_config(provider, &roots, pins.to_vec())?;
            let connector = ().chain(TcpConnector::default()).chain(PinnedTlsConnector::new(tls));
            ureq::Agent::with_parts(config, connector, DefaultResolver::default())
        };
        Ok(HttpTransport { agent })
    }
}

fn map_error(e: ureq::Error) -> TransportError {
    use TransportErrorKind as K;
    if is_not_pinned(&e) {
        return TransportError::new(K::ServerKeyNotPinned, e.to_string());
    }
    let kind = match &e {
        ureq::Error::Timeout(_) => K::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => K::Timeout,
        ureq::Error::Io(_) => K::Io,
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => K::Connect,
        ureq::Error::Tls(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::Pem(_)
        | ureq::Error::TlsRequired => K::Tls,
        ureq::Error::BodyExceedsLimit(_) => K::TooLarge,
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => K::Redirect,
        _ => K::Io,
    };
    TransportError::new(kind, e.to_string())
}

impl Transport for HttpTransport {
    fn get(
        &self,
        url: &str,
        timeout: Duration,
        max_body: usize,
    ) -> Result<HttpResponse, TransportError> {
        let mut resp = self
            .agent
            .get(url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .call()
            .map_err(map_error)?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = resp
            .body_mut()
            .with_config()
            .limit(max_body as u64)
            .read_to_vec()
            .map_err(map_error)?;
        Ok(HttpResponse::new(status, headers, body))
    }
}
