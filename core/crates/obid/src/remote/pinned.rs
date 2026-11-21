//! The window's client for another computer's direct address (docs/contracts/remote.md,
//! COMPUTERS-11): it trusts exactly the pinned certificate, and still checks the handshake is
//! signed by that certificate's key. No CA and no name: whoever else answers at that address,
//! another machine after a network change or a new certificate on the same one, is refused, with
//! [`NOT_THE_PINNED`] and the certificate it showed.

use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

use super::pin::Pin;

/// What a refused certificate's error says first.
pub const NOT_THE_PINNED: &str = "another computer answered: its certificate is";

#[derive(Debug)]
pub struct PinnedVerifier {
    pin: Pin,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl PinnedVerifier {
    pub fn new(pin: Pin) -> Self {
        PinnedVerifier {
            pin,
            algorithms: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if self.pin.matches(end_entity.as_ref()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "{NOT_THE_PINNED} {}, not the pinned {}",
                Pin::of_certificate(end_entity.as_ref()),
                self.pin
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// A TLS client configuration that trusts only `pin`.
pub fn tls(pin: Pin) -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier::new(pin)))
        .with_no_client_auth())
}

/// An HTTP client for one computer's direct address: only `pin` is trusted, no proxy, and a
/// request gives up after `limit`.
pub fn client(pin: Pin, limit: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .use_preconfigured_tls(tls(pin)?)
        .no_proxy()
        .connect_timeout(Duration::from_secs(10).min(limit))
        .timeout(limit)
        .build()
        .map_err(|e| e.to_string())
}

/// [`client`] for the window's requests (crate::remote::forward): a route that doesn't answer in
/// `connect` gives way to the next; a whole answer takes at most a minute, but a stream (`whole`
/// false) runs as long as it's followed.
pub fn streaming_client(
    pin: Pin,
    connect: Duration,
    whole: bool,
) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder()
        .use_preconfigured_tls(tls(pin)?)
        .no_proxy()
        .connect_timeout(connect);
    let builder = if whole {
        builder.timeout(Duration::from_secs(60))
    } else {
        builder
    };
    builder.build().map_err(|e| e.to_string())
}

/// What a failed request through [`client`] says, its refused certificate named when that was why.
pub fn said(error: &reqwest::Error) -> String {
    let mut source: Option<&dyn std::error::Error> = Some(error);
    while let Some(e) = source {
        let text = e.to_string();
        if let Some(at) = text.find(NOT_THE_PINNED) {
            return text[at..].to_string();
        }
        source = e.source();
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_11_a_certificate_other_than_the_pinned_one_is_refused() {
        let a = CertificateDer::from(b"certificate a".to_vec());
        let b = CertificateDer::from(b"certificate b".to_vec());
        let v = PinnedVerifier::new(Pin::of_certificate(a.as_ref()));
        let name = ServerName::try_from("192.168.1.2").unwrap();
        let now = UnixTime::now();
        assert!(v.verify_server_cert(&a, &[], &name, &[], now).is_ok());
        let refused = v.verify_server_cert(&b, &[], &name, &[], now).unwrap_err();
        let text = refused.to_string();
        assert!(text.contains(NOT_THE_PINNED), "{text}");
        assert!(
            text.contains(&Pin::of_certificate(b.as_ref()).to_string()),
            "{text}"
        );
        assert!(tls(Pin([1; 32])).is_ok());
    }
}
