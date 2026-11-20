//! A server's certificate as obid read it where a tab stopped (BROWSER-28): its host and port,
//! its SHA-256 fingerprint (what the person's Go on anyway names) and its key's hash (what
//! chrome-headless-shell's `--ignore-certificate-errors-spki-list` trusts). A value the engine
//! keeps on the tab; it writes nothing.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenCertificate {
    pub host: String,
    pub port: u16,
    /// SHA-256 of the certificate's DER, lowercase hex.
    pub fingerprint: String,
    /// SHA-256 of its SubjectPublicKeyInfo's DER, base64.
    pub spki: String,
}
