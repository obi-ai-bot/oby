//! `obid pair-computer` (docs/contracts/remote.md, COMPUTERS-7 and 8; docs/api/remote.md): what the
//! person's own SSH runs on the other computer when they add it from their Mac. It reads the
//! Mac's name and its device key on stdin, one line each, and nothing else; names the key in
//! obid's `home/pairing` (the SSH login is the owner's presence there, as `home/pairing` already
//! is: crate::devices); registers it through the running obid's own API; makes a one-time
//! invitation for that obid's gateway, never through the Obysphere; and prints one JSON line,
//! `{"device": …, "invite": …}`, which goes back over SSH only.
//!
//! The API token stays on that computer, read by this process only. The name and the key never
//! go on a command line (another user's `ps` would show them), so a key whose `authorized_keys`
//! line forces this command can do nothing but pair.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::addresses;
use crate::config::Config;

/// What the Mac sends, at most: a name and a key, two short lines.
const MOST: u64 = 4096;

/// Why it paired nothing, and the exit code: 2 for what was sent, 1 for this obid.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub code: u8,
    pub said: String,
}

fn refused(code: u8, said: impl Into<String>) -> Refused {
    Refused {
        code,
        said: said.into(),
    }
}

/// The Mac's name and its device key, from stdin as the Mac sends them. Anything but a P-256 key
/// (X9.63, base64) on the second line, or anything after it, is refused.
pub fn read_request(mut input: impl Read) -> Result<(String, String), Refused> {
    const NOT_A_KEY: &str = "That isn't a device key.";
    let mut text = String::new();
    (&mut input)
        .take(MOST + 1)
        .read_to_string(&mut text)
        .map_err(|_| refused(2, NOT_A_KEY))?;
    if text.len() as u64 > MOST {
        return Err(refused(2, NOT_A_KEY));
    }
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let name = lines.next().unwrap_or_default();
    let key = lines.next().unwrap_or_default().trim();
    if lines.any(|l| !l.trim().is_empty()) || !crate::devices::is_p256_public_key(key) {
        return Err(refused(2, NOT_A_KEY));
    }
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect::<String>()
        .trim()
        .to_string();
    let name = if name.is_empty() {
        "Another computer".to_string()
    } else {
        name
    };
    Ok((name, key.to_string()))
}

/// Pairs the Mac that sent `input` with the obid `config` names, and answers the line to print.
pub async fn run(config: &Path, input: impl Read) -> Result<String, Refused> {
    let (name, key) = read_request(input)?;
    let config = Config::load(config).map_err(|e| refused(1, e.to_string()))?;
    let home = &config.mac_home;
    let kept = addresses::read(home);
    let api = kept.as_ref().map_or(config.listen, |a| a.api);
    let gateway = kept
        .as_ref()
        .and_then(|a| a.gateway)
        .or(config.gateway.filter(|g| g.port() != 0))
        .ok_or_else(|| {
            refused(
                1,
                "This obid has no gateway, so no other computer can reach it: set \"gateway\" (an address on 127.0.0.1) in its configuration and restart it.",
            )
        })?;
    let token = std::fs::read_to_string(home.join(crate::token::FILE))
        .map(|t| t.trim_end_matches(['\n', '\r']).to_string())
        .map_err(|_| {
            refused(
                1,
                "obid hasn't started on this computer yet: start it, then add this computer again.",
            )
        })?;
    crate::store::write_private(&home.join("pairing"), format!("{key}\n").as_bytes())
        .map_err(|e| refused(1, format!("can't name the key in obid's home: {e}")))?;
    let answer = register_and_invite(api, gateway, &token, &name, &key).await;
    if answer.is_err() {
        // Used up by a registration that went through; a refused one leaves nothing to take.
        let _ = std::fs::remove_file(home.join("pairing"));
    }
    answer
}

async fn register_and_invite(
    api: std::net::SocketAddr,
    gateway: std::net::SocketAddr,
    token: &str,
    name: &str,
    key: &str,
) -> Result<String, Refused> {
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| refused(1, e.to_string()))?;
    let call = async |path: &str, body: Value| -> Result<Value, Refused> {
        let answer = http
            .post(format!("http://{api}{path}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|_| refused(1, format!("obid doesn't answer on {api}: is it running?")))?;
        let status = answer.status();
        let body: Value = answer.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let said = body["error"].as_str().unwrap_or("no reason given");
            return Err(refused(
                1,
                format!("obid refused {path} ({status}): {said}"),
            ));
        }
        Ok(body)
    };
    let device = call("/v1/devices", json!({"name": name, "publicKey": key})).await?;
    let invite = call(
        "/v1/pairing/invite",
        json!({"url": format!("http://{gateway}"), "name": name, "computer": true}),
    )
    .await?;
    Ok(json!({"device": device, "invite": invite}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device's key from a fixed private scalar: no randomness, no key in the source.
    fn key() -> String {
        use base64::Engine as _;
        use p256::elliptic_curve::sec1::ToEncodedPoint as _;
        let secret = p256::SecretKey::from_slice(&[7; 32]).unwrap();
        let point = secret.public_key().to_encoded_point(false);
        base64::engine::general_purpose::STANDARD.encode(point.as_bytes())
    }

    #[test]
    fn computers_7_it_reads_a_name_and_a_device_key_and_nothing_else() {
        let (name, k) = read_request(format!("Studio Mac\n{}\n", key()).as_bytes()).unwrap();
        assert_eq!((name.as_str(), k.as_str()), ("Studio Mac", key().as_str()));
        let (name, _) = read_request(format!("\u{1b}[2J\r\n{}", key()).as_bytes()).unwrap();
        assert_eq!(name, "[2J", "control characters go");
        let (name, _) = read_request(format!("\n{}\n", key()).as_bytes()).unwrap();
        assert_eq!(name, "Another computer");
        for wrong in [
            String::new(),
            "id -un\n".into(),
            "Mac\nnot-a-key\n".into(),
            format!("Mac\n{}\nrm -rf ~\n", key()),
            format!("Mac\n{}", "A".repeat(5000)),
        ] {
            let refusal = read_request(wrong.as_bytes()).unwrap_err();
            assert_eq!(refusal.code, 2, "{wrong:?}");
            assert_eq!(refusal.said, "That isn't a device key.");
        }
    }
}
