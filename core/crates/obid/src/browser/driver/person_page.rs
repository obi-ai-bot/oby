//! The person's ways out of a page state (BROWSER-26, board 10): Reload (Try again), Go back, and
//! the page's own dialog answered, Leave or Stay, run on the tab they hold, then the page read as
//! it is after (a reload on the browser's error page says it worked and still shows a
//! `chrome-error://` URL, so the URL is what counts) and told to the core, the agent's refs dead.
//! Who may (the holder of the tab) is the API's and the core's to check first. It owns no state and
//! writes nothing.

use std::time::Duration;

use tokio::time::Instant;

use super::{Call, Driver, Session};
use crate::browser::contract as c;
use crate::browser::engine::{Refusal, is_error_page};

/// How long the person's own command may take: a reload of a slow page included.
const PERSON_LIMIT: Duration = Duration::from_secs(30);
/// How long an accepted dialog may take to send the page on (a confirm's `location.href`).
const AFTER_ACCEPT: Duration = Duration::from_millis(1500);
const AFTER_ACCEPT_EVERY: Duration = Duration::from_millis(100);
/// How long the page may take to answer again after a move that navigated under agent-browser.
const AFTER_MOVE: Duration = Duration::from_secs(5);

/// agent-browser's words when the page navigated while it was still asking it (its own
/// `Runtime.evaluate` for Back or Reload): the page moved, the answer was lost (BROWSER-26).
fn navigated_under(e: &str) -> bool {
    e.contains("Inspected target navigated or closed")
        || e.contains("Execution context was destroyed")
}

impl Driver {
    /// Runs the person's way out `verb` (the contract's `reload`, `back` or `dialog`, `accept` and
    /// `prompt_text` for a dialog) on `tab`, whose browser target is `target`, in `job`'s browser.
    /// Answers the page's URL and title after, or why it didn't get there: a page that still shows
    /// the browser's error page is a failure, in words that say so.
    pub async fn person_page(
        &self,
        job: &str,
        tab: &str,
        target: &str,
        verb: &str,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> Result<(String, Option<String>), Refusal> {
        let session = self.session_of(job).ok_or_else(|| {
            Refusal::Browser(
                "This job's browser isn't running: it starts again on its agent's next call."
                    .into(),
            )
        })?;
        let limit = self.options.system_limit;
        let read = |call: Call| match call {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(Refusal::Browser(e)),
            Call::TimedOut(ms) => Err(Refusal::TimedOut { ms }),
        };
        let argv: Vec<String> = match verb {
            c::VERB_RELOAD => vec!["reload".into()],
            c::VERB_BACK => vec!["back".into()],
            c::VERB_DIALOG if accept => {
                let mut v = vec!["dialog".into(), "accept".into()];
                if let Some(p) = prompt_text {
                    v.push(p.into());
                }
                v
            }
            c::VERB_DIALOG => vec!["dialog".into(), "dismiss".into()],
            other => {
                return Err(Refusal::BadArgs(format!(
                    "a way out: reload, back or dialog, not {other}"
                )));
            }
        };
        let (was_certificate, known_url) = {
            let core = self.core.lock().unwrap();
            let t = core.tab(tab);
            (
                t.is_some_and(|t| t.certificate),
                t.map(|t| t.url.clone()).unwrap_or_default(),
            )
        };
        // The tab brought to the front only when it isn't: switching tabs waits behind a dialog,
        // listing them doesn't.
        let tabs = read(session.call(&["tab".into(), "list".into()], limit).await)?;
        let active = tabs["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["active"] == true)
            .and_then(|t| t["targetId"].as_str());
        if active != Some(target) {
            read(
                session
                    .call(&["tab".into(), target.to_string()], limit)
                    .await,
            )?;
        }
        match session.call(&argv, PERSON_LIMIT).await {
            // The page navigated under agent-browser's own call: the move happened, and the URL
            // read below says where the page is.
            Call::Failed(e)
                if matches!(verb, c::VERB_RELOAD | c::VERB_BACK) && navigated_under(&e) =>
            {
                tracing::info!(
                    "the person's {verb} on tab {tab}: the page navigated under it ({e})"
                );
            }
            other => {
                read(other)?;
            }
        }
        if verb == c::VERB_DIALOG {
            self.core.lock().unwrap().dialog(tab, None);
            if accept {
                // An accepted dialog may send the page on: give it a moment to start.
                self.wait_for_another_url(&session, &known_url).await;
            }
        }
        let url = self.url_after_move(&session, limit).await?;
        let title = match session.call(&["get".into(), "title".into()], limit).await {
            Call::Ok(v) => v["title"].as_str().map(str::to_string),
            _ => None,
        };
        self.core
            .lock()
            .unwrap()
            .person_moved(tab, &url, title.as_deref());
        if is_error_page(&url) {
            let what = if was_certificate {
                "the page still stops at a certificate problem (net::ERR_CERT_*)"
            } else {
                "the page still doesn't load: the browser shows its own error page"
            };
            return Err(Refusal::Browser(format!("{verb}: {what}.")));
        }
        Ok((url, title))
    }

    /// The page's URL after the person's move: read again, up to [`AFTER_MOVE`], while the page
    /// is still landing (agent-browser answering that it navigated under it).
    async fn url_after_move(&self, session: &Session, limit: Duration) -> Result<String, Refusal> {
        let until = Instant::now() + AFTER_MOVE;
        loop {
            match session.call(&["get".into(), "url".into()], limit).await {
                Call::Ok(v) => return Ok(v["url"].as_str().unwrap_or_default().to_string()),
                Call::Failed(e) if navigated_under(&e) && Instant::now() < until => {
                    tokio::time::sleep(AFTER_ACCEPT_EVERY).await;
                }
                Call::Failed(e) => return Err(Refusal::Browser(e)),
                Call::TimedOut(ms) => return Err(Refusal::TimedOut { ms }),
            }
        }
    }

    /// Up to [`AFTER_ACCEPT`], until the page's URL is another than `before`.
    async fn wait_for_another_url(&self, session: &Session, before: &str) {
        let until = Instant::now() + AFTER_ACCEPT;
        while Instant::now() < until {
            if let Call::Ok(v) = session
                .call(
                    &["get".into(), "url".into()],
                    self.options.system_limit.min(AFTER_ACCEPT),
                )
                .await
                && v["url"].as_str().is_some_and(|u| u != before)
            {
                return;
            }
            tokio::time::sleep(AFTER_ACCEPT_EVERY).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    use super::super::{Binaries, Options, stand_in};
    use super::*;
    use crate::browser::engine::{Actor, Browsers};

    fn driver(home: &Path) -> Arc<Driver> {
        let installed = stand_in::install(home);
        let options = Options::new(home.to_path_buf(), Binaries::InPlace(installed));
        Arc::new(Driver::new(options, Arc::new(Mutex::new(Browsers::new()))))
    }

    async fn tool(d: &Driver, home: &Path, job: &str, name: &str, args: Value) -> Value {
        let begun = d.core().lock().unwrap().begin(
            &Actor::Agent(job.into()),
            name,
            &args,
            &home.join("room"),
            d.now_ms(),
        );
        match begun {
            Ok(ticket) => d.run(job, &ticket).await,
            Err(r) => r.to_json(),
        }
    }

    #[tokio::test]
    async fn browser_27_the_page_as_read_right_before_an_act_is_checked_and_a_certificate_failure_marks_the_tab()
     {
        let home = stand_in::scratch("certificate");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        tool(
            &d,
            &home,
            "job-a",
            "oby_browser_open",
            json!({"url": "http://127.0.0.1:1/"}),
        )
        .await;
        // The page became the browser's error page by itself: the evaluate is refused before it
        // runs, the URL read right before it.
        let session = d.session_of("job-a").unwrap();
        let url_file = session.socket_dir.join(format!("{}.url", session.name));
        std::fs::write(&url_file, "chrome-error://chromewebdata/\n").unwrap();
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_evaluate",
            json!({"expression": "1"}),
        )
        .await;
        assert_eq!(v["error"]["code"], "certificate_page", "{v}");
        assert!(
            !stand_in::calls(&home, "job-a")
                .iter()
                .any(|c| c.contains(" eval ")),
            "the evaluate never reached agent-browser"
        );
        std::fs::write(&url_file, "http://127.0.0.1:1/\n").unwrap();
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_evaluate",
            json!({"expression": "1"}),
        )
        .await;
        assert_eq!(v["ok"], true, "{v}");
        // A navigation that stops at a certificate problem marks the tab.
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_navigate",
            json!({"url": "https://127.0.0.1:1/cert"}),
        )
        .await;
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("ERR_CERT_"),
            "{v}"
        );
        assert!(d.core().lock().unwrap().tab("tab-1").unwrap().certificate);
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_evaluate",
            json!({"expression": "certificateErrorPageController.proceed()"}),
        )
        .await;
        assert_eq!(v["error"]["code"], "certificate_page", "{v}");
        // The person's reload while it still stops there: refused in words that say so.
        let r = d
            .person_page("job-a", "tab-1", "T1", c::VERB_RELOAD, false, None)
            .await
            .unwrap_err();
        assert_eq!(r.code(), "browser");
        assert!(r.message().contains("certificate"), "{}", r.message());
        assert!(d.core().lock().unwrap().tab("tab-1").unwrap().certificate);
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_26_reload_back_and_a_dialog_answered_move_the_page_for_the_person() {
        let home = stand_in::scratch("ways-out");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        tool(
            &d,
            &home,
            "job-a",
            "oby_browser_open",
            json!({"url": "http://127.0.0.1:1/"}),
        )
        .await;
        tool(
            &d,
            &home,
            "job-a",
            "oby_browser_navigate",
            json!({"url": "http://127.0.0.1:1/second"}),
        )
        .await;
        tool(&d, &home, "job-a", "oby_browser_snapshot", json!({})).await;
        let (url, title) = d
            .person_page("job-a", "tab-1", "T1", c::VERB_BACK, false, None)
            .await
            .unwrap();
        assert_eq!(url, "http://127.0.0.1:1/");
        assert_eq!(title.as_deref(), Some("Fixture"));
        {
            let core = d.core().lock().unwrap();
            let t = core.tab("tab-1").unwrap();
            assert_eq!(t.url, "http://127.0.0.1:1/");
            assert!(t.refs.is_none(), "the agent's refs died");
        }
        let (url, _) = d
            .person_page("job-a", "tab-1", "T1", c::VERB_RELOAD, false, None)
            .await
            .unwrap();
        assert_eq!(url, "http://127.0.0.1:1/");
        d.core()
            .lock()
            .unwrap()
            .dialog("tab-1", Some(json!("Leave?")));
        d.person_page("job-a", "tab-1", "T1", c::VERB_DIALOG, false, None)
            .await
            .unwrap();
        assert!(
            d.core()
                .lock()
                .unwrap()
                .tab("tab-1")
                .unwrap()
                .dialog
                .is_none()
        );
        let calls = stand_in::calls(&home, "job-a");
        for want in ["back", "reload", "dialog dismiss"] {
            assert!(calls.iter().any(|c| c.ends_with(want)), "{want}: {calls:?}");
        }
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_26_a_back_or_reload_the_page_navigated_under_still_moves_the_page() {
        let home = stand_in::scratch("navclosed");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        tool(
            &d,
            &home,
            "job-a",
            "oby_browser_open",
            json!({"url": "http://127.0.0.1:1/"}),
        )
        .await;
        tool(
            &d,
            &home,
            "job-a",
            "oby_browser_navigate",
            json!({"url": "http://127.0.0.1:1/second"}),
        )
        .await;
        // agent-browser's back and reload answer "Inspected target navigated or closed" when the
        // page navigates under its own evaluate: the page moved all the same.
        let session = d.session_of("job-a").unwrap();
        std::fs::write(
            session
                .socket_dir
                .join(format!("{}.navclosed", session.name)),
            "",
        )
        .unwrap();
        let (url, _) = d
            .person_page("job-a", "tab-1", "T1", c::VERB_BACK, false, None)
            .await
            .unwrap();
        assert_eq!(url, "http://127.0.0.1:1/");
        let (url, _) = d
            .person_page("job-a", "tab-1", "T1", c::VERB_RELOAD, false, None)
            .await
            .unwrap();
        assert_eq!(url, "http://127.0.0.1:1/");
        // Any other failure is still the browser's refusal.
        let refused = d
            .person_page("job-a", "tab-1", "T1", "nope", false, None)
            .await
            .unwrap_err();
        assert_eq!(refused.code(), "bad_args");
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }
}
