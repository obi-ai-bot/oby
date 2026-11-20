//! A direct address's certificate, pinned (docs/contracts/remote.md, COMPUTERS-2): the SHA-256 of its
//! DER. No CA and no name: the other computer makes its own self-signed certificate, and its pin
//! is what says who answers at an address.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pin(pub [u8; 32]);

impl Pin {
    pub fn of_certificate(der: &[u8]) -> Pin {
        Pin(sha2::Sha256::digest(der).into())
    }

    /// `sha256:<64 hex>`, or OpenSSL's `AB:CD:…` form.
    pub fn parse(text: &str) -> Result<Pin, String> {
        let hex_part = text.trim().strip_prefix("sha256:").unwrap_or(text.trim());
        let clean: String = hex_part.chars().filter(|c| *c != ':').collect();
        let bytes = hex::decode(clean.to_ascii_lowercase())
            .map_err(|_| format!("{text:?} isn't a SHA-256 fingerprint"))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| format!("{text:?} isn't 32 bytes"))?;
        Ok(Pin(bytes))
    }

    pub fn matches(&self, der: &[u8]) -> bool {
        Pin::of_certificate(der) == *self
    }
}

impl std::fmt::Display for Pin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sha256:{}", hex::encode(self.0))
    }
}

impl Serialize for Pin {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Pin {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Pin::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_2_a_pin_reads_both_forms_and_matches_only_its_certificate() {
        let p = Pin::of_certificate(b"cert-a");
        assert_eq!(Pin::parse(&p.to_string()).unwrap(), p);
        let colons: Vec<String> = p.0.iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(Pin::parse(&colons.join(":")).unwrap(), p);
        assert!(p.matches(b"cert-a"));
        assert!(!p.matches(b"cert-b"));
        assert!(Pin::parse("sha256:abcd").is_err());
        assert!(Pin::parse("sha256:zz").is_err());
    }
}
