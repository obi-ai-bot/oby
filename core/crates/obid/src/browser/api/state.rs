//! The browser's API state (BROWSER-2 to 7, 11 to 15, 25): every job and its browser, every tab
//! with its hold and generation, asks, downloads, the settings and the event log with its `seq`.
//! [`Api`] owns that state, in memory only; it writes no file, starts no process and reads no
//! clock. The secret's value never enters it. It never decides who calls: the routes check obid's
//! owner token and the device's signed words (`crate::approval`) and the job's broker token
//! (`crate::connections`) first.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::Value;

use super::{Job, Lease, Refused, Start, Take, browser_off, tab_rec::TabRec};
use crate::browser::contract as c;

/// The browser's API: who may call, tabs and their holds, asks, downloads, settings and events.
#[derive(Debug)]
pub struct Api {
    pub settings: c::Settings,
    jobs: BTreeMap<String, Job>,
    tabs: BTreeMap<String, TabRec>,
    asks: BTreeMap<String, c::Ask>,
    downloads: Vec<c::Download>,
    events: VecDeque<c::Event>,
    next_seq: u64,
    next_id: u64,
}

/// How many events the API keeps for a reader that reconnects.
pub const EVENTS_KEPT: usize = 2000;

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

impl Api {
    pub fn new() -> Api {
        Api {
            settings: c::Settings::default(),
            jobs: BTreeMap::new(),
            tabs: BTreeMap::new(),
            asks: BTreeMap::new(),
            downloads: vec![],
            events: VecDeque::new(),
            next_seq: 1,
            next_id: 1,
        }
    }

    fn id(&mut self, prefix: &str) -> String {
        let n = self.next_id;
        self.next_id += 1;
        format!("{prefix}-{n}")
    }

    // ── Jobs ───────────────────────────────────────────────────────────────────────────────────

    /// A job starts. Who it is comes from obid's broker token (`crate::connections`), not from
    /// here.
    pub fn add_job(&mut self, id: &str, agent: &str) {
        self.jobs.insert(
            id.into(),
            Job {
                id: id.into(),
                agent: agent.into(),
                current_tab: None,
                refs: BTreeMap::new(),
                seen_downloads: BTreeSet::new(),
                browser: c::BrowserState::NotStarted,
                browser_reason: None,
            },
        );
    }

    pub fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.get(id)
    }

    pub fn job_mut(&mut self, id: &str) -> Option<&mut Job> {
        self.jobs.get_mut(id)
    }

    /// A job ends: its tabs leave the list.
    pub fn end_job(&mut self, id: &str) {
        self.jobs.remove(id);
        // Its asks go with it: nobody is left to hear the answer, and a long-running obid keeps
        // nothing of an ended job but its downloads, which stay listed while its profile is kept.
        self.asks.retain(|_, a| a.job != id);
        let gone: Vec<String> = self
            .tabs
            .values()
            .filter(|t| matches!(&t.tab.owner, c::Owner::Agent { job, .. } if job == id))
            .map(|t| t.tab.id.clone())
            .collect();
        for t in gone {
            self.tabs.remove(&t);
            self.push(c::EventBody::TabClosed { tab_id: t });
        }
    }

    /// Whether the agents may use the browser at all.
    pub fn tools_allowed(&self) -> Result<(), Refused> {
        if !self.settings.agents_use_browser {
            return Err(browser_off());
        }
        Ok(())
    }

    pub fn put_settings(&mut self, patch: c::SettingsPatch) -> c::Settings {
        if let Some(b) = patch.agents_use_browser {
            self.settings.agents_use_browser = b;
        }
        if let Some(m) = patch.agents_use_mac {
            self.settings.agents_use_mac = m;
        }
        let settings = self.settings;
        self.push(c::EventBody::Settings { settings });
        settings
    }

    // ── Holds ──────────────────────────────────────────────────────────────────────────────────

    fn rec(&self, tab: &str) -> Result<&TabRec, Refused> {
        self.tabs
            .get(tab)
            .ok_or_else(|| Refused::new(404, "no_such_tab", format!("There is no tab {tab}.")))
    }

    fn rec_mut(&mut self, tab: &str) -> Result<&mut TabRec, Refused> {
        self.tabs
            .get_mut(tab)
            .ok_or_else(|| Refused::new(404, "no_such_tab", format!("There is no tab {tab}.")))
    }

    /// An agent's act on a tab of its own is let through: refused while the person holds it.
    pub fn lease(&mut self, job: &str, tab: &str) -> Result<Lease, Refused> {
        let t = self.rec(tab)?;
        if !matches!(&t.tab.owner, c::Owner::Agent { job: j, .. } if j == job) {
            return Err(Refused::new(
                403,
                "not_your_tab",
                "This tab belongs to another job.",
            ));
        }
        if t.tab.held_by.is_some() {
            return Err(Refused::new(
                409,
                "human_control",
                "The person controls this tab. Reads still work; act again once they give it back.",
            ));
        }
        Ok(Lease {
            tab: tab.into(),
            generation: t.tab.hold_generation,
        })
    }

    /// Right before the act runs: the hold hasn't changed since it was let through. Then it counts
    /// as running until [`Api::done`], and a take waits for it.
    pub fn commit(&mut self, lease: &Lease) -> Result<(), Refused> {
        let t = self.rec_mut(&lease.tab)?;
        if t.tab.hold_generation != lease.generation {
            return Err(Refused::new(
                409,
                "control_changed",
                "The person took this tab while your act was waiting to run, so it didn't run. Act again once they give it back, from a new snapshot.",
            ));
        }
        t.inflight += 1;
        Ok(())
    }

    pub fn done(&mut self, lease: &Lease) {
        if let Some(t) = self.tabs.get_mut(&lease.tab) {
            t.inflight = t.inflight.saturating_sub(1);
        }
    }

    pub fn inflight(&self, tab: &str) -> u32 {
        self.tabs.get(tab).map(|t| t.inflight).unwrap_or(0)
    }

    /// The person takes a tab: no new act is let through from now; acts already running finish
    /// first (a [`Take::Wait`]).
    pub fn take(&mut self, device: &str, tab: &str) -> Result<Take, Refused> {
        let t = self.rec_mut(tab)?;
        match t.tab.held_by.as_deref() {
            Some(d) if d != device => {
                return Err(Refused::new(
                    409,
                    "held_by_other",
                    "Another of the owner's devices holds this tab.",
                ));
            }
            Some(_) => {}
            None => {
                t.tab.held_by = Some(device.into());
                t.tab.hold_generation += 1;
            }
        }
        let (n, answer) = (t.inflight, Self::hold_answer(&t.tab));
        let tab = t.tab.clone();
        self.push(c::EventBody::Tab { tab });
        Ok(if n > 0 {
            Take::Wait(n)
        } else {
            Take::Held(answer)
        })
    }

    pub fn give_back(&mut self, device: &str, tab: &str) -> Result<c::HoldAnswer, Refused> {
        let t = self.rec_mut(tab)?;
        if t.tab.held_by.as_deref() != Some(device) {
            return Err(Refused::new(
                403,
                "not_holder",
                "Only the device that took this tab gives it back.",
            ));
        }
        t.tab.held_by = None;
        t.tab.hold_generation += 1;
        let (answer, tab) = (Self::hold_answer(&t.tab), t.tab.clone());
        self.push(c::EventBody::Tab { tab });
        Ok(answer)
    }

    pub fn hold_of(&self, tab: &str) -> Result<c::HoldAnswer, Refused> {
        Ok(Self::hold_answer(&self.rec(tab)?.tab))
    }

    fn hold_answer(t: &c::Tab) -> c::HoldAnswer {
        c::HoldAnswer {
            tab_id: t.id.clone(),
            held_by: t.held_by.clone(),
            hold_generation: t.hold_generation,
        }
    }

    /// The person's input or secret on a tab: only while they hold it. Answers the tab's job.
    pub fn person_holds(&self, device: Option<&str>, tab: &str) -> Result<String, Refused> {
        let t = self.rec(tab)?;
        match (&t.tab.held_by, device) {
            (None, _) => Err(Refused::new(
                409,
                "not_held",
                "Take control of the tab first: the person's input reaches only a tab they hold.",
            )),
            (Some(h), Some(d)) if h != d => Err(Refused::new(
                403,
                "not_holder",
                "Another of the owner's devices holds this tab.",
            )),
            _ => match &t.tab.owner {
                c::Owner::Agent { job, .. } => Ok(job.clone()),
                c::Owner::Person => Ok(String::new()),
            },
        }
    }

    // ── Tabs, as the engine reports them ───────────────────────────────────────────────────────

    /// A job's tabs as the engine's status says (`tabs` and the quoted titles): changes become
    /// `tab` and `tabClosed` events.
    pub fn sync_job(&mut self, job: &str, engine_tabs: &[Value], titles: &Value) {
        let Some(agent) = self.jobs.get(job).map(|j| j.agent.clone()) else {
            return;
        };
        let mut present = BTreeSet::new();
        for et in engine_tabs {
            let Some(id) = et["tab"].as_str() else {
                continue;
            };
            present.insert(id.to_string());
            if et["current"] == true
                && let Some(j) = self.jobs.get_mut(job)
            {
                j.current_tab = Some(id.into());
            }
            let rec = self.tabs.entry(id.into()).or_insert_with(|| TabRec {
                tab: c::Tab {
                    id: id.into(),
                    owner: c::Owner::Agent {
                        job: job.into(),
                        agent: agent.clone(),
                    },
                    title: String::new(),
                    url: String::new(),
                    state: c::TabState::Loading,
                    held_by: None,
                    hold_generation: 0,
                    computer: c::Computer::this_mac(),
                    ask: None,
                    dialog: None,
                },
                inflight: 0,
                last_error: None,
            });
            let before = rec.tab.clone();
            let url = et["url"].as_str().unwrap_or_default();
            if url != rec.tab.url {
                rec.last_error = None;
            }
            rec.tab.url = url.into();
            rec.tab.title = titles[id].as_str().unwrap_or_default().into();
            if et["dialog"] != true {
                rec.tab.dialog = None;
            }
            rec.tab.state = if et["settling"] == true {
                c::TabState::TimedOut
            } else if rec.tab.dialog.is_some() || et["dialog"] == true {
                c::TabState::Dialog
            } else if let Some(s) = rec.last_error {
                s
            } else {
                c::TabState::Ready
            };
            if rec.tab != before {
                let tab = rec.tab.clone();
                self.push(c::EventBody::Tab { tab });
            }
        }
        let gone: Vec<String> = self
            .tabs
            .values()
            .filter(|t| matches!(&t.tab.owner, c::Owner::Agent { job: j, .. } if j == job))
            .filter(|t| !present.contains(&t.tab.id))
            .map(|t| t.tab.id.clone())
            .collect();
        for id in gone {
            self.tabs.remove(&id);
            self.push(c::EventBody::TabClosed { tab_id: id });
        }
    }

    /// A page loaded in the tab: whatever failed before is over, even at the same address (the
    /// next [`Api::sync_job`] says it's ready).
    pub fn loaded(&mut self, tab: &str) {
        if let Some(t) = self.tabs.get_mut(tab) {
            t.last_error = None;
        }
    }

    /// A command on a tab failed: what the page shows now.
    pub fn failed(&mut self, tab: &str, message: &str) {
        let state = failure_state(message);
        if let Some(t) = self.tabs.get_mut(tab) {
            t.last_error = Some(state);
            t.tab.state = state;
            let tab = t.tab.clone();
            self.push(c::EventBody::Tab { tab });
        }
    }

    /// A dialog the page opened (its words), or none after the agent answered it.
    pub fn dialog(&mut self, tab: &str, words: Option<String>) {
        if let Some(t) = self.tabs.get_mut(tab) {
            t.tab.dialog = words;
            t.tab.state = if t.tab.dialog.is_some() {
                c::TabState::Dialog
            } else {
                c::TabState::Ready
            };
            let tab = t.tab.clone();
            self.push(c::EventBody::Tab { tab });
        }
    }

    pub fn tabs(&self) -> c::TabsAnswer {
        c::TabsAnswer {
            tabs: self.tabs.values().map(|t| t.tab.clone()).collect(),
            settings: self.settings,
            browsers: self
                .jobs
                .keys()
                .filter_map(|j| self.job_browser(j))
                .collect(),
            seq: self.last_seq(),
        }
    }

    // ── A job's browser ────────────────────────────────────────────────────────────────────────

    pub fn job_browser(&self, job: &str) -> Option<c::JobBrowser> {
        self.jobs.get(job).map(|j| c::JobBrowser {
            job: j.id.clone(),
            agent: j.agent.clone(),
            state: j.browser,
            reason: j.browser_reason.clone(),
        })
    }

    /// A job's browser is now in `state`: a `browser` event when that's new.
    pub fn set_browser(&mut self, job: &str, state: c::BrowserState, reason: Option<String>) {
        let Some(j) = self.jobs.get_mut(job) else {
            return;
        };
        if j.browser == state && j.browser_reason == reason {
            return;
        }
        j.browser = state;
        j.browser_reason = reason;
        if let Some(browser) = self.job_browser(job) {
            self.push(c::EventBody::Browser { browser });
        }
    }

    /// Who starts a job's browser: the first caller, while the others wait for it.
    pub fn claim_start(&mut self, job: &str) -> Start {
        match self.jobs.get(job).map(|j| j.browser) {
            None => Start::NoJob,
            Some(c::BrowserState::Ready) => Start::Up,
            Some(c::BrowserState::Starting) => Start::Wait,
            Some(c::BrowserState::NotStarted | c::BrowserState::Failed) => {
                self.set_browser(job, c::BrowserState::Starting, None);
                Start::Mine
            }
        }
    }

    pub fn tab(&self, id: &str) -> Option<&c::Tab> {
        self.tabs.get(id).map(|t| &t.tab)
    }

    // ── Asks ───────────────────────────────────────────────────────────────────────────────────

    pub fn ask(
        &mut self,
        job: &str,
        tab: &str,
        kind: c::AskKind,
        reason: &str,
        choices: Vec<String>,
        field: Option<String>,
    ) -> c::Ask {
        let ask = c::Ask {
            id: self.id("ask"),
            tab_id: tab.into(),
            job: job.into(),
            kind,
            reason: reason.into(),
            choices,
            field,
            answered: false,
            choice: None,
        };
        self.asks.insert(ask.id.clone(), ask.clone());
        if let Some(t) = self.tabs.get_mut(tab) {
            t.tab.ask = Some(ask.clone());
        }
        self.push(c::EventBody::Ask { ask: ask.clone() });
        ask
    }

    pub fn answer(&mut self, id: &str, body: c::AnswerBody) -> Result<c::Ask, Refused> {
        let ask = self
            .asks
            .get_mut(id)
            .ok_or_else(|| Refused::new(404, "no_such_ask", format!("There is no ask {id}.")))?;
        if ask.kind == c::AskKind::Choice {
            match &body.choice {
                Some(ch) if ask.choices.contains(ch) => {}
                _ => {
                    return Err(Refused::new(
                        400,
                        "bad_request",
                        "A choice's answer is one of its choices.",
                    ));
                }
            }
        }
        ask.answered = true;
        ask.choice = body.choice;
        let ask = ask.clone();
        if let Some(t) = self.tabs.get_mut(&ask.tab_id)
            && t.tab.ask.as_ref().map(|a| &a.id) == Some(&ask.id)
        {
            t.tab.ask = None;
            let tab = t.tab.clone();
            self.push(c::EventBody::Tab { tab });
        }
        self.push(c::EventBody::AskAnswered {
            ask_id: ask.id.clone(),
        });
        Ok(ask)
    }

    pub fn asks_of(&self, job: &str) -> Vec<c::Ask> {
        self.asks
            .values()
            .filter(|a| a.job == job)
            .cloned()
            .collect()
    }

    // ── Downloads ──────────────────────────────────────────────────────────────────────────────

    pub fn download_seen(
        &mut self,
        job: &str,
        tab: Option<String>,
        name: &str,
        started_by: c::StartedBy,
        place: c::Place,
        bytes: u64,
    ) -> c::Download {
        let d = c::Download {
            id: self.id("dl"),
            job: job.into(),
            tab_id: tab,
            name: name.into(),
            started_by,
            place,
            bytes,
        };
        self.downloads.push(d.clone());
        self.push(c::EventBody::Download {
            download: d.clone(),
        });
        d
    }

    pub fn downloads(&self) -> c::DownloadsAnswer {
        c::DownloadsAnswer {
            downloads: self.downloads.clone(),
            seq: self.last_seq(),
        }
    }

    /// Save a copy: only of a download still in an agent's profile.
    pub fn saveable(&self, id: &str) -> Result<c::Download, Refused> {
        let d = self.downloads.iter().find(|d| d.id == id).ok_or_else(|| {
            Refused::new(
                404,
                "no_such_download",
                format!("There is no download {id}."),
            )
        })?;
        if d.place != c::Place::Profile {
            return Err(Refused::new(
                400,
                "bad_request",
                "That download is already the person's.",
            ));
        }
        Ok(d.clone())
    }

    // ── Events ─────────────────────────────────────────────────────────────────────────────────

    pub fn push(&mut self, body: c::EventBody) -> c::Event {
        let e = c::Event {
            seq: self.next_seq,
            body,
        };
        self.next_seq += 1;
        self.events.push_back(e.clone());
        while self.events.len() > EVENTS_KEPT {
            self.events.pop_front();
        }
        e
    }

    /// The events after `seq`, oldest first, while the API still holds them.
    pub fn events_after(&self, seq: u64) -> Vec<c::Event> {
        self.events
            .iter()
            .filter(|e| e.seq > seq)
            .cloned()
            .collect()
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// What the screens say an agent's act does: `Clicking Sign in`.
    pub fn action_text(&self, job: &str, tool: &str, args: &Value) -> String {
        let name = |r: &Value| {
            let r = r.as_str().unwrap_or_default().trim_start_matches('@');
            self.jobs
                .get(job)
                .and_then(|j| j.refs.get(r))
                .filter(|n| !n.is_empty())
                .cloned()
                .unwrap_or_else(|| "the page".into())
        };
        let host = |u: &Value| {
            let u = u.as_str().unwrap_or_default();
            u.split("://")
                .nth(1)
                .unwrap_or(u)
                .split('/')
                .next()
                .unwrap_or(u)
                .to_string()
        };
        match (tool, args["action"].as_str()) {
            ("oby_browser_open", _) => format!("Opening {}", host(&args["url"])),
            ("oby_browser_navigate", _) => format!("Going to {}", host(&args["url"])),
            ("oby_browser_evaluate", _) => "Running a script".into(),
            ("oby_browser_dialog", _) => "Answering the page's dialog".into(),
            ("oby_browser_close_tab", _) => "Closing the tab".into(),
            ("oby_browser_act", Some("click")) => format!("Clicking {}", name(&args["ref"])),
            ("oby_browser_act", Some("type")) => format!("Typing into {}", name(&args["ref"])),
            ("oby_browser_act", Some("hover")) => format!("Pointing at {}", name(&args["ref"])),
            ("oby_browser_act", Some("select")) => format!("Choosing in {}", name(&args["ref"])),
            ("oby_browser_act", Some("upload")) => format!("Uploading to {}", name(&args["ref"])),
            ("oby_browser_act", Some("drag")) => format!("Dragging {}", name(&args["ref"])),
            ("oby_browser_act", Some("press")) => {
                format!("Pressing {}", args["key"].as_str().unwrap_or("a key"))
            }
            ("oby_browser_act", Some("scroll")) => "Scrolling".into(),
            _ => "Using the browser".into(),
        }
    }

    /// An MCP tool result's structured content, as the agent sees it: page data stays quoted under
    /// `outside`; only the driver's own timing keys go.
    pub fn for_agent(&self, mut engine_answer: Value) -> Value {
        for k in ["deadlineMs", "withinDeadline"] {
            if let Some(o) = engine_answer.as_object_mut() {
                o.remove(k);
            }
        }
        engine_answer
    }
}

/// A failed command's words, as a tab state (board 10).
pub fn failure_state(message: &str) -> c::TabState {
    let m = message.to_ascii_lowercase();
    if m.contains("err_cert") || m.contains("certificate") {
        c::TabState::Certificate
    } else if m.contains("crash") || m.contains("target closed") {
        c::TabState::Crashed
    } else if m.contains("timed out") || m.contains("timeout") {
        c::TabState::TimedOut
    } else {
        c::TabState::Failed
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::browser::engine;

    fn api() -> Api {
        Api::new()
    }

    fn with_tab(a: &mut Api, job: &str, tab: &str) {
        a.add_job(job, "claude");
        a.sync_job(
            job,
            &[json!({"tab": tab, "url": "http://x/", "current": true})],
            &json!({tab: "X"}),
        );
    }

    #[test]
    fn browser_25_an_ended_jobs_tabs_leave_the_list() {
        let mut a = api();
        with_tab(&mut a, "j1", "tab-1");
        with_tab(&mut a, "j2", "tab-2");
        a.ask("j1", "tab-1", c::AskKind::SignIn, "Sign in", vec![], None);
        a.ask("j2", "tab-2", c::AskKind::SignIn, "Sign in", vec![], None);
        a.end_job("j1");
        assert!(a.job("j1").is_none());
        assert!(a.tab("tab-1").is_none());
        assert!(a.tab("tab-2").is_some(), "another job's tab stays");
        assert!(a.asks_of("j1").is_empty(), "its asks went with it");
        assert_eq!(a.asks_of("j2").len(), 1, "another job's ask stays");
        let last = a.events_after(0).pop().unwrap();
        assert_eq!(
            last.body,
            c::EventBody::TabClosed {
                tab_id: "tab-1".into()
            }
        );
    }

    #[test]
    fn browser_6_an_act_let_through_before_a_take_is_refused_and_a_take_waits_for_running_acts() {
        let mut a = api();
        with_tab(&mut a, "j1", "tab-1");
        // Let through, then the person takes before it runs: refused, never run.
        let l = a.lease("j1", "tab-1").unwrap();
        assert!(matches!(a.take("d1", "tab-1").unwrap(), Take::Held(_)));
        assert_eq!(a.commit(&l).unwrap_err().reason, "control_changed");
        assert_eq!(a.lease("j1", "tab-1").unwrap_err().reason, "human_control");
        // Given back, the old lease still doesn't count: its generation is gone.
        a.give_back("d1", "tab-1").unwrap();
        assert_eq!(a.commit(&l).unwrap_err().reason, "control_changed");
        // Running when the person takes: the take waits for it.
        let l2 = a.lease("j1", "tab-1").unwrap();
        a.commit(&l2).unwrap();
        assert_eq!(a.take("d1", "tab-1").unwrap(), Take::Wait(1));
        a.done(&l2);
        assert_eq!(a.inflight("tab-1"), 0);
        // Another device can't take a held tab or give it back.
        assert_eq!(a.take("d2", "tab-1").unwrap_err().reason, "held_by_other");
        assert_eq!(a.give_back("d2", "tab-1").unwrap_err().reason, "not_holder");
        // Another job's tab is never this one's.
        with_tab(&mut a, "j2", "tab-2");
        assert_eq!(a.lease("j1", "tab-2").unwrap_err().reason, "not_your_tab");
    }

    #[test]
    fn browser_15_the_switch_off_refuses_every_tool_and_both_are_on_by_default() {
        let mut a = api();
        assert!(a.tools_allowed().is_ok());
        a.put_settings(c::SettingsPatch {
            agents_use_browser: Some(false),
            agents_use_mac: None,
        });
        let e = a.tools_allowed().unwrap_err();
        assert_eq!(e.reason, "browser_off");
        assert!(e.message.contains("Agents can use the browser"));
        assert!(a.settings.agents_use_mac);
        assert_eq!(a.events_after(0).last().unwrap().body.kind(), "settings");
    }

    #[test]
    fn browser_7_the_persons_input_reaches_only_a_tab_they_hold() {
        let mut a = api();
        with_tab(&mut a, "j1", "tab-1");
        assert_eq!(
            a.person_holds(None, "tab-1").unwrap_err().reason,
            "not_held"
        );
        a.take("d1", "tab-1").unwrap();
        assert_eq!(a.person_holds(Some("d1"), "tab-1").unwrap(), "j1");
        assert_eq!(
            a.person_holds(Some("d2"), "tab-1").unwrap_err().reason,
            "not_holder"
        );
    }

    #[test]
    fn browser_12_tabs_and_events_follow_the_engine() {
        let mut a = api();
        a.add_job("j1", "claude");
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-1", "url": "http://x/", "current": true})],
            &json!({"tab-1": "Home"}),
        );
        let t = a.tab("tab-1").unwrap().clone();
        assert_eq!((t.title.as_str(), t.state), ("Home", c::TabState::Ready));
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-1", "url": "http://x/", "settling": true})],
            &json!({"tab-1": "Home"}),
        );
        assert_eq!(a.tab("tab-1").unwrap().state, c::TabState::TimedOut);
        let n = a.events_after(0).len();
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-1", "url": "http://x/", "settling": true})],
            &json!({"tab-1": "Home"}),
        );
        assert_eq!(a.events_after(0).len(), n, "no change, no event");
        a.sync_job("j1", &[], &json!({}));
        assert_eq!(a.events_after(0).last().unwrap().body.kind(), "tabClosed");
        // A failure, then a good load at the same address: ready again.
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-2", "url": "http://x/"})],
            &json!({}),
        );
        a.failed("tab-2", "Navigation failed: net::ERR_CONNECTION_REFUSED");
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-2", "url": "http://x/"})],
            &json!({}),
        );
        assert_eq!(a.tab("tab-2").unwrap().state, c::TabState::Failed);
        a.loaded("tab-2");
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-2", "url": "http://x/"})],
            &json!({}),
        );
        assert_eq!(a.tab("tab-2").unwrap().state, c::TabState::Ready);
        assert_eq!(
            failure_state("net::ERR_CERT_AUTHORITY_INVALID"),
            c::TabState::Certificate
        );
        let seqs: Vec<u64> = a.events_after(0).iter().map(|e| e.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] + 1 == w[1]));
    }

    /// The lists carry the last event they reflect, so a reader can tell an answer older than an
    /// event it applied (LAB-UI-LIVE-6): taken under the same lock as the state it shows.
    #[test]
    fn browser_4_the_lists_carry_the_last_event_they_reflect() {
        let mut a = Api::new();
        assert_eq!(
            (a.tabs().seq, a.downloads().seq),
            (0, 0),
            "before any event"
        );
        a.add_job("j1", "codex");
        a.sync_job(
            "j1",
            &[json!({"tab": "tab-1", "url": "http://x/", "current": true})],
            &json!({}),
        );
        let before = a.tabs();
        a.failed("tab-1", "Navigation failed: net::ERR_CONNECTION_REFUSED");
        let after = a.tabs();
        assert_eq!(after.seq, a.last_seq());
        assert_eq!(after.seq, before.seq + 1, "the failure is one event later");
        assert_eq!(before.tabs[0].state, c::TabState::Ready);
        assert_eq!(after.tabs[0].state, c::TabState::Failed);
        let missed = a.events_after(before.seq);
        assert_eq!(missed.len(), 1, "exactly what the older answer lacks");
        assert_eq!(a.downloads().seq, a.last_seq());
    }

    #[test]
    fn asks_answer_with_a_choice_and_page_data_stays_quoted() {
        let mut a = api();
        with_tab(&mut a, "j1", "tab-1");
        let ask = a.ask(
            "j1",
            "tab-1",
            c::AskKind::Choice,
            "which?",
            vec!["A".into(), "B".into()],
            None,
        );
        assert_eq!(
            a.answer(
                &ask.id,
                c::AnswerBody {
                    choice: Some("C".into())
                }
            )
            .unwrap_err()
            .reason,
            "bad_request"
        );
        let done = a
            .answer(
                &ask.id,
                c::AnswerBody {
                    choice: Some("B".into()),
                },
            )
            .unwrap();
        assert!(done.answered && done.choice.as_deref() == Some("B"));
        assert!(a.tab("tab-1").unwrap().ask.is_none());
        let v = json!({"ok": true, "result": {"outside": engine::outside("u", "evaluate result", json!(2))}, "deadlineMs": 1});
        assert_eq!(
            a.for_agent(v.clone())["result"]["outside"]["note"],
            engine::OUTSIDE_NOTE
        );
        assert!(a.for_agent(v).get("deadlineMs").is_none());
    }

    #[test]
    fn every_reason_the_core_answers_is_the_contracts() {
        let mut a = api();
        with_tab(&mut a, "j1", "tab-1");
        let rs = [
            a.lease("j1", "nope").unwrap_err(),
            a.saveable("dl-9").unwrap_err(),
            a.answer("ask-9", c::AnswerBody::default()).unwrap_err(),
            browser_off(),
        ];
        for r in rs {
            assert!(c::REASONS.contains(&r.reason), "{}", r.reason);
            assert!(r.body()["error"].is_string());
        }
    }
}
