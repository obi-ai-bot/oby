//! obid's tests' stand-in agent-browser (BROWSER-10, 11, 23 to 25): a shell script answering
//! `--json` lines as agent-browser 0.43.0 does for the few commands the driver runs, starting a
//! daemon (double-forked, its command line beginning with the binary's path, its PID written into
//! the socket folder) with a "Chrome" under it that SIGTERM takes down, and sleeping for a URL with
//! `slow` in it, leaving the daemon busy until the test clears it. Every call is logged in the
//! job's `HOME`. Tests only (obid's, and a debug build's integration tests'): it writes the script
//! under the home it is given.

use std::path::{Path, PathBuf};

use super::Session;
use crate::browser::install::Installed;

const SCRIPT: &str = r#"#!/bin/bash
self="$0"
if [ "$1" = "__daemon" ]; then
  (exec -a "$2" /bin/sleep 100000) &
  chrome=$!
  trap 'kill $chrome 2>/dev/null; exit 0' TERM
  echo $$ > "$3"
  wait $chrome
  exit 0
fi
echo "$*" >> "$HOME/calls.log"
session=""; exe=""; words=()
while [ $# -gt 0 ]; do
  case "$1" in
    --json|--no-auto-dialog) shift ;;
    --session) session="$2"; shift 2 ;;
    --executable-path) exe="$2"; shift 2 ;;
    --profile|--download-path|--idle-timeout) shift 2 ;;
    *) words+=("$1"); shift ;;
  esac
done
dir="$AGENT_BROWSER_SOCKET_DIR"
pidfile="$dir/$session.pid"; urlfile="$dir/$session.url"; busy="$dir/$session.busy"
ok() { printf '{"success":true,"data":%s}\n' "$1"; exit 0; }
fail() { printf '{"success":false,"error":"%s"}\n' "$1"; exit 0; }
up() {
  if [ -s "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then return; fi
  rm -f "$pidfile"
  [ -f "$urlfile" ] || echo "about:blank" > "$urlfile"
  ( exec -a "$self" /bin/bash "$self" __daemon "$exe" "$pidfile" </dev/null >/dev/null 2>&1 & )
  for _ in $(seq 1 500); do [ -s "$pidfile" ] && return; /bin/sleep 0.01; done
  fail "the stand-in daemon didn't start"
}
case "${words[0]} ${words[1]}" in
  "get url")
    up; [ -f "$busy" ] && exec /bin/sleep 100000
    ok "{\"url\":\"$(cat "$urlfile")\"}" ;;
  "open "*)
    up; u="${words[1]}"
    case "$u" in
      *slow*) touch "$busy"; echo "$u" > "$urlfile"; exec /bin/sleep 100000 ;;
      *refused*) fail "net::ERR_CONNECTION_REFUSED at $u" ;;
    esac
    echo "$u" > "$urlfile"; ok "{\"url\":\"$u\",\"title\":\"Fixture\"}" ;;
  "tab list") up; ok "{\"tabs\":[{\"targetId\":\"T1\",\"url\":\"$(cat "$urlfile")\",\"active\":true}]}" ;;
  "tab T1") up; ok '{}' ;;
  "snapshot ") up; ok '{"snapshot":"- button \"Go\" [ref=e1]","refs":{"e1":{"role":"button","name":"Go"}}}' ;;
  "click "*) up; ok '{}' ;;
  "get box") up; ok '{"x":10,"y":20,"width":100,"height":20}' ;;
  "stream enable") up; ok '{}' ;;
  "stream status") up; ok "{\"port\":$(cat "$HOME/stream-port" 2>/dev/null || echo 0)}" ;;
  *) fail "the stand-in doesn't know ${words[*]}" ;;
esac
"#;

/// Writes the stand-in where obid would install agent-browser under `home`, and names a Chrome
/// path beside it (never run: the stand-in's "Chrome" is a sleep with that name).
pub fn install(home: &Path) -> Installed {
    let root = home.join("browser");
    let bin = root.join("agent-browser-0.43.0/package/bin/agent-browser");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, SCRIPT).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    Installed {
        agent_browser: bin,
        chrome: root.join("chrome-158.0.8127.36/chrome"),
    }
}

/// Every call the stand-in got for `job`, one line each, flags included.
pub fn calls(home: &Path, job: &str) -> Vec<String> {
    std::fs::read_to_string(Session::job_dir(home, job).join("home/calls.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A fresh home for one test, under the system's temporary folder.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("obid-drv-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Waits, up to `limit`, until `ready` holds; false if it never did.
pub async fn within<F, Fut>(limit: std::time::Duration, mut ready: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let until = tokio::time::Instant::now() + limit;
    loop {
        if ready().await {
            return true;
        }
        if tokio::time::Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
