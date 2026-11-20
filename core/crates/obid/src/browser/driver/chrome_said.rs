//! What Chrome itself says when it won't start (BROWSER-23, 24): agent-browser reports only that
//! Chrome "exited early", so obid runs the same binary once more on its own, headless, on an empty
//! profile of its own and for at most 10 seconds, and keeps the end of what it printed. That is
//! the reason a person (or CI) needs: a library missing on Linux, a sandbox the system refuses.
//! It owns no state and writes only its throwaway profile, removed after.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

/// How long Chrome may take to show why it stops.
const SAID_WITHIN: Duration = Duration::from_secs(10);
/// How many of its last lines are kept.
const LINES_KEPT: usize = 12;

/// The last lines Chrome printed while starting headless on `scratch`'s throwaway profile, or why
/// it couldn't be asked.
pub async fn chrome_said(chrome: &Path, scratch: &Path) -> String {
    let profile = scratch.join("chrome-said");
    let _ = std::fs::create_dir_all(&profile);
    let run = tokio::process::Command::new(chrome)
        .args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            "--dump-dom",
        ])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("about:blank")
        .env("HOME", &profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    let said = match tokio::time::timeout(SAID_WITHIN, run).await {
        Ok(Ok(out)) => {
            let text = String::from_utf8_lossy(&out.stderr).into_owned();
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            // Chrome's own reason comes before a crash's stack trace: its FATAL, ERROR and failed
            // checks first, else the last lines.
            let reasons: Vec<&str> = lines
                .iter()
                .copied()
                .filter(|l| {
                    ["FATAL", "ERROR:", "Check failed", "error while loading"]
                        .iter()
                        .any(|k| l.contains(k))
                })
                .take(LINES_KEPT)
                .collect();
            let tail = if reasons.is_empty() {
                lines[lines.len().saturating_sub(LINES_KEPT)..].join(" | ")
            } else {
                reasons.join(" | ")
            };
            match (out.status.code(), tail.is_empty()) {
                (Some(0), true) => "it started on its own and printed nothing".to_string(),
                (code, true) => format!("it printed nothing (exit {code:?})"),
                (code, false) => format!("{tail} (exit {code:?})"),
            }
        }
        Ok(Err(e)) => format!("it couldn't be run: {e}"),
        Err(_) => format!(
            "it was still running after {} s and printed nothing that stopped it",
            SAID_WITHIN.as_secs()
        ),
    };
    let _ = std::fs::remove_dir_all(&profile);
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_chrome_that_stops_says_why_in_its_last_lines() {
        let dir = super::super::stand_in::scratch("chrome-said");
        let chrome = dir.join("chrome");
        std::fs::write(
            &chrome,
            "#!/bin/bash\necho 'starting' >&2\necho '[1:1:FATAL:zygote_host_impl_linux.cc] No usable sandbox!' >&2\nfor i in $(seq 1 30); do echo \"#$i 0x0 (chrome+0x$i)\" >&2; done\nexit 127\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&chrome).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&chrome, perms).unwrap();
        let said = chrome_said(&chrome, &dir).await;
        assert!(
            said.contains("No usable sandbox"),
            "the reason, not the stack's tail: {said}"
        );
        assert!(!said.contains("#30"), "{said}");
        assert!(said.contains("127"), "{said}");
        assert!(
            !dir.join("chrome-said").exists(),
            "its throwaway profile is gone"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
