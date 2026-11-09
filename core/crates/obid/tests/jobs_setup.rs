//! A repository's setup in the job's box (docs/contracts/publishing.md, PUBLISHING-23 and 24), the
//! tests of it that make a sandbox of their own: macOS can't make one inside a job's, so
//! scripts/oby-check.sh leaves this binary unjudged there and plans it only when the setup's or the
//! box's code changes (src/jobs/setup.rs, sandbox.rs, egress.rs). Moved from tests/jobs_api.rs as
//! they were, on its helpers' terms: the real obid binary, asked over HTTP, with the stand-in agent
//! (tests/agents/fake-claude) run in the job sandbox.

mod support;

use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::{Answer, Folder, Obid, fixture};

/// jobs_api's configuration: the fixture repository, no check of its own, and the stand-in agent.
fn config(folder: &Path, remote: &Path) -> Value {
    let agent = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-claude");
    let mut config = support::config(folder, remote, &agent);
    config["repos"][0].as_object_mut().unwrap().remove("check");
    config
}

fn start(folder: &Path, config: &Value) -> Obid {
    support::start(folder, config, None)
}

#[cfg(target_os = "macos")]
fn start_with(folder: &Path, config: &Value, path_first: Option<&Path>) -> Obid {
    support::start(folder, config, path_first)
}

fn request(obid: &Obid, method: &str, target: &str, body: Option<&Value>) -> Answer {
    support::call(obid, method, target, body)
}

fn post_job(obid: &Obid, job: Value) -> Answer {
    request(obid, "POST", "/v1/jobs", Some(&json!({"job": job})))
}

fn wait(obid: &Obid, id: &str) -> Value {
    let answer = request(obid, "GET", &format!("/v1/jobs/{id}/wait"), None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// jobs_api's stand-in `gh` in `folder/bin`: it opens pull request 7.
#[cfg(target_os = "macos")]
fn fake_gh(folder: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = folder.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{calls}'\nenv >> '{env}'\ncase \"$1 $2\" in\n  \"pr list\") exit 0 ;;\n  \"pr create\") echo https://github.com/obi-e2e/fixture-app/pull/7 ;;\n  \"pr view\") exec cat '{view}' ;;\n  *) exit 1 ;;\nesac\n",
        calls = folder.join("gh-calls").display(),
        env = folder.join("gh-env").display(),
        view = folder.join("pr-view.json").display()
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// PUBLISHING-23: the setup runs in the job's box, with the box's own home and, where the box
/// confines its network, the agent's way out (obid's proxy).
#[test]
fn publishing_23_setup_runs_in_the_box_with_the_agents_reach() {
    let folder = Folder::new("setup-in-the-box");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote);
    config["repos"][0]["setup"] =
        json!("printf '%s\\n%s\\n' \"$HOME\" \"$HTTPS_PROXY\" > setup-env");
    let obid = start(&folder.0, &config);
    let answer = post_job(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "research", "task": "SEES: setup-env"}),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let job = wait(&obid, answer.body["id"].as_str().unwrap());
    assert_eq!(job["summary"], "sees setup-env: true", "{job}");
    let worktree = PathBuf::from(job["worktree"].as_str().unwrap());
    let seen = std::fs::read_to_string(worktree.join("setup-env")).unwrap();
    let (home, proxy) = seen
        .trim_end()
        .split_once('\n')
        .unwrap_or((seen.trim_end(), ""));
    assert!(!home.is_empty(), "{seen}");
    assert_ne!(Path::new(home), folder.0.join("env-home"), "{seen}");
    // The box's HOME is the job's own temporary folder (its `spec.temp`), never the person's.
    let tmp = obid.home.canonicalize().unwrap().join("tmp");
    assert!(Path::new(home).starts_with(&tmp), "{seen}");
    if cfg!(target_os = "macos") {
        // obid's own proxy on loopback, its login in the URL (`http://oby:<token>@127.0.0.1:…`).
        assert!(
            proxy.starts_with("http://") && proxy.contains("@127.0.0.1:"),
            "{seen}"
        );
    }
}

/// PUBLISHING-24: the setup runs in the check's own checkout before the check: the check needs a
/// file only the setup makes there (it makes none before the agent, which has no hello.txt yet,
/// so nothing the job commits can carry it). macOS only: elsewhere a check runs without it.
#[cfg(target_os = "macos")]
#[test]
fn publishing_24_setup_installs_before_the_check_runs() {
    let folder = Folder::new("setup-before-check");
    let remote = fixture(&folder.0.join("fixture"));
    let bin = fake_gh(&folder.0);
    let mut config = config(&folder.0, &remote);
    config["repos"][0]["setup"] =
        json!("test -f hello.txt && echo installed > installed.txt; true");
    let obid = start_with(&folder.0, &config, Some(&bin));
    let answer = post_job(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "test -f installed.txt && test -f hello.txt"}),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let job = wait(&obid, answer.body["id"].as_str().unwrap());
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["checks"][0]["steps"][0]["passed"], true, "{job}");
    assert_eq!(job["commits"], 1, "{job}");
}

/// PUBLISHING-24: a setup that fails before the check judged nothing: no fix round spent, the job
/// ends done for the owner's Publish by hand, and why says it was the setup.
#[cfg(target_os = "macos")]
#[test]
fn publishing_24_a_setup_that_fails_before_the_check_spends_no_round() {
    let folder = Folder::new("setup-fails-before-check");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote);
    // It passes before the agent, which makes hello.txt, and fails in the check's checkout.
    config["repos"][0]["setup"] = json!("test -f hello.txt && exit 3; true");
    let obid = start(&folder.0, &config);
    let answer = post_job(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "true", "rounds": 3}),
    );
    assert_eq!(answer.status, 201, "{}", answer.text);
    let job = wait(&obid, answer.body["id"].as_str().unwrap());
    assert_eq!(job["status"], "done", "{job}");
    assert_eq!(job["round"], 1, "{job}");
    assert!(
        job["reason"].as_str().is_some_and(|r| r
            .starts_with("Not published: its setup failed: `test -f hello.txt && exit 3; true`")),
        "{job}"
    );
}

/// PUBLISHING-24: a Stop during the setup before the check ends the job stopped, never as a setup
/// that failed: nothing in the check's log, no reason that names it.
#[cfg(target_os = "macos")]
#[test]
fn publishing_24_a_stop_during_the_checks_setup_ends_stopped() {
    let folder = Folder::new("setup-stopped-before-check");
    let remote = fixture(&folder.0.join("fixture"));
    let mut config = config(&folder.0, &remote);
    // Only in the check's checkout, where the agent's hello.txt is.
    config["repos"][0]["setup"] =
        json!("test -f hello.txt || exit 0; touch setup-started; sleep 30");
    let obid = start(&folder.0, &config);
    let id = post_job(
        &obid,
        json!({"repo": "obi-e2e/fixture-app", "kind": "build", "task": "COMMIT: hello.txt",
               "check": "true"}),
    )
    .body["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let started = obid
        .home
        .join(format!("checks/{id}/check-{id}"))
        .join("setup-started");
    let give_up_at = Instant::now() + Duration::from_secs(60);
    while !started.exists() {
        assert!(
            Instant::now() < give_up_at,
            "the check's setup never started"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    request(&obid, "POST", &format!("/v1/jobs/{id}/stop"), None);
    let job = wait(&obid, &id);
    assert_eq!(job["status"], "stopped", "{job}");
    assert!(!job.to_string().contains("setup failed"), "{job}");
    assert!(
        !obid.home.join("logs").join(&id).join("check.log").exists(),
        "{job}"
    );
}
