//! The pins obid downloads against (BROWSER-23): agent-browser 0.43.0's npm tarball and its binary,
//! and chrome-headless-shell 158.0.8127.36's zip (the version agent-browser 0.43.0 installs as
//! Chrome for Testing; the owner's choice, docs/decisions/2026-11-20-headless-shell.md), per
//! platform. Google publishes no hashes for these zips, so these are obid's own, recorded on
//! 2026-11-20 over HTTPS and matching the lab's `SHELL_PINNED`; agent-browser's tarballs matched
//! npm's `dist.integrity` and their binaries the lab's `PINNED`. A Chrome security update is a new
//! pin here. It owns only these constant tables and writes nothing.

use std::path::Path;

use super::{Archive, Options, Pin};

pub const AGENT_BROWSER_VERSION: &str = "0.43.0";
pub const CHROME_VERSION: &str = "158.0.8127.36";
pub const AGENT_BROWSER_BASE: &str = "https://registry.npmjs.org/@agent-browser";
pub const CHROME_BASE: &str = "https://storage.googleapis.com/chrome-for-testing-public";

/// Per platform: npm's name for it, the tarball's SHA-256, its size, and its binary's SHA-256.
const AGENT_BROWSER: &[(&str, &str, &str, u64, &str)] = &[
    (
        "darwin-arm64",
        "darwin-arm64",
        "c8b9457c850956a40dc5a4895505fbf3f222e31b2ad3eec9ce6b43bd04842fc2",
        8_200_878,
        "05338a8eeddf0b3f9ebf0c87a6c21aedc10685608c68618e1a8483c43eae9c3f",
    ),
    (
        "darwin-x64",
        "darwin-x64",
        "0ff552ff465b35c121dd208103b165d1d30db470b2b201e58e9a7b2fc78928d8",
        8_616_135,
        "49a5ab27297213c1bc0005701a6a4542399abfc9d2cb7520ab519ebb94b8b13d",
    ),
    (
        "linux-x64",
        "linux-x64-gnu",
        "aa00b14ee8294edfeb1bc01bc6418cf4b7e50d75b046a9ab497c10384207383f",
        8_815_814,
        "8e69c9fb662514c9cf36e4911e94e913742ba066e55102486350e049dd847612",
    ),
    (
        "linux-arm64",
        "linux-arm64-gnu",
        "465823648aef4a94d20bc2ae91beffdef8f68d6750a795dddbc2d13928209435",
        8_300_261,
        "34be55c2b92d8d199fb9d1d62a905ee5b6ac5b1f3e8c24e5b20597f370d905e5",
    ),
];

/// Per platform: Google's name for it, chrome-headless-shell's zip's SHA-256 and size.
/// linux-arm64 has no build.
const CHROME: &[(&str, &str, &str, u64)] = &[
    (
        "darwin-arm64",
        "mac-arm64",
        "c24d40a3786322ff3a5e3ac6eed162226cbaf07ebe468715407827226b0f8897",
        105_057_133,
    ),
    (
        "darwin-x64",
        "mac-x64",
        "e0c5d07d8f5bcc5bdd5ef370e7daf3770856b585fcc9eecf441c7e3bb2e3d1cf",
        112_306_247,
    ),
    (
        "linux-x64",
        "linux64",
        "2d3a23017323c54fd457a678e4351f895c4076d8050e39d5aa6247f93448b73a",
        132_087_165,
    ),
];

/// This computer's platform, as the pins name it, or None where the browser has no build.
pub fn this_platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

/// agent-browser's pin for `platform`, downloaded from under `base` (npm's registry but in tests).
pub fn agent_browser(platform: &str, base: &str) -> Result<Pin, String> {
    let (_, npm, sha256, bytes, binary) = AGENT_BROWSER
        .iter()
        .find(|row| row.0 == platform)
        .ok_or_else(|| no_build("agent-browser", platform))?;
    Ok(Pin {
        name: format!("agent-browser {AGENT_BROWSER_VERSION}"),
        url: format!("{base}/cli-{npm}/-/cli-{npm}-{AGENT_BROWSER_VERSION}.tgz"),
        sha256: (*sha256).into(),
        bytes: *bytes,
        archive: Archive::TarGz,
        folder: format!("agent-browser-{AGENT_BROWSER_VERSION}"),
        executable: "package/bin/agent-browser".into(),
        executable_sha256: Some((*binary).into()),
    })
}

/// chrome-headless-shell's pin for `platform`, downloaded from under `base` (Google's bucket but
/// in tests).
pub fn chrome(platform: &str, base: &str) -> Result<Pin, String> {
    let (_, google, sha256, bytes) = CHROME
        .iter()
        .find(|row| row.0 == platform)
        .ok_or_else(|| no_build("chrome-headless-shell", platform))?;
    Ok(Pin {
        name: format!("chrome-headless-shell {CHROME_VERSION}"),
        url: format!("{base}/{CHROME_VERSION}/{google}/chrome-headless-shell-{google}.zip"),
        sha256: (*sha256).into(),
        bytes: *bytes,
        archive: Archive::Zip,
        folder: format!("chrome-headless-shell-{CHROME_VERSION}"),
        executable: format!("chrome-headless-shell-{google}/chrome-headless-shell"),
        executable_sha256: None,
    })
}

fn no_build(what: &str, platform: &str) -> String {
    format!(
        "{what} has no build for {platform}, so Oby's browser can't run on this computer (it runs on macOS, and on Linux on x86-64)."
    )
}

/// This computer's pins, downloaded from where they are published, installed under `home`.
pub fn pinned_options(home: &Path) -> Options {
    let platform = this_platform().unwrap_or(std::env::consts::OS);
    Options::new(
        home.to_path_buf(),
        agent_browser(platform, AGENT_BROWSER_BASE),
        chrome(platform, CHROME_BASE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_23_every_platform_has_its_pins_and_linux_arm64_has_no_chrome_said_plainly() {
        for p in ["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64"] {
            let pin = agent_browser(p, AGENT_BROWSER_BASE).unwrap();
            assert_eq!(pin.sha256.len(), 64, "{p}");
            assert!(pin.url.ends_with("-0.43.0.tgz"), "{}", pin.url);
            assert!(pin.executable_sha256.is_some());
        }
        assert_eq!(
            agent_browser("darwin-arm64", AGENT_BROWSER_BASE)
                .unwrap()
                .url,
            "https://registry.npmjs.org/@agent-browser/cli-darwin-arm64/-/cli-darwin-arm64-0.43.0.tgz"
        );
        let mac = chrome("darwin-arm64", CHROME_BASE).unwrap();
        assert_eq!(
            mac.url,
            "https://storage.googleapis.com/chrome-for-testing-public/158.0.8127.36/mac-arm64/chrome-headless-shell-mac-arm64.zip"
        );
        assert_eq!(
            mac.executable,
            "chrome-headless-shell-mac-arm64/chrome-headless-shell"
        );
        assert_eq!(mac.folder, "chrome-headless-shell-158.0.8127.36");
        assert_eq!(mac.bytes, 105_057_133);
        assert_eq!(
            chrome("linux-x64", CHROME_BASE).unwrap().executable,
            "chrome-headless-shell-linux64/chrome-headless-shell"
        );
        let why = chrome("linux-arm64", CHROME_BASE).unwrap_err();
        assert!(
            why.contains("chrome-headless-shell has no build for linux-arm64"),
            "{why}"
        );
    }
}
