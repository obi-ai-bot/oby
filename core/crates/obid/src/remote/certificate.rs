//! This computer's own certificate for its direct addresses (docs/contracts/remote.md,
//! COMPUTERS-10): a self-signed P-256 certificate obid makes the first time it serves a direct
//! address, and keeps in `home/keys` (`direct-certificate.der` and `direct-key.p8`, 0600). Its
//! SHA-256 is the pin another computer's window trusts; there is no CA and no name to check, so
//! it never expires in use (its `notAfter` is RFC 5280's "no well-defined expiration",
//! 99991231235959Z). A new one is made only when the files are gone or don't read, which every
//! window that pinned the old one refuses until it's added again.

use std::path::{Path, PathBuf};

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair as _};

use super::pin::Pin;

const CERTIFICATE: &str = "direct-certificate.der";
const KEY: &str = "direct-key.p8";

/// The certificate (DER), its private key (PKCS#8 v1, DER) and its pin.
pub struct Certificate {
    pub der: Vec<u8>,
    pub key: Vec<u8>,
    pub pin: Pin,
}

fn paths(home: &Path) -> (PathBuf, PathBuf) {
    let keys = home.join("keys");
    (keys.join(CERTIFICATE), keys.join(KEY))
}

/// The one kept in `home/keys`, or a new one made and kept now.
pub fn load_or_make(home: &Path, name: &str) -> Result<Certificate, String> {
    let (certificate, key) = paths(home);
    if let (Ok(der), Ok(key)) = (std::fs::read(&certificate), std::fs::read(&key))
        && EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &key)
            .is_ok_and(|pair| holds(&der, pair.public_key().as_ref()))
    {
        let pin = Pin::of_certificate(&der);
        return Ok(Certificate { der, key, pin });
    }
    let made = make(name, obi_protocol::time::now())?;
    crate::store::make_private_dir(&home.join("keys"))
        .map_err(|e| format!("can't make {}: {e}", home.join("keys").display()))?;
    crate::store::write_private(&key, &made.key)
        .and_then(|()| crate::store::write_private(&certificate, &made.der))
        .map_err(|e| format!("can't keep the direct certificate: {e}"))?;
    tracing::info!("made this computer's direct certificate, {}", made.pin);
    Ok(made)
}

/// Whether `der` carries `public` (the key pair's own point) as its key.
fn holds(der: &[u8], public: &[u8]) -> bool {
    der.windows(public.len()).any(|w| w == public)
}

/// A new self-signed certificate for `name` (its subject's common name, as a person reads it),
/// valid from a day before `now` (seconds since the epoch).
pub fn make(name: &str, now: i64) -> Result<Certificate, String> {
    let rng = SystemRandom::new();
    let key = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
        .map_err(|_| "can't make a P-256 key".to_string())?;
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, key.as_ref())
        .map_err(|_| "can't read the P-256 key just made".to_string())?;
    let mut serial = [0u8; 16];
    getrandom::fill(&mut serial).map_err(|e| format!("no randomness: {e}"))?;
    serial[0] &= 0x7f;
    serial[0] |= 0x01;
    let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let name = der::sequence(&[der::set(&[der::sequence(&[
        der::oid(OID_COMMON_NAME),
        der::utf8(if name.is_empty() { "Oby" } else { &name }),
    ])])]);
    let algorithm = der::sequence(&[der::oid(OID_ECDSA_SHA256)]);
    let tbs = der::sequence(&[
        der::explicit(0, &der::integer(&[2])),
        der::integer(&serial),
        algorithm.clone(),
        name.clone(),
        der::sequence(&[
            der::utc_time(now - 86_400),
            der::generalized("99991231235959Z"),
        ]),
        name,
        der::sequence(&[
            der::sequence(&[der::oid(OID_EC_PUBLIC_KEY), der::oid(OID_P256)]),
            der::bit_string(pair.public_key().as_ref()),
        ]),
    ]);
    let signature = pair
        .sign(&rng, &tbs)
        .map_err(|_| "can't sign the certificate".to_string())?;
    let der = der::sequence(&[tbs, algorithm, der::bit_string(signature.as_ref())]);
    Ok(Certificate {
        pin: Pin::of_certificate(&der),
        der,
        key: key.as_ref().to_vec(),
    })
}

const OID_COMMON_NAME: &[u8] = &[0x55, 0x04, 0x03];
const OID_ECDSA_SHA256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];

/// Just enough DER for one certificate.
mod der {
    fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        match body.len() {
            n if n < 0x80 => out.push(n as u8),
            n if n <= 0xff => out.extend([0x81, n as u8]),
            n => out.extend([0x82, (n >> 8) as u8, n as u8]),
        }
        out.extend_from_slice(body);
        out
    }
    pub fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
        tlv(0x30, &parts.concat())
    }
    pub fn set(parts: &[Vec<u8>]) -> Vec<u8> {
        tlv(0x31, &parts.concat())
    }
    pub fn oid(encoded: &[u8]) -> Vec<u8> {
        tlv(0x06, encoded)
    }
    pub fn utf8(text: &str) -> Vec<u8> {
        tlv(0x0c, text.as_bytes())
    }
    /// A positive integer from its big-endian bytes, its high bit clear.
    pub fn integer(bytes: &[u8]) -> Vec<u8> {
        tlv(0x02, bytes)
    }
    pub fn bit_string(bytes: &[u8]) -> Vec<u8> {
        let mut body = vec![0];
        body.extend_from_slice(bytes);
        tlv(0x03, &body)
    }
    pub fn explicit(n: u8, inner: &[u8]) -> Vec<u8> {
        tlv(0xa0 | n, inner)
    }
    pub fn utc_time(seconds: i64) -> Vec<u8> {
        let text = obi_protocol::time::format(seconds);
        // 2026-11-20T13:24:26Z → 261120132426Z
        let digits: String = text.chars().filter(char::is_ascii_digit).collect();
        tlv(0x17, format!("{}Z", &digits[2..14]).as_bytes())
    }
    pub fn generalized(text: &str) -> Vec<u8> {
        tlv(0x18, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_10_a_made_certificate_is_kept_and_its_pin_stays() {
        let home = std::env::temp_dir().join(format!("obid-certificate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let first = load_or_make(&home, "Studio").unwrap();
        assert_eq!(first.pin, Pin::of_certificate(&first.der));
        let again = load_or_make(&home, "Studio").unwrap();
        assert_eq!(again.pin, first.pin, "kept across starts");
        use std::os::unix::fs::PermissionsExt as _;
        for file in [CERTIFICATE, KEY] {
            let mode = std::fs::metadata(home.join("keys").join(file))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{file}");
        }
        // A key that isn't the certificate's: a new pair, a new pin.
        let other = make("Studio", 0).unwrap();
        std::fs::write(home.join("keys").join(KEY), &other.key).unwrap();
        let fresh = load_or_make(&home, "Studio").unwrap();
        assert_ne!(fresh.pin, first.pin);
        std::fs::remove_dir_all(home.join("keys")).unwrap();
        assert_ne!(load_or_make(&home, "Studio").unwrap().pin, fresh.pin);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn computers_10_the_certificate_is_one_rustls_serves() {
        let made = make("Studio \u{1b}box", 1_800_000_000).unwrap();
        let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let served = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![rustls::pki_types::CertificateDer::from(made.der.clone())],
                rustls::pki_types::PrivateKeyDer::Pkcs8(made.key.clone().into()),
            );
        assert!(served.is_ok(), "{:?}", served.err());
        assert_ne!(make("Studio", 0).unwrap().pin, made.pin, "each one new");
    }
}
