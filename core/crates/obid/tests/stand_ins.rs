//! The stand-ins end by themselves once nobody is left to answer. A test kills obid when it ends and
//! deletes its folder, with a side call perhaps still running: what doesn't end stays on the Mac.

mod support;

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use support::Folder;

/// `tests/agents/<name>`, in `room`, with `home` as its home and only `PATH` from this environment.
fn stand_in(name: &str, room: &Path, home: &Path, args: &[&str]) -> Child {
    Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/agents")
            .join(name),
    )
    .args(args)
    .env_clear()
    .env("PATH", std::env::var_os("PATH").unwrap())
    .env("HOME", home)
    .env("CLAUDE_CODE_ENABLE_TODO_TOOLS", "1")
    .current_dir(room)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap()
}

/// Whether it ended within `limit`; if not, this ends it, so a failing test leaves nothing either.
fn ends_within(child: &mut Child, limit: Duration) -> bool {
    let give_up_at = Instant::now() + limit;
    while Instant::now() < give_up_at {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

/// A folder holding a `room` to work in and a `home`.
fn folder(name: &str) -> Folder {
    let folder = Folder::new(name);
    std::fs::create_dir(folder.0.join("room")).unwrap();
    std::fs::create_dir(folder.0.join("home")).unwrap();
    folder
}

#[test]
fn the_planner_ends_when_its_working_folder_is_gone() {
    let folder = folder("planner-folder-gone");
    let (room, home) = (folder.0.join("room"), folder.0.join("home"));
    let mut planner = stand_in("fake-planner", &room, &home, &["-p", "--json-schema", "{}"]);
    std::fs::remove_dir_all(&room).unwrap();
    drop(planner.stdin.take());
    assert!(
        ends_within(&mut planner, Duration::from_secs(20)),
        "tests/agents/fake-planner went on with its working folder gone"
    );
}

#[test]
fn the_planner_ends_when_its_lock_never_frees() {
    let folder = folder("planner-lock-stuck");
    let (room, home) = (folder.0.join("room"), folder.0.join("home"));
    std::fs::create_dir_all(home.join(".fake-planner/lock")).unwrap();
    let mut planner = stand_in("fake-planner", &room, &home, &["-p", "--json-schema", "{}"]);
    drop(planner.stdin.take());
    assert!(
        ends_within(&mut planner, Duration::from_secs(180)),
        "tests/agents/fake-planner waited on its lock forever"
    );
}

#[test]
fn claude_told_to_wait_forever_ends_after_its_stdin_closes() {
    let folder = folder("claude-stdin-closed");
    let (room, home) = (folder.0.join("room"), folder.0.join("home"));
    let mut claude = stand_in("fake-claude", &room, &home, &[]);
    let mut stdin = claude.stdin.take().unwrap();
    stdin
        .write_all(b"{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"QUIET\"}}\n")
        .unwrap();
    // Its init line: it has the prompt.
    let mut line = String::new();
    BufReader::new(claude.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.contains("init"), "{line}");
    drop(stdin);
    // A killed obid's next start ends it: until then it goes on, as Claude Code does.
    std::thread::sleep(Duration::from_secs(2));
    assert!(claude.try_wait().unwrap().is_none(), "it ended at once");
    assert!(
        ends_within(&mut claude, Duration::from_secs(180)),
        "tests/agents/fake-claude went on with nobody to talk to"
    );
}
