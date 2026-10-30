//! The GitHub login from the Mac (docs/runner-api.md, "obid only: GitHub's sign-in"), on the real
//! obid binary with a stand-in gh: `GET /v1/github` says whether gh is signed in and as whom;
//! `POST /v1/github/sign-in` runs gh's own device sign-in and passes on only its code and GitHub's
//! device page; gh never gets to change the person's own git settings. The reviewer, a second
//! account, signs in the same way to obid's own settings folder, never as the owner's account.

mod support;

use std::path::Path;

use serde_json::json;
use support::*;

/// A stand-in gh: signed in once `$HOME/signed` is there (its status said 2 seconds late while
/// `$HOME/slow-status` is); its sign-in prints a code and waits for
/// `$HOME/approve`, as the real one waits for the person on GitHub's page, and gives up once obid
/// or the test's folder is gone, so a test that ends leaves none behind. With `GH_CONFIG_DIR` (the
/// reviewer's), its sign-in waits for `$HOME/approve-reviewer`, then signs in as the login in
/// `$HOME/reviewer-login` with the token in that folder's `hosts.yml`, as `--insecure-storage` does.
const GH: &str = r#"#!/bin/sh
if [ -n "$GH_CONFIG_DIR" ]; then
  case "$1 $2" in
    "auth status")
      if [ -f "$GH_CONFIG_DIR/hosts.yml" ]; then
        login=$(sed -n 's/^ *user: //p' "$GH_CONFIG_DIR/hosts.yml")
        echo "{\"hosts\":{\"github.com\":[{\"state\":\"success\",\"active\":true,\"host\":\"github.com\",\"login\":\"$login\"}]}}"
      else
        echo '{"hosts":{}}'
      fi ;;
    "auth login")
      printf '%s\n' "$*" > "$HOME/reviewer-login-args"
      printf '%s\n' "$GH_CONFIG_DIR" > "$HOME/reviewer-config-dir"
      printf '%s\n' "$DBUS_SESSION_BUS_ADDRESS" > "$HOME/reviewer-bus"
      echo '! One-time code (RV12-CD34) copied to clipboard' >&2
      while [ ! -f "$HOME/approve-reviewer" ]; do
        kill -0 "$PPID" 2>/dev/null && [ -d "$HOME" ] || exit 1
        sleep 0.1
      done
      printf 'github.com:\n    oauth_token: gho_REVIEWER_NEVER_SHOWN\n    user: %s\n' "$(cat "$HOME/reviewer-login")" > "$GH_CONFIG_DIR/hosts.yml"
      echo 'gho_REVIEWER_NEVER_SHOWN' >&2 ;;
    *) exit 1 ;;
  esac
  exit 0
fi
case "$1 $2" in
  "auth status")
    if [ -f "$HOME/signed" ]; then
      said='{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"octo-cat","tokenSource":"keyring"}]}}'
    else
      said='{"hosts":{}}'
    fi
    if [ -f "$HOME/slow-status" ]; then sleep 2; fi
    echo "$said" ;;
  "auth login")
    echo $$ > "$HOME/gh-pid"
    if [ -f "$HOME/slow" ]; then sleep 3; fi
    printf '%s\n' "$*" > "$HOME/login-args"
    printf '%s\n' "$GIT_CONFIG_GLOBAL" > "$HOME/git-config-global"
    echo '! One-time code (AB12-CD34) copied to clipboard' >&2
    echo 'Open this URL to continue in your web browser: https://github.com/login/device' >&2
    echo 'gho_NEVER_PASSED_ON' >&2
    while [ ! -f "$HOME/approve" ]; do
      kill -0 "$PPID" 2>/dev/null && [ -d "$HOME" ] || exit 1
      sleep 0.1
    done
    touch "$HOME/signed" ;;
  *) exit 1 ;;
esac
"#;

fn stand_in_gh(folder: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("gh"), GH).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[test]
fn signs_in_with_ghs_own_device_sign_in() {
    let folder = Folder::new("github-sign-in");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let home = folder.0.join("env-home");

    let before = call(&obid, "GET", "/v1/github", None);
    assert_eq!(before.status, 200, "{}", before.text);
    assert_eq!(
        before.body,
        json!({"gh": true, "signedIn": false, "user": null, "signIn": null,
               "reviewer": {"signedIn": false, "user": null, "signIn": null, "error": null}})
    );

    let started = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    assert_eq!(started.body["url"], "https://github.com/login/device");
    assert_eq!(started.body["userCode"], "AB12-CD34");
    assert!(started.body["expiresAt"].is_string());
    assert!(!started.text.contains("gho_"), "{}", started.text);
    // Asked again, the same sign-in.
    let again = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(again.body["userCode"], "AB12-CD34");
    let under_way = call(&obid, "GET", "/v1/github", None);
    assert_eq!(under_way.body["signIn"]["userCode"], "AB12-CD34");

    // gh's own login for github.com, with https for git, its page opened by the app.
    let args = std::fs::read_to_string(home.join("login-args")).unwrap();
    assert_eq!(
        args.trim(),
        "auth login --hostname github.com --git-protocol https --web --skip-ssh-key"
    );
    // git's global settings, which gh may write, are a file of the sign-in's own.
    let global = std::fs::read_to_string(home.join("git-config-global")).unwrap();
    assert!(
        global
            .trim()
            .starts_with(&*folder.0.join("home/tmp").to_string_lossy()),
        "{global}"
    );

    std::fs::write(home.join("approve"), "").unwrap();
    let signed = within(20, "gh to say it's signed in", || {
        let now = call(&obid, "GET", "/v1/github", None);
        (now.body["signedIn"] == true).then_some(now.body)
    });
    assert_eq!(
        signed,
        json!({"gh": true, "signedIn": true, "user": "octo-cat", "signIn": null,
               "reviewer": {"signedIn": false, "user": null, "signIn": null, "error": null}})
    );
    assert!(!obid.log().contains("gho_"));
}

#[test]
fn a_sign_in_can_be_ended() {
    let folder = Folder::new("github-sign-in-end");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let started = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    let ended = call(&obid, "DELETE", "/v1/github/sign-in", None);
    assert_eq!(ended.status, 204, "{}", ended.text);
    let after = call(&obid, "GET", "/v1/github", None);
    assert_eq!(after.body["signIn"], json!(null));
    assert_eq!(after.body["signedIn"], false);
}

/// While gh is slow to print its code, obid answers GET /v1/github at once (no lock held while gh
/// starts), a second POST says the sign-in is starting, and the first gets the code.
#[test]
fn a_starting_sign_in_holds_nothing_up() {
    let folder = Folder::new("github-sign-in-slow");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    std::fs::write(folder.0.join("env-home/slow"), "").unwrap();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| call(&obid, "POST", "/v1/github/sign-in", None));
        std::thread::sleep(std::time::Duration::from_millis(800));
        let asked = std::time::Instant::now();
        let status = call(&obid, "GET", "/v1/github", None);
        assert_eq!(status.status, 200, "{}", status.text);
        assert!(
            asked.elapsed() < std::time::Duration::from_secs(2),
            "GET waited for gh"
        );
        assert_eq!(status.body["signIn"], json!(null));
        let second = call(&obid, "POST", "/v1/github/sign-in", None);
        assert_eq!(second.status, 409, "{}", second.text);
        assert!(second.text.contains("starting"), "{}", second.text);
        let first = first.join().unwrap();
        assert_eq!(first.status, 200, "{}", first.text);
        assert_eq!(first.body["userCode"], "AB12-CD34");
    });
}

/// A sign-in started right after the last one ended gets a folder of its own, which the ended
/// one's task never removes.
#[test]
fn back_to_back_sign_ins_keep_their_own_folders() {
    let folder = Folder::new("github-sign-in-again");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let home = folder.0.join("env-home");
    let global = || {
        std::path::PathBuf::from(
            std::fs::read_to_string(home.join("git-config-global"))
                .unwrap()
                .trim(),
        )
    };
    let first = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(first.status, 200, "{}", first.text);
    let first_folder = global().parent().unwrap().to_path_buf();
    let ended = call(&obid, "DELETE", "/v1/github/sign-in", None);
    assert_eq!(ended.status, 204, "{}", ended.text);
    let second = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(second.status, 200, "{}", second.text);
    let second_folder = global().parent().unwrap().to_path_buf();
    assert_ne!(first_folder, second_folder);
    assert!(!first_folder.exists(), "{first_folder:?}");
    // The ended one's task is long done; the new one's folder is still there while it waits.
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(second_folder.is_dir(), "{second_folder:?}");
    assert_eq!(
        call(&obid, "GET", "/v1/github", None).body["signIn"]["userCode"],
        "AB12-CD34"
    );
}

/// A sign-in left waiting when its test ends (obid gone, the approve file never written) ends too:
/// the stand-in gh never outlives its test.
#[test]
fn a_waiting_sign_in_ends_with_its_test() {
    let folder = Folder::new("github-sign-in-left");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let started = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    let pid: i32 = within(10, "the stand-in gh to say its pid", || {
        std::fs::read_to_string(folder.0.join("env-home/gh-pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    });
    let alive = || nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok();
    assert!(alive());
    drop(obid);
    within(10, "the stand-in gh to end", || (!alive()).then_some(()));
}

/// The reviewer signs in through the same device page into obid's own settings folder
/// (`home/github/reviewer`, 0700), with its token kept there and gh away from the keyring; its token
/// is never answered or logged; and `DELETE` signs it out, removing the folder.
#[test]
fn the_reviewer_signs_in_and_out() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Folder::new("github-reviewer");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let home = folder.0.join("env-home");
    std::fs::write(home.join("signed"), "").unwrap();
    std::fs::write(home.join("reviewer-login"), "oby-reviewer").unwrap();

    let started = call(&obid, "POST", "/v1/github/reviewer/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    assert_eq!(started.body["userCode"], "RV12-CD34");
    assert_eq!(started.body["url"], "https://github.com/login/device");
    let under_way = call(&obid, "GET", "/v1/github", None);
    assert_eq!(
        under_way.body["reviewer"]["signIn"]["userCode"],
        "RV12-CD34"
    );
    assert_eq!(under_way.body["signIn"], json!(null));
    let args = std::fs::read_to_string(home.join("reviewer-login-args")).unwrap();
    assert_eq!(
        args.trim(),
        "auth login --hostname github.com --git-protocol https --web --skip-ssh-key --insecure-storage"
    );
    let staging = std::fs::read_to_string(home.join("reviewer-config-dir")).unwrap();
    assert!(
        staging
            .trim()
            .starts_with(&*folder.0.join("home/tmp").to_string_lossy()),
        "{staging}"
    );
    assert_eq!(
        std::fs::read_to_string(home.join("reviewer-bus"))
            .unwrap()
            .trim(),
        "unix:path=/dev/null/no-keyring"
    );

    std::fs::write(home.join("approve-reviewer"), "").unwrap();
    let signed = within(20, "the reviewer to be signed in", || {
        let now = call(&obid, "GET", "/v1/github", None);
        (now.body["reviewer"]["signedIn"] == true).then_some(now)
    });
    assert_eq!(
        signed.body["reviewer"],
        json!({"signedIn": true, "user": "oby-reviewer", "signIn": null, "error": null})
    );
    assert_eq!(signed.body["user"], "octo-cat");
    let kept = folder.0.join("home/github/reviewer");
    assert!(kept.join("hosts.yml").is_file());
    assert_eq!(
        std::fs::metadata(&kept).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(!std::path::Path::new(staging.trim()).exists(), "{staging}");
    assert!(!signed.text.contains("gho_"), "{}", signed.text);
    assert!(!obid.log().contains("gho_REVIEWER"));

    // Its login is kept for good, signed in or out, so its approval never passes for a
    // teammate's (crate::merge).
    let used = folder.0.join("home/github/reviewers.json");
    assert_eq!(
        std::fs::read_to_string(&used).unwrap(),
        r#"["oby-reviewer"]"#
    );

    let out = call(&obid, "DELETE", "/v1/github/reviewer/sign-in", None);
    assert_eq!(out.status, 204, "{}", out.text);
    assert!(!kept.exists());
    assert_eq!(
        std::fs::read_to_string(&used).unwrap(),
        r#"["oby-reviewer"]"#
    );
    assert_eq!(
        call(&obid, "GET", "/v1/github", None).body["reviewer"],
        json!({"signedIn": false, "user": null, "signIn": null, "error": null})
    );
}

/// The reviewer must be another account: no sign-in before the owner's gh is signed in, and one
/// that ends signed in as the owner's own account keeps nothing and says why.
#[test]
fn the_reviewer_is_never_the_owners_own_account() {
    let folder = Folder::new("github-reviewer-same");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let home = folder.0.join("env-home");

    let first = call(&obid, "POST", "/v1/github/reviewer/sign-in", None);
    assert_eq!(first.status, 409, "{}", first.text);
    assert!(
        first.text.contains("Sign in to GitHub first"),
        "{}",
        first.text
    );

    std::fs::write(home.join("signed"), "").unwrap();
    std::fs::write(home.join("reviewer-login"), "Octo-Cat").unwrap();
    let started = call(&obid, "POST", "/v1/github/reviewer/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    std::fs::write(home.join("approve-reviewer"), "").unwrap();
    let refused = within(20, "the reviewer's sign-in to end", || {
        let now = call(&obid, "GET", "/v1/github", None);
        now.body["reviewer"]["error"].as_str().map(str::to_owned)
    });
    assert!(refused.contains("your own GitHub account"), "{refused}");
    let now = call(&obid, "GET", "/v1/github", None);
    assert_eq!(now.body["reviewer"]["signedIn"], false);
    assert_eq!(now.body["reviewer"]["signIn"], json!(null));
    assert!(!folder.0.join("home/github/reviewer").exists());
}

/// A reviewer whose login obid can't record for good isn't signed in: its approval could
/// otherwise pass for a teammate's once it signed out.
#[test]
fn a_reviewer_obid_cant_record_isnt_kept() {
    let folder = Folder::new("github-reviewer-unrecorded");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = start(&folder.0, &config, Some(&bin));
    let home = folder.0.join("env-home");
    std::fs::write(home.join("signed"), "").unwrap();
    std::fs::write(home.join("reviewer-login"), "oby-reviewer").unwrap();
    // A record it can't read.
    std::fs::create_dir_all(folder.0.join("home/github")).unwrap();
    std::fs::write(folder.0.join("home/github/reviewers.json"), "not json").unwrap();

    let started = call(&obid, "POST", "/v1/github/reviewer/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);
    std::fs::write(home.join("approve-reviewer"), "").unwrap();
    let refused = within(20, "the reviewer's sign-in to end", || {
        let now = call(&obid, "GET", "/v1/github", None);
        now.body["reviewer"]["error"].as_str().map(str::to_owned)
    });
    assert!(
        refused.contains("couldn't keep the reviewer's login"),
        "{refused}"
    );
    assert_eq!(
        call(&obid, "GET", "/v1/github", None).body["reviewer"]["signedIn"],
        false
    );
    assert!(!folder.0.join("home/github/reviewer").exists());
}

/// SIGN-IN-13 (docs/contracts/sign-in.md): a status read that began before gh's sign-in ended
/// keeps nothing, so a sign-in that just succeeded is never shown as one that ended without a
/// login (`signedIn` false, `signIn` null).
#[test]
fn sign_in_13_a_finished_sign_in_is_never_read_as_unfinished() {
    let folder = Folder::new("github-status-race");
    let remote = folder.0.join("remote.git");
    let config = config(&folder.0, &remote, &stub_agent());
    let bin = stand_in_gh(&folder.0);
    let obid = std::sync::Arc::new(start(&folder.0, &config, Some(&bin)));
    let home = folder.0.join("env-home");
    std::fs::create_dir_all(&home).unwrap();
    let started = call(&obid, "POST", "/v1/github/sign-in", None);
    assert_eq!(started.status, 200, "{}", started.text);

    // A read that sees gh signed out, and answers only after the sign-in has ended.
    std::fs::write(home.join("slow-status"), "").unwrap();
    let early = {
        let obid = obid.clone();
        std::thread::spawn(move || call(&obid, "GET", "/v1/github", None))
    };
    std::thread::sleep(std::time::Duration::from_millis(500));
    std::fs::remove_file(home.join("slow-status")).unwrap();
    std::fs::write(home.join("approve"), "").unwrap();
    within(10, "gh signed in", || {
        home.join("signed").exists().then_some(())
    });
    let early = early.join().unwrap();
    assert_eq!(early.status, 200, "{}", early.text);

    let after = within(10, "the sign-in to end", || {
        let now = call(&obid, "GET", "/v1/github", None);
        now.body["signIn"].is_null().then_some(now.body)
    });
    assert_eq!(after["signedIn"], true, "{after}");
    assert_eq!(after["user"], "octo-cat", "{after}");
}
