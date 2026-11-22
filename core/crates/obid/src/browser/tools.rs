//! The agents' tools over obid's driver (BROWSER-1 to 3, 5, 6, 9 to 13, 15), in the lab's order
//! (`browserapi.rs`, `call_tool`): the switch first, an unknown tool next, the job's browser up
//! (within its own allowance) before any tool that reaches a page, then each act let through,
//! committed right before it runs and done after, with its `action` events (started, done,
//! failed, refused), the tabs synced after every tool, a failed load marking its tab and a good
//! one clearing it, and what a finished tool teaches the screens (a snapshot's ref names,
//! `evaluate`'s `scriptRun`, a dialog, a download). It owns no state: the API's and the core's are
//! told through their methods, and it writes only a person's download into their Downloads.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::Service;
use super::api::{Refused, browser_off_for_another_oby, mcp_refusal, mcp_result};
use super::box_edge;
use super::box_files;
use super::contract as c;
use super::engine::{self, Actor, DownloadFate, Kind, download_rule};

/// How long after an act (or the person's click) a download is looked for.
const DOWNLOAD_WATCH: Duration = Duration::from_secs(5);
const DOWNLOAD_EVERY: Duration = Duration::from_millis(250);

impl Service {
    /// One of the agents' tools, called by `job`'s agent, whose own folder is `room`: the MCP tool
    /// result it answers.
    pub async fn call_tool(
        self: &Arc<Self>,
        job: &str,
        room: &Path,
        name: &str,
        args: &Value,
    ) -> Value {
        let foreign = self.api(|a| a.job(job).is_some_and(|j| j.origin.foreign()));
        if foreign && !box_edge::foreign_browser() {
            return mcp_refusal(&box_edge::not_yet());
        }
        if let Err(e) = self.api(|a| a.tools_allowed()) {
            // Another Oby's task finishes without one: its own owner can't turn it on (BROWSER-18).
            return mcp_refusal(&if foreign {
                browser_off_for_another_oby()
            } else {
                e
            });
        }
        if !c::MCP_TOOLS.contains(&name) {
            return mcp_refusal(&Refused::new(
                404,
                "unknown_tool",
                format!("There is no browser tool named {name}."),
            ));
        }
        // The box's edge: another Oby's task opens web pages, never the host's own services or
        // files (BROWSER-18).
        if foreign && matches!(name, "oby_browser_open" | "oby_browser_navigate") {
            let url = args["url"].as_str().unwrap_or_default();
            if let Err(e) = box_edge::in_box(url, &self.host_ports()) {
                let tab = args["tab"].as_str().map(str::to_string);
                self.act_event(job, tab, name, args, c::Outcome::Refused, Some(e.reason));
                return mcp_refusal(&e);
            }
        }
        // Its worktree is the box's to write, links included: an upload is a copy in obid's own
        // folder for the job, read through no link, and its screenshots go into that folder
        // (box_files, BROWSER-18).
        let own = self.window().room(job);
        let boxed;
        let (room, args) = match foreign {
            false => (room, args),
            true => match box_files::uploads(room, self.room(job).as_deref(), &own, name, args) {
                Ok(copied) => {
                    boxed = copied;
                    (own.as_path(), &boxed)
                }
                Err(path) => {
                    let refused = engine::Refusal::UploadOutsideRoom(path).to_json();
                    return mcp_result(self.api(|a| a.for_agent(refused)));
                }
            },
        };
        let actor = Actor::Agent(job.into());
        // Every tool that reaches a page needs Oby's window: its tabs are the window's webviews
        // (T3 Code's design). With no window the call is refused, never run elsewhere.
        if !matches!(name, "oby_browser_list_tabs" | "oby_browser_ask_person") {
            if !self.relay().window() {
                let (reason, message) = super::relay::RelayError::NoWindow.reason();
                return mcp_refusal(&Refused::new(409, reason, message));
            }
            let up = self.api(|a| {
                let ready = a
                    .job(job)
                    .is_some_and(|j| j.browser == c::BrowserState::Ready);
                if !ready {
                    a.set_browser(job, c::BrowserState::Ready, None);
                }
                ready
            });
            if !up {
                self.flush();
            }
        }
        match name {
            "oby_browser_ask_person" => self.ask_person(job, &actor, args),
            "oby_browser_list_tabs" => {
                let mut v = self.core_status(&actor);
                self.sync(job);
                v["result"]["asks"] =
                    serde_json::to_value(self.api(|a| a.asks_of(job))).unwrap_or_default();
                v["result"]["browser"] =
                    serde_json::to_value(self.api(|a| a.job_browser(job))).unwrap_or_default();
                mcp_result(self.api(|a| a.for_agent(v)))
            }
            _ => self.page_tool(job, &actor, room, name, args).await,
        }
    }

    fn core_status(&self, actor: &Actor) -> Value {
        self.window()
            .core()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status(actor)
    }

    /// `ask_person`: the engine's takeover request with the ask's kind, on the tab it resolves.
    fn ask_person(&self, job: &str, actor: &Actor, args: &Value) -> Value {
        let kind: c::AskKind = match serde_json::from_value(args["kind"].clone()) {
            Ok(k) => k,
            Err(_) => {
                return mcp_refusal(&Refused::new(
                    400,
                    "bad_args",
                    format!("kind: one of {}.", c::ASK_KINDS.join(", ")),
                ));
            }
        };
        let reason = args["reason"].as_str().unwrap_or("").to_string();
        let said = self
            .window()
            .core()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ask_takeover(actor, args["tab"].as_str(), &reason);
        let said = match said {
            Ok(v) => v,
            Err(r) => return mcp_result(r.to_json()),
        };
        let tab = said["result"]["tab"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.sync(job);
        let choices = args["choices"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let field = args["field"].as_str().map(str::to_string);
        let ask = self.api(|a| a.ask(job, &tab, kind, &reason, choices, field));
        self.flush();
        mcp_result(json!({"ok": true, "result": {"tab": tab, "ask": ask.id, "asked": true}}))
    }

    /// A tool that reaches the page: the core lets it start, the driver runs it.
    async fn page_tool(
        self: &Arc<Self>,
        job: &str,
        actor: &Actor,
        room: &Path,
        name: &str,
        args: &Value,
    ) -> Value {
        let kind = engine::tool(name).map(|t| t.kind).unwrap_or(Kind::Act);
        let current = self.api(|a| a.job(job).and_then(|j| j.current_tab.clone()));
        let tab = args["tab"].as_str().map(str::to_string).or(current);
        let opens_new = name == "oby_browser_open" && (args["newTab"] == true || tab.is_none());
        // Acts on an existing tab go through the hold: let through, then committed right before
        // they run.
        let lease = if kind == Kind::Act || (kind == Kind::Open && !opens_new) {
            let Some(tab) = tab.clone() else {
                return mcp_refusal(&Refused::new(
                    409,
                    "no_tab",
                    "This job has no tab yet: open one with oby_browser_open.",
                ));
            };
            match self.api(|a| a.lease(job, &tab)) {
                Ok(l) => Some(l),
                Err(e) => {
                    self.act_event(
                        job,
                        Some(tab),
                        name,
                        args,
                        c::Outcome::Refused,
                        Some(e.reason),
                    );
                    return mcp_refusal(&e);
                }
            }
        } else {
            None
        };
        let acts = kind != Kind::Read;
        if let Some(l) = &lease {
            if !self.commit_delay.is_zero() {
                tokio::time::sleep(self.commit_delay).await;
            }
            if let Err(e) = self.api(|a| a.commit(l)) {
                self.act_event(
                    job,
                    Some(l.tab.clone()),
                    name,
                    args,
                    c::Outcome::Refused,
                    Some(e.reason),
                );
                return mcp_refusal(&e);
            }
        }
        if acts {
            self.act_event(job, tab.clone(), name, args, c::Outcome::Started, None);
        }
        let begun = {
            let window = self.window();
            let mut core = window.core().lock().unwrap_or_else(|e| e.into_inner());
            core.begin(actor, name, args, room, window.now_ms())
        };
        let v = match begun {
            Ok(ticket) => self.window().run(job, &ticket).await,
            Err(r) => r.to_json(),
        };
        if let Some(l) = &lease {
            self.api(|a| a.done(l));
        }
        let ok = v["ok"] == true;
        let the_tab = v["result"]["tab"]
            .as_str()
            .or(v["tab"].as_str())
            .map(str::to_string)
            .or(tab.clone());
        let loads = matches!(name, "oby_browser_open" | "oby_browser_navigate");
        // A page that loaded clears the tab's failure, even at the address it failed from.
        if ok
            && loads
            && let Some(t) = &the_tab
        {
            self.api(|a| a.loaded(t));
        }
        self.sync(job);
        if !ok {
            let code = v["error"]["code"].as_str().unwrap_or("browser");
            let message = v["error"]["message"].as_str().unwrap_or("");
            // A command that failed in the browser: what the tab shows now (BROWSER-12).
            if code == "browser"
                && loads
                && let Some(t) = &the_tab
            {
                self.api(|a| a.failed(t, message));
            }
            if acts {
                let reason = c::REASONS.iter().copied().find(|r| *r == code);
                self.act_event(job, the_tab.clone(), name, args, c::Outcome::Failed, reason);
            }
        } else {
            self.after_ok(job, the_tab.clone(), name, args, &v);
            if acts {
                self.act_event(job, the_tab.clone(), name, args, c::Outcome::Done, None);
            }
            if name == "oby_browser_act" {
                self.watch_downloads(job, the_tab, Actor::Agent(job.into()));
            }
        }
        self.flush();
        mcp_result(self.api(|a| a.for_agent(v)))
    }

    /// What a finished tool teaches the API: a snapshot's ref names, a dialog, a script's result.
    fn after_ok(&self, job: &str, tab: Option<String>, name: &str, args: &Value, v: &Value) {
        let r = &v["result"];
        let quoted = r["outside"].clone();
        match name {
            "oby_browser_snapshot" => {
                let refs: BTreeMap<String, String> = quoted["data"]["refs"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| {
                                (k.clone(), v["name"].as_str().unwrap_or_default().into())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.api(|a| {
                    if let Some(j) = a.job_mut(job) {
                        j.refs = refs;
                    }
                });
            }
            "oby_browser_evaluate" => {
                let result: c::Outside =
                    serde_json::from_value(quoted).unwrap_or_else(|_| c::Outside {
                        from: String::new(),
                        what: "evaluate result".into(),
                        note: engine::OUTSIDE_NOTE.into(),
                        data: Value::Null,
                    });
                self.api(|a| {
                    a.push(c::EventBody::ScriptRun {
                        tab_id: tab.clone().unwrap_or_default(),
                        job: job.into(),
                        expression: args["expression"].as_str().unwrap_or_default().into(),
                        result,
                    })
                });
            }
            "oby_browser_dialog" => {
                if let Some(t) = &tab {
                    self.api(|a| a.dialog(t, None));
                }
            }
            _ => {}
        }
        if r["dialogOpened"] == true
            && let Some(t) = &tab
        {
            let words = r["outside"]["data"].as_str().map(str::to_string);
            self.api(|a| a.dialog(t, words.or(Some(String::new()))));
        }
    }

    /// An `action` event: what an agent's act does, as the screens say it, and how it went.
    fn act_event(
        &self,
        job: &str,
        tab: Option<String>,
        tool: &str,
        args: &Value,
        outcome: c::Outcome,
        reason: Option<&str>,
    ) {
        self.api(|a| {
            let text = a.action_text(job, tool, args);
            a.push(c::EventBody::Action {
                tab_id: tab,
                job: job.into(),
                tool: tool.into(),
                text,
                pointer: None,
                outcome,
                reason: reason.map(str::to_string),
            })
        });
        self.flush();
    }

    /// Looks at a job's downloads for a few seconds after an act (BROWSER-13): an agent's stays in
    /// its profile; the person's is delivered into their Downloads.
    pub(super) fn watch_downloads(
        self: &Arc<Self>,
        job: &str,
        tab: Option<String>,
        started_by: Actor,
    ) {
        let me = self.clone();
        let job = job.to_string();
        tokio::spawn(async move {
            let until = tokio::time::Instant::now() + DOWNLOAD_WATCH;
            while tokio::time::Instant::now() < until {
                tokio::time::sleep(DOWNLOAD_EVERY).await;
                let seen = me.api(|a| {
                    a.job(&job)
                        .map(|j| j.seen_downloads.clone())
                        .unwrap_or_default()
                });
                let new: Vec<_> = me
                    .window()
                    .downloads(&job)
                    .into_iter()
                    .filter(|f| !seen.contains(&f.name))
                    .collect();
                if new.is_empty() {
                    continue;
                }
                for file in new {
                    let (who, place) = match download_rule(&started_by) {
                        DownloadFate::Deliver => (c::StartedBy::Person, c::Place::Delivered),
                        DownloadFate::KeepInProfile => (c::StartedBy::Agent, c::Place::Profile),
                    };
                    let mut name = file.name.clone();
                    if place == c::Place::Delivered {
                        let to = me.downloads_to().to_path_buf();
                        let _ = crate::store::make_private_dir(&to);
                        match me.window().save_copy(&job, &file.name, &to) {
                            Ok(copy) => {
                                if let Some(n) = copy.file_name() {
                                    name = n.to_string_lossy().into_owned();
                                }
                                let _ = std::fs::remove_file(
                                    me.window().downloads_folder(&job).join(&file.name),
                                );
                            }
                            Err(e) => {
                                tracing::warn!("job {job}: a download wasn't delivered: {e}");
                                continue;
                            }
                        }
                    }
                    me.api(|a| {
                        if let Some(j) = a.job_mut(&job) {
                            j.seen_downloads.insert(file.name.clone());
                        }
                        a.download_seen(&job, tab.clone(), &name, who, place, file.bytes);
                    });
                }
                me.flush();
                break;
            }
        });
    }
}
