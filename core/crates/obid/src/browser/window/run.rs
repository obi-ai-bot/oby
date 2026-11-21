//! Running one agent's command in the window's browser (BROWSER-9 to 12, 27): the core's
//! [`Ticket`] checked right before it runs (the tab's URL read first when it names refs, so a ref
//! the page navigated away from never reaches the page, and when it works the page, so the
//! browser's own error page is never the agent's to act on; never while the page shows a dialog),
//! run under its own deadline over the window's relay and never run again once it timed out (its
//! tab then settles as the page answers). A finished command's answer is shaped as the lab's
//! `shape`: page words under `outside`. A failed one answers `browser` with the page's words and
//! its tab, so the API marks it failed (BROWSER-12).
//!
//! It owns no state of its own: the core's is told through [`Browsers`]'s methods. It writes
//! nothing but a screenshot into the agent's room when asked.
//!
//! [`Browsers`]: crate::browser::engine::Browsers

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::Instant;

use super::WindowDriver;
use crate::browser::driver::Call;
use crate::browser::engine::{Refusal, Ticket, tool, works_the_page};

/// How long obid's own reads around a command may take.
const SYSTEM_LIMIT: Duration = Duration::from_secs(5);

/// A refused command's answer: the refusal, how long it took, its command and tab.
fn answer_refused(refusal: Refusal, started: Instant, cmd: u64, tab: Option<&str>) -> Value {
    let mut v = refusal.to_json();
    v["elapsedMs"] = json!(started.elapsed().as_millis() as u64);
    v["cmd"] = json!(cmd);
    if let Some(t) = tab {
        v["tab"] = json!(t);
    }
    v
}

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|w| w.to_string()).collect()
}

impl WindowDriver {
    /// What is left of a ticket's deadline.
    fn left(&self, ticket: &Ticket) -> Duration {
        Duration::from_millis(ticket.deadline_ms.saturating_sub(self.now_ms()))
    }

    /// obid's own read around a command: its data, or why it failed.
    async fn system(&self, job: &str, args: &[&str], limit: Duration) -> Result<Value, String> {
        match self.call(job, &words(args), limit).await {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(e),
            Call::TimedOut(ms) => Err(format!("the page took over {ms} ms to {}", args.join(" "))),
        }
    }

    /// Runs a ticket the core let start, for `job`'s browser, and answers the agent.
    pub async fn run(&self, job: &str, ticket: &Ticket) -> Value {
        let started = Instant::now();
        let tab = ticket.tab.as_deref();
        let refuse = |r: Refusal| answer_refused(r, started, ticket.cmd, tab);
        let failed = |r: Refusal| {
            self.core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finished(ticket, false);
            refuse(r)
        };
        if !self.relay.window() {
            return failed(Refusal::Browser(
                crate::browser::relay::RelayError::NoWindow.reason().1,
            ));
        }
        // Before it runs, nothing of it has happened: a read that runs out of time is `expired`.
        let before = |call: Call| match call {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(Refusal::Browser(e)),
            Call::TimedOut(_) => Err(Refusal::Expired),
        };
        if let Some(target) = &ticket.target {
            let tabs = match before(
                self.call(job, &words(&["tab", "list"]), self.left(ticket))
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
                    self.call(job, &words(&["tab", target]), self.left(ticket))
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
                self.call(job, &words(&["get", "url"]), self.left(ticket))
                    .await,
            ) {
                Ok(v) => v["url"].as_str().map(str::to_string),
                Err(r) => return failed(r),
            }
        };
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
        match self.call(job, &ticket.argv, self.left(ticket)).await {
            Call::TimedOut(ms) => {
                // It may have partly happened: never run again, and its tab settles first.
                self.core
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .timed_out(ticket);
                if let Some(tab) = tab {
                    self.settle(job, tab).await;
                }
                refuse(Refusal::TimedOut { ms })
            }
            Call::Failed(e) => {
                if matches!(ticket.tool, "oby_browser_open" | "oby_browser_navigate")
                    && let Some(tab) = tab
                {
                    self.core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .navigation_failed(tab, &e);
                }
                failed(Refusal::Browser(e))
            }
            Call::Ok(data) => {
                let shaped = self.shape(job, ticket, data).await;
                self.core
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .finished(ticket, true);
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

    /// A timed-out command's tab, settling: its URL read once more (a webview has no daemon still
    /// busy with the command), then the core hears it is usable again.
    async fn settle(&self, job: &str, tab: &str) {
        let url = match self.system(job, &["get", "url"], SYSTEM_LIMIT).await {
            Ok(v) => v["url"].as_str().map(str::to_string),
            Err(_) => None,
        };
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        let url = url
            .or_else(|| core.tab(tab).map(|t| t.url.clone()))
            .unwrap_or_default();
        core.settled(tab, &url);
    }

    /// What a finished command answers the agent, and what the core learns from it.
    async fn shape(&self, job: &str, ticket: &Ticket, data: Value) -> Result<Value, String> {
        let limit = SYSTEM_LIMIT;
        let tab = ticket.tab.clone();
        let mut r = json!({});
        match ticket.tool {
            "oby_browser_open" => {
                let tabs = self.system(job, &["tab", "list"], limit).await?;
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
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let id = match &tab {
                    Some(id) => {
                        core.navigated(id, &url);
                        core.observed(id, &url, Some(&title));
                        id.clone()
                    }
                    None => core.opened(job, &target, &url, &title),
                };
                r["tab"] = json!(id);
                r["url"] = json!(url);
                core.quote(&mut r, &url, "page title", json!(title));
            }
            "oby_browser_navigate" => {
                let url = data["url"].as_str().unwrap_or_default().to_string();
                let id = tab.unwrap_or_default();
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                core.navigated(&id, &url);
                r["tab"] = json!(id);
                r["url"] = json!(url);
                core.quote(&mut r, &url, "page title", data["title"].clone());
            }
            "oby_browser_snapshot" => {
                let id = tab.unwrap_or_default();
                let url = self.system(job, &["get", "url"], limit).await?["url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let names: BTreeSet<String> = data["refs"]
                    .as_object()
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
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
                let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let url = core.tab(&id).map(|t| t.url.clone()).unwrap_or_default();
                r["tab"] = json!(id);
                core.quote(&mut r, &url, "evaluate result", data["result"].clone());
            }
            "oby_browser_close_tab" => {
                let id = tab.unwrap_or_default();
                self.core
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .closed(&id);
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
                    let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
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
                    && let Ok(now) = self.system(job, &["get", "url"], limit).await
                {
                    let url = now["url"].as_str().unwrap_or_default().to_string();
                    self.core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .observed(&id, &url, None);
                }
            }
        }
        Ok(r)
    }
}
