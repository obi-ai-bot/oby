//! Running one agent's command (BROWSER-9 to 12, 27): the core's [`Ticket`] checked right before it
//! runs (the tab's URL read first when it names refs, so a ref the page navigated away from never
//! reaches agent-browser, and when it works the page, so the browser's own error page is never the
//! agent's to act on; never while the page shows a dialog, whose URL agent-browser can't read until
//! it is answered), run under its own deadline, killed at it and never run again; its tab then
//! settles in the background (`get url` tried until the browser answers) and the core hears
//! `settled`. A finished command's answer is shaped as the lab's `shape`: page words under
//! `outside`. A failed one answers `browser` with agent-browser's words and its tab, so the API
//! marks it failed (BROWSER-12); a navigation that stopped at a certificate problem marks its tab
//! for the core (BROWSER-27).
//!
//! It owns no state of its own: the core's is told through [`Browsers`]'s methods, and the
//! settling tasks are [`Driver`]'s. It writes nothing; agent-browser writes a screenshot into the
//! agent's room when asked.
//!
//! [`Browsers`]: crate::browser::engine::Browsers

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::Instant;

use super::{Call, Driver, Session};
use crate::browser::engine::{Refusal, Ticket, tool, works_the_page};

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|w| w.to_string()).collect()
}

impl Driver {
    /// What is left of a ticket's deadline.
    fn left(&self, ticket: &Ticket) -> Duration {
        Duration::from_millis(ticket.deadline_ms.saturating_sub(self.now_ms()))
    }

    /// obid's own read around a command: its data, or why it failed.
    async fn system(
        &self,
        session: &Session,
        args: &[&str],
        limit: Duration,
    ) -> Result<Value, String> {
        match session.call(&words(args), limit).await {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(e),
            Call::TimedOut(ms) => Err(format!(
                "agent-browser {} took over {ms} ms",
                args.join(" ")
            )),
        }
    }

    /// Runs a ticket the core let start, for `job`'s browser, and answers the agent.
    pub async fn run(&self, job: &str, ticket: &Ticket) -> Value {
        let started = Instant::now();
        let tab = ticket.tab.as_deref();
        let refuse = |r: Refusal| Self::answer_refused(r, started, ticket.cmd, tab);
        let failed = |r: Refusal| {
            self.core.lock().unwrap().finished(ticket, false);
            refuse(r)
        };
        let Some(session) = self.session_of(job) else {
            return failed(Refusal::Browser(
                "This job's browser isn't running: it starts again on its next call.".into(),
            ));
        };
        // Before it runs, nothing of it has happened: a read that runs out of time is `expired`.
        let before = |call: Call| match call {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(Refusal::Browser(e)),
            Call::TimedOut(_) => Err(Refusal::Expired),
        };
        if let Some(target) = &ticket.target {
            let tabs = match before(
                session
                    .call(&words(&["tab", "list"]), self.left(ticket))
                    .await,
            ) {
                Ok(v) => v,
                Err(r) => return failed(r),
            };
            let active = tabs["tabs"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["active"] == true);
            if active.and_then(|t| t["targetId"].as_str()) != Some(target.as_str())
                && let Err(r) = before(
                    session
                        .call(&words(&["tab", target]), self.left(ticket))
                        .await,
                )
            {
                return failed(r);
            }
        }
        // Right before it runs: the page as it is now, for the refs it names and, for a tool that
        // works the page, whether it is the browser's own error page (`page_allows`). Never while
        // the page shows a dialog: reading its URL waits for the dialog.
        let works = tool(ticket.tool).is_some_and(works_the_page);
        let dialog_open = tab.is_some_and(|id| {
            self.core
                .lock()
                .unwrap()
                .tab(id)
                .is_some_and(|t| t.dialog.is_some())
        });
        let url_now = if ticket.refs.is_empty() && !(works && !dialog_open) {
            None
        } else {
            match before(
                session
                    .call(&words(&["get", "url"]), self.left(ticket))
                    .await,
            ) {
                Ok(v) => v["url"].as_str().map(str::to_string),
                Err(r) => return failed(r),
            }
        };
        // A trusted key on another host than the one it was trusted for (BROWSER-29).
        if let (Some(u), Some(tab)) = (&url_now, tab)
            && let Some((_, seen)) = self.trusted_elsewhere(&session, u).await
        {
            self.core.lock().unwrap().certificate_seen(tab, seen, u);
        }
        let may = self
            .core
            .lock()
            .unwrap()
            .may_start(ticket, url_now.as_deref(), self.now_ms());
        if let Err(r) = may {
            return failed(r);
        }
        if ticket.tool == "oby_browser_screenshot"
            && let Some(folder) = ticket.argv.get(1).and_then(|p| Path::new(p).parent())
            && let Err(e) = crate::store::make_private_dir(folder)
        {
            // agent-browser writes the PNG but doesn't make its folder.
            return failed(Refusal::Browser(format!(
                "obid couldn't make the screenshot's folder: {e}"
            )));
        }
        match session.call(&ticket.argv, self.left(ticket)).await {
            Call::TimedOut(ms) => {
                // It may have partly happened: never run again, and its tab settles first.
                self.core.lock().unwrap().timed_out(ticket);
                if let Some(tab) = tab {
                    self.settle(job, session, tab);
                }
                refuse(Refusal::TimedOut { ms })
            }
            Call::Failed(e) => {
                if matches!(ticket.tool, "oby_browser_open" | "oby_browser_navigate")
                    && let Some(tab) = tab
                {
                    self.core.lock().unwrap().navigation_failed(tab, &e);
                    let url = ticket.argv.iter().find(|a| a.starts_with("https://"));
                    self.certificate_stopped(tab, url, &e).await;
                }
                failed(Refusal::Browser(e))
            }
            Call::Ok(data) => {
                let shaped = self.shape(&session, ticket, data).await;
                self.core.lock().unwrap().finished(ticket, true);
                // Landed on a trusted key's other host: stopped there (BROWSER-29).
                let landed = shaped.as_ref().ok().and_then(|r| {
                    Some((
                        r["tab"]
                            .as_str()
                            .map(str::to_string)
                            .or(ticket.tab.clone())?,
                        r["url"].as_str()?.to_string(),
                    ))
                });
                if matches!(ticket.tool, "oby_browser_open" | "oby_browser_navigate")
                    && let Some((tab, url)) = landed
                    && let Some((host, seen)) = self.trusted_elsewhere(&session, &url).await
                {
                    self.core.lock().unwrap().certificate_seen(&tab, seen, &url);
                    return Self::answer_refused(
                        Refusal::TrustedElsewhere(host),
                        started,
                        ticket.cmd,
                        Some(&tab),
                    );
                }
                match shaped {
                    Ok(result) => json!({
                        "ok": true,
                        "result": result,
                        "cmd": ticket.cmd,
                        "elapsedMs": started.elapsed().as_millis() as u64,
                    }),
                    Err(e) => refuse(Refusal::Browser(format!(
                        "The command ran, but obid couldn't read the tab after it: {e}"
                    ))),
                }
            }
        }
    }

    /// A timed-out command's tab, settling: `get url` tried (each try up to `settle_try`) until the
    /// browser answers, then the core hears it is usable again. Stopped when the job ends.
    fn settle(&self, job: &str, session: Session, tab: &str) {
        let (core, live, every) = (
            self.core.clone(),
            self.live.clone(),
            self.options.settle_try,
        );
        let (job_id, tab) = (job.to_string(), tab.to_string());
        let task = tokio::spawn(async move {
            loop {
                let still = live
                    .lock()
                    .unwrap()
                    .get(&job_id)
                    .is_some_and(|l| l.session.name == session.name);
                if !still {
                    return;
                }
                match session.call(&words(&["get", "url"]), every).await {
                    Call::Ok(v) => {
                        let url = v["url"].as_str().unwrap_or_default().to_string();
                        core.lock().unwrap().settled(&tab, &url);
                        return;
                    }
                    Call::Failed(_) => {
                        // The browser answered: it isn't busy any more.
                        let mut core = core.lock().unwrap();
                        let url = core.tab(&tab).map(|t| t.url.clone()).unwrap_or_default();
                        core.settled(&tab, &url);
                        return;
                    }
                    Call::TimedOut(_) => {}
                }
            }
        });
        let mut settling = self.settling.lock().unwrap();
        let tasks = settling.entry(job.into()).or_default();
        tasks.retain(|t| !t.is_finished());
        tasks.push(task.abort_handle());
    }

    /// What a finished command answers the agent, and what the core learns from it.
    async fn shape(
        &self,
        session: &Session,
        ticket: &Ticket,
        data: Value,
    ) -> Result<Value, String> {
        let limit = self.options.system_limit;
        let tab = ticket.tab.clone();
        let mut r = json!({});
        match ticket.tool {
            "oby_browser_open" => {
                let tabs = self.system(session, &["tab", "list"], limit).await?;
                let active = tabs["tabs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|t| t["active"] == true)
                    .cloned()
                    .ok_or("no active tab after open")?;
                let target = active["targetId"].as_str().unwrap_or_default().to_string();
                let url = active["url"].as_str().unwrap_or_default().to_string();
                let title = data["title"].as_str().unwrap_or_default().to_string();
                let mut core = self.core.lock().unwrap();
                let id = match &tab {
                    Some(id) => {
                        core.navigated(id, &url);
                        core.observed(id, &url, Some(&title));
                        id.clone()
                    }
                    None => core.opened(&session.job, &target, &url, &title),
                };
                r["tab"] = json!(id);
                r["url"] = json!(url);
                core.quote(&mut r, &url, "page title", json!(title));
            }
            "oby_browser_navigate" => {
                let url = data["url"].as_str().unwrap_or_default().to_string();
                let id = tab.unwrap_or_default();
                let mut core = self.core.lock().unwrap();
                core.navigated(&id, &url);
                r["tab"] = json!(id);
                r["url"] = json!(url);
                core.quote(&mut r, &url, "page title", data["title"].clone());
            }
            "oby_browser_snapshot" => {
                let id = tab.unwrap_or_default();
                let url = self.system(session, &["get", "url"], limit).await?["url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let names: BTreeSet<String> = data["refs"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                let mut core = self.core.lock().unwrap();
                core.snapshot_taken(&id, &url, names);
                r["tab"] = json!(id);
                r["url"] = json!(url);
                core.quote(
                    &mut r,
                    &url,
                    "accessibility snapshot",
                    json!({"tree": data["snapshot"], "refs": data["refs"]}),
                );
            }
            "oby_browser_evaluate" => {
                let id = tab.unwrap_or_default();
                let core = self.core.lock().unwrap();
                let url = core.tab(&id).map(|t| t.url.clone()).unwrap_or_default();
                r["tab"] = json!(id);
                core.quote(&mut r, &url, "evaluate result", data["result"].clone());
            }
            "oby_browser_close_tab" => {
                let id = tab.unwrap_or_default();
                self.core.lock().unwrap().closed(&id);
                r["tab"] = json!(id);
                r["closed"] = json!(true);
            }
            "oby_browser_screenshot" => {
                r["tab"] = json!(tab);
                r["path"] = data["path"].clone();
            }
            _ => {
                let id = tab.unwrap_or_default();
                r["tab"] = json!(id);
                r["done"] = json!(ticket.argv.first());
                let dialog = data["warning"].as_str();
                {
                    let mut core = self.core.lock().unwrap();
                    if let Some(w) = dialog {
                        // A dialog the page opened: its words are the page's.
                        core.dialog(&id, Some(json!(w)));
                        core.quote(&mut r, "", "dialog", json!(w));
                        r["dialogOpened"] = json!(true);
                    } else if ticket.tool == "oby_browser_dialog" {
                        core.dialog(&id, None);
                    }
                }
                // What the page is now: a click that navigated kills the refs.
                if dialog.is_none()
                    && matches!(ticket.tool, "oby_browser_act" | "oby_browser_dialog")
                    && let Ok(now) = self.system(session, &["get", "url"], limit).await
                {
                    let url = now["url"].as_str().unwrap_or_default().to_string();
                    self.core.lock().unwrap().observed(&id, &url, None);
                }
            }
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::super::{Binaries, Options, stand_in};
    use super::*;
    use crate::browser::engine::{Actor, Browsers, CmdState};

    const WAIT: Duration = Duration::from_secs(10);

    fn driver(home: &Path) -> Arc<Driver> {
        let installed = stand_in::install(home);
        let mut options = Options::new(home.to_path_buf(), Binaries::InPlace(installed));
        options.settle_try = Duration::from_millis(300);
        Arc::new(Driver::new(options, Arc::new(Mutex::new(Browsers::new()))))
    }

    /// One tool call as the job's agent: the core lets it start, the driver runs it.
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
    async fn browser_9_an_open_makes_the_jobs_tab_and_its_title_is_outside_data() {
        let home = stand_in::scratch("open");
        let d = driver(&home);
        d.start("job-a").await.unwrap();
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_open",
            json!({"url": "http://127.0.0.1:1/"}),
        )
        .await;
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["result"]["tab"], "tab-1");
        assert_eq!(v["result"]["outside"]["data"], "Fixture");
        assert_eq!(v["result"]["marksTurnUntrusted"], true);
        let owner = {
            let core = d.core().lock().unwrap();
            let t = core.tab("tab-1").unwrap();
            (t.conversation.clone(), t.target.clone())
        };
        assert_eq!(owner, ("job-a".to_string(), "T1".to_string()));
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_10_a_ref_from_before_the_page_navigated_never_reaches_agent_browser() {
        let home = stand_in::scratch("stale");
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
        let snap = tool(&d, &home, "job-a", "oby_browser_snapshot", json!({})).await;
        assert_eq!(
            snap["result"]["outside"]["data"]["refs"]["e1"]["name"], "Go",
            "{snap}"
        );
        // The page navigates by itself (a redirect, a script): agent-browser's ref would re-resolve
        // silently on the new page.
        let session = d.session_of("job-a").unwrap();
        std::fs::write(
            session.socket_dir.join(format!("{}.url", session.name)),
            "http://127.0.0.1:1/elsewhere\n",
        )
        .unwrap();
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_act",
            json!({"action": "click", "ref": "e1"}),
        )
        .await;
        assert_eq!(v["error"]["code"], "stale_ref", "{v}");
        let calls = stand_in::calls(&home, "job-a");
        assert!(!calls.iter().any(|c| c.ends_with("click @e1")), "{calls:?}");
        // A new snapshot, and the same click runs.
        tool(&d, &home, "job-a", "oby_browser_snapshot", json!({})).await;
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_act",
            json!({"action": "click", "ref": "e1"}),
        )
        .await;
        assert_eq!(v["ok"], true, "{v}");
        assert!(
            stand_in::calls(&home, "job-a")
                .iter()
                .any(|c| c.ends_with("click @e1"))
        );
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_11_a_command_past_its_deadline_times_out_is_never_replayed_and_its_tab_settles()
     {
        let home = stand_in::scratch("deadline");
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
        let started = Instant::now();
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_navigate",
            json!({"url": "http://127.0.0.1:1/slow", "deadlineMs": 2000}),
        )
        .await;
        assert_eq!(v["error"]["code"], "timed_out", "{v}");
        // Back at the deadline, never after the stand-in's endless sleep; the margin is for a
        // busy machine, not for the command.
        assert!(
            started.elapsed() < Duration::from_millis(2000 + 10_000),
            "{:?}",
            started.elapsed()
        );
        let cmd = v["cmd"].as_u64().unwrap();
        {
            let core = d.core().lock().unwrap();
            assert_eq!(core.entry(cmd).unwrap().state, CmdState::TimedOut);
            assert!(!core.may_replay(cmd));
            assert!(core.tab("tab-1").unwrap().settling.is_some());
        }
        // While the browser is still busy with it, calls on the tab answer `settling`.
        let v = tool(&d, &home, "job-a", "oby_browser_snapshot", json!({})).await;
        assert_eq!(v["error"]["code"], "settling", "{v}");
        // The browser finishes: the tab settles by itself.
        let session = d.session_of("job-a").unwrap();
        let busy = session.socket_dir.join(format!("{}.busy", session.name));
        assert!(
            stand_in::within(WAIT, || async { busy.exists() }).await,
            "the slow command ran"
        );
        std::fs::remove_file(&busy).unwrap();
        assert!(
            stand_in::within(WAIT, || async {
                d.core()
                    .lock()
                    .unwrap()
                    .tab("tab-1")
                    .unwrap()
                    .settling
                    .is_none()
            })
            .await
        );
        assert_eq!(
            d.core().lock().unwrap().tab("tab-1").unwrap().url,
            "http://127.0.0.1:1/slow"
        );
        // Run once, never again.
        let slow = stand_in::calls(&home, "job-a")
            .iter()
            .filter(|c| c.ends_with("open http://127.0.0.1:1/slow"))
            .count();
        assert_eq!(slow, 1);
        let v = tool(&d, &home, "job-a", "oby_browser_snapshot", json!({})).await;
        assert_eq!(v["ok"], true, "{v}");
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn browser_12_a_failed_load_answers_browser_with_its_tab() {
        let home = stand_in::scratch("refused");
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
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_navigate",
            json!({"url": "http://127.0.0.1:1/refused"}),
        )
        .await;
        assert_eq!(v["error"]["code"], "browser", "{v}");
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("ERR_CONNECTION_REFUSED")
        );
        assert_eq!(v["tab"], "tab-1");
        let cmd = v["cmd"].as_u64().unwrap();
        assert_eq!(
            d.core().lock().unwrap().entry(cmd).unwrap().state,
            CmdState::Failed
        );
        d.end("job-a").await;
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn a_command_for_a_job_with_no_live_browser_says_so_and_runs_nothing() {
        let home = stand_in::scratch("not-live");
        let d = driver(&home);
        let v = tool(
            &d,
            &home,
            "job-a",
            "oby_browser_open",
            json!({"url": "http://127.0.0.1:1/"}),
        )
        .await;
        assert_eq!(v["error"]["code"], "browser", "{v}");
        assert!(stand_in::calls(&home, "job-a").is_empty());
        let _ = std::fs::remove_dir_all(home);
    }
}
