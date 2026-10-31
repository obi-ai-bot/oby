//! The chat reads before it answers (Engine/ReadJob.swift, #302): `oby_job` answers one job's
//! report, its pull request, its last Mac checks and Oby's last verdict, the way Claude Code reads a
//! file first. All of it is what agents and tools wrote: quoted data in the turn's fence, never
//! instructions.

use std::path::Path;

use serde_json::Value;

use super::briefing::{cause, line, post_mortem};
use crate::failures::Record;
use crate::job::Job;

/// The most one read puts in the file, in bytes.
pub const SIZE: usize = 8 * 1024;
/// The most failure records one job's read carries, newest first.
pub const FAILURES: usize = 3;

/// `text` cut at `size` bytes on a whole character, with a note when it was cut.
pub fn clamp(text: &str, size: usize) -> String {
    if text.len() <= size {
        return text.to_owned();
    }
    let tail = format!("\n…(cut: this is the first {} KB)\n", size / 1024);
    let room = size.saturating_sub(tail.len());
    let mut end = 0;
    for (index, c) in text.char_indices() {
        if index + c.len_utf8() > room {
            break;
        }
        end = index + c.len_utf8();
    }
    format!("{}{tail}", &text[..end])
}

/// One job's section for a read: its status, report, pull request, Mac checks and Oby's
/// verdict. Each part says when it has nothing, so the brain doesn't guess.
pub fn section(job: &Job, home: &Path) -> String {
    let mut lines = vec![
        format!("## Read: job {} ({})", job.id, line(&job.short_title(), 60)),
        String::new(),
    ];
    let mut status = format!("Status: {}", job.status.as_str());
    if !job.step.is_empty() {
        status.push_str(&format!(", step: {}", line(&job.step, 200)));
    }
    lines.push(status);
    if let Some(limit) = &job.limit {
        lines.push(format!("Stopped at a limit: {}", line(limit, 200)));
    }
    // Why it stopped without ending done, and its one next step (#725): apart from the report.
    if let Some(reason) = &job.reason {
        lines.push(format!("Why it stopped: {}", line(reason, 600)));
    }
    if let Some(next) = &job.next_step {
        lines.push(format!("Next step: {next}"));
    }
    // What its conversation did each time it stalled (super::supervisor), in obid's words.
    for step in &job.supervised {
        let started = if step.jobs.is_empty() {
            String::new()
        } else {
            format!(", started {}", step.jobs.join(", "))
        };
        lines.push(match step.decision.as_str() {
            "asked" => "When it stalled, the next step was left to the owner".to_owned(),
            decision => format!(
                "When it stalled, Oby's conversation chose {decision} ({} of {} automatic steps{started})",
                step.count,
                super::supervisor::MOST
            ),
        });
    }
    lines.extend([String::new(), "### Report".into()]);
    // The replies to the owner's words keep their own room past a long report (#1661).
    lines.push(match crate::job::steered(&job.summary) {
        ("", None) => "(none yet)".into(),
        (report, None) => report.chars().take(3000).collect(),
        (report, Some(replies)) => format!(
            "{}\n\n{}",
            report.chars().take(2000).collect::<String>(),
            replies.chars().take(1500).collect::<String>()
        ),
    });
    if let Some(stalled) = &job.stalled {
        lines.extend([String::new(), "### Handoff note".into()]);
        lines.push(stalled.note.chars().take(3000).collect());
    }
    // What obid sent for it, ids only: a push that went out is never read as nothing (#627).
    lines.extend([String::new(), "### Outgoing actions".into()]);
    if job.outgoing.is_empty() {
        lines.push("(nothing went out)".into());
    }
    for sent in &job.outgoing {
        lines.push(match &sent.branch {
            Some(branch) => format!("- {} {} to {branch}", sent.action, sent.id),
            None => format!("- {} {}", sent.action, sent.id),
        });
    }
    lines.extend([String::new(), "### Pull request".into()]);
    lines.push(match (&job.published, job.pr) {
        (Some(url), _) => format!("{url}: its state isn't in this section yet"),
        (None, Some(pr)) => format!("#{pr}: its state isn't in this section yet"),
        (None, None) => "(no pull request)".into(),
    });
    if let Some(error) = &job.publish_error {
        lines.push(format!("Publishing failed: {}", line(error, 300)));
    }
    lines.extend([String::new(), "### Last Mac check".into()]);
    lines.extend(mac_checks(job));
    lines.extend([
        String::new(),
        "### Oby's last verdict on its pull request".into(),
        "(no verdict yet)".into(),
        String::new(),
        "### Failure post-mortem".into(),
    ]);
    // Its failure records, newest first, each with what the job kept and its post-mortem (#591).
    let records: Vec<Record> = crate::failures::list(home, crate::failures::MOST)
        .into_iter()
        .filter(|record| record.job == job.id)
        .take(FAILURES)
        .collect();
    if records.is_empty() {
        lines.push("(no failure recorded)".into());
    }
    for record in &records {
        lines.push(format!(
            "Failure {}: {}, {} round{} used.",
            record.id,
            cause(record.trigger),
            record.rounds,
            if record.rounds == 1 { "" } else { "s" }
        ));
        lines.push(post_mortem(record));
    }
    lines.join("\n")
}

/// The job's last Mac checks: the merge check, and the Apple build and test its agent asked for.
fn mac_checks(job: &Job) -> Vec<String> {
    let mut lines = vec![];
    let apple = job.apple_ran.as_ref().or(job.apple_check.as_ref());
    for (name, check) in [
        ("Merge check", job.mac_check.as_ref()),
        ("Apple build and test", apple),
    ] {
        let Some(check) = check else { continue };
        let mut head = format!(
            "{name} (round {}): {}",
            check["round"].as_u64().unwrap_or(1),
            check["state"].as_str().unwrap_or("unknown")
        );
        if let Some(mac) = check["mac"].as_str() {
            head.push_str(&format!(" on {mac}"));
        }
        lines.push(head);
        let steps = check["steps"].as_array().cloned().unwrap_or_default();
        let mut named: Vec<String> = vec![];
        for step in &steps {
            let label = step["name"]
                .as_str()
                .or(step["label"].as_str())
                .unwrap_or("step");
            lines.push(format!(
                "- Step “{}”: {}, {} s",
                line(label, 120),
                if step["passed"] == true {
                    "passed"
                } else {
                    "FAILED"
                },
                step["seconds"].as_u64().unwrap_or(0)
            ));
            for failing in step["failing"].as_array().into_iter().flatten().take(10) {
                let test = failing["test"].as_str().unwrap_or("");
                named.push(test.to_owned());
                let message = failing["message"].as_str().unwrap_or("");
                lines.push(format!(
                    "  - Failing test {}{}",
                    line(test, 160),
                    if message.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", line(message, 300))
                    }
                ));
            }
        }
        let rest: Vec<String> = check["failures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|t| !named.iter().any(|n| n == t))
            .take(10)
            .map(|t| line(t, 160))
            .collect();
        if !rest.is_empty() {
            lines.push(format!("Failing tests: {}", rest.join(", ")));
        }
        if let Some(problem) = check["problem"].as_str() {
            lines.push(format!(
                "Problem (the Mac couldn't run it): {}",
                line(problem, 400)
            ));
        }
    }
    if lines.is_empty() {
        vec!["(no Mac check ran)".into()]
    } else {
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A finished job whose Mac check failed in a step, on one test.
    fn failed(id: &str, title: &str) -> Job {
        let mut job = Job::new(
            id.into(),
            "deskree-inc/oby".into(),
            "build".into(),
            "Work on #60".into(),
            "claude".into(),
            "default".into(),
        );
        job.title = Some(title.into());
        job.status = crate::job::Status::Done;
        job.summary = "Added the vault.\nIt builds in the Engine.".into();
        job.mac_check = Some(json!({
            "round": 1, "state": "failed", "mac": "Studio",
            "steps": [{"name": "Engine build", "passed": false, "seconds": 41},
                      {"name": "MacTests", "passed": false, "seconds": 90,
                       "failing": [{"test": "MacTests.VaultTests/sealsTheKey()", "message": "Engine/Vault.swift:88: expected 32 bytes"}]}],
            "failures": ["MacTests.VaultTests/sealsTheKey()", "MacTests.Other/also()"]}));
        job
    }

    /// ReadJobTests.theSectionHasTheVerdictTheAppleCheckAndThePostMortem, for what obid keeps.
    #[test]
    fn the_section_has_the_report_and_the_mac_checks() {
        let mut job = failed("aaaa1111", "Vault secrets");
        job.apple_ran =
            Some(json!({"round": 2, "state": "error", "problem": "the simulator hung"}));
        let text = section(&job, Path::new("/nonexistent"));
        for part in [
            "## Read: job aaaa1111 (Vault secrets)",
            "Status: done",
            "### Report\nAdded the vault.\nIt builds in the Engine.",
            "Merge check (round 1): failed on Studio",
            "- Step “MacTests”: FAILED, 90 s",
            "  - Failing test MacTests.VaultTests/sealsTheKey(): Engine/Vault.swift:88: expected 32 bytes",
            "Failing tests: MacTests.Other/also()",
            "Apple build and test (round 2): error",
            "Problem (the Mac couldn't run it): the simulator hung",
        ] {
            assert!(text.contains(part), "{part}\n\n{text}");
        }
        // Nothing to say is said, so the brain doesn't guess.
        let mut bare = failed("bbbb2222", "Watch home");
        bare.mac_check = None;
        let empty = section(&bare, Path::new("/nonexistent"));
        for part in [
            "(no Mac check ran)",
            "(no verdict yet)",
            "(no failure recorded)",
            "(nothing went out)",
            "(no pull request)",
        ] {
            assert!(empty.contains(part), "{part}");
        }
        // A push that went out is listed, and a pull request that failed after it says so (#627).
        bare.outgoing = vec![crate::publish::Outgoing::new(
            "push",
            &bare.repo,
            &"a".repeat(40),
            Some("oby/build-bbbb2222"),
        )];
        bare.publish_error = Some("Pushed, but couldn't open the pull request: gh broke".into());
        let pushed = section(&bare, Path::new("/nonexistent"));
        assert!(
            pushed.contains(&format!("- push {} to oby/build-bbbb2222", "a".repeat(40))),
            "{pushed}"
        );
        assert!(!pushed.contains("(nothing went out)"), "{pushed}");
        // A job that needs the owner says why apart from its report, with its handoff note (#725).
        bare.status = crate::job::Status::Interrupted;
        bare.reason = Some("Stalled: no progress for 15 minutes.".into());
        bare.next_step = Some(crate::job::TRY_AGAIN.into());
        bare.stalled = Some(crate::job::Stalled {
            why: "quiet".into(),
            text: "no progress for 15 minutes".into(),
            at: 0,
            round: 1,
            branch: "oby/build-bbbb2222".into(),
            commit: None,
            commits: 0,
            session: true,
            note: "Done: the parser.\nLeft: the tests.".into(),
            note_by: "agent".into(),
        });
        let needs = section(&bare, Path::new("/nonexistent"));
        for part in [
            "Why it stopped: Stalled: no progress for 15 minutes.",
            "Next step: Try again",
            "### Handoff note\nDone: the parser.\nLeft: the tests.",
        ] {
            assert!(needs.contains(part), "{part}\n\n{needs}");
        }
        assert!(
            pushed.contains("Publishing failed: Pushed, but couldn't open the pull request"),
            "{pushed}"
        );
    }

    /// #591: read_job on a failed job carries its failure records, each with what the job kept and
    /// its post-mortem, inside the turn's fence; another job's record stays out.
    #[test]
    fn a_failed_jobs_read_quotes_its_post_mortem() {
        use crate::failures::{Evidence, Record, Trigger};
        let home = std::env::temp_dir().join(format!("obid-read-{}", uuid::Uuid::new_v4()));
        let folder = home.join("failures");
        std::fs::create_dir_all(&folder).unwrap();
        let keep = |job: &str, date: i64| {
            let record = Record {
                id: format!("{job}-limit-{date}"),
                trigger: Trigger::Limit,
                date,
                job: job.into(),
                repo: "deskree-inc/oby".into(),
                title: "Vault secrets".into(),
                what: "Stopped at a limit: 3 rounds and `swift test` still fails".into(),
                rounds: 3,
                evidence: Evidence {
                    summary: "Nothing committed: VaultTests still fail.".into(),
                    step: "Stage 2 of 5, Work: Round 3 of 3".into(),
                    check: Some("error: VaultTests.testSecret() failed".into()),
                },
            };
            std::fs::write(
                folder.join(format!("{}.json", record.id)),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
            record
        };
        let mine = keep("aaaa1111", 1_791_582_173);
        let theirs = keep("bbbb2222", 1_791_582_000);
        let mut job = failed("aaaa1111", "Vault secrets");
        job.status = crate::job::Status::Failed;
        let text = section(&job, &home);
        let part = format!(
            "### Failure post-mortem\nFailure {}: stopped at a limit, 3 rounds used.\nWhat Oby saw: Stopped at a limit: 3 rounds and `swift test` still fails\nWhere it was: Stage 2 of 5, Work: Round 3 of 3\nPost-mortem: none yet. Oby hasn't studied this failure.\nThe end of its report:\nNothing committed: VaultTests still fail.\nThe end of its check:\nerror: VaultTests.testSecret() failed",
            mine.id
        );
        assert!(text.contains(&part), "{part}\n\n{text}");
        assert!(!text.contains(&theirs.id), "{text}");
        assert!(!text.contains("(no failure recorded)"), "{text}");
        std::fs::remove_dir_all(&home).ok();
    }

    /// ReadJobTests.aReadIsCappedAtAboutEightKilobytes
    #[test]
    fn a_read_is_capped_at_about_eight_kilobytes() {
        let text = clamp(&"é long report line ".repeat(4000), SIZE);
        assert!(text.len() <= SIZE && text.contains("…(cut:"));
        assert_eq!(clamp("short", SIZE), "short");
        // The job's own report is cut before that, so a section stays well inside the cap.
        let mut long = failed("aaaa1111", "Vault secrets");
        long.summary = "report ".repeat(4000);
        assert!(section(&long, Path::new("/nonexistent")).len() < SIZE);
        // TASKCONV-14: the reply to the owner's words keeps its room past a long report.
        long.summary = format!(
            "{}\n\n{}Ended the line with a period.",
            "report ".repeat(4000),
            crate::job::STEERED
        );
        let read = section(&long, Path::new("/nonexistent"));
        assert!(read.len() < SIZE);
        assert!(
            read.contains("After what you told it: Ended the line with a period."),
            "{read}"
        );
    }
}
