//! The window's relay (T3 Code's desktop browser channel): obid's half. Each agent tab is a
//! `<webview>` in the Electron window, whose main process relays CDP between the webview's
//! debugger and obid. obid sends [`RelayCommand`]s to whoever follows `GET /v1/browser/relay`
//! (never kept or replayed), and hears [`RelayEvent`]s on `POST /v1/browser/relay`, in order.
//!
//! It owns, in memory only: the tabs the window was asked to open and whether each is attached,
//! the CDP calls waiting for their answers (each by its id, which obid assigns), the page
//! events the tools wait on, and each foreign job's door for its box partition (BROWSER-18), sent
//! again to a window that follows anew and when a tab of that job's is made, never logged. With
//! nobody following the stream there is no window: a call is refused `no_window` at once, never
//! run elsewhere. It writes nothing to disk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot, watch};

use super::contract::{PointerPhase, RelayCommand, RelayEvent};

/// How many commands a slow window may fall behind before it misses some (it then reconnects).
const COMMANDS: usize = 1024;
/// How many page events the tools' waits may fall behind.
const PAGE_EVENTS: usize = 1024;

/// Why a CDP call has no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayError {
    /// Nobody follows the relay stream, or it went away while the call waited.
    NoWindow,
    /// The tab's webview isn't attached, or it went away while the call waited.
    Detached,
    /// No answer within the call's time.
    TimedOut(u64),
    /// The page's debugger answered with an error: its message.
    Cdp(String),
}

impl RelayError {
    /// The contract's reason for it, and a sentence an agent can act on.
    pub fn reason(&self) -> (&'static str, String) {
        match self {
            RelayError::NoWindow => (
                "no_window",
                "Oby's window isn't open on this Mac, so its browser isn't there. Ask the person to open Oby.".into(),
            ),
            RelayError::Detached => (
                "browser",
                "The tab's page went away (closed, crashed, or the person opened its developer tools).".into(),
            ),
            RelayError::TimedOut(ms) => (
                "timed_out",
                format!("The page didn't answer within {ms} ms."),
            ),
            RelayError::Cdp(m) => ("browser", format!("The page refused: {m}")),
        }
    }
}

/// An event a tab's page sent (`Page.loadEventFired`, `Page.javascriptDialogOpening`, …).
#[derive(Clone, Debug, PartialEq)]
pub struct PageEvent {
    pub tab: String,
    pub method: String,
    pub params: Value,
}

/// What obid knows of one tab the window was asked to open.
#[derive(Clone, Debug, Default)]
struct RelayTab {
    job: String,
    attached: bool,
    shown: Option<bool>,
    load_failed: Option<(String, i64, String)>,
}

/// A CDP call waiting for its answer.
struct Pending {
    tab: String,
    answer: oneshot::Sender<Result<Value, RelayError>>,
}

#[derive(Default)]
struct State {
    tabs: HashMap<String, RelayTab>,
    pending: HashMap<u64, Pending>,
    /// Each foreign job's box partition and its browser door, by job.
    boxes: HashMap<String, (String, String)>,
}

/// The relay: see the module's header.
pub struct Relay {
    commands: broadcast::Sender<RelayCommand>,
    page: broadcast::Sender<PageEvent>,
    /// Bumped on every `attached` and `detached`, so a wait for one wakes.
    changed: watch::Sender<u64>,
    state: Mutex<State>,
    next_id: AtomicU64,
    /// How many follow the stream now (the window's main, normally one).
    following: AtomicUsize,
}

impl Default for Relay {
    fn default() -> Relay {
        Relay::new()
    }
}

impl Relay {
    pub fn new() -> Relay {
        Relay {
            commands: broadcast::channel(COMMANDS).0,
            page: broadcast::channel(PAGE_EVENTS).0,
            changed: watch::channel(0).0,
            state: Mutex::new(State::default()),
            next_id: AtomicU64::new(1),
            following: AtomicUsize::new(0),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether a window follows the relay stream now.
    pub fn window(&self) -> bool {
        self.following.load(Ordering::SeqCst) > 0
    }

    /// The stream's reader, for `GET /v1/browser/relay`. Dropping the [`Following`] (the stream
    /// ended) fails every waiting call with [`RelayError::NoWindow`] once nobody follows.
    /// Every foreign job's door goes to it first, before any later command.
    pub fn follow(self: &Arc<Self>) -> (broadcast::Receiver<RelayCommand>, Following) {
        let rx = self.commands.subscribe();
        self.following.fetch_add(1, Ordering::SeqCst);
        let state = self.state();
        for job in state.boxes.keys() {
            self.send_box(&state, job);
        }
        drop(state);
        (rx, Following(self.clone()))
    }

    /// Job `job`'s box partition is held to `proxy`, its browser door: the window is told once,
    /// and again only for another door.
    pub fn set_box(&self, job: &str, partition: &str, proxy: &str) {
        let mut state = self.state();
        let door = (partition.to_owned(), proxy.to_owned());
        if state.boxes.get(job) != Some(&door) {
            state.boxes.insert(job.to_owned(), door);
            self.send_box(&state, job);
        }
    }

    /// Job `job` ended: its door is forgotten, and nothing is sent.
    pub fn forget_box(&self, job: &str) {
        self.state().boxes.remove(job);
    }

    fn send_box(&self, state: &State, job: &str) {
        if let Some((partition, proxy)) = state.boxes.get(job) {
            self.send(RelayCommand::Box {
                thread_id: job.to_owned(),
                partition: partition.clone(),
                proxy: proxy.clone(),
            });
        }
    }

    /// The page events from now on, for a tool's wait.
    pub fn page_events(&self) -> broadcast::Receiver<PageEvent> {
        self.page.subscribe()
    }

    fn send(&self, command: RelayCommand) {
        let _ = self.commands.send(command);
    }

    /// A tab obid just made for `job`, which the window mounts from its `tab` event (T3's way):
    /// obid waits for its `attached`. A foreign job's door goes again, so a box the window
    /// cleared with its last tab gets it back.
    pub fn expect(&self, job: &str, tab: &str) -> Result<(), RelayError> {
        if !self.window() {
            return Err(RelayError::NoWindow);
        }
        let mut state = self.state();
        state.tabs.insert(
            tab.into(),
            RelayTab {
                job: job.into(),
                ..RelayTab::default()
            },
        );
        self.send_box(&state, job);
        Ok(())
    }

    /// Whether the tab's webview is attached now.
    pub fn attached(&self, tab: &str) -> bool {
        self.state().tabs.get(tab).is_some_and(|t| t.attached)
    }

    /// Waits up to `within` for the tab's webview to attach.
    pub async fn wait_attached(&self, tab: &str, within: Duration) -> Result<(), RelayError> {
        let mut changed = self.changed.subscribe();
        let wait = async {
            loop {
                if self.attached(tab) {
                    return Ok(());
                }
                if !self.window() {
                    return Err(RelayError::NoWindow);
                }
                if changed.changed().await.is_err() {
                    return Err(RelayError::NoWindow);
                }
            }
        };
        tokio::time::timeout(within, wait)
            .await
            .unwrap_or(Err(RelayError::TimedOut(within.as_millis() as u64)))
    }

    /// The page's load failure the window reported, if any: `(url, code, description)`.
    pub fn load_failed(&self, tab: &str) -> Option<(String, i64, String)> {
        self.state()
            .tabs
            .get(tab)
            .and_then(|t| t.load_failed.clone())
    }

    /// Whether the tab's webview is on screen, as the window said last (unknown counts as shown).
    pub fn shown(&self, tab: &str) -> bool {
        self.state()
            .tabs
            .get(tab)
            .and_then(|t| t.shown)
            .unwrap_or(true)
    }

    /// One CDP command for the tab's page: its `result`, or why there is none within `within`.
    pub async fn cdp(
        &self,
        tab: &str,
        method: &str,
        params: Value,
        within: Duration,
    ) -> Result<Value, RelayError> {
        self.cdp_call(tab, method, params, within, false).await
    }

    /// [`Relay::cdp`] for the agent's own script (`oby_browser_evaluate`): its frame goes out
    /// marked `agentCode`, so the window may refuse it (BROWSER-33).
    pub async fn cdp_agent(
        &self,
        tab: &str,
        method: &str,
        params: Value,
        within: Duration,
    ) -> Result<Value, RelayError> {
        self.cdp_call(tab, method, params, within, true).await
    }

    async fn cdp_call(
        &self,
        tab: &str,
        method: &str,
        params: Value,
        within: Duration,
        agent_code: bool,
    ) -> Result<Value, RelayError> {
        if !self.window() {
            return Err(RelayError::NoWindow);
        }
        let (tx, rx) = oneshot::channel();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let job = {
            let mut state = self.state();
            let job = match state.tabs.get(tab) {
                Some(t) if t.attached => t.job.clone(),
                _ => return Err(RelayError::Detached),
            };
            state.pending.insert(
                id,
                Pending {
                    tab: tab.into(),
                    answer: tx,
                },
            );
            job
        };
        let message = json!({"id": id, "method": method, "params": params}).to_string();
        self.send(RelayCommand::Cdp {
            thread_id: job,
            tab_id: tab.into(),
            message,
            agent_code,
        });
        match tokio::time::timeout(within, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(RelayError::NoWindow),
            Err(_) => {
                self.state().pending.remove(&id);
                Err(RelayError::TimedOut(within.as_millis() as u64))
            }
        }
    }

    /// obid is done with the tab: the window drops it, and obid forgets it.
    pub fn release(&self, tab: &str) {
        let job = {
            let mut state = self.state();
            Self::fail_tab(&mut state, tab, RelayError::Detached);
            state.tabs.remove(tab).map(|t| t.job)
        };
        if let Some(job) = job {
            self.send(RelayCommand::Release {
                thread_id: job,
                tab_id: tab.into(),
            });
        }
        self.bump();
    }

    /// Draws the agent's pointer on the tab.
    pub fn pointer(&self, tab: &str, phase: PointerPhase, x: f64, y: f64) {
        let job = self.state().tabs.get(tab).map(|t| t.job.clone());
        if let Some(job) = job {
            self.send(RelayCommand::Pointer {
                thread_id: job,
                tab_id: tab.into(),
                phase,
                x,
                y,
            });
        }
    }

    /// Whether an agent's turn runs for `job`.
    pub fn turn(&self, job: &str, active: bool) {
        self.send(RelayCommand::Turn {
            thread_id: job.into(),
            active,
        });
    }

    /// The window's events, in order (`POST /v1/browser/relay`).
    pub fn receive(&self, events: Vec<RelayEvent>) {
        for event in events {
            match event {
                RelayEvent::Attached { tab_id, .. } => {
                    if let Some(t) = self.state().tabs.get_mut(&tab_id) {
                        t.attached = true;
                    }
                    self.bump();
                }
                RelayEvent::Detached { tab_id, .. } => {
                    {
                        let mut state = self.state();
                        if let Some(t) = state.tabs.get_mut(&tab_id) {
                            t.attached = false;
                        }
                        Self::fail_tab(&mut state, &tab_id, RelayError::Detached);
                    }
                    self.bump();
                }
                RelayEvent::Cdp {
                    tab_id, message, ..
                } => self.cdp_from_window(&tab_id, &message),
                RelayEvent::LoadFailed {
                    tab_id,
                    url,
                    code,
                    description,
                    ..
                } => {
                    if let Some(t) = self.state().tabs.get_mut(&tab_id) {
                        t.load_failed = Some((url, code, description));
                    }
                }
                RelayEvent::Shown { tab_id, shown, .. } => {
                    if let Some(t) = self.state().tabs.get_mut(&tab_id) {
                        t.shown = Some(shown);
                    }
                }
                RelayEvent::Unknown => {}
            }
        }
    }

    /// A CDP frame from a page: an answer to a waiting call (by its id), or a page event.
    fn cdp_from_window(&self, tab: &str, message: &str) {
        let Ok(frame) = serde_json::from_str::<Value>(message) else {
            return;
        };
        if let Some(id) = frame["id"].as_u64() {
            let pending = {
                let mut state = self.state();
                // Only the tab it was sent to may answer it.
                match state.pending.get(&id) {
                    Some(p) if p.tab == tab => state.pending.remove(&id),
                    _ => None,
                }
            };
            if let Some(p) = pending {
                let answer = match frame.get("error") {
                    Some(e) => Err(RelayError::Cdp(
                        e["message"].as_str().unwrap_or("an error").to_string(),
                    )),
                    None => Ok(frame["result"].clone()),
                };
                let _ = p.answer.send(answer);
            }
            return;
        }
        if let Some(method) = frame["method"].as_str() {
            if (method == "Page.loadEventFired" || method == "Page.frameNavigated")
                && let Some(t) = self.state().tabs.get_mut(tab)
            {
                t.load_failed = None;
            }
            let _ = self.page.send(PageEvent {
                tab: tab.into(),
                method: method.into(),
                params: frame["params"].clone(),
            });
        }
    }

    fn fail_tab(state: &mut State, tab: &str, why: RelayError) {
        let ids: Vec<u64> = state
            .pending
            .iter()
            .filter(|(_, p)| p.tab == tab)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(p) = state.pending.remove(&id) {
                let _ = p.answer.send(Err(why.clone()));
            }
        }
    }

    fn bump(&self) {
        self.changed.send_modify(|n| *n += 1);
    }

    /// The stream went away: once nobody follows, every waiting call fails `no_window` and every
    /// tab counts as detached until the window says `attached` again (T3's announce on connect).
    fn unfollowed(&self) {
        if self.following.fetch_sub(1, Ordering::SeqCst) == 1 {
            let mut state = self.state();
            for (_, p) in state.pending.drain() {
                let _ = p.answer.send(Err(RelayError::NoWindow));
            }
            for t in state.tabs.values_mut() {
                t.attached = false;
            }
            drop(state);
            self.bump();
        }
    }
}

/// One reader of the relay stream; dropped when the stream ends.
pub struct Following(Arc<Relay>);

impl Drop for Following {
    fn drop(&mut self) {
        self.0.unfollowed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOON: Duration = Duration::from_secs(5);

    fn attached(relay: &Relay, job: &str, tab: &str) {
        relay.receive(vec![RelayEvent::Attached {
            thread_id: job.into(),
            tab_id: tab.into(),
        }]);
    }

    #[tokio::test]
    async fn relay_no_window_is_refused_at_once_and_never_runs_elsewhere() {
        let relay = Arc::new(Relay::new());
        assert_eq!(relay.expect("j1", "tab-1"), Err(RelayError::NoWindow));
        assert_eq!(
            relay.cdp("tab-1", "Page.navigate", json!({}), SOON).await,
            Err(RelayError::NoWindow)
        );
        assert_eq!(RelayError::NoWindow.reason().0, "no_window");
    }

    #[tokio::test]
    async fn relay_a_tab_waits_for_its_webview_then_cdp_is_answered_by_its_id() {
        let relay = Arc::new(Relay::new());
        let (mut rx, _following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        assert_eq!(
            relay
                .wait_attached("tab-1", Duration::from_millis(20))
                .await,
            Err(RelayError::TimedOut(20))
        );
        // Not attached yet: no CDP goes to it.
        assert_eq!(
            relay
                .cdp("tab-1", "Runtime.evaluate", json!({}), SOON)
                .await,
            Err(RelayError::Detached)
        );
        attached(&relay, "j1", "tab-1");
        relay.wait_attached("tab-1", SOON).await.unwrap();
        let call = {
            let relay = relay.clone();
            tokio::spawn(async move {
                relay
                    .cdp(
                        "tab-1",
                        "Runtime.evaluate",
                        json!({"expression": "1+1"}),
                        SOON,
                    )
                    .await
            })
        };
        let RelayCommand::Cdp {
            thread_id,
            tab_id,
            message,
            ..
        } = rx.recv().await.unwrap()
        else {
            panic!("a cdp command");
        };
        assert_eq!((thread_id.as_str(), tab_id.as_str()), ("j1", "tab-1"));
        let frame: Value = serde_json::from_str(&message).unwrap();
        assert_eq!(frame["method"], "Runtime.evaluate");
        assert_eq!(frame["params"]["expression"], "1+1");
        // Another tab can't answer it; its own tab can.
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j2".into(),
            tab_id: "tab-2".into(),
            message: json!({"id": frame["id"], "result": {"stolen": true}}).to_string(),
        }]);
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: json!({"id": frame["id"], "result": {"result": {"value": 2}}}).to_string(),
        }]);
        assert_eq!(call.await.unwrap(), Ok(json!({"result": {"value": 2}})));
    }

    #[tokio::test]
    async fn relay_a_page_error_a_detach_and_the_window_leaving_each_end_a_waiting_call() {
        let relay = Arc::new(Relay::new());
        let (mut rx, following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        attached(&relay, "j1", "tab-1");
        let id_of = |c: RelayCommand| match c {
            RelayCommand::Cdp { message, .. } => {
                serde_json::from_str::<Value>(&message).unwrap()["id"].clone()
            }
            other => panic!("{other:?}"),
        };
        // The page's own error.
        let call = {
            let r = relay.clone();
            tokio::spawn(async move { r.cdp("tab-1", "DOM.focus", json!({}), SOON).await })
        };
        let id = id_of(rx.recv().await.unwrap());
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: json!({"id": id, "error": {"message": "No node"}}).to_string(),
        }]);
        assert_eq!(call.await.unwrap(), Err(RelayError::Cdp("No node".into())));
        // The webview going away.
        let call = {
            let r = relay.clone();
            tokio::spawn(async move { r.cdp("tab-1", "DOM.focus", json!({}), SOON).await })
        };
        let _ = rx.recv().await;
        relay.receive(vec![RelayEvent::Detached {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
        }]);
        assert_eq!(call.await.unwrap(), Err(RelayError::Detached));
        // The window leaving.
        attached(&relay, "j1", "tab-1");
        let call = {
            let r = relay.clone();
            tokio::spawn(async move { r.cdp("tab-1", "DOM.focus", json!({}), SOON).await })
        };
        let _ = rx.recv().await;
        drop(following);
        assert_eq!(call.await.unwrap(), Err(RelayError::NoWindow));
        assert!(!relay.window());
        assert!(!relay.attached("tab-1"));
    }

    #[tokio::test]
    async fn relay_a_call_with_no_answer_times_out_and_a_late_answer_goes_nowhere() {
        let relay = Arc::new(Relay::new());
        let (mut rx, _following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        attached(&relay, "j1", "tab-1");
        let r = relay
            .cdp(
                "tab-1",
                "Page.navigate",
                json!({}),
                Duration::from_millis(50),
            )
            .await;
        assert_eq!(r, Err(RelayError::TimedOut(50)));
        let id = match rx.recv().await.unwrap() {
            RelayCommand::Cdp { message, .. } => {
                serde_json::from_str::<Value>(&message).unwrap()["id"].clone()
            }
            other => panic!("{other:?}"),
        };
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: json!({"id": id, "result": {}}).to_string(),
        }]);
        assert!(relay.state().pending.is_empty());
    }

    #[tokio::test]
    async fn relay_page_events_reach_a_waiting_tool_and_a_load_failure_is_kept_until_a_load() {
        let relay = Arc::new(Relay::new());
        let (_rx, _following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        let mut events = relay.page_events();
        relay.receive(vec![
            RelayEvent::LoadFailed {
                thread_id: "j1".into(),
                tab_id: "tab-1".into(),
                url: "https://nope.invalid/".into(),
                code: -105,
                description: "ERR_NAME_NOT_RESOLVED".into(),
            },
            RelayEvent::Shown {
                thread_id: "j1".into(),
                tab_id: "tab-1".into(),
                shown: false,
            },
        ]);
        assert_eq!(relay.load_failed("tab-1").unwrap().1, -105);
        assert!(!relay.shown("tab-1"));
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: json!({"method": "Page.loadEventFired", "params": {"timestamp": 1}})
                .to_string(),
        }]);
        let e = events.recv().await.unwrap();
        assert_eq!(
            (e.tab.as_str(), e.method.as_str()),
            ("tab-1", "Page.loadEventFired")
        );
        assert_eq!(relay.load_failed("tab-1"), None);
    }

    /// A window that reconnects hears only what obid sends from then on: a command sent to the
    /// window that left (a click, a navigate) is never run again.
    #[tokio::test]
    async fn relay_a_reconnecting_window_gets_only_new_commands() {
        let relay = Arc::new(Relay::new());
        let (mut first, following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        attached(&relay, "j1", "tab-1");
        let call = {
            let r = relay.clone();
            tokio::spawn(async move {
                r.cdp("tab-1", "Input.dispatchMouseEvent", json!({}), SOON)
                    .await
            })
        };
        assert!(matches!(
            first.recv().await.unwrap(),
            RelayCommand::Cdp { .. }
        ));
        drop(following);
        // The call in flight failed, clearly, and was never sent again.
        assert_eq!(call.await.unwrap(), Err(RelayError::NoWindow));
        let (mut again, _following) = relay.follow();
        relay.turn("j1", true);
        assert_eq!(
            again.recv().await.unwrap(),
            RelayCommand::Turn {
                thread_id: "j1".into(),
                active: true
            }
        );
        assert!(
            again.try_recv().is_err(),
            "nothing else, and nothing from before"
        );
    }

    /// BROWSER-18: a foreign job's door goes to the window once when set, before anything else
    /// to a window that follows anew, and again when obid makes a tab of that job's; never once
    /// the job ended.
    #[tokio::test]
    async fn browser_18_the_window_is_given_a_foreign_jobs_door_and_again_when_it_follows_anew() {
        let relay = Arc::new(Relay::new());
        let (mut first, _following) = relay.follow();
        let door = RelayCommand::Box {
            thread_id: "j1".into(),
            partition: "oby-box-j1".into(),
            proxy: "http://oby:t@127.0.0.1:1".into(),
        };
        relay.set_box("j1", "oby-box-j1", "http://oby:t@127.0.0.1:1");
        relay.set_box("j1", "oby-box-j1", "http://oby:t@127.0.0.1:1");
        assert_eq!(first.recv().await.unwrap(), door);
        assert!(first.try_recv().is_err(), "the same door again sends none");
        let (mut again, _again) = relay.follow();
        relay.turn("j1", true);
        assert_eq!(again.recv().await.unwrap(), door);
        assert!(matches!(
            again.recv().await.unwrap(),
            RelayCommand::Turn { .. }
        ));
        relay.expect("j1", "tab-1").unwrap();
        assert_eq!(again.recv().await.unwrap(), door);
        assert_eq!(door.kind(), "box");
        relay.forget_box("j1");
        let (mut after, _after) = relay.follow();
        relay.expect("j1", "tab-2").unwrap();
        assert!(after.try_recv().is_err(), "no door once the job ended");
    }

    /// BROWSER-33: the agent's own script goes out marked `"agentCode": true`; obid's own frames
    /// carry no `agentCode` key, byte for byte as before.
    #[tokio::test]
    async fn browser_33_the_agents_own_script_is_marked_agent_code_and_obids_are_not() {
        let relay = Arc::new(Relay::new());
        let (mut rx, _following) = relay.follow();
        relay.expect("j1", "tab-1").unwrap();
        attached(&relay, "j1", "tab-1");
        let mut lines = Vec::new();
        for agent in [true, false] {
            let call = {
                let r = relay.clone();
                tokio::spawn(async move {
                    let params = json!({"expression": "1+1"});
                    if agent {
                        r.cdp_agent("tab-1", "Runtime.evaluate", params, SOON).await
                    } else {
                        r.cdp("tab-1", "Runtime.evaluate", params, SOON).await
                    }
                })
            };
            let command = rx.recv().await.unwrap();
            let line = serde_json::to_string(&command).unwrap();
            let RelayCommand::Cdp { message, .. } = command else {
                panic!("a cdp command");
            };
            let id = serde_json::from_str::<Value>(&message).unwrap()["id"].clone();
            relay.receive(vec![RelayEvent::Cdp {
                thread_id: "j1".into(),
                tab_id: "tab-1".into(),
                message: json!({"id": id, "result": {}}).to_string(),
            }]);
            assert_eq!(call.await.unwrap(), Ok(json!({})));
            lines.push(line);
        }
        assert!(lines[0].contains(r#""agentCode":true"#), "{}", lines[0]);
        for line in &lines {
            let v: Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["type"], "cdp");
            assert_eq!(
                (v["threadId"].as_str(), v["tabId"].as_str()),
                (Some("j1"), Some("tab-1"))
            );
            assert!(v["message"].is_string());
        }
        let obids: Value = serde_json::from_str(&lines[1]).unwrap();
        assert!(obids.get("agentCode").is_none(), "{}", lines[1]);
    }

    #[test]
    fn relay_the_wire_is_t3s_unions_keyed_by_thread_and_tab() {
        let e: RelayEvent = serde_json::from_value(
            json!({"type": "loadFailed", "threadId": "j1", "tabId": "tab-1", "url": "u", "code": -3, "description": "d"}),
        )
        .unwrap();
        assert!(matches!(e, RelayEvent::LoadFailed { code: -3, .. }));
        let e: RelayEvent =
            serde_json::from_value(json!({"type": "somethingNew", "threadId": "j1"})).unwrap();
        assert_eq!(e, RelayEvent::Unknown);
        let c = RelayCommand::Pointer {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            phase: PointerPhase::Click,
            x: 1.5,
            y: 2.0,
        };
        assert_eq!(
            serde_json::to_value(&c).unwrap(),
            json!({"type": "pointer", "threadId": "j1", "tabId": "tab-1", "phase": "click", "x": 1.5, "y": 2.0})
        );
        assert_eq!(c.kind(), "pointer");
    }
}
