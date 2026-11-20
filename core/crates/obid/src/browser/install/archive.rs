//! How a pinned download is packed (BROWSER-23): a gzipped tarball (agent-browser's npm package)
//! or a zip (Chrome for Testing). It owns no state and writes nothing.

/// How a pinned download is packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archive {
    /// A gzipped tarball, unpacked with the system's `tar`.
    TarGz,
    /// A zip, unpacked with `ditto` on macOS and `unzip` on Linux.
    Zip,
}
