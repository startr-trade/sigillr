//! [`ClientBuilder`]: the pinned instance, online or mirror, the cache bounds and timeouts.

use std::sync::Arc;
use std::time::Duration;

use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

use crate::Client;
use crate::answer::Source;
use crate::clock::{Clock, SystemClock};
use crate::pin::{Pin, parse_pin};
use crate::transport::{HttpTransport, Transport};

/// The default and highest cache lifetime cap: a caller can lower it, never raise it.
pub(crate) const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(60);
/// The default cache budget.
pub(crate) const DEFAULT_MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
/// The default connect timeout.
pub(crate) const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The default total timeout of one lookup.
pub(crate) const DEFAULT_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
/// The fixed body limit.
pub(crate) const MAX_BODY: usize = 1024 * 1024;

/// Why a [`ClientBuilder`] could not build a [`Client`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The URL does not parse, or lacks a scheme or host.
    #[error("{url:?} is not a usable base URL: {why}")]
    BadUrl {
        /// The URL as given.
        url: String,
        /// What is wrong with it.
        why: &'static str,
    },
    /// The URL is not `https`, and not `http` on a loopback host.
    #[error("{url:?} is not https (plain http is allowed only on localhost, 127.0.0.1 or [::1])")]
    InsecureUrl {
        /// The URL as given.
        url: String,
    },
    /// The PEM holds no certificate, or one that does not parse.
    #[error("bad root certificate: {0}")]
    BadRootCertificate(String),
    /// A server key pin is not `sha256//` followed by the standard base64 of 32 bytes, or
    /// [`ClientBuilder::pin_server_keys`] was given no pin.
    #[error("{pin:?} is not a server key pin (sha256//<base64>): {why}")]
    BadPin {
        /// The pin as given.
        pin: String,
        /// What is wrong with it.
        why: &'static str,
    },
    /// The TLS configuration could not be built.
    #[error("TLS configuration failed: {0}")]
    Tls(String),
}

/// The settings a [`Client`] runs with, checked.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// The pinned instance origin, without a trailing `/`.
    pub(crate) instance: String,
    /// The base URL requests go to, without a trailing `/`.
    pub(crate) base: String,
    /// Online or mirror.
    pub(crate) source: Source,
    pub(crate) max_lifetime: Duration,
    pub(crate) max_cache_bytes: usize,
    pub(crate) total_timeout: Duration,
    pub(crate) accept_stale_mirror: bool,
}

/// Builds a [`Client`]. Start with [`Client::builder`].
pub struct ClientBuilder {
    instance: String,
    online: Option<String>,
    mirror: Option<String>,
    max_lifetime: Duration,
    max_cache_bytes: usize,
    connect_timeout: Duration,
    total_timeout: Duration,
    accept_stale_mirror: bool,
    extra_roots: Vec<CertificateDer<'static>>,
    pins: Vec<Pin>,
    transport: Option<Arc<dyn Transport>>,
    clock: Option<Arc<dyn Clock>>,
}

impl ClientBuilder {
    pub(crate) fn new(instance: &str) -> Self {
        ClientBuilder {
            instance: instance.to_owned(),
            online: None,
            mirror: None,
            max_lifetime: DEFAULT_MAX_LIFETIME,
            max_cache_bytes: DEFAULT_MAX_CACHE_BYTES,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            total_timeout: DEFAULT_TOTAL_TIMEOUT,
            accept_stale_mirror: true,
            extra_roots: Vec::new(),
            pins: Vec::new(),
            transport: None,
            clock: None,
        }
    }

    /// The online service's base URL. Default: the instance origin itself.
    pub fn online(mut self, base_url: &str) -> Self {
        self.online = Some(base_url.to_owned());
        self
    }

    /// Answer only from this mirror; the online service is never contacted, not even as a
    /// fallback. Replaces [`online`](Self::online), whatever the order of the calls.
    pub fn mirror(mut self, base_url: &str) -> Self {
        self.mirror = Some(base_url.to_owned());
        self
    }

    /// The caller's cap on a cached answer's lifetime. It can lower the server's
    /// `cache_max_age`, never raise it, and it is itself capped at the 60 s default.
    pub fn max_lifetime(mut self, d: Duration) -> Self {
        self.max_lifetime = d.min(DEFAULT_MAX_LIFETIME);
        self
    }

    /// The cache's byte budget. Default 8 MiB.
    pub fn max_cache_bytes(mut self, n: usize) -> Self {
        self.max_cache_bytes = n;
        self
    }

    /// The connect timeout and the total timeout of one lookup. Defaults 5 s and 10 s.
    pub fn timeouts(mut self, connect: Duration, total: Duration) -> Self {
        self.connect_timeout = connect;
        self.total_timeout = total;
        self
    }

    /// Whether answers from a mirror past its staleness limit are returned (marked
    /// `mirror.stale`) or refused with [`crate::Error::MirrorStale`]. Default true.
    pub fn accept_stale_mirror(mut self, yes: bool) -> Self {
        self.accept_stale_mirror = yes;
        self
    }

    /// Trust these PEM root certificates as well as the operating system's store.
    pub fn add_root_certificates_pem(mut self, pem: &[u8]) -> Result<Self, ConfigError> {
        let mut added = 0;
        for cert in CertificateDer::pem_slice_iter(pem) {
            let cert = cert.map_err(|e| ConfigError::BadRootCertificate(e.to_string()))?;
            self.extra_roots.push(cert);
            added += 1;
        }
        if added == 0 {
            return Err(ConfigError::BadRootCertificate(
                "the PEM holds no certificate".to_owned(),
            ));
        }
        Ok(self)
    }

    /// Pin the server's TLS key (R-5), for the online service and a mirror alike. Each pin is
    /// `sha256//` followed by the standard base64 of the SHA-256 of the server certificate's
    /// SubjectPublicKeyInfo, the form `curl --pinnedpubkey` takes. A handshake must first pass
    /// the usual checks (the chain to the trust roots and the host name) and is then refused
    /// unless the server's key is one of the pins: no lookup is sent and the lookup fails with
    /// [`crate::Error::ServerKeyNotPinned`]. There is no fallback. Several pins may be set, so
    /// the next key can be pinned before a certificate is renewed; calls add to the pins.
    ///
    /// A pin of another form, or an empty list, is [`ConfigError::BadPin`].
    pub fn pin_server_keys(mut self, pins: &[&str]) -> Result<Self, ConfigError> {
        if pins.is_empty() {
            return Err(ConfigError::BadPin {
                pin: String::new(),
                why: "no pin was given",
            });
        }
        for p in pins {
            self.pins.push(parse_pin(p)?);
        }
        Ok(self)
    }

    /// Use this transport instead of [`HttpTransport`]; the TLS settings and pins are then unused.
    pub fn transport(mut self, t: Arc<dyn Transport>) -> Self {
        self.transport = Some(t);
        self
    }

    /// Use this clock instead of [`SystemClock`].
    pub fn clock(mut self, c: Arc<dyn Clock>) -> Self {
        self.clock = Some(c);
        self
    }

    /// Check the URLs and build the client.
    pub fn build(self) -> Result<Client, ConfigError> {
        let instance = check_url(&self.instance)?;
        let (source, base) = match (&self.mirror, &self.online) {
            (Some(m), _) => (Source::Mirror, check_url(m)?),
            (None, Some(o)) => (Source::Online, check_url(o)?),
            (None, None) => (Source::Online, instance.clone()),
        };
        let transport: Arc<dyn Transport> = match self.transport {
            Some(t) => t,
            None => Arc::new(HttpTransport::new(
                self.connect_timeout,
                &self.extra_roots,
                &self.pins,
            )?),
        };
        let clock: Arc<dyn Clock> = self.clock.unwrap_or_else(|| Arc::new(SystemClock));
        let cfg = Config {
            instance,
            base,
            source,
            max_lifetime: self.max_lifetime,
            max_cache_bytes: self.max_cache_bytes,
            total_timeout: self.total_timeout,
            accept_stale_mirror: self.accept_stale_mirror,
        };
        Ok(Client::from_parts(cfg, transport, clock))
    }
}

/// Drop one trailing `/`.
pub(crate) fn trim_slash(s: &str) -> &str {
    s.strip_suffix('/').unwrap_or(s)
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "[::1]"
        || host == "::1"
}

/// The URL rules: it parses as an `http::Uri` with a scheme and a host, has no query, and is
/// `https`, or `http` on a loopback host. Returns it with one trailing `/` dropped.
fn check_url(url: &str) -> Result<String, ConfigError> {
    let bad = |why| ConfigError::BadUrl {
        url: url.to_owned(),
        why,
    };
    let uri: http::Uri = url.parse().map_err(|_| bad("it does not parse"))?;
    let scheme = uri.scheme_str().ok_or_else(|| bad("it has no scheme"))?;
    let host = uri
        .host()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| bad("it has no host"))?;
    if uri.query().is_some() {
        return Err(bad("it has a query"));
    }
    let secure = scheme.eq_ignore_ascii_case("https")
        || (scheme.eq_ignore_ascii_case("http") && is_loopback(host));
    if !secure {
        return Err(ConfigError::InsecureUrl {
            url: url.to_owned(),
        });
    }
    Ok(trim_slash(url).to_owned())
}
