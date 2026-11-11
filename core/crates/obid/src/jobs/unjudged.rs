//! Why a failed check judged nothing: a check that says so (PUBLISHING-22), the machine's
//! failure, not the code's (PUBLISHING-17), obid stopping one that did nothing or ran too long
//! (PUBLISHING-29), or no checkout to run it in (PUBLISHING-8); and how obid runs it again by
//! itself until one judges, or for 24 hours, then ends the job with Publish on its page and no
//! card or alert (PUBLISHING-31).

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// How a reason [`unchecked`] made ends its first line: obid runs such a check again.
const UNJUDGED: &str = ", so nothing counts as checked.";

/// The first wait before a check that judged nothing runs again: `OBID_RECHECK_MS`, else 10
/// minutes. Each next one doubles, at most 24 of these apart, and obid stops trying once its waits
/// would pass 144 of these (24 hours); 0 runs none again (tests).
fn first_wait() -> Duration {
    let ms = std::env::var("OBID_RECHECK_MS")
        .ok()
        .and_then(|v| v.parse().ok());
    Duration::from_millis(ms.unwrap_or(10 * 60 * 1000))
}

/// The wait before the next run of a check whose `tries` runs (one at least) judged nothing, from
/// `first`; none once the waits would pass 144 `first`s (PUBLISHING-31).
pub(super) fn next_wait(tries: u32, first: Duration) -> Option<Duration> {
    let waits = || std::iter::successors(Some(first), move |w| Some((*w * 2).min(first * 24)));
    let waited: Duration = waits().take(tries as usize).sum();
    (waited < first * 144).then(|| waits().nth(tries as usize - 1).unwrap_or(first))
}

/// The most a check may run: `OBID_CHECK_LIMIT_SECONDS` (tests), else [`CHECK_LIMIT`].
pub(super) fn limit() -> Duration {
    std::env::var("OBID_CHECK_LIMIT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(CHECK_LIMIT, Duration::from_secs)
}

impl Jobs {
    /// Runs the repository's check on commit `head` ([`Jobs::check_once`]), and again by itself
    /// while it judges nothing (PUBLISHING-31): with no card and no alert, the job's line saying
    /// why and when, its slot free meanwhile, the next run's time kept on the job (`unjudged`) so a
    /// restart waits it out. A run that judges ends it as any check does; a stop ends the wait.
    /// After 24 hours of runs, the last one's, its reason saying obid stopped trying. Not for a
    /// review, which answers with what it has.
    pub(super) async fn check(
        self: &Arc<Self>,
        id: &str,
        command: &str,
        head: &str,
        spec: &Spec,
        signal: &watch::Receiver<Signal>,
    ) -> Result<CheckRun, String> {
        let now = obi_protocol::time::now;
        // A wait a restart cut short goes on to its time; one obid stopped trying is over.
        let kept = self.get(id).and_then(|job| job.unjudged);
        let mut wait = kept
            .as_ref()
            .and_then(|u| u.next)
            .map(|at| Duration::from_secs(u64::try_from(at - now()).unwrap_or(0)));
        if kept.is_some_and(|u| u.next.is_none()) {
            self.update(id, |job| job.unjudged = None);
        }
        loop {
            if let Some(wait) = wait.take() {
                if let Some(u) = self.get(id).and_then(|j| j.unjudged) {
                    self.update(id, |job| job.step = waiting(&u.why, wait));
                }
                let idle = slots::Idle::new(self, id);
                let mut stop = signal.clone();
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    _ = stop.wait_for(|s| *s != Signal::Run) => return Ok(CheckRun::default()),
                }
                idle.back(signal).await;
                if *signal.borrow() != Signal::Run {
                    return Ok(CheckRun::default());
                }
                self.update(id, |job| job.step = format!("Running {command}"));
            }
            let mut run = self.check_once(id, command, head, spec, signal).await?;
            let Some(job) = self.get(id).filter(|_| *signal.borrow() == Signal::Run) else {
                return Ok(run);
            };
            let reason = run.unchecked.clone().unwrap_or_default();
            let first = reason.lines().next().unwrap_or_default();
            let unjudged = first
                .strip_suffix(UNJUDGED)
                .filter(|_| job.kind != "review");
            let Some(why) = unjudged else {
                if job.unjudged.is_some() {
                    self.update(id, |job| job.unjudged = None);
                }
                return Ok(run);
            };
            let why = why
                .strip_prefix("Not published: ")
                .unwrap_or(why)
                .to_owned();
            let tries = job.unjudged.map_or(0, |u| u.tries) + 1;
            let next = next_wait(tries, first_wait());
            let at = next.map(|w| now() + i64::try_from(w.as_secs()).unwrap_or(i64::MAX / 2));
            tracing::info!("job {id}: its check judged nothing ({why}), try {tries}");
            // Its line with it, at once: the job never shows the wait without why.
            self.update(id, |job| {
                if let Some(next) = next {
                    job.step = waiting(&why, next);
                }
                job.unjudged = Some(crate::job::Unjudged {
                    tries,
                    next: at,
                    why: why.clone(),
                })
            });
            if next.is_none() {
                let rest = reason.strip_prefix(first).unwrap_or_default();
                run.unchecked = Some(format!("{first}{GAVE_UP}{rest}"));
                return Ok(run);
            }
            wait = next;
        }
    }
}

/// What a check's reason says once obid stopped running it again (PUBLISHING-31).
const GAVE_UP: &str = " Oby ran it again by itself for 24 hours and none of its runs judged it: Publish it from its page once you've read its change.";

/// The job's line while it waits `wait` to run a check that judged nothing because of `why`.
fn waiting(why: &str, wait: Duration) -> String {
    let minutes = Duration::from_secs(wait.as_secs().div_ceil(60) * 60);
    format!(
        "Its check judged nothing: {why}. Oby runs it again in {}.",
        super::spell(minutes)
    )
}

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

/// The job's reason when its check judged nothing (PUBLISHING-8), which [`Jobs::check`] runs
/// again (PUBLISHING-31).
pub(super) fn unchecked(why: impl std::fmt::Display) -> String {
    format!("Not published: {why}{UNJUDGED}")
}

/// The job's reason when its check judged, but not the change (PUBLISHING-18): it isn't run again,
/// and waits for the owner's Publish by hand.
pub(super) fn by_hand(why: impl std::fmt::Display) -> String {
    unchecked(why) + " Read its change, then Publish it by hand."
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PUBLISHING-31: 10 minutes, then twice the last, at most 4 hours apart; none once the waits
    /// would pass 24 hours; none at all from 0. The line rounds a wait up to whole minutes.
    #[test]
    fn publishing_31_waits_double_to_4_hours_for_24_hours() {
        let minutes = |m: u64| Duration::from_secs(m * 60);
        let first = minutes(10);
        let waits: Vec<_> = (1..).map_while(|tries| next_wait(tries, first)).collect();
        let expected = [10, 20, 40, 80, 160, 240, 240, 240, 240].map(minutes);
        assert_eq!(waits, expected);
        assert!(waits.iter().sum::<Duration>() <= minutes(24 * 60));
        assert_eq!(next_wait(1, Duration::ZERO), None);
        assert_eq!(
            waiting(
                "the check did nothing for 15 minutes",
                Duration::from_secs(301)
            ),
            "Its check judged nothing: the check did nothing for 15 minutes. Oby runs it again in 6 minutes."
        );
    }
}
