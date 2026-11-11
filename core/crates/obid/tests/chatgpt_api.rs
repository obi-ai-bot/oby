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

/// Only ChatGPT logins can run here: Claude Code's stand-in has no login.
fn world_of_chatgpt_alone(name: &str) -> World {
    world(name)
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
            account(&world.obid, &id)
                .filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        account(&world.obid, &id).filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        account(&world.obid, &id).filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        account(&world.obid, &first)
            .filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        account(&world.obid, &second)
            .filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        account(&world.obid, &id).filter(|a| !a["label"].is_null() && a.get("signingIn").is_none())
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
        let posted = call_typed(
            &world.obid,
            "POST",
            "/v1/jobs",
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "check": "true",
            "agent": "codex", "account": account, "task": task}}),
        );
        (posted.status != 422).then_some(posted)
    });
    assert_eq!(posted.status, 201, "{}", posted.text);
    // (the owner's own never waits for a yes: `ended` would wait with it)
    assert!(posted.body["waiting"].is_null(), "{}", posted.text);
    posted.body["id"].as_str().unwrap().to_owned()
}

/// The job once it ended. Bounded: a job that never ends fails its test, and doesn't hang it.
fn ended(world: &World, id: &str) -> Value {
    within(180, "the job ended", || {
        let answer = call(&world.obid, "GET", &format!("/v1/jobs/{id}"), None);
        assert_eq!(answer.status, 200, "{}", answer.text);
        (!["queued", "running"].contains(&answer.body["status"].as_str().unwrap_or_default()))
            .then_some(answer.body)
    })
}

/// Waits until job `id` waits at its account's limit, with no card (LIMITS-15).
fn waits_at_limit(world: &World, id: &str) -> Value {
    let waits = within(180, "the job waits at its limit", || {
        let job = call(&world.obid, "GET", &format!("/v1/jobs/{id}"), None).body;
        (job["status"] == "queued" && !job["resumesAt"].is_null()).then_some(job)
    });
    assert!(waits["accountLimit"].is_null(), "{waits}");
    let decisions = call(&world.obid, "GET", "/v1/limit-decisions", None).body;
    assert_eq!(decisions, json!([]), "no card (LIMITS-15)");
    waits
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
        running.contains(r#"shell_environment_policy.exclude=["OBY_MCP_TOKEN"]"#),
        "commands don't get the broker token"
    );
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
/// 401 and sends the same request again, with none of its tries spent and Codex none the wiser.
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
    assert!(started.elapsed() < Duration::from_secs(25), "no try's wait");
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

/// CHATGPT-13: a usage limit, before the stream begins or after: the job waits with no card
/// (LIMITS-15), the account marked as held, a second job's too. Nothing else goes to OpenAI
/// meanwhile, whatever Codex tries.
#[test]
fn chatgpt_13_a_usage_limit_waits_with_no_card() {
    let world = world("chatgpt-13");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let body =
        json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}});
    world
        .openai
        .with(|s| s.responses.push_back(Reply::Json(429, body.clone())));
    let first = job(&world, &id, "RELAY");
    waits_at_limit(&world, &first);
    assert_eq!(posts(&world), 1, "Codex doesn't try a 429 again");

    // The hold obid inferred stays obid's: the row says it's held.
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
    waits_at_limit(&world, &job(&world, &second, "RELAY"));
    assert_eq!(
        posts(&world),
        before + 1,
        "what Codex tried after it stayed here"
    );
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
            .is_some_and(|s| s.starts_with("Trying again in"))
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

/// LIMITS-15: a ChatGPT login reports no usage, so it never shows room: a job at its limit doesn't
/// move onto one by itself, it waits, and moves there only on the owner's click.
#[test]
fn chatgpt_13_nothing_moves_onto_a_chatgpt_account_by_itself() {
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
    // No account named, so Oby may move it (LIMITS-15).
    let posted = call_typed(
        &world.obid,
        "POST",
        "/v1/jobs",
        &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
        "task": "LIMITED WEEK: 86400", "agent": "claude"}}),
    );
    assert_eq!(posted.status, 201, "{}", posted.text);
    let job = posted.body["id"].as_str().unwrap().to_owned();
    let stays = waits_at_limit(&world, &job);
    assert_eq!(
        stays["account"], "stub",
        "it moves only on the owner's click"
    );
    assert_eq!(posts(&world), 0);
}

/// The owner's click in an app of theirs: signed by the device's key.
fn allow(world: &World, id: &str) -> Answer {
    call_typed(
        &world.obid,
        "POST",
        &format!("/v1/accounts/{id}/background"),
        &json!({"given": true}),
    )
}

/// A routine on `agent` and `account`, made paused-free, its id.
fn routine(world: &World, name: &str, task: &str, agent: &str, account: &str) -> String {
    let made = call(
        &world.obid,
        "POST",
        "/v1/routines",
        Some(
            &json!({"name": name, "repo": "obi-e2e/fixture-app", "kind": "research",
            "task": task, "agent": agent, "account": account,
            "schedule": {"every": "weekdays", "at": "09:00"},
            "zone": "America/Sao_Paulo", "createdBy": "mac"}),
        ),
    );
    assert_eq!(made.status, 201, "{}", made.text);
    made.body["id"].as_str().unwrap().to_owned()
}

/// Runs the routine now: its job's id.
fn run_routine(world: &World, id: &str) -> String {
    let ran = within(10, "the routine ran", || {
        let ran = call(&world.obid, "POST", &format!("/v1/routines/{id}/run"), None);
        ran.body["job"]["id"].is_string().then_some(ran)
    });
    assert_eq!(ran.status, 201, "{}", ran.text);
    ran.body["job"]["id"].as_str().unwrap().to_owned()
}

fn job_now(world: &World, id: &str) -> Value {
    call(&world.obid, "GET", &format!("/v1/jobs/{id}"), None).body
}

/// A second way in beside a ChatGPT login: Claude Code's stand-in, signed in.
fn world_with_claude(name: &str) -> World {
    let folder = Folder::new(name);
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
    World {
        _folder: folder,
        openai,
        obid,
    }
}

/// CHATGPT-20: with no yes, a routine on Auto passes the login by for the next account, even when
/// the owner's default is that login. With nothing else it waits and says why: not failed, nothing
/// sent to OpenAI, and on the owner's yes it starts there.
#[test]
fn chatgpt_20_no_consent_no_background_use() {
    let world = world_with_claude("chatgpt-20-skips");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let set = call(
        &world.obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"defaultAccount": id})),
    );
    assert_eq!(set.status, 200, "{}", set.text);
    let auto = routine(&world, "On Auto", "Nothing to do", "auto", "auto");
    let job = run_routine(&world, &auto);
    let ran = job_now(&world, &job);
    assert_eq!(ran["account"], "stub", "the next account: {ran}");
    assert!(ran["waiting"].is_null(), "{ran}");
    ended(&world, &job);
    assert_eq!(posts(&world), 0);

    let world = world_of_chatgpt_alone("chatgpt-20-waits");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let auto = routine(&world, "On Auto", "RELAY", "auto", "auto");
    let job = run_routine(&world, &auto);
    let waits = within(10, "it says why", || {
        let now = job_now(&world, &job);
        now["waiting"]["reserved"].is_array().then_some(now)
    });
    assert_eq!(waits["status"], "queued", "{waits}");
    assert_eq!(waits["waiting"]["kind"], "limit", "a kind the apps know");
    let step = waits["step"].as_str().unwrap();
    assert!(step.contains("work I run by myself"), "{step}");
    let on_routine = call(&world.obid, "GET", &format!("/v1/routines/{auto}"), None).body;
    assert_eq!(on_routine["failuresInARow"], 0, "{on_routine}");
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(job_now(&world, &job)["status"], "queued");
    assert_eq!(posts(&world), 0, "nothing went to OpenAI");

    let start = call(&world.obid, "POST", &format!("/v1/jobs/{job}/start"), None);
    assert_ne!(start.status, 200, "Start now can't say yes: {}", start.text);

    let answer = allow(&world, &id);
    assert_eq!(answer.status, 200, "{}", answer.text);
    assert_eq!(answer.body["backgroundConsent"], true);
    let done = ended(&world, &job);
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["account"], id.as_str());
    assert!(posts(&world) > 0, "it ran on the plan");
}

/// CHATGPT-20: work with no signature on it is nobody's to watch, and a routine naming the login
/// waits on it, never going elsewhere.
#[test]
fn chatgpt_20_unsigned_jobs_and_named_routines_wait_too() {
    let world = world_with_claude("chatgpt-20-unsigned");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let posted = within(10, "the job was taken", || {
        let posted = call(
            &world.obid,
            "POST",
            "/v1/jobs",
            Some(
                &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research",
                "agent": "codex", "task": "RELAY"}}),
            ),
        );
        (posted.status != 422).then_some(posted)
    });
    assert_eq!(posted.status, 201, "{}", posted.text);
    let unsigned = posted.body["id"].as_str().unwrap().to_owned();
    assert!(job_now(&world, &unsigned)["waiting"]["reserved"].is_array());

    let named = routine(&world, "Named", "RELAY", "codex", &id);
    let job = run_routine(&world, &named);
    let waits = job_now(&world, &job);
    assert!(waits["waiting"]["reserved"].is_array(), "{waits}");
    assert_eq!(waits["account"], id.as_str());
    assert_eq!(posts(&world), 0);
    assert_eq!(allow(&world, &id).status, 200);
    let done = ended(&world, &job);
    assert_eq!(done["account"], id.as_str(), "{done}");
    assert_eq!(done["status"], "done", "{done}");
}

/// CHATGPT-21: a yes is for one account, and it goes with the login.
#[test]
fn chatgpt_21_consent_is_per_account() {
    let world = world_of_chatgpt_alone("chatgpt-21");
    let (one, two) = (new_account(&world.obid), new_account(&world.obid));
    signed_in(&world, &one);
    let two_says = signed_in(&world, &two);
    assert!(two_says.get("backgroundConsent").is_none(), "{two_says}");
    assert_eq!(allow(&world, &one).status, 200);
    let says = |id: &str| account(&world.obid, id).unwrap();
    assert_eq!(says(&one)["backgroundConsent"], true);
    assert!(says(&two).get("backgroundConsent").is_none());

    let on_two = run_routine(&world, &routine(&world, "Two", "RELAY", "codex", &two));
    assert!(job_now(&world, &on_two)["waiting"]["reserved"].is_array());
    assert_eq!(posts(&world), 0);
    let on_one = run_routine(&world, &routine(&world, "One", "RELAY", "codex", &one));
    assert_eq!(ended(&world, &on_one)["status"], "done");
    assert!(job_now(&world, &on_two)["waiting"]["reserved"].is_array());

    let stop = call(
        &world.obid,
        "POST",
        &format!("/v1/jobs/{on_two}/stop"),
        None,
    );
    assert_eq!(stop.status, 200, "{}", stop.text);
    let gone = call(&world.obid, "DELETE", &format!("/v1/accounts/{one}"), None);
    assert!(gone.status < 300, "{}", gone.text);
    let kept: Value =
        serde_json::from_slice(&std::fs::read(world.obid.home.join("settings.json")).unwrap())
            .unwrap();
    assert!(kept.get("backgroundConsent").is_none(), "{kept}");
}

/// CHATGPT-22: what the owner started needs no yes, on Auto too, nor does their signed follow-up
/// of a routine's job.
#[test]
fn chatgpt_22_foreground_needs_no_consent() {
    let world = world_of_chatgpt_alone("chatgpt-22");
    let id = new_account(&world.obid);
    let a = signed_in(&world, &id);
    assert!(a.get("backgroundConsent").is_none(), "{a}");
    let posted = within(10, "the job was taken", || {
        let posted = call_typed(
            &world.obid,
            "POST",
            "/v1/jobs",
            &json!({"job": {"repo": "obi-e2e/fixture-app", "kind": "research", "task": "RELAY"}}),
        );
        (posted.status != 422).then_some(posted)
    });
    assert_eq!(posted.status, 201, "{}", posted.text);
    let mine = posted.body["id"].as_str().unwrap().to_owned();
    assert!(
        job_now(&world, &mine)["waiting"].is_null(),
        "it doesn't wait"
    );
    let done = ended(&world, &mine);
    assert_eq!(
        (&done["status"], &done["account"]),
        (&json!("done"), &json!(id)),
        "{done}"
    );
    assert!(posts(&world) > 0);

    let job = run_routine(&world, &routine(&world, "Mine next", "RELAY", "codex", &id));
    assert!(job_now(&world, &job)["waiting"]["reserved"].is_array());
    assert_eq!(
        call(&world.obid, "POST", &format!("/v1/jobs/{job}/stop"), None).status,
        200
    );
    let next = call_typed(
        &world.obid,
        "POST",
        &format!("/v1/jobs/{job}/messages"),
        &json!({"text": "RELAY again"}),
    );
    assert_eq!(next.status, 201, "{}", next.text);
    let followed = next.body["id"].as_str().unwrap().to_owned();
    assert_eq!(ended(&world, &followed)["status"], "done");
}

/// CHATGPT-23: taking the yes back stops work nobody's watching on it now, with why, and what comes
/// next waits. What the owner started goes on.
#[test]
fn chatgpt_23_withdrawn_consent_stops_it() {
    let world = world_of_chatgpt_alone("chatgpt-23");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    assert_eq!(allow(&world, &id).status, 200);
    let name = routine(&world, "Running", "SLEEP", "codex", &id);
    let running = run_routine(&world, &name);
    within(30, "it runs", || {
        (job_now(&world, &running)["status"] == "running").then_some(())
    });
    let mine = job(&world, &id, "SLEEP");
    within(30, "the owner's runs", || {
        (job_now(&world, &mine)["status"] == "running").then_some(())
    });

    // A no needs no signature: a key that can't sign never keeps anyone from taking it back.
    let off = call(
        &world.obid,
        "POST",
        &format!("/v1/accounts/{id}/background"),
        Some(&json!({"given": false})),
    );
    assert_eq!(off.status, 200, "{}", off.text);
    assert_eq!(off.body["backgroundConsent"], false);
    let stopped = within(10, "it stopped", || {
        let now = job_now(&world, &running);
        (now["status"] == "stopped").then_some(now)
    });
    assert!(
        stopped["reason"].as_str().unwrap().contains("turned off"),
        "{stopped}"
    );
    assert_eq!(job_now(&world, &mine)["status"], "running", "the owner's");
    assert_eq!(
        call(&world.obid, "POST", &format!("/v1/jobs/{mine}/stop"), None).status,
        200
    );

    let again = run_routine(&world, &routine(&world, "Next", "RELAY", "codex", &id));
    assert!(job_now(&world, &again)["waiting"]["reserved"].is_array());
}

/// CHATGPT-24: the token alone never says yes, nor does a signature over another request or one
/// used already; a phone's gateway doesn't have the route; and an account that isn't a ChatGPT login
/// has nothing to answer.
#[test]
fn chatgpt_24_the_token_alone_cant_give_consent() {
    let world = world_of_chatgpt_alone("chatgpt-24");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let path = format!("/v1/accounts/{id}/background");
    let yes = json!({"given": true});
    let says = || account(&world.obid, &id).unwrap();

    let token_alone = call(&world.obid, "POST", &path, Some(&yes));
    assert_eq!(token_alone.status, 403, "{}", token_alone.text);
    assert!(says().get("backgroundConsent").is_none());
    assert!(
        !world.obid.home.join("settings.json").is_file()
            || !std::fs::read_to_string(world.obid.home.join("settings.json"))
                .unwrap()
                .contains("backgroundConsent")
    );

    let other = words_header(&world.obid, "POST", &path, Some(&json!({"given": false})));
    let wrong = call_with_headers(
        &world.obid,
        TOKEN,
        "POST",
        &path,
        Some(&yes),
        &[("X-Oby-Words", &other)],
    );
    assert_eq!(
        wrong.status, 403,
        "a signature over other words: {}",
        wrong.text
    );
    assert!(says().get("backgroundConsent").is_none());

    let once = words_header(&world.obid, "POST", &path, Some(&yes));
    let first = call_with_headers(
        &world.obid,
        TOKEN,
        "POST",
        &path,
        Some(&yes),
        &[("X-Oby-Words", &once)],
    );
    assert_eq!(first.status, 200, "{}", first.text);
    assert_eq!(
        call(&world.obid, "POST", &path, Some(&json!({"given": false}))).status,
        200
    );
    let replayed = call_with_headers(
        &world.obid,
        TOKEN,
        "POST",
        &path,
        Some(&yes),
        &[("X-Oby-Words", &once)],
    );
    assert_eq!(replayed.status, 403, "used already: {}", replayed.text);
    assert_eq!(says()["backgroundConsent"], false);

    let codex = call(
        &world.obid,
        "POST",
        "/v1/accounts/codex/background",
        Some(&json!({"given": false})),
    );
    assert_eq!(codex.status, 422, "{}", codex.text);
}

/// CHATGPT-25: a routine's job past a limit doesn't move onto a ChatGPT login by itself, with or
/// without its yes (it shows no room, LIMITS-15): it waits, with no card.
#[test]
fn chatgpt_25_a_limit_never_moves_a_routines_job_onto_the_login() {
    let world = world_with_claude("chatgpt-25");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    assert_eq!(allow(&world, &id).status, 200);
    let limited = routine(&world, "Limited", "LIMITED WEEK: 86400", "claude", "stub");
    let job = run_routine(&world, &limited);
    let waits = waits_at_limit(&world, &job);
    assert_eq!(waits["account"], "stub", "{waits}");
    assert_eq!(posts(&world), 0);
}

/// CHATGPT-26: an account from before there was a question starts with no yes, and nothing that ran
/// before fails: the owner's own work runs, a routine's waits, and it isn't a failure.
#[test]
fn chatgpt_26_an_existing_account_starts_with_no_yes_and_nothing_fails() {
    let world = world_of_chatgpt_alone("chatgpt-26");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let name = routine(&world, "Old routine", "RELAY", "auto", "auto");
    let world = restarted(world);
    let a = account(&world.obid, &id).unwrap();
    assert_eq!(a["signedIn"], true, "{a}");
    assert!(
        a.get("backgroundConsent").is_none(),
        "asked, not refused: {a}"
    );
    assert_eq!(ended(&world, &job(&world, &id, "RELAY"))["status"], "done");
    let waits = run_routine(&world, &name);
    assert!(job_now(&world, &waits)["waiting"]["reserved"].is_array());
    let kept = call(&world.obid, "GET", &format!("/v1/routines/{name}"), None).body;
    assert_eq!(kept["failuresInARow"], 0, "{kept}");
}

/// CHATGPT-20: words no app signed, to a routine's job that ended, start work nobody's watching. The
/// owner's signed words start theirs (CHATGPT-22).
#[test]
fn chatgpt_20_unsigned_words_to_a_routines_job_stay_in_the_background() {
    let world = world_of_chatgpt_alone("chatgpt-20-api");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    assert_eq!(allow(&world, &id).status, 200);
    let job = run_routine(&world, &routine(&world, "Ran", "RELAY", "codex", &id));
    assert_eq!(ended(&world, &job)["status"], "done");
    let off = call(
        &world.obid,
        "POST",
        &format!("/v1/accounts/{id}/background"),
        Some(&json!({"given": false})),
    );
    assert_eq!(off.status, 200, "{}", off.text);
    let next = call(
        &world.obid,
        "POST",
        &format!("/v1/jobs/{job}/messages"),
        Some(&json!({"text": "RELAY again"})),
    );
    assert_eq!(next.status, 201, "{}", next.text);
    let next = next.body["id"].as_str().unwrap();
    assert!(
        job_now(&world, next)["waiting"]["reserved"].is_array(),
        "{}",
        job_now(&world, next)
    );
}

/// CHATGPT-20: a job waiting for the yes isn't using the account: it doesn't keep the owner from
/// signing it in or removing it, and a job with nothing left to wait for says so.
#[test]
fn chatgpt_20_a_wait_doesnt_keep_the_account_from_changing() {
    let world = world_of_chatgpt_alone("chatgpt-20-change");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let job = run_routine(&world, &routine(&world, "Named", "RELAY", "codex", &id));
    assert!(job_now(&world, &job)["waiting"]["reserved"].is_array());
    let page = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in?with=chatgpt"),
        None,
    );
    assert_eq!(page.status, 200, "signing in again: {}", page.text);
    let ended_it = call(
        &world.obid,
        "DELETE",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
    assert!(ended_it.status < 300, "{}", ended_it.text);
    let gone = within(10, "the account went", || {
        let gone = call(&world.obid, "DELETE", &format!("/v1/accounts/{id}"), None);
        (gone.status != 409).then_some(gone)
    });
    assert!(gone.status < 300, "removing it: {}", gone.text);
    let stopped = within(10, "it says why", || {
        // (the scheduler looks when something changes: a new routine is one)
        let now = job_now(&world, &job);
        (now["status"] == "stopped").then_some(now)
    });
    assert!(
        stopped["reason"].as_str().unwrap().contains("was removed"),
        "{stopped}"
    );
}

/// CHATGPT-21: a device login replacing a ChatGPT login takes the yes with it: whoever signs in with
/// ChatGPT here next is asked.
#[test]
fn chatgpt_21_a_device_login_forgets_the_yes() {
    let world = world("chatgpt-21-device");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    assert_eq!(allow(&world, &id).status, 200);
    let page = call(
        &world.obid,
        "GET",
        &format!("/v1/accounts/{id}/sign-in"),
        None,
    );
    assert_eq!(page.status, 200, "{}", page.text);
    within(30, "the device login took", || {
        account(&world.obid, &id).filter(|a| a.get("login").is_none() && a["signedIn"] == true)
    });
    let kept: Value =
        serde_json::from_slice(&std::fs::read(world.obid.home.join("settings.json")).unwrap())
            .unwrap();
    assert!(kept.get("backgroundConsent").is_none(), "{kept}");
    let again = signed_in(&world, &id);
    assert_eq!(again["login"], "chatgpt");
    assert!(
        again.get("backgroundConsent").is_none(),
        "asked anew: {again}"
    );
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
    // A command of real Codex's has the rest of its environment, and not the broker token.
    world.openai.with(|s| {
        s.responses.push_back(Reply::Sse(vec![
            json!({"type": "response.created", "response": {"id": "r2"}}),
            json!({"type": "response.output_item.done", "item": {"type": "function_call", "id": "fc",
                "call_id": "c1", "name": "exec_command", "arguments": json!({"cmd": "env"}).to_string()}}),
            json!({"type": "response.completed", "response": {"id": "r2", "usage": {
                "input_tokens": 1, "output_tokens": 1, "total_tokens": 2,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0}}}}),
        ]));
        s.responses.push_back(Reply::Sse(said("Ran it.")));
    });
    let running = job(&world, &id, "Run env.");
    // (obid asks the owner before `env` in every mode: it's the owner here)
    let waiting = within(60, "its approval", || {
        let job = call(&world.obid, "GET", &format!("/v1/jobs/{running}"), None).body;
        job["permission"]["id"].is_string().then_some(job)
    });
    let allowed = call_signed(
        &world.obid,
        "POST",
        &format!("/v1/jobs/{running}/permission"),
        Some(&json!({"id": waiting["permission"]["id"], "decision": "allow"})),
    );
    assert_eq!(allowed.status, 200, "{}", allowed.text);
    let ran = ended(&world, &running);
    let sent = world.openai.with(|s| s.bodies.join("\n"));
    assert!(sent.contains("PATH="), "the command ran: {ran}");
    assert!(
        !sent.contains("OBY_MCP_TOKEN"),
        "the broker token reached a command"
    );
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

/// A job of `account` waiting before it uses the relay: its address and broker token, for calling the
/// relay as Codex does, as often as a test needs. `.obi/go` lets it finish.
fn held_job(world: &World, account: &str) -> (String, String, String) {
    let id = job(world, account, "RELAY HOLD");
    let seen = world
        .obid
        .home
        .join("worktrees")
        .join(&id)
        .join(".obi/env-seen.json");
    let env = within(30, "the run is under way", || std::fs::read(&seen).ok());
    let env: Value = serde_json::from_slice(&env).unwrap();
    let base = format!("http://127.0.0.1:{}/v1/agent/chatgpt/{id}", world.obid.port);
    (id, base, env["OBY_MCP_TOKEN"].as_str().unwrap().to_owned())
}

fn post_as_codex(base: &str, broker: &str) -> (u16, Vec<(String, String)>, String) {
    let auth = format!("Bearer {broker}");
    http(
        "POST",
        &format!("{base}/responses"),
        &[("Authorization", &auth)],
        Some("{}"),
    )
}

/// One request's answer through the relay, then whether the next one reaches OpenAI: what a run that
/// wasn't stopped does.
fn then_goes_on(world: &World, base: &str, broker: &str, what: &str) {
    let before = posts(world);
    let (status, _, text) = post_as_codex(base, broker);
    assert_eq!(status, 200, "{what}: the next request: {text}");
    assert_eq!(
        posts(world),
        before + 1,
        "{what}: the next request reached OpenAI"
    );
}

/// CHATGPT-14: a model's words naming a failure are words. Only an event that is one is one: a job
/// editing the relay's own file writes `response.failed` all the time.
#[test]
fn chatgpt_14_a_models_words_naming_a_failure_fail_nothing() {
    let world = world("chatgpt-14-words");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let (job, base, broker) = held_job(&world, &id);
    let words = r#"the event {"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}} and {"type":"error"} and "error""#;
    let mut completed = said("fine");
    completed[2]["response"]["instructions"] = json!(words);
    for events in [said(words), completed] {
        world
            .openai
            .with(|s| s.responses.push_back(Reply::Sse(events)));
        let (status, _, text) = post_as_codex(&base, &broker);
        assert_eq!(status, 200);
        assert!(text.contains("response.completed"), "{text}");
        then_goes_on(&world, &base, &broker, words);
    }
    std::fs::write(
        world.obid.home.join("worktrees").join(&job).join(".obi/go"),
        "",
    )
    .unwrap();
    assert_eq!(ended(&world, &job)["status"], "done");
}

/// CHATGPT-14: only what OpenAI's page says is final stops a run. A stream that fails with a code it
/// doesn't list, or a status it doesn't, goes on to Codex as it came, and the next request goes out.
#[test]
fn chatgpt_14_what_openai_doesnt_call_final_never_stops_a_run() {
    let world = world("chatgpt-14-unknown");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let (job, base, broker) = held_job(&world, &id);
    for code in [
        "server_error",
        "rate_limit_exceeded",
        "brand_new_code",
        "context_length_exceeded",
    ] {
        world
            .openai
            .with(|s| s.responses.push_back(Reply::Sse(failed(code, "Hm."))));
        let (status, _, text) = post_as_codex(&base, &broker);
        assert_eq!(status, 200, "{code}");
        assert!(text.contains(code), "{code}: as it came: {text}");
        then_goes_on(&world, &base, &broker, code);
    }
    let context = json!({"error": {"code": "context_length_exceeded", "message": "Too long.",
        "type": "invalid_request_error", "param": null}});
    for (status, body) in [
        (408, json!({"detail": "Timed out"})),
        (
            409,
            json!({"error": {"code": "conflict", "message": "Busy."}}),
        ),
        (
            404,
            json!({"error": {"code": "brand_new_code", "message": "Hm."}}),
        ),
        (400, context),
    ] {
        world
            .openai
            .with(|s| s.responses.push_back(Reply::Json(status, body.clone())));
        let (got, headers, text) = post_as_codex(&base, &broker);
        assert_eq!(got, status, "{body}");
        let said: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        assert_eq!(said, body, "as it came");
        assert!(
            headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("content-type") && v.contains("json")),
            "{headers:?}"
        );
        then_goes_on(&world, &base, &broker, &body.to_string());
    }
    // A 403 that isn't OpenAI's admission saying so: an edge's page, not a policy of the plan's.
    world.openai.with(|s| {
        s.responses
            .push_back(Reply::Text(403, "<html>blocked</html>".into()))
    });
    let (got, _, text) = post_as_codex(&base, &broker);
    assert_eq!((got, text.as_str()), (403, "<html>blocked</html>"));
    then_goes_on(&world, &base, &broker, "a 403 with no word of OpenAI's");
    // The same for a 401: OpenAI's own say so in JSON, and this says nothing of the login.
    world.openai.with(|s| {
        s.responses
            .push_back(Reply::Text(401, "<html>sign in to the proxy</html>".into()))
    });
    let (got, _, text) = post_as_codex(&base, &broker);
    assert_eq!(
        (got, text.as_str()),
        (401, "<html>sign in to the proxy</html>")
    );
    then_goes_on(&world, &base, &broker, "a 401 with no word of OpenAI's");
    std::fs::write(
        world.obid.home.join("worktrees").join(&job).join(".obi/go"),
        "",
    )
    .unwrap();
    assert_eq!(ended(&world, &job)["status"], "done");
}

/// CHATGPT-13: a usage limit can also come as a plain stream `error` event, in either shape, which
/// real Codex ignores (spike, 2026-11-03): the relay reads it, and what Codex tries next stays here.
#[test]
fn chatgpt_13_a_limit_in_a_plain_error_event_stops_the_run() {
    let world = world("chatgpt-13-error-event");
    let limit = "subscription_sharing_usage_limit_exceeded";
    for event in [
        json!({"type": "error", "code": limit, "message": "Over.", "param": null}),
        json!({"type": "error", "error": {"code": limit, "message": "Over."}}),
    ] {
        // (an account each: the first is held after its limit)
        let id = new_account(&world.obid);
        signed_in(&world, &id);
        let (job, base, broker) = held_job(&world, &id);
        world.openai.with(|s| {
            s.responses.push_back(Reply::Sse(vec![
                json!({"type": "response.created", "response": {"id": "r"}}),
                event.clone(),
            ]))
        });
        assert_eq!(post_as_codex(&base, &broker).0, 200);
        let before = posts(&world);
        let (status, _, _) = post_as_codex(&base, &broker);
        assert_eq!(status, 429, "{event}");
        assert_eq!(posts(&world), before, "{event}: it stayed here");
        std::fs::write(
            world.obid.home.join("worktrees").join(&job).join(".obi/go"),
            "",
        )
        .unwrap();
        waits_at_limit(&world, &job);
    }
}

/// The hold obid inferred from a limit with no time gives way to a run that went through: a limit
/// that passed needn't be waited out.
#[test]
fn chatgpt_13_a_success_through_the_relay_lifts_the_hour_obid_inferred() {
    let world = world("chatgpt-13-lift");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let (job_on, base, broker) = held_job(&world, &id);
    let body =
        json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}});
    world
        .openai
        .with(|s| s.responses.push_back(Reply::Json(429, body)));
    waits_at_limit(&world, &job(&world, &id, "RELAY"));
    assert!(!account(&world.obid, &id).unwrap()["fullUntil"].is_null());
    // A job that was under way still gets through.
    let (status, _, _) = post_as_codex(&base, &broker);
    assert_eq!(status, 200);
    within(5, "the hold is lifted", || {
        account(&world.obid, &id).filter(|a| a["fullUntil"].is_null())
    });
    std::fs::write(
        world
            .obid
            .home
            .join("worktrees")
            .join(&job_on)
            .join(".obi/go"),
        "",
    )
    .unwrap();
    ended(&world, &job_on);
}

/// An empty model list is no list: the owner's model stays, as when the list can't be had.
#[test]
fn chatgpt_1_an_empty_model_list_keeps_the_owners_model() {
    let world = world("chatgpt-1-no-models");
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let options = call(&world.obid, "GET", "/v1/settings/models/options", None).body;
    let chosen = options["codex"]["models"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let tuned = call(
        &world.obid,
        "PATCH",
        &format!("/v1/accounts/{id}"),
        Some(&json!({"model": chosen})),
    );
    assert_eq!(tuned.status, 200, "{}", tuned.text);
    world.openai.with(|s| s.models = json!({"models": []}));
    let done = ended(&world, &job(&world, &id, "RELAY"));
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(done["model"], chosen.as_str(), "{done}");
}

/// A configuration with a device login beside what's asked for, conversations on Codex first.
fn talk_world(name: &str, device_full: bool, claude: bool) -> World {
    let folder = Folder::new(name);
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    config["agents"]["codex"]["bin"] = json!(agent("fake-codex-harness"));
    config["conversation"] = json!({"agent": "codex"});
    let device = folder.0.join("home/accounts/device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(
        device.join("auth.json"),
        json!({"OPENAI_API_KEY": "fake"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"]["accounts"] =
        json!([{"id": "device", "name": "Device", "configDir": device}]);
    if claude {
        let stub = folder.0.join("home/accounts/stub");
        std::fs::create_dir_all(&stub).unwrap();
        std::fs::write(stub.join(".credentials.json"), "{}").unwrap();
    }
    if device_full {
        let now = obi_protocol::time::now();
        std::fs::write(
            folder.0.join("home/usage.json"),
            json!({"device": {"fullUntil": now + 3600}}).to_string(),
        )
        .unwrap();
    }
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

fn talk_next(world: &World) -> Value {
    call(&world.obid, "GET", "/v1/status", None).body["talkNext"].clone()
}

/// Nobody moves on to a ChatGPT login without the owner's click: with a full device login, Auto
/// doesn't land there, for a new conversation or one already on the device login, and no words reach
/// OpenAI.
#[test]
fn chatgpt_talk_a_full_device_login_doesnt_land_on_chatgpt() {
    let world = talk_world("chatgpt-talk-full", false, false);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    assert_eq!(
        talk_next(&world),
        "device",
        "it sorts first only by chance otherwise"
    );
    let first = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "REMEMBER", "new": true})),
    );
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    assert_eq!(posts(&world), 0, "{}", first.text);
    // Its limit: it's full, and ChatGPT is what's left.
    let until = obi_protocol::time::now() + 3600;
    std::fs::write(
        world.obid.home.join("usage.json"),
        json!({"device": {"fullUntil": until}}).to_string(),
    )
    .unwrap();
    let world = restarted(world);
    assert_ne!(talk_next(&world), json!(id), "a new conversation");
    let next = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "conversation": conversation})),
    );
    assert_eq!(next.status, 200, "{}", next.text);
    assert_eq!(posts(&world), 0, "{}", next.text);
    assert!(!next.text.contains("From the stand-in."), "{}", next.text);
}

/// [`World`] again on the same home, as a restart would.
fn restarted(world: World) -> World {
    let World {
        _folder,
        openai,
        mut obid,
    } = world;
    assert!(obid.stop().success());
    let config: Value =
        serde_json::from_slice(&std::fs::read(_folder.0.join("config.json")).unwrap()).unwrap();
    let obid = start_with(
        &_folder.0,
        &config,
        None,
        &[("OBID_TEST_CHATGPT_ORIGIN", &openai.origin)],
    );
    World {
        _folder,
        openai,
        obid,
    }
}

/// Auto chooses ChatGPT only on a Mac with nothing else: not because it sorts first.
#[test]
fn chatgpt_talk_auto_is_chatgpt_only_with_nothing_else() {
    // Codex is the tool Auto looks at first, and the ChatGPT account has more room than the other.
    let world = talk_world("chatgpt-talk-auto", false, true);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let removed = call(&world.obid, "DELETE", "/v1/accounts/device", None);
    assert!(removed.status < 300, "{}", removed.text);
    assert_eq!(talk_next(&world), "stub", "Claude Code is there");
    // Nothing else signed in.
    let removed = call(&world.obid, "DELETE", "/v1/accounts/stub", None);
    assert!(removed.status < 300, "{}", removed.text);
    assert_eq!(talk_next(&world), json!(id));
}

/// A conversation the owner put on ChatGPT stays there: at its limit it says so, and nothing moves to
/// another tool by itself, on the turn or on the next.
#[test]
fn chatgpt_talk_a_chatgpt_limit_moves_nothing_to_another_account() {
    let world = talk_world("chatgpt-talk-limit", false, false);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    let put = call(
        &world.obid,
        "PUT",
        "/v1/settings/jobs",
        Some(&json!({"talkAccount": id})),
    );
    assert_eq!(put.status, 200, "{}", put.text);
    assert_eq!(talk_next(&world), json!(id));
    let first = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "new": true})),
    );
    assert!(first.text.contains("From the stand-in."), "{}", first.text);
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    world.openai.with(|s| {
        s.responses.push_back(Reply::Json(
            429,
            json!({"error": {"code": "subscription_sharing_usage_limit_exceeded", "message": "Over."}}),
        ))
    });
    let limit = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "conversation": conversation})),
    );
    assert!(limit.text.contains("Usage limit reached"), "{}", limit.text);
    assert!(
        !limit.text.contains("resume="),
        "not on the device login: {}",
        limit.text
    );
    let turns = world
        .obid
        .home
        .join("conversations")
        .join(&conversation)
        .join("config/turns.jsonl");
    let before = std::fs::read_to_string(&turns)
        .unwrap_or_default()
        .lines()
        .count();
    // The next turn, held: no move, and no time it can't know.
    let next = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "REMEMBER", "conversation": conversation})),
    );
    assert!(next.text.contains("Usage limit reached"), "{}", next.text);
    assert!(
        !next.text.contains("until"),
        "no inferred time: {}",
        next.text
    );
    assert_eq!(
        std::fs::read_to_string(&turns)
            .unwrap_or_default()
            .lines()
            .count(),
        before
    );
    // The owner's click moves it.
    let moved = call(
        &world.obid,
        "PATCH",
        &format!("/v1/conversations/{conversation}"),
        Some(&json!({"talkAccount": "device"})),
    );
    assert_eq!(moved.status, 200, "{}", moved.text);
    let there = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "REMEMBER", "conversation": conversation})),
    );
    assert!(there.text.contains("resume="), "{}", there.text);
}

/// A conversation on ChatGPT under Auto (it was all there was) stays on it when something else comes:
/// that's no click either.
#[test]
fn chatgpt_talk_a_conversation_on_chatgpt_stays_when_another_account_comes() {
    let folder = Folder::new("chatgpt-talk-stays");
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
    let first = within(10, "the account is free", || {
        let first = call(
            &world.obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": "RELAY", "new": true})),
        );
        first.text.contains("From the stand-in.").then_some(first)
    });
    let conversation = first.body["conversation"].as_str().unwrap().to_owned();
    // Claude Code's stand-in signs in: Auto would look at it first for a new conversation.
    let stub = world._folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&stub).unwrap();
    std::fs::write(stub.join(".credentials.json"), "{}").unwrap();
    assert_eq!(talk_next(&world), "stub");
    let before = posts(&world);
    let next = call(
        &world.obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "RELAY", "conversation": conversation})),
    );
    assert!(next.text.contains("From the stand-in."), "{}", next.text);
    assert_eq!(posts(&world), before + 1, "still on ChatGPT");
}

/// CHATGPT-13: a job a ChatGPT login's limit stopped, with no other account with room, waits by
/// itself with no card (LIMITS-15), and nothing goes to OpenAI while the login is held.
#[test]
fn chatgpt_13_a_job_waits_with_no_card() {
    let folder = Folder::new("chatgpt-13-no-wait");
    let openai = OpenAi::start();
    let mut config = configured(&folder.0);
    let device = folder.0.join("home/accounts/device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(
        device.join("auth.json"),
        json!({"OPENAI_API_KEY": "fake"}).to_string(),
    )
    .unwrap();
    config["agents"]["codex"]["accounts"] =
        json!([{"id": "device", "name": "Device", "configDir": device}]);
    let now = obi_protocol::time::now();
    std::fs::write(
        folder.0.join("home/usage.json"),
        json!({"device": {"fullUntil": now + 3600}}).to_string(),
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
    signed_in(&world, "device");
    let job = job(&world, "device", "RELAY");
    let waits = within(60, "the job waits", || {
        let found = call(&world.obid, "GET", &format!("/v1/jobs/{job}"), None).body;
        (found["status"] == "queued" && !found["resumesAt"].is_null()).then_some(found)
    });
    assert!(waits["accountLimit"].is_null(), "{waits}");
    let decisions = call(&world.obid, "GET", "/v1/limit-decisions", None).body;
    assert_eq!(decisions, json!([]), "no card (LIMITS-15)");
    assert_eq!(posts(&world), 0, "held, so nothing went to OpenAI");
}

/// A conversation on a ChatGPT login, nothing else signed in.
fn talk_on_chatgpt(name: &str) -> (World, String) {
    let folder = Folder::new(name);
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
    (world, id)
}

fn talk(world: &World, said: &str) -> String {
    within(15, "a turn", || {
        let answer = call(
            &world.obid,
            "POST",
            "/v1/plans",
            Some(&json!({"said": said, "new": true})),
        );
        (answer.status == 200).then_some(answer.text)
    })
}

/// On a ChatGPT login only the relay's refusal says it's a usage limit (CHATGPT-13). A turn that
/// failed for another reason holds nothing, and the next one goes to OpenAI.
fn a_failed_turn_holds_nothing(name: &str, code: &str, words: &str, said: &str) {
    let (world, id) = talk_on_chatgpt(name);
    for _ in 0..6 {
        world
            .openai
            .with(|s| s.responses.push_back(Reply::Sse(failed(code, words))));
    }
    let first = talk(&world, said);
    assert!(!first.contains("usage limit"), "{first}");
    let account = account(&world.obid, &id).unwrap();
    assert!(account["fullUntil"].is_null(), "{account}");
    let before = posts(&world);
    let next = talk(&world, "RELAY");
    assert!(next.contains("From the stand-in."), "{next}");
    assert!(posts(&world) > before);
}

/// OpenAI's per-minute rate limit, in its own words, is no usage limit.
#[test]
fn chatgpt_13_a_rate_limit_in_a_stream_doesnt_hold_a_conversation() {
    a_failed_turn_holds_nothing(
        "chatgpt-13-rate-limit",
        "rate_limit_exceeded",
        "Rate limit reached for gpt-x on tokens per min. Please try again in 1.2s.",
        "RELAY",
    );
}

/// The model's words on a failed turn are the model's: they can say anything about limits.
#[test]
fn chatgpt_13_a_models_words_on_a_failed_turn_dont_hold_a_conversation() {
    a_failed_turn_holds_nothing(
        "chatgpt-13-words",
        "server_error",
        "The server had an error.",
        "RELAY SAID The rate limit handling looks fine so far.",
    );
}

/// Nor can they name how long a hold lasts.
#[test]
fn chatgpt_13_a_models_words_dont_name_a_holds_end() {
    a_failed_turn_holds_nothing(
        "chatgpt-13-words-end",
        "server_error",
        "The server had an error.",
        "RELAY SAID usage limit reached|4102444799",
    );
}

/// CHATGPT-14: a stream with no end gets the job's bounded tries (RECOVERY-1), and then the job
/// says what happened: never the model's words, whatever they say, and no hold on the account.
#[test]
fn chatgpt_14_a_cut_stream_gets_the_jobs_bounded_tries() {
    let created = || vec![json!({"type": "response.created", "response": {"id": "r"}})];
    let unterminated = Reply::Sse(created());
    let cut = Reply::Cut(created());
    for (name, task, stream) in [
        (
            "chatgpt-14-cut-words",
            "RELAY\nSAID: The rate limit handling looks fine so far.",
            &unterminated,
        ),
        (
            "chatgpt-14-cut-commentary",
            "RELAY\nSAID: Looking at the code now.",
            &unterminated,
        ),
        ("chatgpt-14-cut-silent", "RELAY", &unterminated),
        ("chatgpt-14-cut-dropped", "RELAY", &cut),
    ] {
        let folder = Folder::new(name);
        let openai = OpenAi::start();
        let mut config = configured(&folder.0);
        // (recovery's waits are at most this long)
        config["stallAfter"] = json!(2);
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
        for _ in 0..30 {
            world.openai.with(|s| s.responses.push_back(stream.clone()));
        }
        let job = job(&world, &id, task);
        within(30, "it waits to try again", || {
            let j = call(&world.obid, "GET", &format!("/v1/jobs/{job}"), None).body;
            j["step"]
                .as_str()
                .is_some_and(|s| s.starts_with("Trying again in"))
                .then_some(())
        });
        let done = ended(&world, &job);
        assert_eq!(done["status"], "interrupted", "{name}: {done}");
        assert_eq!(posts(&world), 24, "{name}: four runs of Codex's six");
        let reason = done["reason"].as_str().unwrap_or("");
        assert!(reason.contains("part way"), "{name}: {reason}");
        assert!(!reason.contains("rate limit"), "{name}: {reason}");
        assert!(!reason.contains("Looking at"), "{name}: {reason}");
        let account = account(&world.obid, &id).unwrap();
        assert!(account["fullUntil"].is_null(), "{name}: {account}");
    }
}

/// Auto lands on a ChatGPT login only with nothing else on the Mac: a device login whose token is
/// ending ("Run codex once") is still there, and it's the owner's to mend.
#[test]
fn chatgpt_talk_a_device_login_whose_token_is_ending_keeps_auto_off_chatgpt() {
    let world = talk_world("chatgpt-talk-ending", false, false);
    let id = new_account(&world.obid);
    signed_in(&world, &id);
    use base64::Engine;
    let expired = format!(
        "x.{}.y",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({"exp": 1000}).to_string())
    );
    std::fs::write(
        world.obid.home.join("accounts/device/auth.json"),
        json!({"tokens": {"access_token": expired, "refresh_token": "r"}}).to_string(),
    )
    .unwrap();
    assert_ne!(account(&world.obid, "device").unwrap()["signedIn"], true);
    assert_ne!(talk_next(&world), json!(id));
    talk(&world, "RELAY");
    assert_eq!(posts(&world), 0, "no words reached OpenAI");
}

/// CHATGPT-8: a 401 on a token another run has already replaced says nothing of the login, however
/// far the new one is from its end: the request goes again on the new one.
#[test]
fn chatgpt_8_a_401_on_a_token_already_replaced_goes_on_with_the_new_one() {
    let world = world("chatgpt-8-replaced");
    let id = new_account(&world.obid);
    world.openai.with(|s| s.expires_in = 400);
    signed_in(&world, &id);
    let (_, base_a, broker_a) = held_job(&world, &id);
    let (_, base_b, broker_b) = held_job(&world, &id);
    world.openai.with(|s| {
        let access = s
            .tokens
            .iter()
            .find(|t| t.contains("access"))
            .cloned()
            .unwrap();
        s.refused.insert(access);
        s.expires_in = 3600;
        s.response_holds.push_back(Duration::from_millis(1500));
        s.response_holds.push_back(Duration::ZERO);
    });
    let slow = std::thread::spawn(move || post_as_codex(&base_b, &broker_b));
    std::thread::sleep(Duration::from_millis(300));
    let (status, _, text) = post_as_codex(&base_a, &broker_a);
    assert_eq!(status, 200, "{text}");
    let (status, _, text) = slow.join().unwrap();
    assert_eq!(status, 200, "the other run goes on: {text}");
    assert_eq!(world.openai.with(|s| s.refreshes.len()), 1);
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
        /// A stream the connection drops under after these events.
        Cut(Vec<Value>),
        /// A page that isn't JSON.
        Text(u16, String),
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
        /// How long the next `/responses` takes before it looks at anything.
        pub response_holds: VecDeque<Duration>,
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
        /// Saw: each `/v1/responses` body.
        pub bodies: Vec<String>,
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
                response_holds: VecDeque::new(),
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
                bodies: Vec::new(),
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

    async fn responses(State(shared): State<Shared>, headers: HeaderMap, body: String) -> Response {
        let hold = shared
            .script
            .lock()
            .unwrap()
            .response_holds
            .pop_front()
            .unwrap_or_default();
        tokio::time::sleep(hold).await;
        let mut script = shared.script.lock().unwrap();
        script.bodies.push(body);
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
            Some(Reply::Cut(events)) => {
                use futures_util::StreamExt;
                // (the answer's start goes out first: an error at once would be no answer at all)
                let pieces = futures_util::stream::iter([Ok(sse_text(&events))]).chain(
                    futures_util::stream::once(async {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        Err(std::io::Error::other("the connection dropped"))
                    }),
                );
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/event-stream")],
                    axum::body::Body::from_stream(pieces),
                )
                    .into_response()
            }
            Some(Reply::Text(status, text)) => (
                StatusCode::from_u16(status).unwrap(),
                [(header::CONTENT_TYPE, "text/html")],
                text,
            )
                .into_response(),
            None => sse(&said("From the stand-in.")),
        }
    }

    fn sse_text(events: &[Value]) -> String {
        events
            .iter()
            .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap_or("")))
            .collect()
    }

    fn sse(events: &[Value]) -> Response {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/event-stream")],
            sse_text(events),
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

/// What every alert builder of crate::push is handed, and where each puts it: a text for each row.
fn alert_rows(text: &str) -> Vec<(&'static str, obid::job::Job, Option<Value>)> {
    use obid::job::{Job, Status};
    let blank = |id: &str| {
        let mut job = Job::new(
            id.into(),
            "deskree-inc/oby".into(),
            "fix".into(),
            "Fix the push".into(),
            "codex".into(),
            "plan".into(),
        );
        job.title = Some("Android push".into());
        job
    };
    let ended = |id: &str, status: Status| {
        let mut job = blank(id);
        job.status = status;
        job.finished = Some(100);
        job
    };
    let asks = |id: &str, title: &str, detail: &str| {
        let mut job = blank(id);
        job.status = Status::Running;
        job.permission = Some(obid::permissions::Permission {
            id: format!("{id}-p"),
            tool: "Bash".into(),
            title: title.into(),
            detail: detail.into(),
            rule: None,
            always: None,
            why: None,
            question: None,
        });
        job
    };
    let mut rows = Vec::new();
    type Made = (Job, Option<Value>);
    let mut row = |what: &'static str, make: &dyn Fn(&str, &str) -> Made| {
        let (job, item) = make(&format!("{what}-{}", uuid::Uuid::new_v4().simple()), text);
        rows.push((what, job, item));
    };
    row("failed-title", &|id, text| {
        let mut job = ended(id, Status::Failed);
        job.title = Some(text.into());
        (job, None)
    });
    row("failed-request", &|id, text| {
        let mut job = ended(id, Status::Failed);
        job.title = None;
        job.task = text.into();
        (job, None)
    });
    row("failed-reason", &|id, text| {
        let mut job = ended(id, Status::Failed);
        job.reason = Some(text.into());
        (job, None)
    });
    row("failed-summary", &|id, text| {
        let mut job = ended(id, Status::Failed);
        job.summary = text.into();
        (job, None)
    });
    row("interrupted-reason", &|id, text| {
        let mut job = ended(id, Status::Interrupted);
        job.reason = Some(text.into());
        (job, None)
    });
    row("ready-publish-error", &|id, text| {
        let mut job = ended(id, Status::Done);
        job.base = Some("dev".into());
        job.commits = Some(1);
        job.publish_error = None;
        job.pushed = Some(0);
        job.checked_head = Some("abc".into());
        job.summary = text.into();
        (job, None)
    });
    row("permission-title", &|id, text| {
        (asks(id, text, "swift test"), None)
    });
    row("permission-detail", &|id, text| {
        (asks(id, "Run swift test", text), None)
    });
    row("review-error", &|id, text| {
        let mut job = ended(id, Status::Done);
        job.published = Some("https://github.com/deskree-inc/oby/pull/7".into());
        job.branch = "oby/fix".into();
        job.outgoing.push(obid::publish::Outgoing::new(
            "push",
            &job.repo,
            "abc",
            Some("oby/fix"),
        ));
        job.review_error = Some(text.into());
        job.review_failed = Some(obid::job::ReviewFailed {
            commit: "abc".into(),
            reviews: vec![],
            needs_you: true,
        });
        (job, None)
    });
    row("inbox-title", &|id, text| {
        let item = json!({"id": id, "kind": "confirm", "job": "", "state": "open", "title": text,
                          "text": "Merge it?", "conversation": "conv"});
        (blank(id), Some(item))
    });
    row("inbox-text", &|id, text| {
        let item = json!({"id": id, "kind": "watch", "job": "", "state": "open", "title": "Listen",
                          "text": text, "triage": "needs_you", "conversation": null});
        (blank(id), Some(item))
    });
    rows
}

/// Whether `said` holds any 16 characters running of `form`, as they were written or in any case.
fn holds_piece(said: &str, form: &str) -> bool {
    let (said, form) = (said.to_ascii_lowercase(), form.to_ascii_lowercase());
    form.as_bytes()
        .windows(16)
        .filter_map(|piece| std::str::from_utf8(piece).ok())
        .any(|piece| said.contains(piece))
}

/// `token` in each form a redaction takes out (crate::secrets), the name of each, and base64 at
/// each offset into a longer run.
fn forms_of(token: &str) -> Vec<(&'static str, String)> {
    use base64::Engine as _;
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
    let wrapped = STANDARD
        .encode(token)
        .as_bytes()
        .chunks(76)
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    vec![
        ("plain", token.to_owned()),
        ("base64", STANDARD.encode(token)),
        ("base64url", URL_SAFE_NO_PAD.encode(token)),
        ("base64 from 1", STANDARD.encode(format!("x{token}"))),
        ("base64 from 2", STANDARD.encode(format!("xy{token}"))),
        ("hex", hex::encode(token)),
        ("hex, upper", hex::encode_upper(token)),
        ("url", token.replace('/', "%2F")),
        ("base64, wrapped", wrapped),
    ]
}

/// CHATGPT-17 (alerts; docs/contracts/chatgpt-sign-in.md): a plan token in anything an alert is built
/// from, plain or encoded, whole or longer than the alert's cut, never reaches a phone, not a piece
/// of it, from any of crate::push's builders: and the alert still goes, saying it was taken out.
/// In this process rather than on the binary: every builder's input is the test's, and only this
/// test here installs a vault. Stand-in tokens in a scratch vault; no token is real.
#[tokio::test(flavor = "multi_thread")]
async fn chatgpt_17_no_token_in_an_alert() {
    use hpke::{Deserializable as _, Serializable as _};
    type Kem = hpke::kem::X25519HkdfSha256;
    type Kdf = hpke::kdf::HkdfSha256;
    type Aead = hpke::aead::ChaCha20Poly1305;

    let folder = Folder::new("chatgpt-17-alerts");
    let home = folder.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    // A short token an alert has room for whole, and long ones it cuts.
    let piece = || uuid::Uuid::new_v4().simple().to_string();
    let long = || (0..20).map(|_| piece()).collect::<Vec<_>>().join("/");
    let (access, refresh, old) = (piece(), long(), long());
    let record = obid::chatgpt::Record {
        client_id: "stand-in".into(),
        sub: "stand-in".into(),
        host_id: "stand-in".into(),
        email: None,
        tokens: Some(obid::chatgpt::Tokens {
            access: access.clone(),
            refresh: refresh.clone(),
            scopes: vec![],
            expires_at: 0,
            earliest_refresh_at: None,
            refreshed_at: 0,
        }),
        previous: vec![obid::chatgpt::OldToken {
            access: old.clone(),
            expires_at: 0,
        }],
    };
    let vault = obid::vault::install(&home, &obid::vault::Location::default_for(&home));
    vault
        .add(
            &serde_json::to_string(&record).unwrap(),
            "oby.chatgpt.stand-in",
        )
        .unwrap();

    // A relay of the test's own, and an iPhone that opens what it's sent.
    let heard: std::sync::Arc<std::sync::Mutex<Vec<Value>>> = Default::default();
    let kept = heard.clone();
    let app = axum::Router::new().route(
        "/v1/push",
        axum::routing::post(move |body: axum::body::Bytes| {
            let kept = kept.clone();
            async move {
                kept.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap());
                axum::http::StatusCode::ACCEPTED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let (secret, public) = <Kem as hpke::Kem>::gen_keypair();
    let push = obid::push::Push::open(&home, &relay);
    push.keep_relayed(obid::push::Relayed {
        push_id: "phone".into(),
        push_key: b64.encode([7_u8; 32]),
        seal_key: b64.encode(public.to_bytes()),
        relay: relay.clone(),
        device: None,
    })
    .unwrap();
    let opened = |given: &Value| -> Value {
        let raw = b64.decode(given["sealed"].as_str().unwrap()).unwrap();
        let enc = <Kem as hpke::Kem>::EncappedKey::from_bytes(&raw[..32]).unwrap();
        let plain = hpke::single_shot_open::<Aead, Kdf, Kem>(
            &hpke::OpModeR::Base,
            &secret,
            &enc,
            obid::push::INFO,
            &raw[32..],
            b"",
        )
        .unwrap();
        let mut alert: Value = serde_json::from_slice(&plain).unwrap();
        alert["collapse"] = given["collapse"].clone();
        alert
    };
    let sent = |heard: &std::sync::Mutex<Vec<Value>>| -> Vec<Value> {
        heard
            .lock()
            .unwrap()
            .drain(..)
            .map(|g| opened(&g))
            .collect()
    };
    let scratch = home.join("tmp");
    let paired = std::collections::HashSet::new();
    // The first look isn't news.
    push.look(&[], &[], &paired, &scratch).await;

    let tokens = [("access", &access), ("refresh", &refresh), ("old", &old)];
    let leaked = |said: &str| {
        tokens.iter().any(|(_, token)| {
            forms_of(token)
                .iter()
                .any(|(_, form)| form.split('\n').any(|line| holds_piece(said, line)))
        })
    };
    let (mut leaks, mut lost, mut unmarked) = (Vec::new(), Vec::new(), Vec::new());
    for (name, token) in tokens {
        // The encodings on two tokens, and a replaced one plain: each alert is a run of gitleaks.
        let forms = forms_of(token);
        for (form, text) in forms
            .into_iter()
            .filter(|(f, _)| name != "old" || *f == "plain")
        {
            // Base64 wrapped over lines holds nothing else on them, as `base64` writes it.
            let text = match form.contains("wrapped") {
                true => text,
                false => format!("Use {text} now"),
            };
            let rows = alert_rows(&text);
            let jobs: Vec<_> = rows
                .iter()
                .filter(|(_, _, item)| item.is_none())
                .map(|(_, job, _)| job.clone())
                .collect();
            let inbox: Vec<Value> = rows.iter().filter_map(|(_, _, i)| i.clone()).collect();
            push.look(&jobs, &inbox, &paired, &scratch).await;
            let alerts = sent(&heard);
            for (what, job, item) in &rows {
                let id = item
                    .as_ref()
                    .map_or(job.id.as_str(), |i| i["id"].as_str().unwrap());
                let place = format!("{name} {form} in {what}");
                let Some(alert) = alerts.iter().find(|a| a["job"] == id || a["item"] == id) else {
                    lost.push(place);
                    continue;
                };
                let said = alert.to_string();
                if leaked(&said) {
                    leaks.push(place);
                } else if !said.contains("[vault: oby.chatgpt.") {
                    unmarked.push(place);
                }
            }
            // FCM's address is Google's, so what it says is read here: `alert` on what `look`
            // hands its FCM part.
            let (clean, _) = obid::push::for_alerts(&jobs, &inbox).unwrap();
            for job in &clean {
                let Some(notice) = obid::push::notice(job) else {
                    continue;
                };
                let (title, body, _) = obid::push::alert(job, notice);
                let said = format!("{title} {body}");
                let place = format!("{name} {form} in FCM's {}", job.id);
                if leaked(&said) {
                    leaks.push(place);
                } else if !said.contains("[vault: oby.chatgpt.") {
                    unmarked.push(place);
                }
            }
        }
    }
    assert!(
        lost.is_empty() && leaks.is_empty() && unmarked.is_empty(),
        "lost {}: {lost:#?}\nleaked {}: {leaks:#?}\nunmarked {}: {unmarked:#?}",
        lost.len(),
        leaks.len(),
        unmarked.len()
    );

    // Every ask is gone: each says "Settled" with the title it had, and what's kept holds none.
    push.look(&[], &[], &paired, &scratch).await;
    let settled = sent(&heard);
    assert!(settled.iter().any(|a| a["category"] == "settled"));
    for alert in &settled {
        assert!(!leaked(&alert.to_string()), "{alert}");
    }
    let kept = std::fs::read_to_string(home.join("push/said.json")).unwrap();
    assert!(!leaked(&kept), "{kept}");

    // A vault that can't be read says nothing and forgets nothing: the next look says it.
    use std::os::unix::fs::PermissionsExt;
    let file = home.join("vault/secrets.json");
    let mode = |bits| std::fs::set_permissions(&file, std::fs::Permissions::from_mode(bits));
    let (_, job, _) = alert_rows(&format!("Use {refresh} now")).swap_remove(0);
    mode(0o644).unwrap();
    assert!(obid::push::for_alerts(&[], &[]).is_err());
    push.look(std::slice::from_ref(&job), &[], &paired, &scratch)
        .await;
    assert_eq!(sent(&heard), Vec::<Value>::new());
    mode(0o600).unwrap();
    push.look(std::slice::from_ref(&job), &[], &paired, &scratch)
        .await;
    let again = sent(&heard);
    assert_eq!(again.len(), 1, "{again:?}");
    assert_eq!(again[0]["job"], job.id.as_str());
    assert!(!leaked(&again[0].to_string()), "{}", again[0]);
}
