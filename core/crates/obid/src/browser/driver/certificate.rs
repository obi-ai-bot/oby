//! Reading a server's certificate the way Go on anyway needs it (BROWSER-28, the owner's "B:
//! headless, trust key", docs/decisions/2026-11-20-go-on-anyway.md), ported from the lab's
//! `browser_cert.rs`: a TLS handshake to the host that accepts any certificate, only to read the
//! leaf the server presents, never to send anything over it. It answers the certificate's SHA-256
//! fingerprint (what the person approves) and the SHA-256 of its SubjectPublicKeyInfo in base64
//! (what chrome-headless-shell's `--ignore-certificate-errors-spki-list` takes). It owns no state
//! and writes nothing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::browser::engine::SeenCertificate;

/// How long reading a server's certificate may take.
const READ_LIMIT: Duration = Duration::from_secs(10);

/// The leaf certificate `host:port` presents.
pub async fn read(host: &str, port: u16) -> Result<SeenCertificate, String> {
    let leaf = tokio::time::timeout(READ_LIMIT, leaf(host, port))
        .await
        .map_err(|_| format!("{host}:{port} didn't answer TLS within 10 s"))??;
    Ok(SeenCertificate {
        host: host.to_string(),
        port,
        fingerprint: hex::encode(Sha256::digest(&leaf)),
        spki: spki_hash(&leaf)?,
    })
}

/// The host (lowercase) and port of an `https:` URL (443 by default), or None for any other URL.
pub fn https_origin(url: &str) -> Option<(String, u16)> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        let port = after.strip_prefix(':').map(str::parse).unwrap_or(Ok(443));
        (h.to_string(), port.ok()?)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()?),
            None => (authority.to_string(), 443),
        }
    };
    (!host.is_empty()).then(|| (host.to_ascii_lowercase(), port))
}

async fn leaf(host: &str, port: u16) -> Result<Vec<u8>, String> {
    let seen: Arc<Mutex<Option<Vec<u8>>>> = Arc::default();
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(ReadOnly {
            seen: seen.clone(),
            provider,
        }))
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| format!("{host} isn't a host name or address"))?;
    let tcp = tokio::net::TcpStream::connect((host, port))
        .await
        .map_err(|e| format!("{host}:{port} refused the connection: {e}"))?;
    // The handshake's end doesn't matter: the leaf is read before it.
    let _ = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await;
    let got = seen.lock().unwrap_or_else(|e| e.into_inner()).take();
    got.ok_or_else(|| format!("{host}:{port} presented no certificate"))
}

/// A verifier that only reads the leaf: it vouches for the certificate for this read alone, and
/// nothing is sent over the connection.
#[derive(Debug)]
struct ReadOnly {
    seen: Arc<Mutex<Option<Vec<u8>>>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for ReadOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(end_entity.to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// SHA-256 of a certificate's SubjectPublicKeyInfo (its whole DER element), base64.
pub fn spki_hash(cert: &[u8]) -> Result<String, String> {
    let spki = spki_of(cert)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(Sha256::digest(spki)))
}

/// The SubjectPublicKeyInfo element of an X.509 certificate's DER: Certificate is a SEQUENCE whose
/// first element, tbsCertificate, is a SEQUENCE of [0] version (optional), serialNumber, signature,
/// issuer, validity, subject, subjectPublicKeyInfo.
fn spki_of(cert: &[u8]) -> Result<&[u8], String> {
    let (_, cert_body, _) = element(cert, 0x30)?;
    let (_, tbs, _) = element(cert_body, 0x30)?;
    let mut rest = tbs;
    if rest.first() == Some(&0xa0) {
        rest = element(rest, 0xa0)?.2;
    }
    // serialNumber, signature, issuer, validity, subject
    for tag in [0x02, 0x30, 0x30, 0x30, 0x30] {
        rest = element(rest, tag)?.2;
    }
    Ok(element(rest, 0x30)?.0)
}

/// A DER element: the whole element, its contents, and what follows it.
type Element<'a> = (&'a [u8], &'a [u8], &'a [u8]);

/// One DER element with tag `tag` at the start of `der`.
fn element(der: &[u8], tag: u8) -> Result<Element<'_>, String> {
    let bad = |why: &str| format!("not a certificate: {why}");
    let (&got, rest) = der.split_first().ok_or_else(|| bad("no DER element"))?;
    if got != tag {
        return Err(bad(&format!("DER tag {got:#04x}, expected {tag:#04x}")));
    }
    let (&first, rest) = rest.split_first().ok_or_else(|| bad("no DER length"))?;
    let (len, rest) = if first < 0x80 {
        (first as usize, rest)
    } else {
        let n = (first & 0x7f) as usize;
        if !(1..=4).contains(&n) || rest.len() < n {
            return Err(bad("a DER length"));
        }
        let len = rest[..n].iter().fold(0usize, |a, b| (a << 8) | *b as usize);
        (len, &rest[n..])
    };
    if rest.len() < len {
        return Err(bad("a DER element runs past its end"));
    }
    let header = der.len() - rest.len();
    Ok((&der[..header + len], &rest[..len], &rest[len..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A self-signed P-256 certificate for 127.0.0.1 (`openssl req -x509 -newkey ec`), with its
    /// SPKI hash as `openssl x509 -pubkey | openssl pkey -pubin -outform der | openssl dgst
    /// -sha256 -binary | base64` prints it.
    const CERT_PEM: &str = include_str!("certificate_fixture.pem");
    const CERT_SPKI: &str = include_str!("certificate_fixture.spki");

    fn der() -> Vec<u8> {
        let b64: String = CERT_PEM
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap()
    }

    #[test]
    fn browser_28_the_spki_hash_is_chromes() {
        assert_eq!(spki_hash(&der()).unwrap(), CERT_SPKI.trim());
    }

    #[test]
    fn browser_28_a_truncated_certificate_is_an_error_not_a_hash() {
        let d = der();
        assert!(spki_hash(&d[..d.len() / 3]).is_err());
        assert!(spki_hash(b"not a certificate").is_err());
    }

    #[test]
    fn browser_28_https_origins() {
        assert_eq!(
            https_origin("https://127.0.0.1:18443/a?b"),
            Some(("127.0.0.1".into(), 18443))
        );
        assert_eq!(
            https_origin("https://Staging.Acme.dev/x"),
            Some(("staging.acme.dev".into(), 443))
        );
        assert_eq!(
            https_origin("https://[::1]:8443/"),
            Some(("::1".into(), 8443))
        );
        assert_eq!(https_origin("http://127.0.0.1:1/"), None);
        assert_eq!(https_origin("chrome-error://chromewebdata/"), None);
    }
}
