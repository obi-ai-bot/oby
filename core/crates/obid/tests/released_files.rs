//! A job file and a `config.json` each released obid wrote still load in today's
//! (docs/contracts/released-files.md, RELEASED-1 to 3): `Store::load_all` skips a job file that
//! doesn't read with one warning, and the job is gone from `GET /v1/jobs`. The files are
//! tests/fixtures/released/<tag>/ (its README says where each came from); the real obid binary
//! starts on each tag's, with only the configuration's machine-specific keys made the test's.
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use support::*;

/// Every released tag's folder, oldest first.
fn tags() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/released");
    let mut tags: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    tags.sort();
    assert!(!tags.is_empty(), "no released tags in {}", root.display());
    tags
}

/// A tag's name, as its folder's.
fn tag_name(tag: &Path) -> String {
    tag.file_name().unwrap().to_string_lossy().into_owned()
}

/// The tag's job files, by file name, as released.
fn released_jobs(tag: &Path) -> Vec<(String, Value)> {
    let mut jobs: Vec<(String, Value)> = std::fs::read_dir(tag.join("jobs"))
        .unwrap()
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let job = serde_json::from_slice(&std::fs::read(e.path()).unwrap()).unwrap();
            (name, job)
        })
        .collect();
    jobs.sort_by(|a, b| a.0.cmp(&b.0));
    jobs
}

/// The tag's `config.json` as released.
fn released_config(tag: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(tag.join("config.json")).unwrap()).unwrap()
}

/// The released configuration with only what is this machine's made the test's: `home`,
/// `listen`, each agent's `bin`, each account's `configDir` (a folder of the test's own), and each
/// repository's `remote` (a bare repository of the test's own, with `allowFileRemotes`). Every
/// other key stays as released.
fn on_this_machine(released: &Value, folder: &Path) -> Value {
    let mut config = released.clone();
    config["home"] = json!(folder.join("home"));
    config["listen"] = json!("127.0.0.1:0");
    for (tool, agent) in config["agents"].as_object_mut().unwrap() {
        agent["bin"] = json!(stub_agent());
        for account in agent["accounts"].as_array_mut().unwrap() {
            let id = account["id"].as_str().unwrap().to_owned();
            let dir = folder.join(format!("accounts/{tool}-{id}"));
            // Signed in, as on the Mac that wrote the files: a stand-in Claude Code login, so its
            // usage, not a missing login, decides whether the waiting job starts.
            if tool == "claude" {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join(".credentials.json"), "{}").unwrap();
            }
            account["configDir"] = json!(dir);
        }
    }
    for (n, repo) in config["repos"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let remote = folder.join(format!("remotes/{n}.git"));
        let made = Command::new("git")
            .args(["init", "--quiet", "--bare"])
            .arg(&remote)
            .status()
            .unwrap();
        assert!(made.success());
        repo["remote"] = json!(format!("file://{}", remote.display()));
    }
    config["allowFileRemotes"] = json!(true);
    config
}

/// obid started on `tag`'s released files, in a folder of `test`'s own: its jobs, and the
/// accounts' usage that holds its waiting job (`usage.json`).
fn start_on(test: &str, tag: &Path) -> (Folder, Obid) {
    let folder = Folder::new(&format!("released-{test}-{}", tag_name(tag)));
    let jobs = folder.0.join("home/jobs");
    std::fs::create_dir_all(&jobs).unwrap();
    for (name, _) in released_jobs(tag) {
        std::fs::copy(tag.join("jobs").join(&name), jobs.join(&name)).unwrap();
    }
    std::fs::copy(tag.join("usage.json"), folder.0.join("home/usage.json")).unwrap();
    let config = on_this_machine(&released_config(tag), &folder.0);
    let obid = start(&folder.0, &config, None);
    (folder, obid)
}

/// What `GET /v1/jobs` lists.
fn listed(obid: &Obid) -> Vec<Value> {
    let answer = call(obid, "GET", "/v1/jobs?limit=1000", None);
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body.as_array().unwrap().clone()
}

/// RELEASED-1: every job file a released obid wrote is listed, with the status it was saved in,
/// still once obid has looked at its waiting jobs again (every 15 seconds, `RESET_TICK`).
#[test]
fn released_1_every_released_job_is_listed() {
    let started: Vec<(PathBuf, Folder, Obid)> = tags()
        .into_iter()
        .map(|tag| {
            let (folder, obid) = start_on("listed", &tag);
            (tag, folder, obid)
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_secs(17));
    for (tag, _folder, obid) in &started {
        let list = listed(obid);
        for (file, job) in released_jobs(tag) {
            let id = job["id"].as_str().unwrap();
            let found = list.iter().find(|j| j["id"] == id).unwrap_or_else(|| {
                panic!(
                    "{}: jobs/{file} isn't listed by GET /v1/jobs: today's obid doesn't read it\n{}",
                    tag_name(tag),
                    obid.log()
                )
            });
            assert_eq!(
                found["status"],
                job["status"],
                "{}: jobs/{file}'s status changed",
                tag_name(tag)
            );
        }
    }
}

/// RELEASED-2: the done job keeps its branch, its pull request's number and its URL.
#[test]
fn released_2_branch_and_pr_are_kept() {
    for tag in tags() {
        let (_folder, obid) = start_on("kept", &tag);
        let list = listed(&obid);
        let (file, done) = released_jobs(&tag)
            .into_iter()
            .find(|(_, j)| j["status"] == "done")
            .unwrap_or_else(|| panic!("{}: no done job", tag_name(&tag)));
        let found = list
            .iter()
            .find(|j| j["id"] == done["id"])
            .unwrap_or_else(|| {
                panic!(
                    "{}: jobs/{file} isn't listed by GET /v1/jobs\n{}",
                    tag_name(&tag),
                    obid.log()
                )
            });
        for key in ["branch", "pr", "published"] {
            assert!(
                !done[key].is_null(),
                "{}: jobs/{file} has no {key}",
                tag_name(&tag)
            );
            assert_eq!(
                found[key],
                done[key],
                "{}: jobs/{file}'s {key} changed",
                tag_name(&tag)
            );
        }
    }
}

/// RELEASED-3: obid starts on a released `config.json`, with its repositories and its agents'
/// accounts.
#[test]
fn released_3_released_config_starts() {
    for tag in tags() {
        let (_folder, obid) = start_on("config", &tag);
        let released = released_config(&tag);
        let repos = call(&obid, "GET", "/v1/repos", None);
        assert_eq!(repos.status, 200, "{}", repos.text);
        for repo in released["repos"].as_array().unwrap() {
            assert!(
                repos.body["repos"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["repo"] == repo["repo"]),
                "{}: repository {} isn't listed: {}",
                tag_name(&tag),
                repo["repo"],
                repos.text
            );
        }
        // The API's accounts are `GET /v1/status`'s (there's no `GET /v1/accounts`).
        let status = call(&obid, "GET", "/v1/status", None);
        assert_eq!(status.status, 200, "{}", status.text);
        for (tool, agent) in released["agents"].as_object().unwrap() {
            for account in agent["accounts"].as_array().unwrap() {
                assert!(
                    status.body["accounts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a["id"] == account["id"] && a["agent"] == tool.as_str()),
                    "{}: {tool}'s account {} isn't listed: {}",
                    tag_name(&tag),
                    account["id"],
                    status.body["accounts"]
                );
            }
        }
    }
}
