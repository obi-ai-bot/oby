//! A conversation's workspace (docs/contracts/folders.md, WORKSPACE-2 to 6), at obid's API: "This
//! folder" or "A new worktree", chosen with the first message and kept; the finishing actions obid
//! answers from git alone (no gh call for a folder with no remote), each done on the owner's signed
//! click; a folder that isn't git works in place; factory jobs keep their own worktree. Each test is
//! named after the scenario it proves. WORKSPACE-1 (the sandbox's write list) is proved where the
//! lanes wire it; its pure builder is proved in src/workspace/writes.rs.
mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use obi_protocol::time;
use serde_json::{Value, json};
use support::*;

fn harness_obid(folder: &Folder, path_first: Option<&Path>) -> Obid {
    let remote = fixture(&folder.0.join("fixture"));
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/agents/fake-harness-jobs");
    let config = config(&folder.0, &remote, &harness);
    let account = folder.0.join("home/accounts/stub");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::write(account.join(".credentials.json"), "{}").unwrap();
    let home = home_of(folder);
    std::fs::create_dir_all(&home).unwrap();
    start_with(
        &folder.0,
        &config,
        path_first,
        &[("OBID_PERSON_HOMES", home.to_str().unwrap())],
    )
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=Owner",
            "-c",
            "user.email=owner@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A git repository on `main` with one commit, no remote, the owner's name in its own settings.
fn repository(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Owner"]);
    git(dir, &["config", "user.email", "owner@example.invalid"]);
    std::fs::write(dir.join("notes.md"), "# Notes\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "First"]);
}

/// Every file of `dir` but its `.git`, by its path, with its bytes.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut all = BTreeMap::new();
    let mut left = vec![dir.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at).unwrap().flatten() {
            let path = entry.path();
            if entry.file_name() == ".git" {
                continue;
            }
            if path.is_dir() {
                left.push(path);
            } else {
                all.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    all
}

/// A new conversation in `folders`, its workspace `workspace` (none: left out).
fn start(obid: &Obid, folders: &[&Path], workspace: Option<&str>) -> String {
    let mut body = json!({"said": "tidy it up", "new": true,
                          "folders": folders.iter().map(|f| text(f)).collect::<Vec<_>>()});
    if let Some(workspace) = workspace {
        body["workspace"] = json!(workspace);
    }
    let answer = call(obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body["conversation"].as_str().unwrap().to_owned()
}

fn workspace(obid: &Obid, id: &str) -> Value {
    let answer = call(
        obid,
        "GET",
        &format!("/v1/conversations/{id}/workspace"),
        None,
    );
    assert_eq!(answer.status, 200, "{}", answer.text);
    answer.body
}

/// obid's sentence for `action` in `folder`, what the owner's device signs.
fn sentence(obid: &Obid, id: &str, folder: &Path, action: &str) -> String {
    let target = format!("/v1/conversations/{id}/workspace");
    let query = format!("{target}?folder={}&finish={action}", text(folder));
    let sentence = call(obid, "GET", &query, None);
    assert_eq!(sentence.status, 200, "{}", sentence.text);
    sentence.body["action"].as_str().unwrap().to_owned()
}

/// The owner's signed click on `sentence` for `action` in `folder`.
fn signed(obid: &Obid, id: &str, folder: &Path, action: &str, sentence: &str) -> Answer {
    let target = format!("/v1/conversations/{id}/workspace");
    let signed_at = time::format(time::now());
    let mut approval = owner_signs(
        obid,
        &format!("oby-finish\n{id}\n{sentence}\n{signed_at}"),
        &signed_at,
    );
    approval["action"] = json!(sentence);
    let body = json!({"folder": text(folder), "finish": action, "approval": approval});
    call(obid, "POST", &target, Some(&body))
}

/// The owner's signed click on `action` in `folder`: obid's own sentence, read first, signed.
fn finish(obid: &Obid, id: &str, folder: &Path, action: &str) -> Answer {
    let sentence = sentence(obid, id, folder, action);
    signed(obid, id, folder, action, &sentence)
}

/// A `gh` that only says it ran, in `bin`: its log, none until it's called.
fn fake_gh(bin: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin).unwrap();
    let log = bin.join("gh.log");
    std::fs::write(
        bin.join("gh"),
        format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 1\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    log
}

/// The gh calls made, but obid's own look at who's signed in (`gh auth`, `gh api user`), which it
/// makes as it starts, whatever any conversation holds.
fn gh_calls(log: &Path) -> Vec<String> {
    let all = std::fs::read_to_string(log).unwrap_or_default();
    (all.lines())
        .filter(|l| !l.starts_with("auth ") && !l.starts_with("api user"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn workspace_2_worktree_leaves_the_folder() {
    let folder = Folder::new("workspace-2");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);
    let before = files(&api);
    let head = git(&api, &["rev-parse", "HEAD"]);

    let id = start(&obid, &[&api], Some("worktree"));
    let seen = workspace(&obid, &id);
    assert_eq!(seen["workspace"], "worktree", "{seen}");
    let place = &seen["places"][0];
    assert_eq!(place["folder"], text(&api), "{seen}");
    assert_eq!(place["worktree"], true, "{seen}");
    let tree = PathBuf::from(place["path"].as_str().unwrap());
    assert_ne!(tree, api);
    assert!(tree.starts_with(folder.0.join("home/workspaces").canonicalize().unwrap()));
    let branch = place["branch"].as_str().unwrap().to_owned();
    assert!(branch.starts_with("oby/"), "{seen}");

    // Work there: a change on its own branch, committed.
    std::fs::write(tree.join("notes.md"), "# Notes\n\nTidied.\n").unwrap();
    git(&tree, &["commit", "-q", "-am", "Tidy"]);
    assert_eq!(git(&tree, &["symbolic-ref", "--short", "HEAD"]), branch);

    // The folder: byte-identical, still on main at its commit.
    assert_eq!(files(&api), before);
    assert_eq!(git(&api, &["symbolic-ref", "--short", "HEAD"]), "main");
    assert_eq!(git(&api, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&api, &["status", "--porcelain"]), "");

    // The worktree goes with its conversation; its branch, and its commit, stay.
    let gone = call(&obid, "DELETE", &format!("/v1/conversations/{id}"), None);
    assert!(gone.status < 300, "{}", gone.text);
    assert!(!tree.exists());
    assert_ne!(git(&api, &["rev-parse", &branch]), head);
    assert_eq!(files(&api), before);
}

#[test]
fn workspace_3_choice_is_kept() {
    let folder = Folder::new("workspace-3");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);

    // Left out, a conversation works in this folder, in place.
    let plain = start(&obid, &[&api], None);
    let seen = workspace(&obid, &plain);
    assert_eq!(seen["workspace"], "folder", "{seen}");
    assert_eq!(seen["places"][0]["path"], text(&api), "{seen}");
    assert_eq!(seen["places"][0]["worktree"], false, "{seen}");

    let id = start(&obid, &[&api], Some("worktree"));
    // Later words can't change it…
    let later = call(
        &obid,
        "POST",
        "/v1/plans",
        Some(&json!({"said": "now here", "conversation": id, "workspace": "folder"})),
    );
    assert_eq!(later.status, 422, "{}", later.text);
    // …nor can a change to the conversation.
    let patched = call(
        &obid,
        "PATCH",
        &format!("/v1/conversations/{id}"),
        Some(&json!({"workspace": "folder"})),
    );
    assert!(patched.status >= 400, "{}", patched.text);
    let seen = workspace(&obid, &id);
    assert_eq!(seen["workspace"], "worktree", "{seen}");
    assert_eq!(seen["places"][0]["worktree"], true, "{seen}");
    // Asked again, it's the same worktree, not a second one.
    assert_eq!(workspace(&obid, &id)["places"], seen["places"]);
}

#[test]
fn workspace_4_no_remote_commit_then_publish() {
    let folder = Folder::new("workspace-4");
    let bin = folder.0.join("bin");
    let gh_log = fake_gh(&bin);
    let obid = harness_obid(&folder, Some(&bin));
    let [notes] = <[PathBuf; 1]>::try_from(picked(&folder, &["notes"])).unwrap();
    repository(&notes);
    std::fs::write(notes.join("todo.md"), "- tidy\n").unwrap();

    let id = start(&obid, &[&notes], None);
    let before = gh_calls(&gh_log);
    let seen = workspace(&obid, &id);
    assert_eq!(seen["places"][0]["actions"], json!(["commit"]), "{seen}");
    assert_eq!(
        gh_calls(&gh_log),
        before,
        "no gh call for a folder with no remote"
    );

    // Commit, unsigned: refused, nothing committed.
    let target = format!("/v1/conversations/{id}/workspace");
    let unsigned = json!({"folder": text(&notes), "finish": "commit",
        "approval": {"device": "nobody", "action": "x", "signedAt": time::format(time::now()), "signature": "AA=="}});
    assert_eq!(call(&obid, "POST", &target, Some(&unsigned)).status, 403);
    assert_eq!(git(&notes, &["rev-list", "--count", "HEAD"]), "1");
    // An action finishing can't take there now: refused too, before any signature.
    let query = format!("{target}?folder={}&finish=push", text(&notes));
    assert_eq!(call(&obid, "GET", &query, None).status, 409);

    // Signed: committed, and once clean it offers Publish repository.
    let done = finish(&obid, &id, &notes, "commit");
    assert_eq!(done.status, 200, "{}", done.text);
    assert_eq!(git(&notes, &["status", "--porcelain"]), "");
    assert_eq!(git(&notes, &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(
        done.body["places"][0]["actions"],
        json!(["publishRepository"]),
        "{}",
        done.text
    );
    assert_eq!(
        gh_calls(&gh_log),
        before,
        "still no gh call until the owner publishes"
    );
}

#[test]
fn workspace_5_not_git_initialize() {
    let folder = Folder::new("workspace-5");
    let obid = harness_obid(&folder, None);
    let [plain] = <[PathBuf; 1]>::try_from(picked(&folder, &["plain"])).unwrap();
    std::fs::write(plain.join("notes.md"), "# Notes\n").unwrap();

    // Even with "A new worktree" asked, a folder that isn't git works in place.
    let id = start(&obid, &[&plain], Some("worktree"));
    let seen = workspace(&obid, &id);
    let place = &seen["places"][0];
    assert_eq!(place["path"], text(&plain), "{seen}");
    assert_eq!(place["worktree"], false, "{seen}");
    assert_eq!(place["actions"], json!(["initializeGit"]), "{seen}");
    assert!(!plain.join(".git").exists());

    let done = finish(&obid, &id, &plain, "initializeGit");
    assert_eq!(done.status, 200, "{}", done.text);
    assert!(plain.join(".git").is_dir());
    // A repository now, its files not yet committed: Commit next.
    assert_eq!(
        done.body["places"][0]["actions"],
        json!(["commit"]),
        "{}",
        done.text
    );
}

#[test]
fn workspace_6_factory_jobs_get_a_worktree() {
    let folder = Folder::new("workspace-6");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);
    let before = files(&api);
    // The conversation chose its own worktree: the job still gets one of its own.
    let id = start(&obid, &[&api], Some("worktree"));
    let seen = workspace(&obid, &id);
    assert_eq!(seen["workspace"], "worktree", "{seen}");
    let conversations = PathBuf::from(seen["places"][0]["path"].as_str().unwrap());

    let job = json!({"repo": "obi-e2e/fixture-app", "kind": "investigate", "task": "Look around"});
    let made = call_typed(&obid, "POST", "/v1/jobs", &json!({ "job": job }));
    assert_eq!(made.status, 201, "{}", made.text);
    let job = made.body["id"].as_str().unwrap().to_owned();
    let ended = ended_within(&obid, &job, 120);
    // Its own worktree in obid's home, never the conversation's folder.
    let tree = ended["worktree"].as_str().unwrap_or_default().to_owned();
    assert!(!tree.is_empty(), "{ended}");
    assert!(
        Path::new(&tree).starts_with(folder.0.join("home/worktrees")),
        "{tree}"
    );
    assert!(!Path::new(&tree).starts_with(&api));
    assert!(!Path::new(&tree).starts_with(&conversations));
    assert!(!Path::new(&tree).starts_with(folder.0.join("home/workspaces")));
    assert_eq!(files(&api), before);
    // The folder's repository has its own worktree and the conversation's, never the job's.
    let listed = git(&api, &["worktree", "list", "--porcelain"]);
    assert_eq!(listed.matches("worktree ").count(), 2, "{listed}");
    assert!(listed.contains(&text(&conversations)), "{listed}");
    assert!(!listed.contains(&tree), "{listed}");
}

/// An executable script at `path`.
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A choice that can't be kept would read as "This folder": the conversation doesn't start, and
/// says why (fail closed).
#[test]
fn workspace_3_a_choice_that_cant_be_kept_starts_nothing() {
    let folder = Folder::new("workspace-3-unkept");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);
    // Where the choices go is a file, so none can be written there.
    std::fs::write(folder.0.join("home/workspaces"), "not a folder").unwrap();
    let ids = || -> Vec<Value> {
        let listed = call(&obid, "GET", "/v1/conversations", None).body;
        (listed.as_array().cloned().unwrap_or_default().iter())
            .map(|c| c["id"].clone())
            .collect()
    };
    let before = ids();

    let body = json!({"said": "tidy it up", "new": true, "workspace": "worktree",
                      "folders": [text(&api)]});
    let answer = call(&obid, "POST", "/v1/plans", Some(&body));
    assert_eq!(answer.status, 500, "{}", answer.text);
    assert!(
        answer
            .text
            .contains("Oby couldn't keep where this conversation works"),
        "{}",
        answer.text
    );
    // No conversation was left to run in the folder.
    assert_eq!(ids(), before);
}

/// A choice there that can't be read gives every folder the room only: no path, no action, and
/// why; never the folder in its place.
#[test]
fn workspace_3_an_unreadable_choice_gives_the_room_only() {
    let folder = Folder::new("workspace-3-unread");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);
    std::fs::write(api.join("todo.md"), "- tidy\n").unwrap();
    let id = start(&obid, &[&api], Some("worktree"));
    std::fs::write(
        folder.0.join(format!("home/workspaces/{id}.json")),
        "{not json",
    )
    .unwrap();

    let seen = workspace(&obid, &id);
    let place = &seen["places"][0];
    assert_eq!(place["folder"], text(&api), "{seen}");
    assert_eq!(place["path"], Value::Null, "{seen}");
    assert_eq!(place["actions"], json!([]), "{seen}");
    assert!(
        place["why"]
            .as_str()
            .unwrap_or_default()
            .contains("its own room"),
        "{seen}"
    );
    let query = format!(
        "/v1/conversations/{id}/workspace?folder={}&finish=commit",
        text(&api)
    );
    assert_eq!(call(&obid, "GET", &query, None).status, 409);
}

/// A worktree that can't be made gives that folder the room only, never the folder itself.
#[test]
fn workspace_2_a_worktree_that_cant_be_made_gives_the_room_only() {
    let folder = Folder::new("workspace-2-unmade");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    // Not git yet: it works in place, as T3 does.
    std::fs::write(api.join("notes.md"), "# Notes\n").unwrap();
    let id = start(&obid, &[&api], Some("worktree"));
    assert_eq!(workspace(&obid, &id)["places"][0]["path"], text(&api));

    // Now a repository, its worktree can't be made: the conversation's worktrees' folder is a file.
    repository_here(&api);
    std::fs::write(folder.0.join(format!("home/workspaces/{id}")), "in the way").unwrap();
    let seen = workspace(&obid, &id);
    let place = &seen["places"][0];
    assert_eq!(place["path"], Value::Null, "{seen}");
    assert_eq!(place["worktree"], false, "{seen}");
    assert_eq!(place["actions"], json!([]), "{seen}");
    assert!(
        place["why"]
            .as_str()
            .unwrap_or_default()
            .contains("couldn't make a worktree"),
        "{seen}"
    );
}

/// [`repository`] in a folder that has files already.
fn repository_here(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Owner"]);
    git(dir, &["config", "user.email", "owner@example.invalid"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "First"]);
}

/// Two folders of one name: each sentence names its own by its full path.
#[test]
fn workspace_5_a_sentence_names_the_folder_by_its_full_path() {
    let folder = Folder::new("workspace-5-paths");
    let obid = harness_obid(&folder, None);
    let [a, b] = <[PathBuf; 2]>::try_from(picked(&folder, &["a/api", "b/api"])).unwrap();
    let id = start(&obid, &[&a, &b], None);
    let first = sentence(&obid, &id, &a, "initializeGit");
    let second = sentence(&obid, &id, &b, "initializeGit");
    assert_eq!(first, format!("Make {} a git repository.", text(&a)));
    assert_eq!(second, format!("Make {} a git repository.", text(&b)));
}

/// A commit takes exactly the files its sentence named, as T3's commit takes the files its dialog
/// shows: never a file the owner didn't see, nor one of the repository outside the folder. The
/// owner's pre-commit hook doesn't run.
#[test]
fn workspace_4_commit_takes_only_the_files_it_named() {
    let folder = Folder::new("workspace-4-files");
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["mono/api"])).unwrap();
    let mono = api.parent().unwrap().to_path_buf();
    std::fs::write(api.join("main.rs"), "fn main() {}\n").unwrap();
    repository(&mono);
    let hooked = folder.0.join("hooked");
    script(
        &mono.join(".git/hooks/pre-commit"),
        &format!("touch '{}'\nexit 1", hooked.display()),
    );
    std::fs::write(api.join("main.rs"), "fn main() { tidy(); }\n").unwrap();
    std::fs::write(api.join("todo.md"), "- tidy\n").unwrap();
    std::fs::write(mono.join("outside.md"), "someone else's\n").unwrap();

    let id = start(&obid, &[&api], None);
    let said = sentence(&obid, &id, &api, "commit");
    assert_eq!(
        said,
        format!(
            "Commit 2 changes in {} on main: api/main.rs, api/todo.md (new).",
            text(&api)
        )
    );

    // A file that comes after the owner read it isn't theirs to commit: refused, nothing done.
    std::fs::write(api.join("late.md"), "- later\n").unwrap();
    let refused = signed(&obid, &id, &api, "commit", &said);
    assert_eq!(refused.status, 403, "{}", refused.text);
    assert_eq!(git(&mono, &["rev-list", "--count", "HEAD"]), "1");
    std::fs::remove_file(api.join("late.md")).unwrap();

    let done = signed(&obid, &id, &api, "commit", &said);
    assert_eq!(done.status, 200, "{}", done.text);
    let committed = git(&mono, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(committed, "api/main.rs\napi/todo.md");
    assert_eq!(git(&mono, &["status", "--porcelain"]), "?? outside.md");
    assert!(!hooked.exists(), "the owner's pre-commit hook ran");
}

/// A `gh` that logs its calls and, for `repo create`, says the repository is `url`; nothing else.
fn creating_gh(bin: &Path, url: &str) -> PathBuf {
    let log = bin.join("gh.log");
    script(
        &bin.join("gh"),
        &format!(
            "echo \"$*\" >> '{}'\ncase \"$1 $2\" in \"repo create\") echo '{url}'; exit 0;; esac\nexit 1",
            log.display()
        ),
    );
    log
}

/// Publish repository: gh makes the repository and pushes nothing; obid sends exactly the commit
/// it scanned, from its own git, and none of the folder's hooks runs.
#[test]
fn workspace_4_publish_sends_exactly_the_commit_it_scanned() {
    let folder = Folder::new("workspace-4-publish");
    let bin = folder.0.join("bin");
    let github = folder.0.join("github/notes.git");
    std::fs::create_dir_all(&github).unwrap();
    git(&github, &["init", "-q", "--bare"]);
    let url = format!("file://{}", github.display());
    let gh_log = creating_gh(&bin, &url);
    let obid = harness_obid(&folder, Some(&bin));
    let [notes] = <[PathBuf; 1]>::try_from(picked(&folder, &["notes"])).unwrap();
    repository(&notes);
    let hooked = folder.0.join("hooked");
    script(
        &notes.join(".git/hooks/pre-push"),
        &format!("touch '{}'", hooked.display()),
    );
    let head = git(&notes, &["rev-parse", "HEAD"]);

    let id = start(&obid, &[&notes], None);
    assert_eq!(
        workspace(&obid, &id)["places"][0]["actions"],
        json!(["publishRepository"])
    );
    let done = finish(&obid, &id, &notes, "publishRepository");
    assert_eq!(done.status, 200, "{}", done.text);

    let created: Vec<String> = gh_calls(&gh_log)
        .into_iter()
        .filter(|c| c.starts_with("repo create"))
        .collect();
    assert_eq!(created, vec!["repo create notes --private".to_owned()]);
    assert_eq!(git(&github, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(git(&notes, &["remote", "get-url", "origin"]), url);
    assert!(!hooked.exists(), "the folder's pre-push hook ran");
    assert_eq!(
        done.body["places"][0]["actions"],
        json!([]),
        "{}",
        done.text
    );
}

/// Push: exactly the commit scanned goes to the branch, and the folder's own settings for its
/// remote (a `receivepack` program of its choosing) never run.
#[test]
fn workspace_4_push_runs_nothing_of_the_folders_settings() {
    let folder = Folder::new("workspace-4-push");
    let remote = folder.0.join("remote/api.git");
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "-q", "--bare"]);
    let obid = harness_obid(&folder, None);
    let [api] = <[PathBuf; 1]>::try_from(picked(&folder, &["api"])).unwrap();
    repository(&api);
    let url = format!("file://{}", remote.display());
    git(&api, &["remote", "add", "origin", &url]);
    // Pushed once by the owner, main its upstream and the remote's default branch.
    git(&api, &["push", "-q", "-u", "origin", "main"]);
    git(
        &api,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    let ran = folder.0.join("ran");
    let program = folder.0.join("receive");
    script(
        &program,
        &format!("touch '{}'\nexec git-receive-pack \"$@\"", ran.display()),
    );
    git(
        &api,
        &["config", "remote.origin.receivepack", &text(&program)],
    );
    git(&api, &["commit", "-q", "--allow-empty", "-m", "Second"]);
    let head = git(&api, &["rev-parse", "HEAD"]);

    let id = start(&obid, &[&api], None);
    assert_eq!(
        workspace(&obid, &id)["places"][0]["actions"],
        json!(["push"])
    );
    let done = finish(&obid, &id, &api, "push");
    assert_eq!(done.status, 200, "{}", done.text);
    assert_eq!(git(&remote, &["rev-parse", "refs/heads/main"]), head);
    assert!(!ran.exists(), "the folder's receivepack setting ran");
    // The folder knows it's pushed: nothing left to push.
    assert_eq!(
        done.body["places"][0]["actions"],
        json!([]),
        "{}",
        done.text
    );
}
