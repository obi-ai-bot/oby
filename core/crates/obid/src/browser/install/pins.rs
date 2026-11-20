//! The pins obid downloads against (BROWSER-23): agent-browser 0.43.0's npm tarball and its
//! binary, and Chrome for Testing 158.0.8127.36's zip (the version agent-browser 0.43.0 installs),
//! per platform. Google publishes no hashes for Chrome for Testing, so these are obid's own,
//! recorded on 2026-11-20 over HTTPS; agent-browser's tarballs matched npm's `dist.integrity` and
//! their binaries the lab's `PINNED`. A Chrome security update is a new pin here. It owns only
//! these constant tables and writes nothing.

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

/// Per platform: Google's name for it, the zip's SHA-256 and size. linux-arm64 has no build.
const CHROME: &[(&str, &str, &str, u64)] = &[
    (
        "darwin-arm64",
        "mac-arm64",
        "b86d3b393def1f87d369b022abff0a51acb015d6fcb79ddfd1c379c9d72fbbe2",
        200_339_070,
    ),
    (
        "darwin-x64",
        "mac-x64",
        "1390e9b1386a1fd8ec2c4940b9ff59cbbda71b896b85182d7efbd73c58213802",
        213_549_680,
    ),
    (
        "linux-x64",
        "linux64",
        "dba9cc838e1ce269c33ea68170881127bd15d078fc3ca5891c90802819a4d7c7",
        208_062_943,
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

/// Chrome for Testing's pin for `platform`, downloaded from under `base` (Google's bucket but in
/// tests).
pub fn chrome(platform: &str, base: &str) -> Result<Pin, String> {
    let (_, google, sha256, bytes) = CHROME
        .iter()
        .find(|row| row.0 == platform)
        .ok_or_else(|| no_build("Chrome for Testing", platform))?;
    let executable = if google.starts_with("mac") {
        format!(
            "chrome-{google}/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
        )
    } else {
        format!("chrome-{google}/chrome")
    };
    Ok(Pin {
        name: format!("Chrome for Testing {CHROME_VERSION}"),
        url: format!("{base}/{CHROME_VERSION}/{google}/chrome-{google}.zip"),
        sha256: (*sha256).into(),
        bytes: *bytes,
        archive: Archive::Zip,
        folder: format!("chrome-{CHROME_VERSION}"),
        executable,
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
            "https://storage.googleapis.com/chrome-for-testing-public/158.0.8127.36/mac-arm64/chrome-mac-arm64.zip"
        );
        assert!(mac.executable.ends_with("MacOS/Google Chrome for Testing"));
        assert_eq!(
            chrome("linux-x64", CHROME_BASE).unwrap().executable,
            "chrome-linux64/chrome"
        );
        let why = chrome("linux-arm64", CHROME_BASE).unwrap_err();
        assert!(
            why.contains("Chrome for Testing has no build for linux-arm64"),
            "{why}"
        );
    }
}
