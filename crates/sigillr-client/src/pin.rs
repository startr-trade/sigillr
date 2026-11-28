//! Server key pinning (R-5): the pin form, the certificate verifier that wraps the default one,
//! and the TLS connector that puts it into the `ureq` agent.
//!
//! A pin is `sha256//` followed by the standard base64 of the SHA-256 of the server
//! certificate's SubjectPublicKeyInfo: the form `curl --pinnedpubkey` takes. With pins set, a
//! handshake first passes the usual checks (chain to the trust roots, host name), and is then
//! refused unless the end-entity certificate's key is one of the pins. A refused handshake
//! sends no request.

use std::fmt;
use std::io::{Read, Write};
use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::crypto::hash::{Hash, HashAlgorithm};
use rustls::server::ParsedCertificate;
use rustls::{
    CertificateError, ClientConfig, ClientConnection, DigitallySignedStruct, OtherError,
    RootCertStore, SignatureScheme, StreamOwned, SupportedCipherSuite,
};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport, TransportAdapter,
};

use crate::builder::ConfigError;

/// The prefix of every pin.
const PREFIX: &str = "sha256//";

/// One pin: the SHA-256 of a SubjectPublicKeyInfo.
pub(crate) type Pin = [u8; 32];

/// Parse a pin of the form `sha256//<base64 of 32 bytes>`. Anything else is
/// [`ConfigError::BadPin`].
pub(crate) fn parse_pin(s: &str) -> Result<Pin, ConfigError> {
    let bad = |why| ConfigError::BadPin {
        pin: s.to_owned(),
        why,
    };
    let b64 = s
        .strip_prefix(PREFIX)
        .ok_or_else(|| bad("it does not start with sha256//"))?;
    let bytes = base64_decode(b64).ok_or_else(|| bad("it is not standard padded base64"))?;
    bytes
        .try_into()
        .map_err(|_| bad("it is not the base64 of 32 bytes"))
}

/// The pin of a DER certificate's key, in the `sha256//…` form, or `None` if the certificate
/// does not parse. Used by the tests to check the pin form against openssl's.
#[cfg(test)]
pub(crate) fn pin_of_certificate(cert: &CertificateDer<'_>, sha256: &dyn Hash) -> Option<String> {
    let digest = spki_sha256(cert, sha256)?;
    Some(format!("{PREFIX}{}", base64_encode(&digest)))
}

/// The SHA-256 of a DER certificate's SubjectPublicKeyInfo.
fn spki_sha256(cert: &CertificateDer<'_>, sha256: &dyn Hash) -> Option<Pin> {
    let parsed = ParsedCertificate::try_from(cert).ok()?;
    let spki = parsed.subject_public_key_info();
    sha256.hash(spki.as_ref()).as_ref().try_into().ok()
}

/// The provider's SHA-256, taken from one of its cipher suites: rustls has no other public
/// way to a provider's hash, and this adds no dependency.
pub(crate) fn provider_sha256(provider: &CryptoProvider) -> Option<&'static dyn Hash> {
    provider
        .cipher_suites
        .iter()
        .map(|s| match s {
            SupportedCipherSuite::Tls13(s) => s.common.hash_provider,
            SupportedCipherSuite::Tls12(s) => s.common.hash_provider,
        })
        .find(|h| h.algorithm() == HashAlgorithm::SHA256)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[cfg(test)]
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Standard base64 with `=` padding; `None` for anything else, including non-zero spare bits.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if s.is_empty() || !s.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).map(|v| v as u32);
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let last = s.len() / 4 - 1;
    for (i, quad) in s.chunks(4).enumerate() {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && i != last) {
            return None;
        }
        let mut n = 0u32;
        for &c in &quad[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        let keep = 3 - pad;
        if bytes[keep..].iter().any(|&b| b != 0) {
            return None;
        }
        out.extend_from_slice(&bytes[..keep]);
    }
    Some(out)
}

/// The error a pin mismatch carries through rustls and ureq, recognised by
/// [`is_not_pinned`].
#[derive(Debug)]
pub(crate) struct NotPinned;

impl fmt::Display for NotPinned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the server's TLS key is not one of the pinned keys")
    }
}

impl std::error::Error for NotPinned {}

/// Whether `e` is, or wraps, a [`NotPinned`].
pub(crate) fn is_not_pinned(e: &ureq::Error) -> bool {
    match e {
        ureq::Error::Other(b) => b.downcast_ref::<NotPinned>().is_some(),
        ureq::Error::Rustls(r) => rustls_is_not_pinned(r),
        _ => false,
    }
}

fn rustls_is_not_pinned(e: &rustls::Error) -> bool {
    matches!(e, rustls::Error::InvalidCertificate(CertificateError::Other(OtherError(o)))
        if o.downcast_ref::<NotPinned>().is_some())
}

/// The default verifier first, then the pin check on the end-entity certificate's key.
struct PinningVerifier {
    inner: Arc<WebPkiServerVerifier>,
    pins: Vec<Pin>,
    sha256: &'static dyn Hash,
}

impl fmt::Debug for PinningVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinningVerifier")
            .field("pins", &self.pins.len())
            .finish_non_exhaustive()
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let verified = self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )?;
        match spki_sha256(end_entity, self.sha256) {
            Some(d) if self.pins.contains(&d) => Ok(verified),
            _ => Err(rustls::Error::InvalidCertificate(CertificateError::Other(
                OtherError(Arc::new(NotPinned)),
            ))),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// The rustls client configuration of a pinned client: the provider, these roots, the
/// [`PinningVerifier`], no client certificate.
pub(crate) fn pinned_client_config(
    provider: Arc<CryptoProvider>,
    roots: &[CertificateDer<'static>],
    pins: Vec<Pin>,
) -> Result<Arc<ClientConfig>, ConfigError> {
    let tls = |e: &dyn fmt::Display| ConfigError::Tls(e.to_string());
    let sha256 = provider_sha256(&provider)
        .ok_or_else(|| ConfigError::Tls("the crypto provider has no SHA-256".to_owned()))?;
    let mut store = RootCertStore::empty();
    // As ureq does for its own configuration: roots that do not parse are skipped.
    store.add_parsable_certificates(roots.iter().cloned());
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(store), Arc::clone(&provider))
        .build()
        .map_err(|e| tls(&e))?;
    let verifier = PinningVerifier {
        inner,
        pins,
        sha256,
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| tls(&e))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// The TLS step of a pinned client's connector chain (after `TcpConnector`). It refuses a
/// plain-http connection, since such a server presents no key that could match a pin.
pub(crate) struct PinnedTlsConnector {
    config: Arc<ClientConfig>,
}

impl PinnedTlsConnector {
    pub(crate) fn new(config: Arc<ClientConfig>) -> Self {
        PinnedTlsConnector { config }
    }
}

impl fmt::Debug for PinnedTlsConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinnedTlsConnector").finish_non_exhaustive()
    }
}

impl<In: Transport> Connector<In> for PinnedTlsConnector {
    type Out = PinnedTlsTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };
        if !details.needs_tls() || transport.is_tls() {
            return Err(ureq::Error::Other(Box::new(NotPinned)));
        }
        let host = details
            .uri
            .host()
            .ok_or(ureq::Error::Tls("the URL has no host"))?;
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        let name = ServerName::try_from(host)
            .map_err(|_| ureq::Error::Tls("the host is not a valid TLS server name"))?
            .to_owned();
        let mut conn = ClientConnection::new(Arc::clone(&self.config), name)?;
        let mut sock = TransportAdapter::new(transport.boxed(), details.current_time.clone());
        sock.set_timeout(details.timeout);
        if let Err(e) = conn.complete_io(&mut sock) {
            let rustls_err = e
                .get_ref()
                .and_then(|i| i.downcast_ref::<rustls::Error>())
                .cloned();
            return Err(match rustls_err {
                Some(r) if rustls_is_not_pinned(&r) => ureq::Error::Other(Box::new(NotPinned)),
                Some(r) => ureq::Error::Rustls(r),
                None => ureq::Error::from(e),
            });
        }
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(PinnedTlsTransport {
            buffers,
            stream: StreamOwned { conn, sock },
        }))
    }
}

/// A TLS connection whose server key matched a pin.
pub(crate) struct PinnedTlsTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}

impl fmt::Debug for PinnedTlsTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinnedTlsTransport").finish_non_exhaustive()
    }
}

impl Transport for PinnedTlsTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use rustls_pki_types::pem::PemObject;

    use super::*;

    /// The pin of fixture A, as openssl gives it:
    /// `openssl x509 -in tests/fixtures/pin_a.cert.pem -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | base64`
    const PIN_A: &str = "sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM=";

    const CERT_A: &[u8] = include_bytes!("../tests/fixtures/pin_a.cert.pem");

    fn sha256() -> &'static dyn Hash {
        provider_sha256(&rustls_graviola::default_provider()).unwrap()
    }

    #[test]
    fn pin_of_fixture_a_matches_openssl() {
        let cert = CertificateDer::from_pem_slice(CERT_A).unwrap();
        assert_eq!(pin_of_certificate(&cert, sha256()).as_deref(), Some(PIN_A));
        assert_eq!(
            parse_pin(PIN_A).unwrap(),
            spki_sha256(&cert, sha256()).unwrap()
        );
    }

    #[test]
    fn pin_parser_refuses_other_forms() {
        let b64 = PIN_A.strip_prefix(PREFIX).unwrap();
        for bad in [
            "",
            b64,
            "sha256/kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM=",
            "sha1//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM=",
            "sha256//",
            "sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM",
            "sha256//kQ0Dg2NlQ1BqgjWl_9HCtS99SPp2edqtyzC4nHPz1OM=",
            "sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1ON=",
            "sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz",
            "sha256//AAAA",
            " sha256//kQ0Dg2NlQ1BqgjWl/9HCtS99SPp2edqtyzC4nHPz1OM=",
        ] {
            assert!(
                matches!(parse_pin(bad), Err(ConfigError::BadPin { .. })),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn base64_round_trips() {
        for n in 0..40u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            let enc = base64_encode(&bytes);
            if n > 0 {
                assert_eq!(base64_decode(&enc).unwrap(), bytes);
            }
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
    }
}
