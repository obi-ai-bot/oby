//! Why a failed check judged nothing, so the job ends for the owner's Publish by hand: a check
//! that says so (PUBLISHING-22), the machine's failure, not the code's (PUBLISHING-17), or obid
//! stopping one that did nothing or ran too long (PUBLISHING-29).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// What a check's output ends with when obid stopped it at `limit`, the most it may run, and why
/// it judged nothing (PUBLISHING-29).
pub(super) fn too_long(limit: Duration) -> (String, String) {
    let limit = super::spell(limit);
    (
        format!("obid stopped it after {limit}, the most a test run may take."),
        format!("the check ran past its limit of {limit}"),
    )
}

/// Ends once the check whose process group is `pgid` has made no progress for `quiet`: nothing
/// new in `heard` (the bytes it wrote) and no CPU used by its group, a thousandth of a core at
/// least (PUBLISHING-29). A reading that fails counts as progress. Then [`too_long`]'s pair for it.
pub(super) async fn idle(pgid: u32, heard: &AtomicUsize, quiet: Duration) -> (String, String) {
    let every = (quiet / 15).max(Duration::from_millis(50));
    let (mut said, mut cpu, mut since) = (0, None, tokio::time::Instant::now());
    loop {
        tokio::time::sleep(every).await;
        let now = (
            heard.load(Ordering::Relaxed),
            crate::meter::group_cpu(pgid).await,
        );
        let used = match (cpu, now.1) {
            (Some(was), Some(is)) => u128::from(is.abs_diff(was)) >= every.as_nanos() / 1000,
            _ => true,
        };
        if used || now.0 != said {
            since = tokio::time::Instant::now();
        }
        (said, cpu) = now;
        if since.elapsed() >= quiet {
            let quiet = super::spell(quiet);
            return (
                format!("obid stopped it: it did nothing for {quiet}."),
                format!("the check did nothing for {quiet}"),
            );
        }
    }
}

/// The job's reason when its check judged nothing, for the owner's Publish by hand (PUBLISHING-8).
pub(super) fn unchecked(why: impl std::fmt::Display) -> String {
    format!(
        "Not published: {why}, so nothing counts as checked. Read its change, then Publish it by hand."
    )
}

/// Why the failed check of `command` judged nothing, from its `output`: a line that starts
/// `couldn't judge: <why>` (trimmed) first, its words cleaned and cut on a second line, since
/// the first is obid's own (job_report hands it to the brain as such); else PUBLISHING-17's signs.
pub(super) fn why(command: &str, output: &str) -> Option<String> {
    let judge = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("couldn't judge: "));
    if let Some(why) = judge {
        // Bidirectional overrides and isolates go too: these are someone else's words on a card.
        let why: String = why
            .chars()
            .filter(|c| !matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
            .collect();
        let why = crate::github_work::clean(&why, 200);
        return Some(unchecked("the check couldn't judge this change") + "\n" + &why);
    }
    environment_signature(command, output).map(|label| {
        unchecked(format!(
            "{label}, which is this Mac's doing, not the change's"
        ))
    })
}

/// Why a failed check of `command` shows, in its `output`, that the machine failed, not the code
/// (PUBLISHING-17): a host that didn't resolve, a registry it couldn't fetch from, a compiler that
/// doesn't match what built its dependencies, or a step whose own first word isn't installed.
/// Each sign starts a line of what the check printed (trimmed), so a source line a compiler or
/// linter quotes (`NN |`) or a diff's `+`/`-` line never matches.
/// ponytail: a short list of the common toolchains' words; a tool that words it otherwise still
/// goes back to the agent. Add a line here when one shows up in a real check.
pub(super) fn environment_signature(command: &str, output: &str) -> Option<&'static str> {
    const HOST: &str = "the check couldn't resolve a host";
    const FETCH: &str = "the check couldn't fetch a dependency";
    const COMPILER: &str = "its compiler doesn't match the one that built its dependencies";
    // A line that starts with the first and holds the second.
    const SIGNS: &[(&str, &str, &str)] = &[
        ("fatal: unable to access", "Could not resolve host", HOST),
        ("curl: (6) Could not resolve host", "", HOST),
        ("Error: getaddrinfo ENOTFOUND", "", HOST),
        ("Error: getaddrinfo EAI_AGAIN", "", HOST),
        ("npm ERR! code ENOTFOUND", "", HOST),
        ("npm error code ENOTFOUND", "", HOST),
        ("npm ERR! code EAI_AGAIN", "", HOST),
        ("npm error code EAI_AGAIN", "", HOST),
        ("npm ERR! code ENOTCACHED", "", FETCH),
        ("npm error code ENOTCACHED", "", FETCH),
        ("ERR_PNPM_NO_OFFLINE", "", FETCH),
        // Yarn Classic's `--offline`, and uv's (`Caused by: ` to 0.12, `cause: ` since 0.13).
        ("error Can't make a request in offline mode", "", FETCH),
        (
            "error Couldn't find any versions for",
            "in our cache",
            FETCH,
        ),
        (
            "Caused by: Network connectivity is disabled",
            "found in the cache",
            FETCH,
        ),
        (
            "cause: Network connectivity is disabled",
            "found in the cache",
            FETCH,
        ),
        ("error[E0514]", "", COMPILER),
    ];
    // What cargo says caused a `failed to get` when it was the network: a bad `path` isn't.
    const CARGO_OFFLINE: &[&str] = &[
        "attempting to make an HTTP request, but --offline was specified",
        "[6] Couldn't resolve host name",
        "[7] Couldn't connect to server",
        "spurious network error",
    ];
    let lines: Vec<&str> = output.lines().map(str::trim).collect();
    let starts = |sign: &str| lines.iter().any(|line| line.starts_with(sign));
    if let Some(&(_, _, label)) = SIGNS.iter().find(|(sign, holds, _)| {
        lines
            .iter()
            .any(|line| line.starts_with(sign) && line.contains(holds))
    }) {
        return Some(label);
    }
    if starts("error: failed to get `") && CARGO_OFFLINE.iter().any(|cause| starts(cause)) {
        return Some(FETCH);
    }
    // Offline, a crate the job's cache never got isn't in the index cargo has (M1's run: a Cursor
    // job whose agent never built): its resolution fails before anything compiles.
    if starts("error: no matching package named `")
        && starts("note: offline mode (via `--offline`)")
    {
        return Some(FETCH);
    }
    // `/bin/sh: cargo: command not found` (bash) or `/bin/sh: 1: cargo: not found` (dash), for
    // the first word of one of its steps: not a pipe's second command, a `$(…)` or a recipe's.
    let firsts: Vec<String> = crate::facts::steps(command)
        .iter()
        .filter_map(|step| step.split_whitespace().find(|word| !word.contains('=')))
        .map(str::to_owned)
        .collect();
    lines
        .iter()
        .filter_map(|line| line.strip_prefix("/bin/sh: "))
        .filter_map(|rest| {
            rest.strip_suffix(": command not found")
                .or_else(|| rest.strip_suffix(": not found"))
        })
        .filter_map(|said| said.rsplit(": ").next())
        .any(|name| firsts.iter().any(|first| first == name))
        .then_some("a command its check runs isn't installed")
}
