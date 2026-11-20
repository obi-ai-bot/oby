//! Where a driver gets the engine (BROWSER-23): downloaded and checked by obid's
//! [`Installer`] on first use, or already in place (obid's tests' stand-in agent-browser, or an
//! engine placed by hand). It owns no state of its own and writes nothing; the installer writes.

use std::sync::Arc;

use crate::browser::install::{Installed, Installer};

/// Where a driver gets the engine.
#[derive(Clone, Debug)]
pub enum Binaries {
    /// Downloaded and checked on first use.
    Download(Arc<Installer>),
    /// Already in place.
    InPlace(Installed),
}

impl Binaries {
    pub async fn get(&self) -> Result<Installed, String> {
        match self {
            Binaries::Download(installer) => installer.ensure().await,
            Binaries::InPlace(installed) => Ok(installed.clone()),
        }
    }
}
