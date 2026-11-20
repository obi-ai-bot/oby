//! Bringing a job's browser up before its first command (BROWSER-2): who starts it, the attempt
//! itself behind [`Launch`], and [`browser_up`], which gives the start its own allowance apart from
//! the command's deadline. It owns no state of its own: the job's browser state is [`Api`]'s, set
//! through [`Api::claim_start`] and [`Api::set_browser`]. It waits on tokio's clock, so a test
//! moves a paused clock instead of sleeping.

use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::{Api, Refused};
use crate::browser::contract as c;

/// Who starts a job's browser ([`Api::claim_start`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    Up,
    Mine,
    Wait,
    NoJob,
}

/// One attempt at starting a job's browser (launching Chrome for its profile): obid's driver, or a
/// test's stub. An implementation may write `async fn launch`.
pub trait Launch: Sync {
    fn launch(&self, job: &str) -> impl Future<Output = Result<(), String>> + Send;
}

fn still_starting(after: Duration) -> Refused {
    Refused::new(
        409,
        "browser_starting",
        format!(
            "The browser is still starting after {} s (the Mac may be busy). Nothing ran; try again in a moment.",
            after.as_secs()
        ),
    )
}

/// Brings a job's browser up before its command runs, within `allowance`, apart from the
/// command's own deadline, which counts only once this answered. The first caller launches
/// (again after a failed attempt, until the allowance passes); the others wait for it. Past the
/// allowance the answer is `browser_starting`, never a bare timeout. `changed` hands each new state
/// to whoever shows it.
pub async fn browser_up<L: Launch>(
    api: &Mutex<Api>,
    launcher: &L,
    job: &str,
    allowance: Duration,
    changed: &(dyn Fn() + Sync),
) -> Result<(), Refused> {
    let started = Instant::now();
    let until = started + allowance;
    let claim = api.lock().unwrap().claim_start(job);
    changed();
    match claim {
        Start::Up => return Ok(()),
        Start::NoJob => return Err(Refused::new(401, "bad_token", "No such job.")),
        Start::Wait => loop {
            let state = api
                .lock()
                .unwrap()
                .job(job)
                .map(|j| (j.browser, j.browser_reason.clone()));
            match state {
                Some((c::BrowserState::Ready, _)) => return Ok(()),
                Some((c::BrowserState::Failed, why)) => {
                    return Err(Refused::new(
                        409,
                        "browser",
                        why.unwrap_or_else(|| "The browser didn't start.".into()),
                    ));
                }
                _ if Instant::now() >= until => return Err(still_starting(started.elapsed())),
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        },
        Start::Mine => {}
    }
    let outcome = loop {
        let left = until.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, launcher.launch(job)).await {
            Ok(Ok(())) => break Ok(()),
            // A failed attempt is tried again while the allowance lasts.
            Ok(Err(e)) if Instant::now() + Duration::from_millis(250) >= until => {
                break Err(Refused::new(
                    409,
                    "browser",
                    format!("The browser didn't start: {e}"),
                ));
            }
            Ok(Err(_)) => tokio::time::sleep(Duration::from_millis(250)).await,
            Err(_) => break Err(still_starting(started.elapsed())),
        }
    };
    {
        let mut a = api.lock().unwrap();
        match &outcome {
            Ok(()) => a.set_browser(job, c::BrowserState::Ready, None),
            Err(e) => a.set_browser(job, c::BrowserState::Failed, Some(e.message.clone())),
        }
    }
    changed();
    outcome
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A stub launch: sleeps `ms` (a busy Mac's Chrome), then is up; counts its attempts.
    struct SlowLaunch {
        ms: u64,
        attempts: AtomicU32,
    }

    impl Launch for SlowLaunch {
        async fn launch(&self, _job: &str) -> Result<(), String> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(self.ms)).await;
            Ok(())
        }
    }

    fn slow(ms: u64) -> SlowLaunch {
        SlowLaunch {
            ms,
            attempts: AtomicU32::new(0),
        }
    }

    fn api_with_job() -> Mutex<Api> {
        let api = Mutex::new(Api::new());
        api.lock().unwrap().add_job("j1", "claude");
        api
    }

    #[tokio::test(start_paused = true)]
    async fn browser_2_a_slow_browser_start_has_its_own_allowance_and_the_command_deadline_counts_after_it()
     {
        // Scaled down 100 times: a 15 s command deadline, a launch that takes 25 s, 60 s allowed.
        let (deadline, allowance) = (Duration::from_millis(150), Duration::from_millis(600));
        let api = api_with_job();
        let launch = slow(250);
        let started = Instant::now();
        browser_up(&api, &launch, "j1", allowance, &|| ())
            .await
            .unwrap();
        // The launch took longer than the command's deadline...
        assert!(started.elapsed() > deadline, "{:?}", started.elapsed());
        assert_eq!(
            api.lock().unwrap().job("j1").map(|j| j.browser),
            Some(c::BrowserState::Ready)
        );
        // ...and the command, its deadline counted only now, still runs within it.
        let command = tokio::time::sleep(Duration::from_millis(20));
        assert!(tokio::time::timeout(deadline, command).await.is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn browser_2_a_browser_still_starting_past_its_allowance_says_so_never_a_bare_timeout() {
        let api = api_with_job();
        let e = browser_up(&api, &slow(1000), "j1", Duration::from_millis(200), &|| ())
            .await
            .unwrap_err();
        assert_eq!(e.reason, "browser_starting");
        assert!(e.message.contains("still starting"), "{}", e.message);
        let a = api.lock().unwrap();
        let kinds: Vec<_> = a
            .events_after(0)
            .into_iter()
            .filter_map(|e| match e.body {
                c::EventBody::Browser { browser } => Some(browser.state),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, [c::BrowserState::Starting, c::BrowserState::Failed]);
        assert_eq!(a.tabs().browsers[0].state, c::BrowserState::Failed);
    }

    #[tokio::test(start_paused = true)]
    async fn browser_2_one_caller_starts_the_browser_and_the_others_wait_for_it() {
        let api = api_with_job();
        let launch = slow(200);
        let allowance = Duration::from_millis(2000);
        let (a, b) = tokio::join!(
            browser_up(&api, &launch, "j1", allowance, &|| ()),
            browser_up(&api, &launch, "j1", allowance, &|| ())
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(launch.attempts.load(Ordering::SeqCst), 1);
        // Up: later calls don't launch again.
        browser_up(&api, &launch, "j1", allowance, &|| ())
            .await
            .unwrap();
        assert_eq!(launch.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(
            api.lock().unwrap().job_browser("j1").unwrap().state,
            c::BrowserState::Ready
        );
    }

    #[tokio::test(start_paused = true)]
    async fn browser_2_a_failed_attempt_is_tried_again_within_the_allowance() {
        struct FailsOnce(AtomicU32);
        impl Launch for FailsOnce {
            async fn launch(&self, _job: &str) -> Result<(), String> {
                match self.0.fetch_add(1, Ordering::SeqCst) {
                    0 => Err("daemon not ready".into()),
                    _ => Ok(()),
                }
            }
        }
        let api = api_with_job();
        let launch = FailsOnce(AtomicU32::new(0));
        browser_up(&api, &launch, "j1", Duration::from_secs(2), &|| ())
            .await
            .unwrap();
        assert_eq!(launch.0.load(Ordering::SeqCst), 2);
    }
}
