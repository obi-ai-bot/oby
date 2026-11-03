//! Continue with ChatGPT on the real obid binary, against a stand-in OpenAI on loopback
//! (docs/contracts/chatgpt-sign-in.md, its CHATGPT-n scenarios): no real sign-in and no real token.
//! Consent is the stand-in's redirect, and the browser's visit to obid's callback is the test's.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use stand_in_openai::*;
use support::*;

fn agent(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/agents/{name}"))
}

/// The tests' configuration with stand-in agents, Codex's among them, and no account yet.
fn configured(folder: &Path) -> Value {
    let remote = fixture(&folder.join("fixture"));
    let mut config = config(folder, &remote, &agent("fake-claude"));
    config["agents"]["codex"] = json!({"bin": agent("fake-codex"), "accounts": []});
    config
}

struct World {
    _folder: Folder,
    openai: OpenAi,
    obid: Obid,
}

fn world(name: &str) -> World {
    let folder = Folder::new(name);
    let openai = OpenAi::start();
    let config = configured(&folder.0);
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    World {
        _folder: folder,
        openai,
        obid,
    }
}

fn account(obid: &Obid, id: &str) -> Option<Value> {
    call(obid, "GET", "/v1/status", None).body["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

fn new_account(obid: &Obid) -> String {
    let added = call(
        obid,
        "POST",
        "/v1/accounts",
        Some(&json!({"agent": "codex"})),
    );
    assert_eq!(added.status, 201, "{}", added.text);
    added.body["id"].as_str().unwrap().to_owned()
}

fn started(obid: &Obid, id: &str) -> String {
    // (a sign-in that just ended still holds the account for a moment)
    let page = within(10, "the sign-in started", || {
        let page = call(
            obid,
            "GET",
            &format!("/v1/accounts/{id}/sign-in?with=chatgpt"),
            None,
        );
        (page.status != 409).then_some(page)
    });
    assert_eq!(page.status, 200, "{}", page.text);
    assert!(page.body["userCode"].is_null(), "no code to type");
    assert_eq!(page.body["pasteCode"], false);
    page.body["url"].as_str().unwrap().to_owned()
}

/// Waits until the account's sign-in has ended.
fn settled(obid: &Obid, id: &str) -> Value {
    within(15, "the sign-in ended", || {
        let a = account(obid, id)?;
        (a.get("signingIn").is_none() && (a["login"] == "chatgpt" || !a["label"].is_null()))
            .then_some(a)
    })
}

fn signed_in(world: &World, id: &str) -> Value {
    let url = started(&world.obid, id);
    let back = world.openai.consent(&url);
    let (status, _, _) = http("GET", &back, &[], None);
    assert_eq!(status, 200);
    settled(&world.obid, id)
}

/// Every file under `dir`, as bytes, with its path.
fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            out.push((path, bytes));
        }
    }
    out
}

fn holds(bytes: &[u8], token: &str) -> bool {
    bytes
        .windows(token.len())
        .any(|window| window == token.as_bytes())
}

fn vault(obid: &Obid) -> PathBuf {
    obid.home.join("vault/secrets.json")
}

/// CHATGPT-1: with no Codex login, one click and consent: signed in, no `auth.json`, and the tokens
/// only in the vault. The seeded login's own account says `ownLogin`, so the app adds a new one.
#[test]
fn chatgpt_1_signs_in_with_no_auth_json_and_tokens_only_in_the_vault() {
    let world = world("chatgpt-1");
    let id = new_account(&world.obid);
    let a = signed_in(&world, &id);
    assert_eq!(a["signedIn"], true, "{a}");
    assert_eq!(a["login"], "chatgpt");
    assert_eq!(a["loginEmail"], "person@example.com");
    assert_eq!(a["email"], "person@example.com");
    assert!(a["label"].is_null(), "{a}");
    let folder = world.obid.home.join("accounts").join(&id);
    assert!(!folder.join("auth.json").exists());
    let tokens = world.openai.with(|s| s.tokens.clone());
    assert_eq!(tokens.len(), 2, "one access token, one refresh token");
    for (path, bytes) in files(&world.obid.home) {
        for token in &tokens {
            assert_eq!(
                holds(&bytes, token),
                path == vault(&world.obid),
                "{token} in {}",
                path.display()
            );
        }
    }
    // The first registration, as OpenAI's page asks.
    let asked = world.openai.with(|s| s.authorized[0].clone());
    assert_eq!(asked["client_id"], "dynamic_agent_client");
    assert_eq!(asked["agent_name_hint"], "Oby");
    assert!(asked["ext_agent_host_id"].starts_with("urn:uuid:"));
    assert_eq!(asked["code_challenge_method"], "S256");
    assert!(asked["redirect_uri"].starts_with("http://127.0.0.1:"));
    assert!(asked["redirect_uri"].ends_with("/auth/callback"));
    assert_eq!(asked["resource"], format!("{}/v1", world.openai.origin));
    assert!(asked["scope"].contains(DIRECT));
    assert!(world.openai.public_key_is_published());
}

/// CHATGPT-1's other half: the seeded account of the Mac's own login can't sign in here, and says
/// so with the word the app adds a new account on.
#[test]
fn chatgpt_1_the_seeded_codex_account_answers_own_login() {
    let folder = Folder::new("chatgpt-1-seeded");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    config["agents"]["codex"]["accounts"] = json!([{"id": "codex", "name": "Codex"}]);
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let refused = call(
        &obid,
        "GET",
        "/v1/accounts/codex/sign-in?with=chatgpt",
        None,
    );
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(refused.body["reason"], "ownLogin");
}

/// CHATGPT-3: a plain GET still runs the tool's own device login, and that account has no marker.
#[test]
fn chatgpt_3_a_plain_get_is_still_the_device_login() {
    let world = world("chatgpt-3");
    let id = new_account(&world.obid);
    let page = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
    assert_eq!(page.status, 200, "{}", page.text);
    assert!(page.body["userCode"].is_string(), "{}", page.text);
    // A sign-in of the other kind under way: busy, and the one under way isn't touched.
    let other = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in?with=chatgpt"),
        None,
    );
    assert_eq!(other.status, 409, "{}", other.text);
    assert_eq!(other.body["reason"], "busy");
    let a = within(30, "the device login ended", || {
        account(&world.obid, &id).filter(|a| a["signedIn"] == true)
    });
    assert!(a.get("login").is_none(), "{a}");
    assert!(a.get("loginEmail").is_none(), "{a}");
    assert!(
        !world
            .obid
            .home
            .join("accounts")
            .join(&id)
            .join("chatgpt.json")
            .exists()
    );
    assert!(world.openai.with(|s| s.authorized.is_empty()));
}

/// CHATGPT-4: a request that isn't from this sign-in is refused and the sign-in goes on; a bad id
/// token or client ends it with no tokens, any issued revoked, and the row says why; there's no
/// second callback.
#[test]
fn chatgpt_4_a_bad_callback_keeps_no_tokens_and_the_row_says_why() {
    let world = world("chatgpt-4");
    let id = new_account(&world.obid);
    let url = started(&world.obid, &id);
    let back = world.openai.consent(&url);
    // Another name for this address (a rebound page), another attempt's state: nothing happens.
    let (status, _, _) = http("GET", &back, &[("Host", "evil.example")], None);
    assert_eq!(status, 400);
    let wrong = back.replace("state=", "state=x");
    let (status, _, _) = http("GET", &wrong, &[], None);
    assert_eq!(status, 400);
    let (status, _, _) = http("GET", back.split('?').next().unwrap(), &[], None);
    assert_eq!(status, 400);
    assert!(account(&world.obid, &id).unwrap()["label"].is_null());
    // The real one still works, once.
    let (status, _, _) = http("GET", &back, &[], None);
    assert_eq!(status, 200);
    settled(&world.obid, &id);
    let (status, _, _) = http("GET", &back, &[], None);
    assert_ne!(status, 200, "no second callback");

    for (name, change) in [
        (
            "signature",
            (|s: &mut Script| s.forged = true) as fn(&mut Script),
        ),
        ("audience", |s: &mut Script| s.wrong_aud = true),
    ] {
        let id = new_account(&world.obid);
        world.openai.with(change);
        let before = world.openai.with(|s| s.revoked.len());
        let url = started(&world.obid, &id);
        let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
        assert_eq!(status, 400, "{name}");
        let a = within(5, "the row said why", || {
            account(&world.obid, &id).filter(|a| !a["label"].is_null())
        });
        assert_eq!(a["signedIn"], false, "{name}");
        assert!(a.get("login").is_none(), "{name}: {a}");
        assert!(
            a["label"].as_str().unwrap().contains("verified"),
            "{name}: {a}"
        );
        assert_eq!(
            world.openai.with(|s| s.revoked.len()),
            before + 1,
            "{name}: revoked"
        );
        let last = world.openai.with(|s| s.tokens.last().cloned().unwrap());
        assert!(
            !holds(&std::fs::read(vault(&world.obid)).unwrap(), &last),
            "{name}"
        );
        world.openai.with(|s| {
            s.forged = false;
            s.wrong_aud = false;
        });
    }

    // Consent denied: the row says so at once, and nothing went to the token endpoint.
    let id = new_account(&world.obid);
    world.openai.with(|s| s.deny = true);
    let url = started(&world.obid, &id);
    let tokens = world.openai.with(|s| s.tokens.len());
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 400);
    let a = within(5, "the row said why", || {
        account(&world.obid, &id).filter(|a| !a["label"].is_null())
    });
    assert!(
        a["label"].as_str().unwrap().contains("wasn't allowed"),
        "{a}"
    );
    assert_eq!(world.openai.with(|s| s.tokens.len()), tokens);
    world.openai.with(|s| s.deny = false);

    // A first registration that comes back with no issued client: incomplete, never the entry point.
    let id = new_account(&world.obid);
    world
        .openai
        .with(|s| s.callback_client = Some("dynamic_agent_client".into()));
    let url = started(&world.obid, &id);
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 400);
    let a = within(5, "the row said why", || {
        account(&world.obid, &id).filter(|a| !a["label"].is_null())
    });
    assert!(a.get("login").is_none(), "{a}");
    world.openai.with(|s| s.callback_client = None);
}

/// CHATGPT-5: signing in again as someone else, or with a client already on another account,
/// replaces nothing. The same person under a new client is a separate account (Personal and Team).
#[test]
fn chatgpt_5_a_different_person_or_a_taken_client_replaces_nothing() {
    let world = world("chatgpt-5");
    let first = new_account(&world.obid);
    signed_in(&world, &first);
    let held = world.openai.with(|s| s.tokens.clone());
    let vault_before = std::fs::read(vault(&world.obid)).unwrap();

    // Again, as another `sub`: refused, its tokens revoked, the old login untouched.
    world.openai.with(|s| s.sub = "stand-in-sub-2".into());
    let url = started(&world.obid, &first);
    let asked = world.openai.with(|s| s.authorized.len());
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 400);
    let again = world.openai.with(|s| s.authorized[asked].clone());
    let saved = world.openai.with(|s| s.authorized[0].clone());
    assert_ne!(
        again["client_id"], "dynamic_agent_client",
        "the saved client"
    );
    assert!(
        !again.contains_key("agent_name_hint"),
        "only on first registration"
    );
    assert_eq!(again["ext_agent_host_id"], saved["ext_agent_host_id"]);
    assert_eq!(again["login_hint"], "person@example.com");
    let a = within(5, "the row said why", || {
        account(&world.obid, &first).filter(|a| !a["label"].is_null())
    });
    assert!(
        a["label"]
            .as_str()
            .unwrap()
            .contains("different ChatGPT login"),
        "{a}"
    );
    assert_eq!(std::fs::read(vault(&world.obid)).unwrap(), vault_before);
    for token in &held {
        assert!(holds(&vault_before, token));
    }

    // A client that's already on another account.
    world.openai.with(|s| {
        s.sub = "stand-in-sub-1".into();
        s.callback_client = Some(again["client_id"].clone());
    });
    let second = new_account(&world.obid);
    let url = started(&world.obid, &second);
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 400);
    let a = within(5, "the row said why", || {
        account(&world.obid, &second).filter(|a| !a["label"].is_null())
    });
    assert!(a["label"].as_str().unwrap().contains("already on"), "{a}");
    assert!(a.get("login").is_none(), "{a}");

    // The same person, a new client: a separate account.
    world.openai.with(|s| s.callback_client = None);
    let third = new_account(&world.obid);
    let a = signed_in(&world, &third);
    assert_eq!(a["login"], "chatgpt");
    assert!(
        account(&world.obid, &first).is_some(),
        "not taken off as the same login"
    );
    assert!(account(&world.obid, &third).is_some());
}

/// CHATGPT-6: a sign-in without the plan's scope is kept, with plan use off, and no run starts on
/// it. Choosing to turn it on asks for consent again, once.
#[test]
fn chatgpt_6_no_plan_scope_keeps_the_sign_in_with_plan_use_off() {
    let world = world("chatgpt-6");
    let id = new_account(&world.obid);
    world
        .openai
        .with(|s| s.scope = "openid profile email offline_access".into());
    let a = signed_in(&world, &id);
    assert_eq!(a["login"], "chatgpt");
    assert_eq!(a["signedIn"], false);
    assert_eq!(a["label"], "ChatGPT plan use is off");
    let asked = world.openai.with(|s| s.authorized[0].clone());
    assert!(
        !asked.contains_key("prompt"),
        "no forced consent on a first sign-in"
    );
    let url = started(&world.obid, &id);
    assert!(url.contains("prompt=consent"), "{url}");
    let _ = call(
        &world.obid,
        "DELETE",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
}

/// CHATGPT-15: removal revokes first. A failed revoke completes with `revoked:false`. An exchange in
/// flight writes nothing afterwards and revokes what it was given.
#[test]
fn chatgpt_15_removal_revokes_and_a_failed_revoke_says_so() {
    let world = world("chatgpt-15");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let refresh = world.openai.with(|s| s.tokens[1].clone());
    let gone = call(&world.obid, "DELETE", &format!("/v1/accounts/{id}"), None);
    assert_eq!(gone.status, 204, "{}", gone.text);
    assert_eq!(
        world.openai.with(|s| s.revoked.clone()),
        vec![refresh.clone()]
    );
    assert!(!holds(
        &std::fs::read(vault(&world.obid)).unwrap_or_default(),
        &refresh
    ));
    assert!(!world.obid.home.join("accounts").join(&id).exists());

    // A revoke that fails three times (5xx): it still completes, and says it wasn't confirmed.
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let refresh = world.openai.with(|s| s.tokens.last().cloned().unwrap());
    world
        .openai
        .with(|s| s.revoke_statuses.extend([503, 503, 503]));
    let gone = call(&world.obid, "DELETE", &format!("/v1/accounts/{id}"), None);
    assert_eq!(gone.status, 200, "{}", gone.text);
    assert_eq!(gone.body["revoked"], false);
    assert!(!holds(
        &std::fs::read(vault(&world.obid)).unwrap_or_default(),
        &refresh
    ));

    // An account with no ChatGPT login stays 204.
    let plain = new_account(&world.obid);
    assert_eq!(
        call(
            &world.obid,
            "DELETE",
            &format!("/v1/accounts/{plain}"),
            None
        )
        .status,
        204
    );

    // Removed while its code is being exchanged: nothing written, the tokens given back.
    let id = new_account(&world.obid);
    world.openai.with(|s| s.hold = Duration::from_secs(2));
    let url = started(&world.obid, &id);
    let back = world.openai.consent(&url);
    let browser = std::thread::spawn(move || http("GET", &back, &[], None).0);
    std::thread::sleep(Duration::from_millis(500));
    let gone = call(&world.obid, "DELETE", &format!("/v1/accounts/{id}"), None);
    assert_eq!(gone.status, 204, "{}", gone.text);
    assert_eq!(browser.join().unwrap(), 400);
    world.openai.with(|s| s.hold = Duration::ZERO);
    let (minted, revoked) = within(10, "the tokens were given back", || {
        let (minted, revoked) = world
            .openai
            .with(|s| (s.tokens.last().cloned(), s.revoked.clone()));
        let minted = minted?;
        revoked.contains(&minted).then_some((minted, revoked))
    });
    assert!(revoked.contains(&minted));
    assert!(!holds(
        &std::fs::read(vault(&world.obid)).unwrap_or_default(),
        &minted
    ));
}

/// CHATGPT-15: a broken `accounts.json` at start revokes nothing and deletes nothing: Live keeps the
/// file but rebuilds accounts from the seed, and the sweep must not take that for removals. Mending
/// the file brings every login back. An account the file's checks leave out, or a seeded one taken
/// out of the configuration, stays in the vault too: absence alone proves nothing.
#[test]
fn chatgpt_15_a_broken_accounts_file_at_start_revokes_nothing() {
    let folder = Folder::new("chatgpt-15-broken");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    let seeded = folder.0.join("home/accounts/seeded");
    config["agents"]["codex"]["accounts"] =
        json!([{"id": "seeded", "name": "Seeded", "configDir": seeded}]);
    let env = [("OBID_TEST_CHATGPT_ORIGIN", openai.origin.as_str())];
    let mut obid = start_with(&folder.0, &config, None, &env);
    let world_sign_in = |obid: &Obid, id: &str| {
        let url = started(obid, id);
        let (status, _, _) = http("GET", &openai.consent(&url), &[], None);
        assert_eq!(status, 200);
        settled(obid, id)
    };
    let added = new_account(&obid);
    world_sign_in(&obid, &added);
    openai.with(|s| s.callback_client = None);
    world_sign_in(&obid, "seeded");
    assert!(obid.stop().success());
    let accounts = obid.home.join("accounts.json");
    let good = std::fs::read(&accounts).unwrap();
    let held = std::fs::read(vault(&obid)).unwrap();
    std::fs::write(&accounts, "this isn't JSON").unwrap();
    let mut obid = start_with(&folder.0, &config, None, &env);
    std::thread::sleep(Duration::from_secs(2));
    assert!(openai.with(|s| s.revoked.is_empty()), "nothing revoked");
    assert_eq!(
        std::fs::read(vault(&obid)).unwrap(),
        held,
        "nothing deleted"
    );
    assert!(obid.stop().success());
    // Mended: as it was.
    std::fs::write(&accounts, &good).unwrap();
    let mut obid = start_with(&folder.0, &config, None, &env);
    let a = account(&obid, &added).unwrap();
    assert_eq!(a["login"], "chatgpt", "{a}");
    assert!(obid.stop().success());
    // A seeded account taken out of the configuration, an added one the file's checks leave out.
    let mut without = config.clone();
    without["agents"]["codex"]["accounts"] = json!([]);
    let mut left_out: Value = serde_json::from_slice(&good).unwrap();
    left_out["added"][0]["configDir"] = json!("/not/obids/own");
    std::fs::write(&accounts, left_out.to_string()).unwrap();
    let obid = start_with(&folder.0, &without, None, &env);
    std::thread::sleep(Duration::from_secs(2));
    assert!(openai.with(|s| s.revoked.is_empty()), "nothing revoked");
    assert_eq!(
        std::fs::read(vault(&obid)).unwrap(),
        held,
        "nothing deleted"
    );
}

/// While its browser page is open an account says `signingIn`: an app waiting on it can tell that
/// from a `label` it already had.
#[test]
fn chatgpt_6_an_account_says_while_it_is_signing_in() {
    let world = world("chatgpt-6-signing");
    let id = new_account(&world.obid);
    world
        .openai
        .with(|s| s.scope = "openid profile email offline_access".into());
    signed_in(&world, &id);
    let before = account(&world.obid, &id).unwrap();
    assert_eq!(before["label"], "ChatGPT plan use is off");
    assert!(before.get("signingIn").is_none(), "{before}");
    world
        .openai
        .with(|s| s.scope = format!("openid offline_access {DIRECT}"));
    let url = within(10, "the sign-in started", || {
        let page = call(
            &world.obid,
            "GET",
            &format!("/v1/accounts/{id}/sign-in?with=chatgpt"),
            None,
        );
        (page.status == 200).then(|| page.body["url"].as_str().unwrap().to_owned())
    });
    let during = account(&world.obid, &id).unwrap();
    assert_eq!(during["signingIn"], true, "{during}");
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 200);
    let after = within(10, "the sign-in ended", || {
        account(&world.obid, &id).filter(|a| a.get("signingIn").is_none())
    });
    assert!(after.get("signingIn").is_none(), "{after}");
}

/// CHATGPT-16: switching kinds removes the old login only after the new one succeeded.
#[test]
fn chatgpt_16_switching_kinds_removes_the_old_login_only_after_success() {
    let world = world("chatgpt-16");
    let id = new_account(&world.obid);
    // Device login first (the stand-in login writes the folder's auth.json).
    let page = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
    assert_eq!(page.status, 200, "{}", page.text);
    within(30, "the device login took", || {
        account(&world.obid, &id).filter(|a| a["signedIn"] == true)
    });
    let auth = world.obid.home.join("accounts").join(&id).join("auth.json");
    assert!(auth.exists());
    // A ChatGPT sign-in that fails leaves it.
    world.openai.with(|s| s.forged = true);
    let url = started(&world.obid, &id);
    let (status, _, _) = http("GET", &world.openai.consent(&url), &[], None);
    assert_eq!(status, 400);
    within(5, "the row said why", || {
        account(&world.obid, &id).filter(|a| !a["label"].is_null())
    });
    assert!(auth.exists(), "a failed sign-in removes nothing");
    assert!(account(&world.obid, &id).unwrap().get("login").is_none());
    // One that takes replaces it.
    world.openai.with(|s| s.forged = false);
    let a = signed_in(&world, &id);
    assert_eq!(a["login"], "chatgpt");
    assert!(!auth.exists(), "the device login went");
    // And back: the device login takes, then the ChatGPT one is revoked and gone.
    let refresh = world.openai.with(|s| s.tokens.last().cloned().unwrap());
    let page = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
    assert_eq!(page.status, 200, "{}", page.text);
    let a = within(30, "the device login took", || {
        account(&world.obid, &id).filter(|a| a["signedIn"] == true && a.get("login").is_none())
    });
    assert!(auth.exists(), "{a}");
    assert!(world.openai.with(|s| s.revoked.contains(&refresh)));
    assert!(!holds(
        &std::fs::read(vault(&world.obid)).unwrap(),
        &refresh
    ));
}

/// CHATGPT-11, as much as sign-in alone shows: the login's tokens come out of every `/v1` answer
/// through the vault's own name, and again after a restart.
#[test]
fn chatgpt_11_every_token_is_taken_out_of_answers_also_after_a_restart() {
    let folder = Folder::new("chatgpt-11");
    let openai = OpenAi::start();
    let config = configured(&folder.0);
    let env = [("OBID_TEST_CHATGPT_ORIGIN", openai.origin.as_str())];
    let mut obid = start_with(&folder.0, &config, None, &env);
    let world_id = new_account(&obid);
    let url = started(&obid, &world_id);
    let (status, _, _) = http("GET", &openai.consent(&url), &[], None);
    assert_eq!(status, 200);
    settled(&obid, &world_id);
    let tokens = openai.with(|s| s.tokens.clone());
    for round in ["now", "after a restart"] {
        for token in &tokens {
            // Who pays is the one thing an answer repeats as it was given.
            let added = call(
                &obid,
                "POST",
                "/v1/accounts",
                Some(&json!({"agent": "codex", "payer": token})),
            );
            assert_eq!(added.status, 201, "{}", added.text);
            assert!(
                !added.text.contains(token.as_str()),
                "{round}: {token} in {}",
                added.text
            );
            assert!(
                added.text.contains("[vault: oby.chatgpt."),
                "{}",
                added.text
            );
        }
        if round == "now" {
            assert!(obid.stop().success());
            obid = start_with(&folder.0, &config, None, &env);
        }
    }
}

/// A job on the account, with `task` for the stand-in Codex: the relay, in its box.
fn job(world: &World, account: &str, task: &str) -> String {
    // (a sign-in that just took still holds the account for a moment)
    let posted = within(10, "the job was taken", || {
        let posted = call(
            &world.obid,
            "POST",
            "/v1/jobs",
            Some(
                &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true",
                "agent": "codex", "account": account, "task": task}}),
            ),
        );
        (posted.status != 422).then_some(posted)
    });
    assert_eq!(posted.status, 201, "{}", posted.text);
    posted.body["id"].as_str().unwrap().to_owned()
}

fn ended(world: &World, id: &str) -> Value {
    let answer = call(&world.obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

fn posts(world: &World) -> usize {
    world
        .openai
        .with(|s| s.bearers.iter().filter(|(_, m, _)| m == "POST").count())
}

/// CHATGPT-1, with runs: a job on the account goes to OpenAI through the relay. OpenAI sees the
/// access token, Codex's process never has it, and the model comes from the account's list.
#[test]
fn chatgpt_1_a_job_runs_through_the_relay_on_the_accounts_own_model() {
    let world = world("chatgpt-1-job");
    let id = new_account(&world.obid);
    let a = signed_in(&world, &id);
    assert_eq!(a["signedIn"], true, "{a}");
    let access = world.openai.with(|s| s.tokens[0].clone());
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_eq!(done["status"], "done", "{done}");
    assert!(summary_of(&done).contains("From the stand-in."), "{done}");
    assert_eq!(done["model"], "gpt-stand-in", "the first the account lists");
    let seen = world.openai.with(|s| s.bearers.clone());
    assert!(
        seen.iter().any(|(token, method, path)| token == &access
            && method == "POST"
            && path == "/v1/responses"),
        "{seen:?}"
    );
    // The periodic read and `account/read` aren't asked on a login that has neither.
    assert!(!done.to_string().contains("rateLimits"));
}

/// CHATGPT-7: the access token is over and the refresh token good: still signed in, and the next
/// run refreshes first.
#[test]
fn chatgpt_7_an_expired_token_stays_signed_in_and_the_next_run_refreshes() {
    let world = world("chatgpt-7");
    world.openai.with(|s| s.expires_in = 1);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    std::thread::sleep(Duration::from_secs(2));
    let a = account(&world.obid, &id).unwrap();
    assert_eq!(a["signedIn"], true, "{a}");
    world.openai.with(|s| s.expires_in = 3600);
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_eq!(done["status"], "done", "{done}");
    let refreshes = world.openai.with(|s| s.refreshes.len());
    assert_eq!(refreshes, 1);
}

/// Every process's environment and arguments, as anything of this user can read them: S0 showed a
/// box reads what an unboxed reader does.
fn every_environment() -> String {
    let flags = if cfg!(target_os = "macos") {
        "-axEww"
    } else {
        "axeww"
    };
    let output = std::process::Command::new("/bin/ps")
        .args([flags, "-o", "command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// CHATGPT-10: while a job runs, no OpenAI token is in any process's environment or arguments, or
/// anywhere in `CODEX_HOME`, and Codex's own `env_key` holds only the job's broker token.
#[test]
fn chatgpt_10_no_process_and_no_file_of_a_running_job_holds_a_token() {
    let world = world("chatgpt-10");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let tokens = world.openai.with(|s| s.tokens.clone());
    let job = job(&world, &id, "RELAY HOLD");
    let worktree = world.obid.home.join("worktrees").join(&job);
    let seen = worktree.join(".obi/env-seen.json");
    let env = within(30, "the run is under way", || std::fs::read(&seen).ok());
    let running = every_environment();
    assert!(
        running.contains("model_providers.openai_chatgpt_plan.base_url"),
        "its own arguments show"
    );
    for token in &tokens {
        assert!(!running.contains(token.as_str()), "{token} in a process");
        assert!(!holds(&env, token), "{token} in its environment");
    }
    let env: Value = serde_json::from_slice(&env).unwrap();
    let broker = env["OBY_MCP_TOKEN"].as_str().unwrap();
    assert!(!tokens.iter().any(|t| t == broker));
    for (path, bytes) in files(&world.obid.home) {
        for token in &tokens {
            assert!(
                !holds(&bytes, token) || path == vault(&world.obid),
                "{token} in {}",
                path.display()
            );
        }
    }
    // The broker token calls nothing but the relay's two routes, and only this job's.
    let host = format!("127.0.0.1:{}", world.obid.port);
    let base = format!("http://{host}/v1/agent/chatgpt/{job}");
    let auth = format!("Bearer {broker}");
    let with_token: &[(&str, &str)] = &[("Authorization", &auth)];
    assert_eq!(
        http("GET", &format!("{base}/models"), with_token, None).0,
        200
    );
    assert_eq!(http("GET", &format!("{base}/models"), &[], None).0, 401);
    assert_eq!(
        http("POST", &format!("{base}/files"), with_token, None).0,
        401
    );
    assert_eq!(
        http("GET", &format!("http://{host}/v1/status"), with_token, None).0,
        401
    );
    assert_eq!(
        http(
            "GET",
            &format!("http://{host}/v1/agent/chatgpt/other/models"),
            with_token,
            None
        )
        .0,
        401
    );
    std::fs::write(worktree.join(".obi/go"), "").unwrap();
    assert_eq!(ended(&world, &job)["status"], "done");
    // It dies with the job.
    assert_eq!(
        http("GET", &format!("{base}/models"), with_token, None).0,
        401
    );
}

/// CHATGPT-11, through the whole way: a model's words holding a token are taken out on their way to
/// the job, as every answer's are.
#[test]
fn chatgpt_11_a_token_in_a_models_words_is_taken_out_on_the_way_through() {
    let world = world("chatgpt-11-through");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let access = world.openai.with(|s| s.tokens[0].clone());
    world.openai.with(|s| {
        s.responses
            .push_back(Reply::Sse(said(&format!("the token is {access} there"))));
    });
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_eq!(done["status"], "done", "{done}");
    assert!(!done.to_string().contains(&access), "{done}");
    assert!(summary_of(&done).contains("[vault: oby.chatgpt."), "{done}");
}

/// CHATGPT-12: a job outlives its token and finishes on the same thread: the relay refreshes on the
/// 401 and sends the same request again, with no `RETRY_WAITS` spent and Codex none the wiser.
#[test]
fn chatgpt_12_a_job_outlives_its_token_with_no_retry_spent() {
    let world = world("chatgpt-12");
    // Past the 5 minutes a refresh comes first, within the 10 a 401 is taken to be its end.
    world.openai.with(|s| s.expires_in = 500);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let first = world.openai.with(|s| {
        let first = s.tokens[0].clone();
        s.refused.insert(first.clone());
        first
    });
    let started = std::time::Instant::now();
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_eq!(done["status"], "done", "{done}");
    assert!(summary_of(&done).contains("From the stand-in."), "{done}");
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "no retry's 30-second wait"
    );
    let (refreshes, seen) = world
        .openai
        .with(|s| (s.refreshes.len(), s.bearers.clone()));
    assert_eq!(refreshes, 1);
    let tokens: Vec<&String> = seen
        .iter()
        .filter(|(_, method, _)| method == "POST")
        .map(|(token, _, _)| token)
        .collect();
    assert_eq!(tokens.len(), 2, "the same request again: {seen:?}");
    assert_eq!(tokens[0], &first);
    assert_ne!(tokens[1], &first);
    assert_eq!(account(&world.obid, &id).unwrap()["signedIn"], true);
}

/// CHATGPT-13: a usage limit, before the stream begins or after: the #1563 question with no reset
/// time, the account marked as held, and the inferred hour never its `resetAt`, a second job's
/// neither. Nothing else goes to OpenAI meanwhile, whatever Codex tries.
#[test]
fn chatgpt_13_a_usage_limit_is_the_question_with_no_reset_time() {
    let world = world("chatgpt-13");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let body =
        json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}});
    world
        .openai
        .with(|s| s.responses.push_back(Reply::Json(429, body.clone())));
    let first = job(&world, &id, "RELAY");
    let stopped = ended(&world, &first);
    assert_eq!(stopped["status"], "interrupted", "{stopped}");
    assert!(
        stopped["reason"]
            .as_str()
            .unwrap()
            .contains("Usage limit reached"),
        "{stopped}"
    );
    assert_eq!(posts(&world), 1, "Codex doesn't try a 429 again");

    let decision = within(20, "the question", || {
        call(&world.obid, "GET", "/v1/limit-decisions", None)
            .body
            .as_array()?
            .iter()
            .find(|d| {
                d["jobs"]
                    .as_array()
                    .is_some_and(|jobs| jobs.iter().any(|j| j["id"] == first.as_str()))
            })
            .cloned()
    });
    assert!(decision["resetAt"].is_null(), "{decision}");
    let options = decision["options"].as_array().unwrap();
    assert!(options.iter().any(|o| o["kind"] == "resume"), "{decision}");

    // The hold obid inferred stays obid's: the row says it's held, and no card says when.
    let held = account(&world.obid, &id).unwrap();
    assert!(!held["fullUntil"].is_null(), "{held}");

    // After the stream began, on a second account: the same, and Codex's retries stay here.
    let second = new_account(&world.obid);
    signed_in(&world, &second);
    world.openai.with(|s| {
        s.responses.push_back(Reply::Sse(failed(
            "subscription_sharing_usage_limit_exceeded",
            "Over.",
        )))
    });
    let before = posts(&world);
    let stopped = ended(&world, &job(&world, &second, "RELAY"));
    assert_eq!(stopped["status"], "interrupted", "{stopped}");
    assert!(
        stopped["reason"]
            .as_str()
            .unwrap()
            .contains("Usage limit reached"),
        "{stopped}"
    );
    assert_eq!(
        posts(&world),
        before + 1,
        "what Codex tried after it stayed here"
    );
    within(20, "its question too", || {
        let all = call(&world.obid, "GET", "/v1/limit-decisions", None).body;
        let all = all.as_array()?;
        (all.len() >= 2).then(|| {
            for decision in all {
                assert!(decision["resetAt"].is_null(), "{decision}");
            }
        })
    });
}

/// CHATGPT-14: a 503 keeps the credentials and backs off; a 403 not-eligible goes to OpenAI once and
/// ends the job saying so; a 401 far from the token's end, or a second straight after a refresh, ends
/// the job with the request id and asks for no sign-in.
#[test]
fn chatgpt_14_a_503_backs_off_a_403_stops_and_a_repeated_401_ends_the_job() {
    let world = world("chatgpt-14");
    let id = new_account(&world.obid);
    signed_in(&world, &id);

    // 403: one request, though Codex would try five more.
    let error =
        json!({"error": {"code": "subscription_sharing_user_not_eligible", "message": "No."}});
    world
        .openai
        .with(|s| s.responses.push_back(Reply::Json(403, error.clone())));
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_ne!(done["status"], "done", "{done}");
    let reason = done["reason"].as_str().unwrap();
    assert!(
        reason.contains("isn't available for this account"),
        "{done}"
    );
    assert!(reason.contains("req_scripted"), "{done}");
    assert_eq!(posts(&world), 1, "no retry of a 403");
    assert_eq!(
        world.openai.with(|s| s.authorized.len()),
        1,
        "no OAuth loop"
    );
    let a = account(&world.obid, &id).unwrap();
    assert_eq!(a["signedIn"], true, "{a}");
    assert!(a["fullUntil"].is_null(), "not a usage limit: {a}");

    // 401, far from the token's end: not cured by a refresh, so none is tried.
    let refreshes = world.openai.with(|s| s.refreshes.len());
    let current = world.openai.with(|s| {
        let token = s
            .tokens
            .iter()
            .rev()
            .find(|t| t.contains("-access-"))
            .cloned()
            .unwrap();
        s.refused.insert(token.clone());
        token
    });
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_ne!(done["status"], "done", "{done}");
    assert!(
        done["reason"].as_str().unwrap().contains("req_refused"),
        "{done}"
    );
    assert_eq!(world.openai.with(|s| s.refreshes.len()), refreshes);
    assert_eq!(account(&world.obid, &id).unwrap()["signedIn"], true);
    world.openai.with(|s| s.refused.remove(&current));

    // 503: credentials kept, and the round tried again by itself.
    for _ in 0..30 {
        world.openai.with(|s| {
            s.responses.push_back(Reply::Json(
                503,
                json!({"detail": "Direct routing is unavailable"}),
            ))
        });
    }
    let id503 = job(&world, &id, "RELAY");
    let retrying = within(60, "it waits to try again", || {
        let j = call(&world.obid, "GET", &format!("/v1/jobs/{id503}"), None).body;
        j["step"]
            .as_str()
            .is_some_and(|s| s.starts_with("Retrying after"))
            .then_some(j)
    });
    assert_eq!(retrying["status"], "running", "{retrying}");
    assert_eq!(account(&world.obid, &id).unwrap()["signedIn"], true);
    let _ = call(&world.obid, "POST", &format!("/v1/jobs/{id503}/stop"), None);

    // A second 401 straight after a refresh: the job ends, the login stays.
    let world = self::world("chatgpt-14-again");
    world.openai.with(|s| s.expires_in = 500);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    world.openai.with(|s| s.refuse_all = true);
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_ne!(done["status"], "done", "{done}");
    assert!(
        done["reason"].as_str().unwrap().contains("req_refused"),
        "{done}"
    );
    assert_eq!(world.openai.with(|s| s.refreshes.len()), 1);
    assert_eq!(account(&world.obid, &id).unwrap()["signedIn"], true);
}

/// CHATGPT-9, at a run: the refresh a run needs says the login ended: the job says what to do, the
/// row stops saying signed in, and nothing is retried.
#[test]
fn chatgpt_9_a_login_ended_mid_job_says_sign_in_again() {
    let world = world("chatgpt-9-job");
    world.openai.with(|s| s.expires_in = 60);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    world.openai.with(|s| {
        s.token_errors
            .push_back((400, json!({"error": "refresh_token_expired"})))
    });
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_ne!(done["status"], "done", "{done}");
    assert!(
        done["reason"]
            .as_str()
            .unwrap()
            .contains("Continue with ChatGPT"),
        "{done}"
    );
    let a = account(&world.obid, &id).unwrap();
    assert_eq!(a["signedIn"], false, "{a}");
    assert_eq!(a["login"], "chatgpt", "the record stays, for its client id");
    let url = started(&world.obid, &id);
    assert!(
        url.contains("client_id=oaiapp_1"),
        "the saved client: {url}"
    );
    assert!(!url.contains("agent_name_hint"), "{url}");
}

/// A conversation on a ChatGPT login goes through the relay with its turn's own token.
#[test]
fn chatgpt_a_conversations_turn_goes_through_the_relay() {
    let folder = Folder::new("chatgpt-conversation");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    config["agents"]["codex"]["bin"] = json!(agent("fake-codex-harness"));
    config["conversation"] = json!({"agent": "codex"});
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let world = World {
        _folder: folder,
        openai,
        obid,
    };
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let answer = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "new": true})),
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert!(
        answer.text.contains("From the stand-in."),
        "{}",
        answer.text
    );
    let access = world.openai.with(|s| s.tokens[0].clone());
    assert!(world.openai.with(|s| {
        s.bearers
            .iter()
            .any(|(t, m, _)| t == &access && m == "POST")
    }));
    // Over its limit: stated, and the turn doesn't go on to another account by itself.
    world.openai.with(|s| {
        s.responses.push_back(Reply::Json(
            429,
            json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}}),
        ))
    });
    let answer = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "new": true})),
    );
    assert!(
        answer.text.contains("Usage limit reached"),
        "{}",
        answer.text
    );
    assert!(!account(&world.obid, &id).unwrap()["fullUntil"].is_null());
}

/// CHATGPT-17: obid keeps that the welcome was shown, across a restart, whatever the app's storage.
#[test]
fn chatgpt_17_obid_keeps_that_the_welcome_was_shown() {
    let folder = Folder::new("chatgpt-17");
    let config = configured(&folder.0);
    let mut obid = start(&folder.0, &config, None);
    let read = call(&obid, "GET", "/v1/settings/chatgpt", None);
    assert_eq!(read.body["welcomed"], false, "{}", read.text);
    let bad = call(
        &obid,
        "PUT",
        "/v1/settings/chatgpt",
        Some(&json!({"welcomed": "yes"})),
    );
    assert_eq!(bad.status, 422, "{}", bad.text);
    let put = call(
        &obid,
        "PUT",
        "/v1/settings/chatgpt",
        Some(&json!({"welcomed": true})),
    );
    assert_eq!(put.body["welcomed"], true, "{}", put.text);
    assert!(obid.stop().success());
    let obid = start(&folder.0, &config, None);
    assert_eq!(
        call(&obid, "GET", "/v1/settings/chatgpt", None).body["welcomed"],
        true
    );
}

/// The owner's decision of 2026-11-02: ChatGPT accounts are offered at a limit, though they report
/// no usage. Offered, never moved to without the owner's click.
#[test]
fn chatgpt_13_a_chatgpt_account_is_offered_at_a_limit_and_nothing_moves_by_itself() {
    let folder = Folder::new("chatgpt-13-offer");
    let openai = OpenAi::start();
    let config = configured(&folder.0);
    let claude = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        json!({"claudeAiOauth": {"accessToken": "fake-claude",
            "expiresAt": (obi_protocol::time::now() + 86400) * 1000}})
        .to_string(),
    )
    .unwrap();
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let world = World {
        _folder: folder,
        openai,
        obid,
    };
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let posted = call(
        &world.obid,
        "POST",
        "/v1/jobs",
        Some(
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
            "task": "LIMITED: 3600", "agent": "claude", "account": "stub"}}),
        ),
    );
    assert_eq!(posted.status, 201, "{}", posted.text);
    let job = posted.body["id"].as_str().unwrap().to_owned();
    let decision = within(30, "the question offers the ChatGPT account", || {
        call(&world.obid, "GET", "/v1/limit-decisions", None)
            .body
            .as_array()?
            .iter()
            .find(|d| {
                d["options"]
                    .as_array()
                    .is_some_and(|o| o.iter().any(|o| o["account"] == id.as_str()))
            })
            .cloned()
    });
    assert_eq!(decision["jobs"][0]["id"], job.as_str());
    let stays = call(&world.obid, "GET", &format!("/v1/jobs/{job}"), None).body;
    assert_eq!(stays["status"], "interrupted", "{stays}");
    assert_eq!(
        stays["account"], "stub",
        "it moves only on the owner's click"
    );
    assert_eq!(posts(&world), 0);
}

/// S0's whole way, on demand: real `codex` as the job's agent in obid's real box, through obid's
/// real relay, to this stand-in, with a shell command approved on the way. Set `OBY_REAL_CODEX` to
/// its binary: never the owner's login, since its `CODEX_HOME` is the job's own.
#[test]
#[ignore = "needs a real codex: OBY_REAL_CODEX=<binary>"]
fn chatgpt_real_codex_talks_through_the_relay() {
    let bin = std::env::var("OBY_REAL_CODEX").expect("OBY_REAL_CODEX");
    let folder = Folder::new("chatgpt-real-talk");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    config["agents"]["codex"]["bin"] = json!(bin);
    config["conversation"] = json!({"agent": "codex"});
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let world = World {
        _folder: folder,
        openai,
        obid,
    };
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    world.openai.with(|s| {
        s.responses
            .push_back(Reply::Sse(said("Hello from the plan.")))
    });
    let answer = within(30, "the account is free", || {
        let answer = call(
            &world.obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": "hi", "new": true})),
        );
        (!answer.text.contains("is being signed in")).then_some(answer)
    });
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert!(
        answer.text.contains("Hello from the plan."),
        "{}",
        answer.text
    );
    // Its next turn picks the thread up, a new process with the provider's settings.
    let conversation = answer.body["conversation"].as_str().unwrap();
    world
        .openai
        .with(|s| s.responses.push_back(Reply::Sse(said("Still here."))));
    let again = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "again", "conversation": conversation})),
    );
    assert!(again.text.contains("Still here."), "{}", again.text);
    let access = world.openai.with(|s| s.tokens[0].clone());
    let seen = world.openai.with(|s| s.bearers.clone());
    assert!(
        seen.iter().all(|(token, _, _)| token == &access),
        "{seen:?}"
    );
}

#[test]
#[ignore = "needs a real codex: OBY_REAL_CODEX=<binary>"]
fn chatgpt_real_codex_runs_through_the_relay() {
    let bin = std::env::var("OBY_REAL_CODEX").expect("OBY_REAL_CODEX");
    let folder = Folder::new("chatgpt-real");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    config["agents"]["codex"]["bin"] = json!(bin);
    let obid = start_with(
        &folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    let world = World {
        _folder: folder,
        openai,
        obid,
    };
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    world.openai.with(|s| {
        s.responses
            .push_back(Reply::Sse(said("Real Codex, through the relay.")));
    });
    let done = ended(&world, &job(&world, &id, "Say hello."));
    assert_eq!(done["status"], "done", "{done}");
    assert!(summary_of(&done).contains("through the relay"), "{done}");
    let access = world.openai.with(|s| s.tokens[0].clone());
    let seen = world.openai.with(|s| s.bearers.clone());
    assert!(
        seen.iter().all(|(token, _, _)| token == &access),
        "{seen:?}"
    );
    // Over its limit: the code decides, not Codex's words about a 429.
    world.openai.with(|s| {
        s.responses.push_back(Reply::Json(
            429,
            json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}}),
        ));
    });
    let stopped = ended(&world, &job(&world, &id, "Say hello again."));
    assert_eq!(stopped["status"], "interrupted", "{stopped}");
    assert!(
        stopped["reason"]
            .as_str()
            .unwrap()
            .contains("Usage limit reached"),
        "{stopped}"
    );
    // Not eligible: said at once, and the one request is all OpenAI gets.
    let world2 = world;
    let again = new_account(&world2.obid);
    signed_in(&world2, &again);
    let before = posts(&world2);
    world2.openai.with(|s| {
        s.responses.push_back(Reply::Json(
            403,
            json!({"error": {"code": "subscription_sharing_user_not_eligible", "message": "No."}}),
        ));
    });
    let refused = ended(&world2, &job(&world2, &again, "Say hello once more."));
    assert!(
        refused["reason"]
            .as_str()
            .unwrap()
            .contains("isn't available"),
        "{refused}"
    );
    assert_eq!(
        posts(&world2),
        before + 1,
        "real Codex retried past the relay"
    );
}

fn summary_of(job: &Value) -> &str {
    job["summary"].as_str().unwrap_or("")
}

#[allow(dead_code)]
mod stand_in_openai {
    //! A stand-in OpenAI on loopback for the ChatGPT tests (docs/contracts/chatgpt-sign-in.md):
    //! authorize, token, JWKS, the OpenID configuration, revoke, `/v1/models` and `/v1/responses`,
    //! as OpenAI's Sign in with ChatGPT pages describe them. No real sign-in, no real token: consent is
    //! this file's redirect, and every token is made here. obid reaches it only through
    //! `OBID_TEST_CHATGPT_ORIGIN`, which only a debug build reads.

    use std::collections::{HashMap, HashSet, VecDeque};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use aws_lc_rs::signature::{KeyPair as _, RsaKeyPair};
    use axum::Router;
    use axum::extract::{Form, Query, State};
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use base64::Engine;
    use serde_json::{Value, json};

    const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    pub const DIRECT: &str = "chatgpt.tokens.use.direct";

    /// What a `POST /v1/responses` answers next.
    #[derive(Clone)]
    pub enum Reply {
        Json(u16, Value),
        /// A stream that ends with these events.
        Sse(Vec<Value>),
    }

    /// How the stand-in behaves next, and what it saw.
    pub struct Script {
        pub sub: String,
        pub email: String,
        pub scope: String,
        /// The person says no.
        pub deny: bool,
        /// The id token is signed with a key OpenAI doesn't publish.
        pub forged: bool,
        /// The id token's audience is another client.
        pub wrong_aud: bool,
        /// The callback names this client, whatever was asked.
        pub callback_client: Option<String>,
        /// How long the token endpoint takes.
        pub hold: Duration,
        pub expires_in: i64,
        pub token_errors: VecDeque<(u16, Value)>,
        pub revoke_statuses: VecDeque<u16>,
        pub responses: VecDeque<Reply>,
        pub models: Value,
        /// Saw: each authorize request's parameters.
        pub authorized: Vec<HashMap<String, String>>,
        /// Saw: each refresh grant's refresh token.
        pub refreshes: Vec<String>,
        /// Saw: every token it made.
        pub tokens: Vec<String>,
        pub revoked: Vec<String>,
        /// Saw: each `/v1` call's bearer token, method and path.
        pub bearers: Vec<(String, String, String)>,
        /// Access tokens it answers 401 for.
        pub refused: HashSet<String>,
        /// Answers 401 for every token.
        pub refuse_all: bool,
        issued_clients: u32,
        minted: u32,
        codes: HashMap<String, Grant>,
    }

    struct Grant {
        client: String,
        nonce: String,
        challenge: String,
    }

    pub struct OpenAi {
        pub origin: String,
        pub script: Arc<Mutex<Script>>,
        good: Arc<RsaKeyPair>,
    }

    #[derive(Clone)]
    struct Shared {
        origin: String,
        script: Arc<Mutex<Script>>,
        good: Arc<RsaKeyPair>,
        other: Arc<RsaKeyPair>,
    }

    impl OpenAi {
        pub fn start() -> OpenAi {
            let good = Arc::new(RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap());
            let other = Arc::new(RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap());
            let script = Arc::new(Mutex::new(Script {
                sub: "stand-in-sub-1".into(),
                email: "person@example.com".into(),
                scope: format!("openid profile email offline_access resource.invoke {DIRECT}"),
                deny: false,
                forged: false,
                wrong_aud: false,
                callback_client: None,
                hold: Duration::ZERO,
                expires_in: 3600,
                token_errors: VecDeque::new(),
                revoke_statuses: VecDeque::new(),
                responses: VecDeque::new(),
                models: json!({"models": [
                    {"slug": "gpt-stand-in", "display_name": "Stand-in", "visibility": "list"},
                    {"slug": "gpt-hidden", "display_name": "Hidden", "visibility": "hide"},
                ]}),
                authorized: Vec::new(),
                refreshes: Vec::new(),
                tokens: Vec::new(),
                revoked: Vec::new(),
                bearers: Vec::new(),
                refused: HashSet::new(),
                refuse_all: false,
                issued_clients: 0,
                minted: 0,
                codes: HashMap::new(),
            }));
            let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
            let shared = Shared {
                origin: origin.clone(),
                script: script.clone(),
                good: good.clone(),
                other,
            };
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                    let app = Router::new()
                        .route("/api/accounts/authorize", get(authorize))
                        .route("/api/accounts/oauth/token", post(token))
                        .route("/api/accounts/oauth/revoke", post(revoke))
                        .route("/.well-known/jwks.json", get(jwks))
                        .route("/.well-known/openid-configuration", get(openid))
                        .route("/v1/models", get(models))
                        .route("/v1/responses", post(responses))
                        .with_state(shared);
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(listener, app).await.unwrap();
                });
            });
            OpenAi {
                origin,
                script,
                good,
            }
        }

        pub fn with<T>(&self, change: impl FnOnce(&mut Script) -> T) -> T {
            change(&mut self.script.lock().unwrap())
        }

        /// The person consents on the page at `url`: where the browser goes next.
        pub fn consent(&self, url: &str) -> String {
            let (status, headers, _) = http("GET", url, &[], None);
            assert_eq!(status, 302, "the stand-in's page redirects");
            headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                .map(|(_, value)| value.clone())
                .expect("a Location")
        }

        pub fn public_key_is_published(&self) -> bool {
            !self.good.public_key().as_ref().is_empty()
        }
    }

    /// One HTTP/1.1 request by hand: its status, headers and body. The caller's `Host` header wins.
    pub fn http(
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> (u16, Vec<(String, String)>, String) {
        let rest = url.strip_prefix("http://").expect("an http URL");
        let (authority, target) = rest
            .split_once('/')
            .map_or((rest, "/".to_owned()), |(a, t)| (a, format!("/{t}")));
        // (status 0: nothing listens there, as after a sign-in's end)
        let Ok(mut stream) = TcpStream::connect(authority) else {
            return (0, Vec::new(), String::new());
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let mut text = format!("{method} {target} HTTP/1.1\r\nConnection: close\r\n");
        if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
            text.push_str(&format!("Host: {authority}\r\n"));
        }
        for (name, value) in headers {
            text.push_str(&format!("{name}: {value}\r\n"));
        }
        let body = body.unwrap_or("");
        if !body.is_empty() || method == "POST" {
            text.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        text.push_str("\r\n");
        text.push_str(body);
        stream.write_all(text.as_bytes()).unwrap();
        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw);
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let (head, rest) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(": "))
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect();
        let chunked = headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
        let text = if chunked {
            unchunk(rest)
        } else {
            rest.to_owned()
        };
        (status, headers, text)
    }

    fn unchunk(mut body: &str) -> String {
        let mut out = String::new();
        while let Some((size, rest)) = body.split_once("\r\n") {
            let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
                break;
            };
            if size == 0 || rest.len() < size {
                break;
            }
            out.push_str(&rest[..size]);
            body = rest[size..].strip_prefix("\r\n").unwrap_or(&rest[size..]);
        }
        out
    }

    async fn authorize(
        State(shared): State<Shared>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Response {
        let mut script = shared.script.lock().unwrap();
        script.authorized.push(query.clone());
        let redirect = query["redirect_uri"].clone();
        let state = query["state"].clone();
        if script.deny {
            let to = format!("{redirect}?error=access_denied&state={state}");
            return (StatusCode::FOUND, [(header::LOCATION, to)]).into_response();
        }
        let asked = query["client_id"].clone();
        let issued = if asked == "dynamic_agent_client" {
            script.issued_clients += 1;
            format!("oaiapp_{}", script.issued_clients)
        } else {
            asked
        };
        let client = script.callback_client.clone().unwrap_or(issued);
        script.minted += 1;
        let code = format!("code-{}", script.minted);
        script.codes.insert(
            code.clone(),
            Grant {
                client: client.clone(),
                nonce: query["nonce"].clone(),
                challenge: query["code_challenge"].clone(),
            },
        );
        let scope = script.scope.replace(' ', "+");
        let to = format!("{redirect}?code={code}&scope={scope}&state={state}&client_id={client}");
        (StatusCode::FOUND, [(header::LOCATION, to)]).into_response()
    }

    fn mint(script: &mut Script, kind: &str) -> String {
        script.minted += 1;
        let token = format!(
            "stand-in-{kind}-{}-{}",
            script.minted,
            uuid::Uuid::new_v4().simple()
        );
        script.tokens.push(token.clone());
        token
    }

    fn id_token(key: &RsaKeyPair, claims: &Value) -> String {
        let head = B64.encode(json!({"alg": "RS256", "kid": "stand-in-key"}).to_string());
        let body = B64.encode(claims.to_string());
        let text = format!("{head}.{body}");
        let mut signature = vec![0; key.public_modulus_len()];
        key.sign(
            &aws_lc_rs::signature::RSA_PKCS1_SHA256,
            &aws_lc_rs::rand::SystemRandom::new(),
            text.as_bytes(),
            &mut signature,
        )
        .unwrap();
        format!("{text}.{}", B64.encode(signature))
    }

    async fn token(
        State(shared): State<Shared>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Response {
        let hold = shared.script.lock().unwrap().hold;
        tokio::time::sleep(hold).await;
        let mut script = shared.script.lock().unwrap();
        let refresh = form.get("grant_type").is_some_and(|g| g == "refresh_token");
        if refresh {
            script.refreshes.push(form["refresh_token"].clone());
        }
        if let Some((status, body)) = script.token_errors.pop_front() {
            return (StatusCode::from_u16(status).unwrap(), axum::Json(body)).into_response();
        }
        let now = obi_protocol::time::now();
        let expires_in = script.expires_in;
        let scope = script.scope.clone();
        if refresh {
            let access = mint(&mut script, "access");
            let rotated = mint(&mut script, "refresh");
            return axum::Json(json!({"access_token": access, "refresh_token": rotated,
            "token_type": "Bearer", "expires_in": expires_in, "scope": scope}))
            .into_response();
        }
        let Some(grant) = script.codes.remove(&form["code"]) else {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        };
        let pkce = B64.encode(obi_protocol::keys::sha256(&form["code_verifier"]));
        if pkce != grant.challenge || form["client_id"] != grant.client {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        }
        let aud = if script.wrong_aud {
            "oaiapp_other".to_owned()
        } else {
            grant.client
        };
        let claims = json!({"iss": shared.origin, "aud": aud, "sub": script.sub, "email": script.email,
        "nonce": grant.nonce, "iat": now, "exp": now + 3600});
        let key = if script.forged {
            &shared.other
        } else {
            &shared.good
        };
        let id = id_token(key, &claims);
        let access = mint(&mut script, "access");
        let rotated = mint(&mut script, "refresh");
        axum::Json(
            json!({"access_token": access, "refresh_token": rotated, "id_token": id,
        "token_type": "Bearer", "expires_in": expires_in, "scope": scope,
        "earliest_refresh_at": now}),
        )
        .into_response()
    }

    async fn revoke(
        State(shared): State<Shared>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Response {
        let mut script = shared.script.lock().unwrap();
        let status = script.revoke_statuses.pop_front().unwrap_or(200);
        if status == 200 {
            script.revoked.push(form["token"].clone());
        }
        StatusCode::from_u16(status).unwrap().into_response()
    }

    /// RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    fn der_ints(der: &[u8]) -> (Vec<u8>, Vec<u8>) {
        fn length(der: &[u8], at: &mut usize) -> usize {
            let first = der[*at] as usize;
            *at += 1;
            if first < 0x80 {
                return first;
            }
            let mut len = 0;
            for _ in 0..first - 0x80 {
                len = len << 8 | der[*at] as usize;
                *at += 1;
            }
            len
        }
        let mut at = 1;
        length(der, &mut at);
        let mut int = || {
            at += 1;
            let len = length(der, &mut at);
            let bytes = der[at..at + len].to_vec();
            at += len;
            bytes
                .into_iter()
                .skip_while(|b| *b == 0)
                .collect::<Vec<u8>>()
        };
        (int(), int())
    }

    async fn jwks(State(shared): State<Shared>) -> axum::Json<Value> {
        let (n, e) = der_ints(shared.good.public_key().as_ref());
        axum::Json(
            json!({"keys": [{"kty": "RSA", "kid": "stand-in-key", "alg": "RS256", "use": "sig",
        "n": B64.encode(n), "e": B64.encode(e)}]}),
        )
    }

    async fn openid(State(shared): State<Shared>) -> axum::Json<Value> {
        axum::Json(json!({"issuer": shared.origin,
        "revocation_endpoint": format!("{}/api/accounts/oauth/revoke", shared.origin)}))
    }

    fn bearer(headers: &HeaderMap) -> String {
        headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or("")
            .to_owned()
    }

    async fn models(State(shared): State<Shared>, headers: HeaderMap) -> Response {
        let mut script = shared.script.lock().unwrap();
        let token = bearer(&headers);
        script
            .bearers
            .push((token.clone(), "GET".into(), "/v1/models".into()));
        if script.refused.contains(&token) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        axum::Json(script.models.clone()).into_response()
    }

    async fn responses(State(shared): State<Shared>, headers: HeaderMap) -> Response {
        let mut script = shared.script.lock().unwrap();
        let token = bearer(&headers);
        script
            .bearers
            .push((token.clone(), "POST".into(), "/v1/responses".into()));
        if script.refuse_all || script.refused.contains(&token) {
            return (
                StatusCode::UNAUTHORIZED,
                [("x-request-id", "req_refused")],
                axum::Json(json!({"detail": "Unauthorized"})),
            )
                .into_response();
        }
        match script.responses.pop_front() {
            Some(Reply::Json(status, body)) => (
                StatusCode::from_u16(status).unwrap(),
                [("x-request-id", "req_scripted")],
                axum::Json(body),
            )
                .into_response(),
            Some(Reply::Sse(events)) => sse(&events),
            None => sse(&said("From the stand-in.")),
        }
    }

    fn sse(events: &[Value]) -> Response {
        let text: String = events
            .iter()
            .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap_or("")))
            .collect();
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/event-stream")],
            text,
        )
            .into_response()
    }

    /// A whole answer, as the Responses API streams one.
    pub fn said(words: &str) -> Vec<Value> {
        vec![
            json!({"type": "response.created", "response": {"id": "resp_1"}}),
            json!({"type": "response.output_item.done", "item": {"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": words}]}}),
            json!({"type": "response.completed", "response": {"id": "resp_1", "usage": {
            "input_tokens": 1, "output_tokens": 1, "total_tokens": 2,
            "input_tokens_details": {"cached_tokens": 0},
            "output_tokens_details": {"reasoning_tokens": 0}}}}),
        ]
    }

    /// A stream that fails with `code` after it began.
    pub fn failed(code: &str, message: &str) -> Vec<Value> {
        vec![
            json!({"type": "response.created", "response": {"id": "resp_2"}}),
            json!({"type": "response.failed", "response": {"id": "resp_2", "status": "failed",
            "error": {"code": code, "message": message}}}),
        ]
    }
}
