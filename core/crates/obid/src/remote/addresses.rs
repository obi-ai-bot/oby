//! Where this obid answers, as it bound them (docs/contracts/remote.md, COMPUTERS-9): its API, its
//! gateway and the gateway's direct addresses, kept in `home/addresses.json` (0600) each time it starts, so `obid pair-computer`
//! reaches the obid that runs, a port the system picked (`:0`) included. Nothing in it is a
//! secret, and nothing reads it but `obid pair-computer`.

use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const FILE: &str = "addresses.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Addresses {
    pub api: SocketAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<SocketAddr>,
    /// The gateway over HTTPS, as bound (crate::remote::direct).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct: Vec<SocketAddr>,
}

/// Keeps them, replacing what an earlier start kept.
pub fn keep(home: &Path, addresses: &Addresses) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(addresses).map_err(std::io::Error::other)?;
    crate::store::write_private(&home.join(FILE), &bytes)
}

/// What the last start kept, if it reads.
pub fn read(home: &Path) -> Option<Addresses> {
    let bytes = std::fs::read(home.join(FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computers_9_the_addresses_obid_bound_are_kept_and_read_back() {
        let home = std::env::temp_dir().join(format!("obid-addresses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(read(&home), None);
        let a = Addresses {
            api: "127.0.0.1:50123".parse().unwrap(),
            gateway: Some("127.0.0.1:50124".parse().unwrap()),
            direct: vec!["0.0.0.0:50125".parse().unwrap()],
        };
        keep(&home, &a).unwrap();
        assert_eq!(read(&home), Some(a));
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(home.join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&home).unwrap();
    }
}
