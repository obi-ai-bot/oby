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
    let remote = folder.join("remote.git");
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
        if script.refused.contains(&token) {
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
            json!({"type": "response.completed", "response": {"id": "resp_1"}}),
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
