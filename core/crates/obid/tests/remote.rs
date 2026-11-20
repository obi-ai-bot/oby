//! Other computers (docs/contracts/remote.md), on the real obid binary: `obid pair-computer`, as
//! the person's SSH runs it on the other computer, pairs the Mac that sent its key on stdin with
//! the obid that runs there, at the addresses it bound; and a computer's invitation never goes
//! through the Obysphere.

mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use support::*;

fn agent() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude")
}

/// obid with a gateway on a port the system picks, as `obid pair-computer` must find it.
fn started(folder: &Folder) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    config["gateway"] = json!("127.0.0.1:0");
    start(&folder.0, &config, None)
}

/// A device key, from a fixed private scalar, as the Mac app's (X9.63, base64).
fn device_key(scalar: u8) -> String {
    use base64::Engine as _;
    use p256::elliptic_curve::sec1::ToEncodedPoint as _;
    let secret = p256::SecretKey::from_slice(&[scalar; 32]).unwrap();
    base64::engine::general_purpose::STANDARD
        .encode(secret.public_key().to_encoded_point(false).as_bytes())
}

/// `obid pair-computer --config <the test's>`, `stdin` on its stdin: its exit code, stdout and
/// stderr. Its environment is empty but `PATH`, as a forced command's nearly is.
fn pair_computer(obid: &Obid, stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(OBID)
        .args(["pair-computer", "--config"])
        .arg(obid.folder.join("config.json"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// One request to the gateway's listener, as the Mac sends it through its tunnel.
fn through(
    obid: &Obid,
    token: Option<&str>,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> (u16, String) {
    through_with(obid, token, method, target, body, &[])
}

/// [`through`], with `headers` too.
fn through_with(
    obid: &Obid,
    token: Option<&str>,
    method: &str,
    target: &str,
    body: Option<&Value>,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut stream = TcpStream::connect(("127.0.0.1", obid.gateway.expect("a gateway"))).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let mut text =
        format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(token) = token {
        text.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if method != "GET" {
        text.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    text.push_str("\r\n");
    text.push_str(&body);
    stream.write_all(text.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap();
    (
        head.split(' ').nth(1).unwrap().parse().unwrap(),
        rest.to_owned(),
    )
}

/// COMPUTERS-7 and 9: the Mac's name and key on stdin pair it with the obid that runs, found at the
/// addresses it bound (its gateway on a port the system picked); the Mac redeems the invitation
/// at the gateway and its token reaches obid there. Anything but a key pairs nothing.
#[test]
fn computers_7_obid_pair_computer_pairs_the_mac_that_sent_its_key() {
    let folder = Folder::new("remote-pair-computer");
    let obid = started(&folder);

    // COMPUTERS-9: the addresses obid bound, its gateway's port the system's.
    let kept: Value =
        serde_json::from_slice(&std::fs::read(obid.home.join("addresses.json")).unwrap()).unwrap();
    assert_eq!(kept["api"], format!("127.0.0.1:{}", obid.port), "{kept}");
    assert_eq!(
        kept["gateway"],
        format!("127.0.0.1:{}", obid.gateway.unwrap()),
        "{kept}"
    );

    // Anything but a name and a device key: refused, nothing written.
    for wrong in ["", "id -un\n", "Mac\nnot-a-key\n"] {
        let (code, out, err) = pair_computer(&obid, wrong);
        assert_eq!(code, 2, "{wrong:?}: {out}{err}");
        assert_eq!(err.trim(), "That isn't a device key.");
        assert!(out.is_empty(), "{out}");
        assert!(!obid.home.join("pairing").exists());
    }
    let extra = format!("Mac\n{}\nrm -rf ~\n", device_key(9));
    assert_eq!(pair_computer(&obid, &extra).0, 2);

    let key = device_key(9);
    let (code, out, err) = pair_computer(&obid, &format!("Studio Mac\n{key}\n"));
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out.lines().count(), 1, "one JSON line: {out}");
    let said: Value = serde_json::from_str(out.trim()).unwrap();
    assert!(!out.contains(TOKEN), "the API token stays on that computer");
    assert!(!obid.home.join("pairing").exists(), "used up");

    // Its key is registered: obid gives the same id back for it, with no pairing named.
    let known = call(
        &obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Studio Mac", "publicKey": key})),
    );
    assert_eq!(known.status, 200, "{}", known.text);
    assert_eq!(said["device"]["id"], known.body["id"], "{said}");

    // The invitation is the gateway's, and the Mac redeems it there once.
    let invite = &said["invite"];
    assert!(invite.get("relay").is_none(), "{invite}");
    assert_eq!(
        invite["url"],
        format!("http://127.0.0.1:{}", obid.gateway.unwrap())
    );
    let redeem = json!({"credential": invite["secret"], "name": "Studio Mac"});
    let (status, text) = through(&obid, None, "POST", "/v1/pairing", Some(&redeem));
    assert_eq!(status, 200, "{text}");
    let token = serde_json::from_str::<Value>(text.trim()).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, text) = through(&obid, Some(&token), "GET", "/v1/status", None);
    assert_eq!(status, 200, "{text}");
    let (status, _) = through(&obid, None, "POST", "/v1/pairing", Some(&redeem));
    assert_eq!(status, 410, "once");
}

/// `X-Oby-Words` signed by the device key `scalar` makes (registered as `device`), as the Mac's
/// window signs the owner's words.
fn words_by(scalar: u8, device: &str, method: &str, path: &str, body: &Value) -> String {
    use base64::Engine as _;
    use p256::ecdsa::signature::Signer as _;
    let key = p256::ecdsa::SigningKey::from_slice(&[scalar; 32]).unwrap();
    let whole = obi_protocol::time::format(obi_protocol::time::now());
    let signed_at = format!("{}.123Z", &whole[..19]);
    let digest = hex::encode(obi_protocol::keys::sha256(body.to_string().as_bytes()));
    let message = format!("oby-words\n{method}\n{path}\n{digest}\n{signed_at}");
    let signature: p256::ecdsa::Signature = key.sign(message.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(signature.to_der());
    format!("{device} {signed_at} {signature}")
}

/// What [`computer_and_phone`] paired: the Mac's key and its id, the computer's device token and
/// pairing, and a phone's token beside it.
struct Paired {
    key: String,
    device: String,
    token: String,
    pairing: String,
    phone_token: String,
}

/// A computer paired over SSH with `obid pair-computer` (the Mac's key 9), and a phone paired on
/// the same gateway, for the difference.
fn computer_and_phone(obid: &Obid) -> Paired {
    let gateway = obid.gateway.unwrap();
    let key = device_key(9);
    let (code, out, err) = pair_computer(obid, &format!("Studio Mac\n{key}\n"));
    assert_eq!(code, 0, "{out}{err}");
    let said: Value = serde_json::from_str(out.trim()).unwrap();
    let device = said["device"]["id"].as_str().unwrap().to_owned();
    let redeem = json!({"credential": said["invite"]["secret"], "name": "Studio Mac"});
    let (status, text) = through(obid, None, "POST", "/v1/pairing", Some(&redeem));
    assert_eq!(status, 200, "{text}");
    let paired: Value = serde_json::from_str(text.trim()).unwrap();
    let token = paired["token"].as_str().unwrap().to_owned();
    let pairing = paired["deviceID"].as_str().unwrap().to_owned();
    assert!(
        !obid.home.join("invite-computer").exists(),
        "presence used up"
    );

    // A phone, paired on the same gateway, for the difference.
    let phone = call(
        obid,
        "POST",
        "/v1/pairing/invite",
        Some(&json!({"url": format!("http://127.0.0.1:{gateway}")})),
    );
    assert_eq!(phone.status, 200, "{}", phone.text);
    let (status, text) = through(
        obid,
        None,
        "POST",
        "/v1/pairing",
        Some(&json!({"credential": phone.body["secret"], "name": "iPhone"})),
    );
    assert_eq!(status, 200, "{text}");
    let phone_token = serde_json::from_str::<Value>(text.trim()).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();

    Paired {
        key,
        device,
        token,
        pairing,
        phone_token,
    }
}

/// COMPUTERS-14 to 16, on the real obid: a computer paired over SSH reaches the gateway's list, as
/// a phone does, never a route kept on loopback, and no path read two ways; the owner's words count
/// through its token only signed by the key `obid pair-computer` registered for it; and the Mac
/// removes it with its own token, which ends it and forgets that key there.
#[test]
fn computers_14_to_16_a_computers_token_its_own_key_and_removing_it() {
    let folder = Folder::new("remote-computer-token");
    let obid = started(&folder);
    let Paired {
        key,
        device,
        token,
        pairing,
        phone_token,
    } = computer_and_phone(&obid);

    // COMPUTERS-14: the gateway's list, as a phone's; never a route obid's docs keep on its
    // loopback API (the vault: docs/contracts/vault.md), pairing or a sign-in.
    let (status, text) = through(&obid, Some(&token), "GET", "/v1/repos", None);
    assert_eq!(status, 200, "on the list: {text}");
    for (method, path) in [
        ("GET", "/v1/obid/vault"),
        ("PUT", "/v1/obid/vault/GITHUB_TOKEN"),
        ("DELETE", "/v1/obid/vault/GITHUB_TOKEN"),
        ("GET", "/v1/pairing/devices"),
        ("POST", "/v1/pairing/invite"),
        ("DELETE", "/v1/pairing/devices/x"),
        ("POST", "/v1/tetrix/sign-in"),
        ("POST", "/v1/repos/setup"),
        ("PUT", "/v1/settings/agents/claude"),
    ] {
        let (status, text) = through(
            &obid,
            Some(&token),
            method,
            path,
            Some(&json!({"value": "s3cret"})),
        );
        assert_eq!(status, 404, "{method} {path}: {text}");
        assert!(
            text.contains("isn't available through the gateway"),
            "{text}"
        );
    }
    let vault = call(&obid, "GET", "/v1/obid/vault", None);
    assert!(
        !vault.text.contains("GITHUB_TOKEN"),
        "nothing saved: {}",
        vault.text
    );
    let listed = call(&obid, "GET", "/v1/pairing/devices", None);
    let entry = listed
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == pairing.as_str());
    assert_eq!(entry.unwrap()["computer"], true, "{}", listed.text);

    // COMPUTERS-15: the owner's words count only signed by this pairing's own key.
    let wider = json!({"mode": "bypass"});
    let path = "/v1/settings/permissions";
    let other = words_header(&obid, "PUT", path, Some(&wider));
    let (status, text) = through_with(
        &obid,
        Some(&token),
        "PUT",
        path,
        Some(&wider),
        &[("X-Oby-Words", &other)],
    );
    assert_eq!(status, 403, "another registered key's words: {text}");
    assert!(text.contains("this computer's own key"), "{text}");
    let own = words_by(9, &device, "PUT", path, &wider);
    let (status, text) = through_with(
        &obid,
        Some(&token),
        "PUT",
        path,
        Some(&wider),
        &[("X-Oby-Words", &own)],
    );
    assert_eq!(status, 200, "its own key's words: {text}");

    // COMPUTERS-16: a phone's token ends nothing; the computer's own ends its pairing and key.
    let (status, _) = through(&obid, Some(&phone_token), "DELETE", "/v1/pairing", None);
    assert_eq!(status, 404);
    let (status, _) = through(&obid, None, "DELETE", "/v1/pairing", None);
    assert_eq!(status, 401);
    let (status, text) = through(&obid, Some(&token), "DELETE", "/v1/pairing", None);
    assert_eq!(status, 204, "{text}");
    let (status, _) = through(&obid, Some(&token), "GET", "/v1/status", None);
    assert_eq!(status, 401, "its token is gone");
    let (status, _) = through(&obid, Some(&phone_token), "GET", "/v1/status", None);
    assert_eq!(status, 200, "the phone stays paired");
    let listed = call(&obid, "GET", "/v1/pairing/devices", None);
    assert!(!listed.text.contains(&pairing), "{}", listed.text);
    // Its key is forgotten: registering it again needs the owner's presence once more.
    let again = call(
        &obid,
        "POST",
        "/v1/devices",
        Some(&json!({"name": "Studio Mac", "publicKey": key})),
    );
    assert_eq!(again.status, 403, "{}", again.text);
}

/// COMPUTERS-14, on the real obid: a path that reads two ways is refused at the gateway before
/// its list is read, whoever's token it carries, and none reaches a pairing route.
#[test]
fn computers_14_a_path_read_two_ways_reaches_no_route() {
    let folder = Folder::new("remote-computer-paths");
    let obid = started(&folder);
    let Paired {
        token,
        pairing,
        phone_token,
        ..
    } = computer_and_phone(&obid);
    for path in [
        "/v1/%70airing/devices",
        "/v1/./pairing/devices",
        "//v1/pairing/devices",
        "/v1/pairing/../pairing/devices",
    ] {
        for bearer in [&token, &phone_token] {
            let (status, text) = through(&obid, Some(bearer), "GET", path, None);
            assert_eq!(status, 404, "{path}: {text}");
            assert!(!text.contains(&pairing), "no paired device listed: {text}");
            assert!(
                text.contains("isn't available through the gateway"),
                "{path}: {text}"
            );
        }
    }
}

/// COMPUTERS-7: without a gateway no other computer can reach this obid, and it says so.
#[test]
fn computers_7_without_a_gateway_it_says_so() {
    let folder = Folder::new("remote-pair-no-gateway");
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &agent()), None);
    let (code, out, err) = pair_computer(&obid, &format!("Mac\n{}\n", device_key(9)));
    assert_eq!(code, 1, "{out}{err}");
    assert!(err.contains("has no gateway"), "{err}");
    assert!(!obid.home.join("pairing").exists());
}

/// COMPUTERS-8: a computer's invitation is for its gateway's url only, never through the Obysphere,
/// even while obid is a member of one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computers_8_a_computers_invitation_never_goes_through_the_obysphere() {
    let folder = Folder::new("remote-not-the-sphere");
    let sphere = FakeSphere::start().await;
    let obid = started(&folder);
    let membership =
        tokio::task::block_in_place(|| join_a(&obid, &sphere, &Owner::new("wellington")));
    assert_eq!(membership["state"], "joined");
    let url = format!("http://127.0.0.1:{}", obid.gateway.unwrap());
    let invitation = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/pairing/invite",
            Some(&json!({"url": url, "name": "Studio Mac", "computer": true})),
        )
    });
    assert_eq!(invitation.status, 200, "{}", invitation.text);
    assert_eq!(invitation.body["url"], url.as_str(), "{}", invitation.text);
    assert!(
        invitation.body.get("relay").is_none(),
        "{}",
        invitation.text
    );
    let redeem = json!({"credential": invitation.body["code"], "name": "Studio Mac"});
    let (status, text) =
        tokio::task::block_in_place(|| through(&obid, None, "POST", "/v1/pairing", Some(&redeem)));
    assert_eq!(status, 200, "redeemed on the listener: {text}");

    let no_url = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/pairing/invite",
            Some(&json!({"name": "Studio Mac", "computer": true})),
        )
    });
    assert_eq!(no_url.status, 400, "{}", no_url.text);
}

/// What `GET /v1/status` says of obid's first direct address: its address and its pin.
fn direct_of(obid: &Obid) -> (String, obid::remote::pin::Pin) {
    let direct = within(30, "obid's direct address", || {
        let status = call(obid, "GET", "/v1/status", None);
        Some(status.body["gateway"]["direct"][0].clone()).filter(|d| !d.is_null())
    });
    assert_eq!(direct["state"], "listening", "{direct}");
    (
        direct["address"].as_str().unwrap().to_owned(),
        obid::remote::pin::Pin::parse(direct["pin"].as_str().unwrap()).unwrap(),
    )
}

/// obid's configuration with a gateway and a direct address, both on ports the system picks.
fn with_direct(folder: &Folder) -> Value {
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote, &agent());
    config["gateway"] = json!("127.0.0.1:0");
    config["direct"] = json!(["127.0.0.1:0"]);
    config
}

/// COMPUTERS-10 and 11: obid serves its gateway over HTTPS with a certificate of its own, kept
/// across starts; a computer pairs and calls it there through the pinned client; any other pin is
/// refused, and so is the old pin once the certificate is made anew.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computers_10_obid_serves_its_gateway_over_https_with_its_own_pinned_certificate() {
    use obid::remote::pinned;
    let folder = Folder::new("remote-direct");
    let config = tokio::task::block_in_place(|| with_direct(&folder));
    let obid = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (address, pin) = tokio::task::block_in_place(|| direct_of(&obid));
    let base = format!("https://{address}");
    let kept: Value =
        serde_json::from_slice(&std::fs::read(obid.home.join("addresses.json")).unwrap()).unwrap();
    assert_eq!(kept["direct"][0], address.as_str(), "{kept}");

    // A computer pairs over the direct address, its certificate pinned.
    let invitation = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/pairing/invite",
            Some(&json!({"url": base, "name": "Studio Mac", "computer": true})),
        )
    });
    assert_eq!(invitation.status, 200, "{}", invitation.text);
    let client = pinned::client(pin, Duration::from_secs(30)).unwrap();
    let redeemed = client
        .post(format!("{base}/v1/pairing"))
        .json(&json!({"credential": invitation.body["code"], "name": "Studio Mac"}))
        .send()
        .await
        .unwrap();
    assert_eq!(redeemed.status(), 200);
    let token = redeemed.json::<Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let status = client
        .get(format!("{base}/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    // Only the gateway's routes: the pairing endpoints aren't there.
    let minted = client
        .post(format!("{base}/v1/pairing/invite"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(minted.status(), 404);

    // Another pin: refused before anything is sent.
    let other = pinned::client(obid::remote::pin::Pin([7; 32]), Duration::from_secs(30)).unwrap();
    let refused = other
        .get(format!("{base}/v1/status"))
        .send()
        .await
        .unwrap_err();
    assert!(
        pinned::said(&refused).starts_with(pinned::NOT_THE_PINNED),
        "{refused}"
    );

    // The certificate is kept across a start, and its pin with it.
    drop(obid);
    let obid = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (_, again) = tokio::task::block_in_place(|| direct_of(&obid));
    assert_eq!(again, pin, "the same certificate after a start");

    // Made anew, it's another computer as far as the old pin goes.
    drop(obid);
    for file in ["direct-certificate.der", "direct-key.p8"] {
        std::fs::remove_file(folder.0.join("home/keys").join(file)).unwrap();
    }
    let obid = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (address, fresh) = tokio::task::block_in_place(|| direct_of(&obid));
    assert_ne!(fresh, pin);
    let refused = client
        .get(format!("https://{address}/v1/status"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap_err();
    let said = pinned::said(&refused);
    assert!(said.contains(&fresh.to_string()), "{said}");
}

/// `obid invite-computer --config <the test's>`: its exit code, stdout and stderr.
fn invite_computer(obid: &Obid) -> (i32, String, String) {
    let out = Command::new(OBID)
        .args(["invite-computer", "--config"])
        .arg(obid.folder.join("config.json"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// COMPUTERS-13: the other computer's owner makes a code at it; the Mac reaches it through the
/// link's pinned route before it sends anything, redeems the code and adds its own key there. A
/// link pinned to another certificate sends no code; a wrong code pairs nothing; a phone's
/// invitation doesn't replace the computer's; an invitation made with the API token alone pairs
/// a computer that can't add its key.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computers_13_a_computer_is_added_with_a_code_its_certificate_pinned() {
    use obid::remote::{code, link::Link, pin::Pin, route::Via};
    let folder = Folder::new("remote-code");
    let config = tokio::task::block_in_place(|| with_direct(&folder));
    let obid = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (address, pin) = tokio::task::block_in_place(|| direct_of(&obid));

    let (status, out, err) = tokio::task::block_in_place(|| invite_computer(&obid));
    assert_eq!(status, 0, "{out}{err}");
    let made: Value = serde_json::from_str(out.trim()).unwrap();
    let code_text = made["code"].as_str().unwrap().to_owned();
    let link_text = made["link"].as_str().unwrap().to_owned();
    let link = Link::decode(&link_text).unwrap();
    assert!(
        matches!(&link.routes[0].via, Via::Https { pin: p, port, .. } if *p == pin && address.ends_with(&format!(":{port}"))),
        "{link:?}"
    );
    assert_eq!(made["fingerprint"].as_str().unwrap().len(), 39, "{made}");
    assert!(
        !obid.home.join("invite-computer").exists(),
        "presence used up"
    );

    // A phone's invitation now doesn't replace the computer's.
    let phone = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/pairing/invite",
            Some(&json!({"url": "https://mac.tail0000.ts.net:8443", "name": "My Mac"})),
        )
    });
    assert_eq!(phone.status, 200, "{}", phone.text);

    // Pinned to another certificate: refused at the handshake, the code never sent.
    let mut impostor = link.clone();
    for route in &mut impostor.routes {
        if let Via::Https { pin, .. } = &mut route.via {
            *pin = Pin([7; 32]);
        }
    }
    let key = device_key(11);
    let refused = code::redeem(&impostor.encode(), &code_text, "Studio Mac", &key)
        .await
        .unwrap_err();
    assert!(
        refused.said.contains("another computer answered"),
        "{refused:?}"
    );

    // A wrong code pairs nothing.
    let wrong = code::redeem(&link_text, "000000", "Studio Mac", &key)
        .await
        .unwrap_err();
    assert_eq!(wrong.status, Some(401), "{wrong:?}");

    // The code: paired, and this Mac's key added there.
    let paired = code::redeem(&link_text, &code_text, "Studio Mac", &key)
        .await
        .unwrap();
    assert!(!paired.token.is_empty());
    let known = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/devices",
            Some(&json!({"name": "Studio Mac", "publicKey": key})),
        )
    });
    assert_eq!(known.status, 200, "{}", known.text);
    assert_eq!(
        paired.device.as_deref(),
        known.body["id"].as_str(),
        "{paired:?}"
    );
    let again = code::redeem(&link_text, &code_text, "Studio Mac", &key)
        .await
        .unwrap_err();
    // Used once: refused, 401 while the phone's invitation is still open.
    assert!(matches!(again.status, Some(401 | 410)), "once: {again:?}");

    // The API token alone: paired, but no key added.
    let token_only = tokio::task::block_in_place(|| {
        call(
            &obid,
            "POST",
            "/v1/pairing/invite",
            Some(&json!({"url": format!("https://{address}"), "name": "Studio", "computer": true})),
        )
    });
    assert_eq!(token_only.status, 200, "{}", token_only.text);
    let link_only = token_only.body["computerLink"].as_str().unwrap();
    let other = device_key(12);
    let plain = code::redeem(
        link_only,
        token_only.body["code"].as_str().unwrap(),
        "Laptop",
        &other,
    )
    .await
    .unwrap();
    assert_eq!(plain.device, None, "no presence, no key");
}
