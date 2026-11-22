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
    // A route on a computer's own list: its token reaches it, a phone's doesn't.
    let (status, text) = through(&obid, Some(&token), "GET", "/v1/skills", None);
    assert_eq!(status, 200, "a computer's: {text}");
    let (status, text) = through(&obid, Some(&phone_token), "GET", "/v1/skills", None);
    assert_eq!(status, 404, "not a phone's: {text}");
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
    // A path spelled another way reaches the route the gateway's list read, never another: an
    // escaped literal is that literal, so it answers as the plain path does.
    for bearer in [&token, &phone_token] {
        let plain = through(
            &obid,
            Some(bearer),
            "GET",
            "/v1/jobs/no-such-job/report",
            None,
        );
        let spelled = through(
            &obid,
            Some(bearer),
            "GET",
            "/v1/jobs/no-such-job/rep%6Frt",
            None,
        );
        assert_eq!(spelled, plain, "the same route");
        assert!(!spelled.1.contains("no such endpoint"), "{}", spelled.1);
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

/// COMPUTERS-19 to 24, on two real obids, one as the Mac and one as Studio, its direct address on
/// this machine's loopback: Studio's owner makes a code there; the Mac adds Studio with it as its
/// owner's signed act, keeps it with its token out of every answer, and the window's requests that
/// name it go there: its status, the owner's words signed by the Mac's key, a route only a
/// computer reaches, its event stream; nothing off a computer's list; then removed, from the Mac.
#[test]
fn computers_19_to_24_the_mac_adds_studio_with_a_code_and_the_window_works_there() {
    let studio_folder = Folder::new("remote-window-studio");
    let studio = start(&studio_folder.0, &with_direct(&studio_folder), None);
    direct_of(&studio);
    let mac_folder = Folder::new("remote-window-mac");
    let mac = started(&mac_folder);

    let (code, out, err) = invite_computer(&studio);
    assert_eq!(code, 0, "{out}{err}");
    let invited: Value = serde_json::from_str(out.trim()).unwrap();
    let link = invited["link"].as_str().unwrap().to_owned();

    // COMPUTERS-20: obid reads the link, so the window never does.
    let read = call(
        &mac,
        "POST",
        "/v1/computers/link",
        Some(&json!({"link": link})),
    );
    assert_eq!(read.status, 200, "{}", read.text);
    assert_eq!(read.body["fingerprint"], invited["fingerprint"]);
    assert_eq!(read.body["routes"][0]["kind"], "loopback", "{}", read.text);
    let bad = call(
        &mac,
        "POST",
        "/v1/computers/link",
        Some(&json!({"link": "nope"})),
    );
    assert_eq!(bad.status, 422, "{}", bad.text);
    assert_eq!(bad.body["reason"], "not_a_link");

    // COMPUTERS-19: adding is the owner's act; a wrong code adds nothing.
    let add = json!({"link": link, "code": invited["code"]});
    let unsigned = call(&mac, "POST", "/v1/computers", Some(&add));
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);
    let wrong = json!({"link": link, "code": "000000"});
    let refused = call_signed(&mac, "POST", "/v1/computers", Some(&wrong));
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(refused.body["reason"], "wrong_code");
    let added = call_signed(&mac, "POST", "/v1/computers", Some(&add));
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();
    assert_eq!(added.body["state"], "connected", "{}", added.text);
    let listed = call(&mac, "GET", "/v1/computers", None);
    assert_eq!(listed.body["current"], "this");
    assert_eq!(listed.body["computers"][0]["id"], id.as_str());
    let kept = std::fs::read_to_string(mac.home.join("computers.json")).unwrap();
    let pairings = call(&studio, "GET", "/v1/pairing/devices", None);
    assert_eq!(pairings.body[0]["computer"], true, "{}", pairings.text);
    for text in [&kept, &listed.text, &added.text] {
        assert!(!text.contains("token"), "no token: {text}");
    }

    // COMPUTERS-21: which computer the window is on, kept; an unknown one is refused.
    let current = call(
        &mac,
        "PUT",
        "/v1/computers/current",
        Some(&json!({"id": id})),
    );
    assert_eq!(current.status, 200, "{}", current.text);
    assert_eq!(current.body["current"], id.as_str());
    let none = call(
        &mac,
        "PUT",
        "/v1/computers/current",
        Some(&json!({"id": "nope"})),
    );
    assert_eq!(none.status, 404, "{}", none.text);

    // COMPUTERS-22: a request naming Studio is Studio's; one naming none stays the Mac's.
    let on = [("Oby-Computer", id.as_str())];
    let there = call_with_headers(&mac, TOKEN, "GET", "/v1/status", None, &on);
    assert_eq!(there.status, 200, "{}", there.text);
    assert!(
        there.body["gateway"]["direct"].is_array(),
        "Studio's: {}",
        there.text
    );
    let here = call(&mac, "GET", "/v1/status", None);
    assert!(
        here.body["gateway"]["direct"].is_null(),
        "the Mac's own: {}",
        here.text
    );
    let unknown = call_with_headers(
        &mac,
        TOKEN,
        "GET",
        "/v1/status",
        None,
        &[("Oby-Computer", "nope")],
    );
    assert_eq!(unknown.status, 404, "{}", unknown.text);
    assert_eq!(unknown.body["reason"], "no_such_computer");
    // The owner's words, signed by the Mac's key, count there; Studio's mode changes, the Mac's
    // doesn't.
    let wider = json!({"mode": "bypass"});
    let path = "/v1/settings/permissions";
    let words = words_header(&mac, "PUT", path, Some(&wider));
    let set = call_with_headers(
        &mac,
        TOKEN,
        "PUT",
        path,
        Some(&wider),
        &[("Oby-Computer", id.as_str()), ("X-Oby-Words", &words)],
    );
    assert_eq!(set.status, 200, "{}", set.text);
    assert_eq!(call(&studio, "GET", path, None).body["mode"], "bypass");
    assert_ne!(call(&mac, "GET", path, None).body["mode"], "bypass");
    // A route only a computer reaches answers; one off its list is Studio's gateway's 404.
    let skills = call_with_headers(&mac, TOKEN, "GET", "/v1/skills", None, &on);
    assert_eq!(skills.status, 200, "{}", skills.text);
    // Refused on the Mac itself, never sent: Studio's gateway would have said so in its own words.
    for (method, target) in [
        ("GET", "/v1/obid/vault"),
        ("PUT", "/v1/obid/vault/X"),
        ("GET", "/v1/%70airing/devices"),
    ] {
        let refused = call_with_headers(
            &mac,
            TOKEN,
            method,
            target,
            Some(&json!({"value": "s3cretvalue1"})),
            &on,
        );
        assert_eq!(refused.status, 404, "{target}: {}", refused.text);
        assert_eq!(
            refused.body["reason"], "not_for_a_computer",
            "{target}: {}",
            refused.text
        );
    }
    // Only the window's own token names a computer: an agent's, or one through the gateway, is
    // refused and never sent.
    let agent = call_with_headers(
        &mac,
        "an-agents-broker-token",
        "POST",
        "/v1/agent/mcp/job-1",
        Some(&json!({})),
        &on,
    );
    assert_eq!(agent.status, 400, "{}", agent.text);
    assert_eq!(agent.body["reason"], "not_the_window");
    let (status, text) = through_with(&mac, Some("a-device-token"), "GET", "/v1/status", None, &on);
    assert_eq!(status, 400, "through the gateway: {text}");
    // Studio's event stream, followed through the Mac.
    // Closed once its answer ends, so the end of the stream is seen.
    let followed = [("Oby-Computer", id.as_str()), ("Connection", "close")];
    let events = follow_events_as(&mac, &followed);
    let studio_set = call(
        &studio,
        "PUT",
        "/v1/computers/current",
        Some(&json!({"id": "this"})),
    );
    assert_eq!(studio_set.status, 200, "{}", studio_set.text);
    let seen = events_named(&events, "computers", 15, |got| !got.is_empty());
    assert_eq!(
        seen[0]["computers"],
        json!([]),
        "Studio's list, not the Mac's: {:?}",
        seen
    );
    // Never the Mac's own events: a change here doesn't reach it.
    let here = call(
        &mac,
        "PATCH",
        &format!("/v1/computers/{id}"),
        Some(&json!({"name": "Studio"})),
    );
    assert_eq!(here.status, 200, "{}", here.text);
    let mixed = events.recv_timeout(Duration::from_secs(2));
    assert!(
        !matches!(&mixed, Ok((name, data)) if name == "computers" && data["computers"] != json!([])),
        "the Mac's event in Studio's stream: {mixed:?}"
    );
    // The window back on This Mac: Studio's stream ends.
    let back = call(
        &mac,
        "PUT",
        "/v1/computers/current",
        Some(&json!({"id": "this"})),
    );
    assert_eq!(back.status, 200, "{}", back.text);
    within(10, "Studio's stream to end", || {
        match events.recv_timeout(Duration::from_millis(200)) {
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Some(()),
            _ => None,
        }
    });

    // COMPUTERS-23: renamed on the Mac only.
    let renamed = call(
        &mac,
        "PATCH",
        &format!("/v1/computers/{id}"),
        Some(&json!({"name": "Studio Mac"})),
    );
    assert_eq!(renamed.status, 200, "{}", renamed.text);
    assert_eq!(renamed.body["name"], "Studio Mac");

    // COMPUTERS-24: removing is the owner's act; it ends the pairing there and forgets it here.
    let target = format!("/v1/computers/{id}");
    let unsigned = call(&mac, "DELETE", &target, None);
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);
    // The window on Studio again, following its events: removing it ends that stream too.
    let again = call(
        &mac,
        "PUT",
        "/v1/computers/current",
        Some(&json!({"id": id})),
    );
    assert_eq!(again.status, 200, "{}", again.text);
    let following = follow_events_as(&mac, &followed);
    let removed = call_signed(&mac, "DELETE", &target, None);
    assert_eq!(removed.status, 200, "{}", removed.text);
    assert_eq!(removed.body["there"], true, "{}", removed.text);
    within(
        10,
        "Studio's stream to end once it's removed",
        || match following.recv_timeout(Duration::from_millis(200)) {
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Some(()),
            _ => None,
        },
    );
    assert_eq!(
        call(&mac, "GET", "/v1/computers", None).body,
        json!({"current": "this", "computers": []})
    );
    assert_eq!(
        call(&studio, "GET", "/v1/pairing/devices", None).body,
        json!([])
    );
    let gone = call_with_headers(&mac, TOKEN, "GET", "/v1/status", None, &on);
    assert_eq!(gone.status, 404, "{}", gone.text);
}

/// COMPUTERS-19 and 22: the computer's device token is in no answer, no event, no line of the
/// Mac's log and no error, a forward that fails included; only in the Mac's vault.
#[test]
fn computers_22_its_token_is_in_no_answer_event_log_or_error() {
    let studio_folder = Folder::new("remote-token-studio");
    let mut studio = start(&studio_folder.0, &with_direct(&studio_folder), None);
    direct_of(&studio);
    let mac_folder = Folder::new("remote-token-mac");
    let mac = started(&mac_folder);
    let events = follow_events(&mac);
    let (code, out, err) = invite_computer(&studio);
    assert_eq!(code, 0, "{out}{err}");
    let invited: Value = serde_json::from_str(out.trim()).unwrap();
    let add = json!({"link": invited["link"], "code": invited["code"]});
    let mut answers = Vec::new();
    let added = call_signed(&mac, "POST", "/v1/computers", Some(&add));
    assert_eq!(added.status, 201, "{}", added.text);
    let id = added.body["id"].as_str().unwrap().to_owned();
    answers.push(added.text.clone());
    let on = [("Oby-Computer", id.as_str())];
    for (method, target) in [("GET", "/v1/computers"), ("POST", "/v1/computers/link")] {
        answers.push(
            call(
                &mac,
                method,
                target,
                Some(&json!({"link": invited["link"]})),
            )
            .text,
        );
    }
    answers.push(call_with_headers(&mac, TOKEN, "GET", "/v1/status", None, &on).text);
    answers.push(call(&mac, "POST", &format!("/v1/computers/{id}/retry"), None).text);
    // A forward that fails: Studio stopped, the error names every route tried.
    studio.stop();
    let failed = call_with_headers(&mac, TOKEN, "GET", "/v1/status", None, &on);
    assert_eq!(failed.status, 502, "{}", failed.text);
    assert_eq!(failed.body["reason"], "unreachable");
    answers.push(failed.text.clone());
    answers.push(call(&mac, "POST", &format!("/v1/computers/{id}/retry"), None).text);
    // The token, from the Mac's vault, where alone it is.
    let vault: Value =
        serde_json::from_slice(&std::fs::read(mac.home.join("vault/secrets.json")).unwrap())
            .unwrap();
    let token = vault["secrets"][format!("oby.computer.{id}")]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(token.len() >= 32, "a device token");
    let mut seen = Vec::new();
    while let Ok((name, data)) = events.recv_timeout(Duration::from_millis(500)) {
        seen.push(format!("{name} {data}"));
    }
    assert!(
        seen.iter().any(|e| e.starts_with("computer.route")),
        "{seen:?}"
    );
    let log = std::fs::read_to_string(&mac.stderr).unwrap_or_default();
    let kept = std::fs::read_to_string(mac.home.join("computers.json")).unwrap();
    for (what, text) in [
        ("answers", answers.join("\n")),
        ("events", seen.join("\n")),
        ("the log", log),
        ("computers.json", kept),
    ] {
        assert!(!text.contains(&token), "the token in {what}");
    }
}

/// The person's `ssh`, as tests/fakes/ssh stands in for it, first on the Mac obid's `PATH`: each
/// call's arguments in `ssh-bin/calls`, its tunnels relayed on this machine. `studios` are the
/// destinations whose computer runs its own `obid pair-computer`; `unknown-host`, `no-ssh` and
/// `no-obid` say what ssh and a shell say then.
fn stand_in_ssh(folder: &Path, studios: &[(&str, &Obid)]) -> PathBuf {
    let bin = folder.join("ssh-bin");
    let hosts = bin.join("hosts");
    std::fs::create_dir_all(&hosts).unwrap();
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fakes/ssh"),
        bin.join("ssh"),
    )
    .unwrap();
    let mut said: Vec<(String, String)> = studios
        .iter()
        .map(|(name, obid)| {
            (
                name.to_string(),
                format!(
                    "exec '{OBID}' pair-computer --config '{}'",
                    obid.folder.join("config.json").display()
                ),
            )
        })
        .collect();
    said.extend([
        (
            "unknown-host".into(),
            "echo 'Host key verification failed.' >&2; exit 255".into(),
        ),
        (
            "no-ssh".into(),
            "echo 'ssh: connect to host no-ssh port 22: Connection refused' >&2; exit 255".into(),
        ),
        (
            "no-obid".into(),
            "echo 'sh: obid: command not found' >&2; exit 127".into(),
        ),
    ]);
    use std::os::unix::fs::PermissionsExt as _;
    for (name, script) in said {
        let host = hosts.join(name);
        std::fs::write(
            &host,
            format!(
                "#!/bin/sh
{script}
"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&host, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// The `computer.adding` events for adding `id`, up to its last: done or failed.
fn adding_of(events: &std::sync::mpsc::Receiver<(String, Value)>, id: &str) -> Vec<Value> {
    events_named(events, "computer.adding", 60, |got| {
        got.iter().any(|e| {
            e["id"] == id
                && (e["state"] == "failed" || (e["step"] == "routes" && e["state"] == "done"))
        })
    })
    .into_iter()
    .filter(|e| e["id"] == id)
    .collect()
}

/// COMPUTERS-25, 17 and 18, on real obids: the Mac adds Studio over the person's own ssh (a
/// stand-in here), as its owner's signed act. ssh runs `obid pair-computer` there, once, with this
/// Mac's name and key on stdin only; the invitation is redeemed through the tunnel, and each step
/// is told as it happens. The window's requests then go through that tunnel; SSH's own refusals
/// say what to do and keep nothing; removing it ends its tunnel. A Studio with a direct address
/// is kept with that faster route first, SSH last.
#[test]
fn computers_25_the_mac_adds_studio_over_ssh_and_works_there_through_its_tunnel() {
    let studio_folder = Folder::new("remote-ssh-studio");
    let studio = started(&studio_folder);
    let fast_folder = Folder::new("remote-ssh-studio-b");
    let fast = start(&fast_folder.0, &with_direct(&fast_folder), None);
    direct_of(&fast);
    let mac_folder = Folder::new("remote-ssh-mac");
    let bin = stand_in_ssh(&mac_folder.0, &[("studio", &studio), ("studio-b", &fast)]);
    let remote = fixture(&mac_folder.0.join("fixture"));
    let mac = start(
        &mac_folder.0,
        &config(&mac_folder.0, &remote, &agent()),
        Some(&bin),
    );
    let calls = || std::fs::read_to_string(bin.join("calls")).unwrap_or_default();
    let events = follow_events(&mac);

    // The owner's act, and a host that can't be an option: refused before ssh runs.
    let asked = json!({"ssh": "studio"});
    let unsigned = call(&mac, "POST", "/v1/computers", Some(&asked));
    assert_eq!(unsigned.status, 403, "{}", unsigned.text);
    assert_eq!(
        unsigned.body["error"],
        obid::routes::NEEDS_THE_OWNERS_APP,
        "refused for being unsigned"
    );
    let option = call_signed(
        &mac,
        "POST",
        "/v1/computers",
        Some(&json!({"ssh": "-oProxyCommand=id"})),
    );
    assert_eq!(option.status, 422, "{}", option.text);
    assert_eq!(option.body["reason"], "not_a_host");
    assert_eq!(calls(), "", "ssh ran for a refused host");

    // Added over SSH, each step told as it happens.
    let started_adding = call_signed(&mac, "POST", "/v1/computers", Some(&asked));
    assert_eq!(started_adding.status, 202, "{}", started_adding.text);
    let adding = started_adding.body["adding"].as_str().unwrap().to_owned();
    let steps = adding_of(&events, &adding);
    let said: Vec<String> = steps
        .iter()
        .map(|e| {
            format!(
                "{} {}",
                e["step"].as_str().unwrap(),
                e["state"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            "ssh doing",
            "ssh done",
            "pair doing",
            "pair done",
            "routes doing",
            "routes done"
        ],
        "{steps:?}"
    );
    let computer = steps.last().unwrap()["computer"].clone();
    let id = computer["id"].as_str().unwrap().to_owned();
    assert_eq!(computer["route"]["kind"], "ssh", "{computer}");
    assert_eq!(computer["state"], "connected", "{computer}");
    // ssh ran `obid pair-computer` once, the destination last after `--`, nothing else on its
    // command line: the Mac's name and key went on stdin.
    let all = calls();
    let ran: Vec<&str> = all
        .lines()
        .filter(|l| !l.contains(" -L "))
        .collect::<Vec<_>>();
    assert_eq!(ran.len(), 1, "{ran:?}");
    assert!(
        ran[0].ends_with(" -- studio obid pair-computer"),
        "{}",
        ran[0]
    );
    assert!(ran[0].contains("BatchMode=yes"), "{}", ran[0]);
    let pairings = call(&studio, "GET", "/v1/pairing/devices", None);
    assert_eq!(pairings.body[0]["computer"], true, "{}", pairings.text);
    let kept = std::fs::read_to_string(mac.home.join("computers.json")).unwrap();
    assert!(!kept.contains("token"), "{kept}");

    // The window's requests for Studio go through the tunnel: the owner's words signed on the
    // Mac set Studio's mode, not the Mac's.
    let path = "/v1/settings/permissions";
    for obid in [&mac, &studio] {
        let ask = call(obid, "PUT", path, Some(&json!({"mode": "ask"})));
        assert_eq!(ask.status, 200, "{}", ask.text);
    }
    let wider = json!({"mode": "bypass"});
    let words = words_header(&mac, "PUT", path, Some(&wider));
    let set = call_with_headers(
        &mac,
        TOKEN,
        "PUT",
        path,
        Some(&wider),
        &[("Oby-Computer", id.as_str()), ("X-Oby-Words", &words)],
    );
    assert_eq!(set.status, 200, "{}", set.text);
    assert_eq!(call(&studio, "GET", path, None).body["mode"], "bypass");
    assert_eq!(call(&mac, "GET", path, None).body["mode"], "ask");
    let tunnels: Vec<String> = calls()
        .lines()
        .filter(|l| l.contains(" -L "))
        .map(str::to_owned)
        .collect();
    assert!(
        tunnels
            .iter()
            .all(|l| l.contains("ExitOnForwardFailure=yes") && l.ends_with(" -- studio")),
        "{tunnels:?}"
    );
    // COMPUTERS-18: trying again finds obid answering through the tunnel.
    let retried = call(&mac, "POST", &format!("/v1/computers/{id}/retry"), None);
    assert_eq!(retried.status, 200, "{}", retried.text);
    assert_eq!(retried.body["state"], "connected", "{}", retried.text);

    // Removing it ends its pairing there and its tunnel here.
    let pids: Vec<i32> = std::fs::read_dir(&bin)
        .unwrap()
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.starts_with("tunnel-").then(|| {
                std::fs::read_to_string(bin.join(&name))
                    .ok()?
                    .trim()
                    .parse()
                    .ok()
            })?
        })
        .collect();
    assert!(!pids.is_empty(), "no tunnel ran");
    let removed = call_signed(&mac, "DELETE", &format!("/v1/computers/{id}"), None);
    assert_eq!(removed.status, 200, "{}", removed.text);
    assert_eq!(removed.body["there"], true, "{}", removed.text);
    within(10, "its tunnels to end", || {
        pids.iter()
            .all(|pid| {
                !Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
            })
            .then_some(())
    });

    // SSH's refusals say what to do, in obid's words, and keep nothing.
    for (host, step, kind, words) in [
        ("unknown-host", "ssh", "hostKeyUnknown", "ssh unknown-host"),
        ("no-ssh", "ssh", "noSshServer", "Remote Login"),
        ("no-obid", "pair", "noObid", "install Oby there"),
    ] {
        let tried = call_signed(&mac, "POST", "/v1/computers", Some(&json!({"ssh": host})));
        assert_eq!(tried.status, 202, "{}", tried.text);
        let steps = adding_of(&events, tried.body["adding"].as_str().unwrap());
        let last = steps.last().unwrap();
        assert_eq!(
            (last["step"].as_str(), last["state"].as_str()),
            (Some(step), Some("failed")),
            "{steps:?}"
        );
        assert_eq!(last["trouble"]["trouble"], kind, "{last}");
        let say = last["trouble"]["say"].as_str().unwrap();
        assert!(say.contains(words), "{host}: {say}");
    }
    assert_eq!(
        call(&mac, "GET", "/v1/computers", None).body["computers"],
        json!([])
    );

    // A Studio with a direct address: kept with it first, SSH the fallback, and the window starts
    // on the faster one.
    let tried = call_signed(
        &mac,
        "POST",
        "/v1/computers",
        Some(&json!({"ssh": "studio-b"})),
    );
    assert_eq!(tried.status, 202, "{}", tried.text);
    let steps = adding_of(&events, tried.body["adding"].as_str().unwrap());
    let computer = &steps.last().unwrap()["computer"];
    assert_eq!(computer["route"]["kind"], "loopback", "{steps:?}");
    let kept: Value =
        serde_json::from_slice(&std::fs::read(mac.home.join("computers.json")).unwrap()).unwrap();
    let kinds: Vec<&str> = kept["computers"][0]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["loopback", "ssh"], "{kept}");
}

/// A TCP server on this machine's 127.0.0.1 that sends back whatever it's sent, as long as the test
/// runs: a dev server on the other computer, or one on the Mac that nothing may reach. Its port,
/// and how many connections it took.
fn echo() -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let taken = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = taken.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(move || {
                let mut reader = stream.try_clone().unwrap();
                let _ = std::io::copy(&mut reader, &mut stream);
            });
        }
    });
    (port, taken)
}

/// An HTTP server on this machine's 127.0.0.1 that answers `GET /` in origin form with `body`, as a
/// dev server reads its path, and anything else `400`; then closes.
fn serves(body: &'static str) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            let (status, body) = match head.starts_with(b"GET / HTTP/1.1\r\n") {
                true => ("200 OK", body),
                false => ("400 Bad Request", ""),
            };
            let answer = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    port
}

/// A port nothing listens on here.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// `studio` added to `mac` with a code, as COMPUTERS-19 adds one: its id on the Mac.
fn add_with_code(mac: &Obid, studio: &Obid) -> String {
    let (code, out, err) = invite_computer(studio);
    assert_eq!(code, 0, "{out}{err}");
    let invited: Value = serde_json::from_str(out.trim()).unwrap();
    let add = json!({"link": invited["link"], "code": invited["code"]});
    let added = call_signed(mac, "POST", "/v1/computers", Some(&add));
    assert_eq!(added.status, 201, "{}", added.text);
    added.body["id"].as_str().unwrap().to_owned()
}

/// A computer's door, as `POST /v1/computers/{id}/door` gives it.
struct Door {
    port: u16,
    token: String,
}

impl Door {
    /// The door of computer `id` on `mac`.
    fn of(mac: &Obid, id: &str) -> Door {
        let asked = call(mac, "POST", &format!("/v1/computers/{id}/door"), None);
        assert_eq!(asked.status, 200, "{}", asked.text);
        let proxy = asked.body["proxy"].as_str().unwrap();
        let (token, at) = proxy
            .strip_prefix("http://oby:")
            .and_then(|rest| rest.split_once('@'))
            .unwrap();
        Door {
            port: at.strip_prefix("127.0.0.1:").unwrap().parse().unwrap(),
            token: token.to_owned(),
        }
    }

    /// `head` sent through the door with `token`'s login: the answer's status (0 when the door
    /// closed with none) and the connection, its answer's head read.
    fn ask(&self, token: &str, head: &str) -> (u16, TcpStream) {
        use base64::Engine as _;
        let login = base64::engine::general_purpose::STANDARD.encode(format!("oby:{token}"));
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let sent = format!("{head}Proxy-Authorization: Basic {login}\r\n\r\n");
        stream.write_all(sent.as_bytes()).unwrap();
        let mut answer = Vec::new();
        let mut byte = [0u8; 1];
        while !answer.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
            answer.push(byte[0]);
        }
        let answer = String::from_utf8_lossy(&answer).into_owned();
        let status = answer
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, stream)
    }

    /// `CONNECT target` through the door.
    fn connect(&self, target: &str) -> (u16, TcpStream) {
        let head = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
        self.ask(&self.token, &head)
    }
}

/// What goes into `tunnel` comes back from the echo at its end.
fn echoes(tunnel: &mut TcpStream) {
    tunnel.write_all(b"hello from the Mac").unwrap();
    let mut back = [0u8; 18];
    tunnel.read_exact(&mut back).unwrap();
    assert_eq!(&back, b"hello from the Mac");
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A WebSocket to `base`'s `/v1/preview/tunnel?port=<port>`, pinned to `pin` for `wss://`, with
/// `token` and `headers`: the refusal's status and body when it isn't upgraded.
async fn tunnel(
    base: &str,
    pin: Option<obid::remote::pin::Pin>,
    token: Option<&str>,
    port: &str,
    headers: &[(&str, &str)],
) -> Result<Socket, (u16, Value)> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    use tokio_tungstenite::tungstenite::http::HeaderName;
    let mut request = format!("{base}/v1/preview/tunnel?port={port}")
        .into_client_request()
        .unwrap();
    if let Some(token) = token {
        let bearer = format!("Bearer {token}").parse().unwrap();
        request.headers_mut().insert("Authorization", bearer);
    }
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes()).unwrap();
        request.headers_mut().insert(name, value.parse().unwrap());
    }
    let connector = pin.map(|pin| {
        tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(
            obid::remote::pinned::tls(pin).unwrap(),
        ))
    });
    match tokio_tungstenite::connect_async_tls_with_config(request, None, false, connector).await {
        Ok((ws, _)) => Ok(ws),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            let body = response
                .body()
                .as_deref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .unwrap_or(Value::Null);
            Err((response.status().as_u16(), body))
        }
        Err(e) => panic!("the tunnel: {e}"),
    }
}

/// COMPUTERS-26, on a real obid as the other computer: a paired computer's own token opens a
/// WebSocket over its direct address, pinned, to a port on its 127.0.0.1, and bytes go through
/// unchanged; obid's own ports, a port nothing listens on and a bad port are refused before any
/// upgrade; a phone's token, none, obid's own API and a request naming a computer get the
/// gateway's unlisted `404`; and an open tunnel ends with its pairing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computers_26_a_computers_token_tunnels_to_its_loopback_and_nothing_else() {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;
    let folder = Folder::new("remote-preview-tunnel");
    let config = tokio::task::block_in_place(|| with_direct(&folder));
    let studio = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (address, pin) = tokio::task::block_in_place(|| direct_of(&studio));
    let paired = tokio::task::block_in_place(|| computer_and_phone(&studio));
    let wss = format!("wss://{address}");
    let (echo_port, _) = echo();
    let echo_port = echo_port.to_string();

    // Its own token, over the direct address: 64 KiB through and back, unchanged.
    let mut ws = tunnel(&wss, Some(pin), Some(&paired.token), &echo_port, &[])
        .await
        .unwrap();
    let mut sent = vec![0u8; 64 * 1024];
    getrandom::fill(&mut sent).unwrap();
    ws.send(Message::Binary(sent.clone().into())).await.unwrap();
    let mut back = Vec::new();
    while back.len() < sent.len() {
        match ws.next().await {
            Some(Ok(Message::Binary(bytes))) => back.extend_from_slice(&bytes),
            Some(Ok(_)) => {}
            other => panic!("the tunnel ended: {other:?}"),
        }
    }
    assert_eq!(back, sent);

    // obid's own ports: its API's, its gateway's and its direct address's.
    let direct_port = address.rsplit_once(':').unwrap().1.to_owned();
    for own in [
        studio.port.to_string(),
        studio.gateway.unwrap().to_string(),
        direct_port,
    ] {
        let (status, body) = tunnel(&wss, Some(pin), Some(&paired.token), &own, &[])
            .await
            .err()
            .unwrap();
        assert_eq!((status, body["reason"].as_str()), (403, Some("own_port")));
    }
    // Nothing listening there: refused, never upgraded.
    let free = free_port().to_string();
    let (status, body) = tunnel(&wss, Some(pin), Some(&paired.token), &free, &[])
        .await
        .err()
        .unwrap();
    assert_eq!(
        (status, body["reason"].as_str()),
        (403, Some("nothing_listens"))
    );
    for bad in ["0", "70000"] {
        let (status, body) = tunnel(&wss, Some(pin), Some(&paired.token), bad, &[])
            .await
            .err()
            .unwrap();
        assert_eq!((status, body["reason"].as_str()), (400, Some("bad_port")));
    }

    // Anyone else: the gateway's unlisted route.
    let unlisted = "That route isn't available through the gateway.";
    for (token, headers) in [
        (Some(paired.phone_token.as_str()), &[][..]),
        (None, &[][..]),
        (Some(paired.token.as_str()), &[("Oby-Computer", "x")][..]),
    ] {
        let (status, body) = tunnel(&wss, Some(pin), token, &echo_port, headers)
            .await
            .err()
            .unwrap();
        assert_eq!((status, body["error"].as_str()), (404, Some(unlisted)));
    }
    let api = format!("ws://127.0.0.1:{}", studio.port);
    let (status, _) = tunnel(&api, None, Some(TOKEN), &echo_port, &[])
        .await
        .err()
        .unwrap();
    assert_eq!(status, 404, "obid's loopback API serves no tunnel");

    // Its pairing ends: the open tunnel closes, and no new one opens.
    let mut open = tunnel(&wss, Some(pin), Some(&paired.token), &echo_port, &[])
        .await
        .unwrap();
    let ended = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = ended.clone();
    tokio::spawn(async move {
        while let Some(Ok(message)) = open.next().await {
            if message.is_close() {
                break;
            }
        }
        seen.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let (status, text) = tokio::task::block_in_place(|| {
        through(&studio, Some(&paired.token), "DELETE", "/v1/pairing", None)
    });
    assert_eq!(status, 204, "{text}");
    tokio::task::block_in_place(|| {
        within(10, "the open tunnel to close", || {
            ended
                .load(std::sync::atomic::Ordering::SeqCst)
                .then_some(())
        })
    });
    let (status, _) = tunnel(&wss, Some(pin), Some(&paired.token), &echo_port, &[])
        .await
        .err()
        .unwrap();
    assert_eq!(status, 404);
}

/// COMPUTERS-27, on three real obids: the Mac's door for Studio carries a `CONNECT` and a plain
/// request to a loopback name to Studio's own 127.0.0.1; nothing but a loopback name passes; a
/// wrong login is `407`; the 65th connection at once gets no tunnel. Never this Mac's own
/// loopback: a port nothing listens on at Studio fails, and a computer no route reaches leaves a
/// server on the Mac's own port untouched. Removing Studio closes its door and what it carries.
#[test]
fn computers_27_the_door_carries_a_loopback_connection_to_its_computer_and_never_this_macs() {
    let studio_folder = Folder::new("remote-door-studio");
    let studio = start(&studio_folder.0, &with_direct(&studio_folder), None);
    direct_of(&studio);
    let gone_folder = Folder::new("remote-door-gone");
    let mut gone = start(&gone_folder.0, &with_direct(&gone_folder), None);
    direct_of(&gone);
    let mac_folder = Folder::new("remote-door-mac");
    let mac = started(&mac_folder);
    let id = add_with_code(&mac, &studio);
    let gone_id = add_with_code(&mac, &gone);
    gone.stop();
    let door = Door::of(&mac, &id);
    let (echo_port, _) = echo();

    // A loopback name's connection is Studio's.
    let (status, mut held) = door.connect(&format!("localhost:{echo_port}"));
    assert_eq!(status, 200);
    echoes(&mut held);

    // Never this Mac's own loopback: nothing listens there at Studio, and it fails.
    let (status, _) = door.connect(&format!("localhost:{}", free_port()));
    assert_eq!(status, 502);
    // A computer no route reaches: the Mac's own server on that port takes nothing.
    let (here, taken) = echo();
    let (status, _) = Door::of(&mac, &gone_id).connect(&format!("127.0.0.1:{here}"));
    assert_eq!(status, 502);
    assert_eq!(taken.load(std::sync::atomic::Ordering::SeqCst), 0);

    // Nothing but a loopback name.
    for target in ["example.com:443", "192.168.1.20:80"] {
        let (status, _) = door.connect(target);
        assert_eq!(status, 403, "{target}");
    }

    // A plain request, answered by Studio's server.
    let page = serves("on-studio.txt");
    let head = format!("GET http://localhost:{page}/ HTTP/1.1\r\nHost: localhost:{page}\r\n");
    let (status, mut plain) = door.ask(&door.token, &head);
    assert_eq!(status, 200);
    let mut body = String::new();
    plain.read_to_string(&mut body).unwrap();
    assert_eq!(body, "on-studio.txt");

    // A wrong login.
    let (status, _) = door.ask(
        &"0".repeat(64),
        &format!("CONNECT localhost:{echo_port} HTTP/1.1\r\n"),
    );
    assert_eq!(status, 407);

    // 64 at once (the one held and 63 more, once those before have closed), and the 65th gets no
    // tunnel.
    let open: Vec<TcpStream> = within(10, "63 more connections at once", || {
        let opened: Vec<TcpStream> = (0..63)
            .map(|_| door.connect(&format!("localhost:{echo_port}")))
            .filter(|(status, _)| *status == 200)
            .map(|(_, stream)| stream)
            .collect();
        (opened.len() == 63).then_some(opened)
    });
    let (status, mut refused) = door.connect(&format!("localhost:{echo_port}"));
    assert_eq!(status, 429);
    refused.read_to_end(&mut Vec::new()).expect("closed");
    drop(open);

    // Removing Studio closes what its door carries, and the door with it.
    let removed = call_signed(&mac, "DELETE", &format!("/v1/computers/{id}"), None);
    assert_eq!(removed.status, 200, "{}", removed.text);
    held.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    within(10, "the door's connection to close", || {
        match held.read(&mut [0u8; 16]) {
            Ok(0) => Some(()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                None
            }
            Err(_) => Some(()),
            Ok(_) => None,
        }
    });
    let (status, _) = door.connect(&format!("localhost:{echo_port}"));
    assert_eq!(status, 407);
}

/// COMPUTERS-27 over SSH, on two real obids and the person's ssh (a stand-in, as COMPUTERS-25's):
/// with Studio's only route its SSH tunnel, the door's connection goes through it.
#[test]
fn computers_27_the_door_carries_a_loopback_connection_to_its_computer_through_its_ssh_tunnel() {
    let studio_folder = Folder::new("remote-door-ssh-studio");
    let studio = started(&studio_folder);
    let mac_folder = Folder::new("remote-door-ssh-mac");
    let bin = stand_in_ssh(&mac_folder.0, &[("studio", &studio)]);
    let remote = fixture(&mac_folder.0.join("fixture"));
    let mac = start(
        &mac_folder.0,
        &config(&mac_folder.0, &remote, &agent()),
        Some(&bin),
    );
    let events = follow_events(&mac);
    let asked = call_signed(
        &mac,
        "POST",
        "/v1/computers",
        Some(&json!({"ssh": "studio"})),
    );
    assert_eq!(asked.status, 202, "{}", asked.text);
    let steps = adding_of(&events, asked.body["adding"].as_str().unwrap());
    let computer = steps.last().unwrap()["computer"].clone();
    assert_eq!(computer["route"]["kind"], "ssh", "{steps:?}");
    let id = computer["id"].as_str().unwrap();

    let (echo_port, _) = echo();
    let (status, mut held) = Door::of(&mac, id).connect(&format!("localhost:{echo_port}"));
    assert_eq!(status, 200);
    echoes(&mut held);
    let calls = std::fs::read_to_string(bin.join("calls")).unwrap();
    assert!(calls.lines().any(|l| l.contains(" -L ")), "{calls}");
}

/// COMPUTERS-28, on two real obids: only the window, with obid's own token on loopback, gets a
/// computer's door, the same one each time; an agent's broker token and a paired device through
/// the gateway don't; an unknown computer has none; and the door's token is in no log line or
/// event.
#[test]
fn computers_28_only_the_window_gets_a_computers_door() {
    let studio_folder = Folder::new("remote-door-window-studio");
    let studio = start(&studio_folder.0, &with_direct(&studio_folder), None);
    direct_of(&studio);
    let mac_folder = Folder::new("remote-door-window-mac");
    let mac = started(&mac_folder);
    let events = follow_events(&mac);
    let paired = computer_and_phone(&mac);
    let id = add_with_code(&mac, &studio);
    let target = format!("/v1/computers/{id}/door");

    let first = call(&mac, "POST", &target, None);
    assert_eq!(first.status, 200, "{}", first.text);
    let proxy = first.body["proxy"].as_str().unwrap().to_owned();
    let (token, at) = proxy
        .strip_prefix("http://oby:")
        .and_then(|rest| rest.split_once('@'))
        .unwrap();
    assert!(
        token.len() == 64
            && token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{proxy}"
    );
    let port = at.strip_prefix("127.0.0.1:").unwrap();
    assert!(port.parse::<u16>().is_ok(), "{proxy}");
    assert_eq!(
        call(&mac, "POST", &target, None).body["proxy"],
        proxy.as_str()
    );
    let (echo_port, _) = echo();
    let (status, mut held) = Door::of(&mac, &id).connect(&format!("localhost:{echo_port}"));
    assert_eq!(status, 200);
    echoes(&mut held);

    let agent = call_with_token(&mac, "an-agents-broker-token", "POST", &target, None);
    assert_eq!(agent.status, 401, "{}", agent.text);
    for device in [&paired.token, &paired.phone_token] {
        let (status, text) = through(&mac, Some(device), "POST", &target, None);
        assert_eq!(status, 404, "through the gateway: {text}");
        assert!(!text.contains(token), "{text}");
    }
    let unknown = call(&mac, "POST", "/v1/computers/nope/door", None);
    assert_eq!(unknown.status, 404, "{}", unknown.text);
    assert_eq!(unknown.body["reason"], "no_such_computer");

    let mut seen = Vec::new();
    while let Ok((name, data)) = events.recv_timeout(Duration::from_millis(500)) {
        seen.push(format!("{name} {data}"));
    }
    assert!(!seen.is_empty(), "the events were followed");
    for (what, text) in [("events", seen.join("\n")), ("the log", mac.log())] {
        assert!(!text.contains(token), "the door's token in {what}");
    }
}

/// COMPUTERS-26: revoking a computer's pairing ends its tunnel even while the service at its end
/// has stopped reading and every buffer between is full: both ends are dropped at once, and what
/// was still queued never reaches the service.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn computers_26_a_revoked_tunnel_ends_while_its_service_reads_nothing() {
    use futures_util::{SinkExt as _, StreamExt as _};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio_tungstenite::tungstenite::Message;
    let folder = Folder::new("remote-tunnel-stalled");
    let config = tokio::task::block_in_place(|| with_direct(&folder));
    let studio = tokio::task::block_in_place(|| start(&folder.0, &config, None));
    let (address, pin) = tokio::task::block_in_place(|| direct_of(&studio));
    let paired = tokio::task::block_in_place(|| computer_and_phone(&studio));

    // A service that takes its connection and reads nothing until the test says.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let (taken, accepted) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let _ = taken.send(stream);
        }
    });
    let ws = tunnel(
        &format!("wss://{address}"),
        Some(pin),
        Some(&paired.token),
        &port,
        &[],
    )
    .await
    .unwrap();
    let mut service =
        tokio::task::block_in_place(|| accepted.recv_timeout(Duration::from_secs(10)).unwrap());
    service
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let (mut sink, mut stream) = ws.split();
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = sent.clone();
    tokio::spawn(async move {
        let frame = vec![7u8; 64 * 1024];
        while counted.load(Ordering::SeqCst) < 512 << 20 {
            if sink
                .send(Message::Binary(frame.clone().into()))
                .await
                .is_err()
            {
                return;
            }
            counted.fetch_add(frame.len(), Ordering::SeqCst);
        }
    });
    let ended = Arc::new(AtomicBool::new(false));
    let seen = ended.clone();
    tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            if message.is_close() {
                break;
            }
        }
        seen.store(true, Ordering::SeqCst);
    });
    // Every buffer between is full: nothing more goes for a second.
    let (mut last, mut still) = (0, 0);
    let stalled = tokio::task::block_in_place(|| {
        within(30, "the tunnel to stall on its service", || {
            let now = sent.load(Ordering::SeqCst);
            still = if now == last && now > 0 { still + 1 } else { 0 };
            last = now;
            (still >= 10).then_some(now)
        })
    });

    let (status, text) = tokio::task::block_in_place(|| {
        through(&studio, Some(&paired.token), "DELETE", "/v1/pairing", None)
    });
    assert_eq!(status, 204, "{text}");
    tokio::task::block_in_place(|| {
        within(10, "the stalled tunnel to close", || {
            ended.load(Ordering::SeqCst).then_some(())
        })
    });
    // The service reads now: its connection is over, and it never gets all that was sent.
    let mut got = 0;
    let mut chunk = vec![0u8; 64 * 1024];
    let over = loop {
        match service.read(&mut chunk) {
            Ok(0) => break Ok(()),
            Ok(n) => got += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break Err(e);
            }
            Err(_) => break Ok(()),
        }
    };
    assert!(
        over.is_ok(),
        "the service's connection stayed open: {over:?}"
    );
    assert!(
        got < stalled,
        "the service got {got} of the {stalled} bytes sent"
    );
}
