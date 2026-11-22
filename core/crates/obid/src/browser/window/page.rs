//! The engine's words in CDP (BROWSER-9 to 13): each command the core lets start
//! (`open <url>`, `snapshot`, `click @e3`, …, the words agent-browser took) said to the tab's page
//! through the window's relay, its answer in the shape agent-browser gave, so the run and its
//! shaping stay the engine's. Page level only: no `Target`, `Browser` or session ids. It owns no
//! state but the driver's; it writes a screenshot into the agent's room when asked.

use std::time::Duration;

use base64::Engine as _;
use serde_json::{Value, json};

use super::Call;
use super::WindowDriver;
use super::snapshot::snapshot;
use crate::browser::contract::PointerPhase;
use crate::browser::relay::RelayError;

/// How long the agent's pointer rests on its target before the click (T3's 160 ms).
const POINTER_REST: Duration = Duration::from_millis(160);
/// How long a page may take to move on after its dialog was accepted.
const LAND_AFTER_DIALOG: Duration = Duration::from_millis(1500);
/// How long the person's Reload, Back or dialog answer may take.
const PERSON_LIMIT: Duration = Duration::from_secs(15);
/// How often a wait looks at the page again.
const WAIT_EVERY: Duration = Duration::from_millis(100);

/// A step's failure: why the command didn't run, in agent-browser's words.
enum Stop {
    Failed(String),
    TimedOut,
}

impl From<RelayError> for Stop {
    fn from(e: RelayError) -> Stop {
        match e {
            RelayError::TimedOut(_) => Stop::TimedOut,
            other => Stop::Failed(other.reason().1),
        }
    }
}

type Step<T> = Result<T, Stop>;

/// A deadline: what is left of the command's time.
#[derive(Clone, Copy)]
struct Until(tokio::time::Instant);

impl Until {
    fn left(&self) -> Duration {
        self.0
            .saturating_duration_since(tokio::time::Instant::now())
    }
}

/// The modifier bits CDP's `Input.dispatchKeyEvent` takes.
fn modifier_bit(m: &str) -> i64 {
    match m.to_ascii_lowercase().as_str() {
        "alt" | "option" => 1,
        "control" | "ctrl" => 2,
        "meta" | "cmd" | "command" | "controlormeta" => 4,
        "shift" => 8,
        _ => 0,
    }
}

/// A key's `key`, `code` and virtual key code, for the keys an agent presses.
fn key_of(key: &str) -> (String, String, i64) {
    let named = [
        ("Enter", 13),
        ("Tab", 9),
        ("Escape", 27),
        ("Backspace", 8),
        ("Delete", 46),
        ("ArrowLeft", 37),
        ("ArrowUp", 38),
        ("ArrowRight", 39),
        ("ArrowDown", 40),
        ("Home", 36),
        ("End", 35),
        ("PageUp", 33),
        ("PageDown", 34),
        ("Space", 32),
    ];
    if let Some((k, code)) = named.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        let key = if *k == "Space" { " " } else { *k };
        return (key.to_string(), k.to_string(), *code);
    }
    let c = key.chars().next().unwrap_or(' ');
    let upper = c.to_ascii_uppercase();
    let code = if c.is_ascii_digit() {
        format!("Digit{c}")
    } else {
        format!("Key{upper}")
    };
    (key.to_string(), code, upper as i64)
}

/// Whether a URL is a `javascript:` one, read as the browser reads it (WHATWG URL,
/// `reqwest::Url`: ` JavaScript:` and `java\tscript:` too).
fn runs_script(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| u.scheme() == "javascript")
}

impl WindowDriver {
    /// One CDP command for `tab`'s page, within what is left of `until`.
    async fn send(&self, tab: &str, method: &str, params: Value, until: Until) -> Step<Value> {
        Ok(self.relay.cdp(tab, method, params, until.left()).await?)
    }

    /// The job's current tab.
    fn current(&self, job: &str) -> Option<String> {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(job)
            .cloned()
    }

    fn set_current(&self, job: &str, tab: &str) {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(job.into(), tab.into());
    }

    /// The page's domain on, once per tab: obid hears its loads and dialogs, and its downloads go
    /// to the job's own folder.
    async fn enable(&self, job: &str, tab: &str, until: Until) -> Step<()> {
        if self
            .enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(tab)
        {
            return Ok(());
        }
        self.send(tab, "Page.enable", json!({}), until).await?;
        let folder = self.downloads_folder(job);
        let _ = crate::store::make_private_dir(&folder);
        self.send(
            tab,
            "Page.setDownloadBehavior",
            json!({"behavior": "allow", "downloadPath": folder}),
            until,
        )
        .await?;
        self.enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tab.into());
        Ok(())
    }

    /// A new tab for `job` at `url`: the core's tab first (so its `tab` event, with its partition,
    /// tells the window to mount its webview), then the webview attached, then its page loaded.
    async fn new_tab(&self, job: &str, url: &str, until: Until) -> Step<Value> {
        let tab = self
            .core
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .open_new(job, url);
        if let Err(e) = self.relay.expect(job, &tab) {
            self.core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed(&tab);
            return Err(e.into());
        }
        self.set_current(job, &tab);
        self.tabs_changed(job);
        let attached = self
            .relay
            .wait_attached(&tab, super::ATTACH_WITHIN.min(until.left()))
            .await;
        if let Err(e) = attached {
            self.relay.release(&tab);
            self.core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed(&tab);
            self.tabs_changed(job);
            return Err(match e {
                RelayError::TimedOut(_) => Stop::Failed(
                    "Oby's window didn't open the tab's page in time (is its window open?).".into(),
                ),
                other => other.into(),
            });
        }
        self.enable(job, &tab, until).await?;
        if let Some((_, _, description)) = self.relay.load_failed(&tab) {
            return Err(Stop::Failed(format!("{url}: {description}")));
        }
        self.page_info(&tab, until).await
    }

    /// The page's URL and title now.
    async fn page_info(&self, tab: &str, until: Until) -> Step<Value> {
        let v = self
            .eval_value(tab, "({url: location.href, title: document.title})", until)
            .await?;
        Ok(json!({"url": v["url"], "title": v["title"]}))
    }

    /// `expression`'s value in the page (awaited, by value), or the page's exception.
    async fn eval_value(&self, tab: &str, expression: &str, until: Until) -> Step<Value> {
        self.eval_with(tab, expression, false, until).await
    }

    /// [`Self::eval_value`], its frame marked `agentCode` when it is the agent's own script
    /// (BROWSER-33).
    async fn eval_with(
        &self,
        tab: &str,
        expression: &str,
        agent_code: bool,
        until: Until,
    ) -> Step<Value> {
        let params = json!({"expression": expression, "returnByValue": true, "awaitPromise": true});
        let r = if agent_code {
            self.relay
                .cdp_agent(tab, "Runtime.evaluate", params, until.left())
                .await?
        } else {
            self.send(tab, "Runtime.evaluate", params, until).await?
        };
        if let Some(e) = r.get("exceptionDetails") {
            let text = e["exception"]["description"]
                .as_str()
                .or_else(|| e["text"].as_str())
                .unwrap_or("the script threw");
            return Err(Stop::Failed(text.to_string()));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Navigates the tab to `url` and waits for its load.
    async fn navigate(&self, job: &str, tab: &str, url: &str, until: Until) -> Step<Value> {
        self.enable(job, tab, until).await?;
        let mut events = self.relay.page_events();
        let params = json!({"url": url});
        // A `javascript:` URL runs the agent's own script in the page: marked (BROWSER-33).
        let r = if runs_script(url) {
            self.relay
                .cdp_agent(tab, "Page.navigate", params, until.left())
                .await?
        } else {
            self.send(tab, "Page.navigate", params, until).await?
        };
        if let Some(e) = r["errorText"].as_str().filter(|e| !e.is_empty()) {
            return Err(Stop::Failed(format!("{url}: {e}")));
        }
        // A navigation inside the page (a fragment) fires no load.
        if r["loaderId"].is_string() {
            let tab_id = tab.to_string();
            let loaded = async {
                loop {
                    match events.recv().await {
                        Ok(e) if e.tab == tab_id && e.method == "Page.loadEventFired" => break,
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
            };
            if tokio::time::timeout(until.left(), loaded).await.is_err() {
                return Err(Stop::TimedOut);
            }
        }
        self.refs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(tab);
        self.page_info(tab, until).await
    }

    /// The DOM node a ref names, from the tab's last snapshot.
    fn node_of(&self, tab: &str, r: &str) -> Step<i64> {
        let name = r.trim_start_matches('@');
        self.refs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(tab)
            .and_then(|m| m.get(name).copied())
            .ok_or_else(|| Stop::Failed(format!("{name} isn't a ref of this page's last snapshot")))
    }

    /// The center of a node, in the page's CSS pixels, scrolled into view first.
    async fn center(&self, tab: &str, node: i64, until: Until) -> Step<(f64, f64)> {
        let _ = self
            .send(
                tab,
                "DOM.scrollIntoViewIfNeeded",
                json!({"backendNodeId": node}),
                until,
            )
            .await;
        let quads = self
            .send(
                tab,
                "DOM.getContentQuads",
                json!({"backendNodeId": node}),
                until,
            )
            .await?;
        let q = quads["quads"][0]
            .as_array()
            .filter(|q| q.len() == 8)
            .ok_or_else(|| Stop::Failed("that element isn't on the page's screen".into()))?;
        let at = |i: usize| q[i].as_f64().unwrap_or(0.0);
        Ok((
            (at(0) + at(2) + at(4) + at(6)) / 4.0,
            (at(1) + at(3) + at(5) + at(7)) / 4.0,
        ))
    }

    /// A mouse event at a point.
    async fn mouse(
        &self,
        tab: &str,
        kind: &str,
        (x, y): (f64, f64),
        clicks: i64,
        until: Until,
    ) -> Step<()> {
        let mut params = json!({"type": kind, "x": x, "y": y});
        if kind != "mouseMoved" {
            params["button"] = json!("left");
            params["clickCount"] = json!(clicks);
        }
        self.send(tab, "Input.dispatchMouseEvent", params, until)
            .await?;
        Ok(())
    }

    /// Clicks at a point, the agent's pointer drawn there first; a dialog the click opened
    /// answers as agent-browser's `warning`.
    async fn click_at(&self, tab: &str, at: (f64, f64), clicks: i64, until: Until) -> Step<Value> {
        self.relay.pointer(tab, PointerPhase::Move, at.0, at.1);
        tokio::time::sleep(POINTER_REST.min(until.left())).await;
        self.relay.pointer(tab, PointerPhase::Click, at.0, at.1);
        let mut events = self.relay.page_events();
        self.mouse(tab, "mouseMoved", at, 0, until).await?;
        self.mouse(tab, "mousePressed", at, clicks, until).await?;
        let tab_id = tab.to_string();
        let dialog = async {
            loop {
                match events.recv().await {
                    Ok(e) if e.tab == tab_id && e.method == "Page.javascriptDialogOpening" => {
                        return Some(e.params["message"].as_str().unwrap_or_default().to_string());
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return None,
                }
            }
        };
        tokio::select! {
            released = self.mouse(tab, "mouseReleased", at, clicks, until) => {
                released?;
                Ok(json!({}))
            }
            Some(words) = dialog => Ok(json!({"warning": words})),
        }
    }

    /// Runs a function on a node in the page (`this` is the element).
    async fn on_node(
        &self,
        tab: &str,
        node: i64,
        function: &str,
        args: Value,
        until: Until,
    ) -> Step<Value> {
        let resolved = self
            .send(
                tab,
                "DOM.resolveNode",
                json!({"backendNodeId": node}),
                until,
            )
            .await?;
        let object = resolved["object"]["objectId"]
            .as_str()
            .ok_or_else(|| Stop::Failed("that element is gone from the page".into()))?
            .to_string();
        let r = self
            .send(
                tab,
                "Runtime.callFunctionOn",
                json!({"objectId": object, "functionDeclaration": function,
                       "arguments": [{"value": args}], "returnByValue": true, "awaitPromise": true}),
                until,
            )
            .await?;
        Ok(r["result"]["value"].clone())
    }

    /// Presses a key chord (`Shift+Tab`, `Enter`).
    async fn press(&self, tab: &str, chord: &str, until: Until) -> Step<()> {
        let parts: Vec<&str> = chord.split('+').filter(|p| !p.is_empty()).collect();
        let (mods, key) = parts.split_at(parts.len().saturating_sub(1));
        let modifiers = mods.iter().map(|m| modifier_bit(m)).sum::<i64>();
        let (key, code, vk) = key_of(key.first().copied().unwrap_or("Enter"));
        let text = (key.chars().count() == 1 && modifiers & !8 == 0).then(|| key.clone());
        let mut down = json!({"type": if text.is_some() { "keyDown" } else { "rawKeyDown" },
            "key": key, "code": code, "windowsVirtualKeyCode": vk, "modifiers": modifiers});
        if let Some(t) = &text {
            down["text"] = json!(t);
        }
        self.send(tab, "Input.dispatchKeyEvent", down, until)
            .await?;
        self.send(
            tab,
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk, "modifiers": modifiers}),
            until,
        )
        .await?;
        Ok(())
    }

    /// One of the engine's commands, said in CDP: its data, as agent-browser answered it.
    pub async fn call(&self, job: &str, words: &[String], limit: Duration) -> Call {
        let started = tokio::time::Instant::now();
        let until = Until(started + limit);
        match self.say(job, words, until).await {
            Ok(v) => Call::Ok(v),
            Err(Stop::Failed(e)) => Call::Failed(e),
            Err(Stop::TimedOut) => Call::TimedOut(started.elapsed().as_millis() as u64),
        }
    }

    async fn say(&self, job: &str, words: &[String], until: Until) -> Step<Value> {
        let w: Vec<&str> = words.iter().map(String::as_str).collect();
        let current = || {
            self.current(job)
                .ok_or_else(|| Stop::Failed("This job has no tab yet.".into()))
        };
        match w.as_slice() {
            ["tab", "list"] => {
                let active = self.current(job);
                let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let tabs: Vec<Value> = core
                    .tabs_of(job)
                    .iter()
                    .map(|t| {
                        json!({"targetId": t.target, "url": t.url,
                                    "active": active.as_deref() == Some(t.id.as_str())})
                    })
                    .collect();
                Ok(json!({"tabs": tabs}))
            }
            ["tab", "new", url] => self.new_tab(job, url, until).await,
            ["tab", "close", target] => {
                self.relay.release(target);
                self.forget(target);
                let next = {
                    let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                    core.tabs_of(job)
                        .iter()
                        .map(|t| t.id.clone())
                        .rfind(|id| id != target)
                };
                match next {
                    Some(n) => self.set_current(job, &n),
                    None => {
                        self.active
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(job);
                    }
                }
                Ok(json!({}))
            }
            ["tab", target] => {
                self.set_current(job, target);
                Ok(json!({}))
            }
            ["open", url] => match self.current(job) {
                None => self.new_tab(job, url, until).await,
                Some(tab) => self.navigate(job, &tab, url, until).await,
            },
            ["get", "url"] => {
                let tab = current()?;
                let url = self.eval_value(&tab, "location.href", until).await?;
                Ok(json!({"url": url}))
            }
            ["snapshot"] => {
                let tab = current()?;
                let tree = self
                    .send(&tab, "Accessibility.getFullAXTree", json!({}), until)
                    .await?;
                let s = snapshot(&tree);
                self.refs
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(tab, s.nodes);
                Ok(json!({"snapshot": s.text, "refs": s.refs}))
            }
            ["eval", expression] => {
                let tab = current()?;
                let result = self.eval_with(&tab, expression, true, until).await?;
                Ok(json!({"result": result}))
            }
            ["wait", rest @ ..] => {
                let tab = current()?;
                let mut probe = String::new();
                let mut wait_until = until;
                let mut i = 0;
                while i < rest.len() {
                    match rest[i] {
                        "--text" if i + 1 < rest.len() => {
                            probe = format!(
                                "(document.body ? document.body.innerText : '').includes({})",
                                json!(rest[i + 1])
                            );
                            i += 2;
                        }
                        "--url" if i + 1 < rest.len() => {
                            probe = format!("location.href.includes({})", json!(rest[i + 1]));
                            i += 2;
                        }
                        "--timeout" if i + 1 < rest.len() => {
                            if let Ok(ms) = rest[i + 1].parse::<u64>() {
                                let own = tokio::time::Instant::now() + Duration::from_millis(ms);
                                wait_until = Until(own.min(until.0));
                            }
                            i += 2;
                        }
                        selector => {
                            probe = format!("!!document.querySelector({})", json!(selector));
                            i += 1;
                        }
                    }
                }
                loop {
                    if self.eval_value(&tab, &probe, wait_until).await? == json!(true) {
                        return Ok(json!({}));
                    }
                    if wait_until.left() <= WAIT_EVERY {
                        return Err(Stop::TimedOut);
                    }
                    tokio::time::sleep(WAIT_EVERY).await;
                }
            }
            ["dialog", "dismiss"] => {
                let tab = current()?;
                self.send(
                    &tab,
                    "Page.handleJavaScriptDialog",
                    json!({"accept": false}),
                    until,
                )
                .await?;
                Ok(json!({}))
            }
            ["dialog", "accept", prompt @ ..] => {
                let tab = current()?;
                let mut p = json!({"accept": true});
                if let Some(text) = prompt.first() {
                    p["promptText"] = json!(text);
                }
                self.send(&tab, "Page.handleJavaScriptDialog", p, until)
                    .await?;
                Ok(json!({}))
            }
            ["set", "viewport", w, h] => {
                let tab = current()?;
                self.send(
                    &tab,
                    "Emulation.setDeviceMetricsOverride",
                    json!({"width": w.parse::<u64>().unwrap_or(1280),
                           "height": h.parse::<u64>().unwrap_or(800),
                           "deviceScaleFactor": 0, "mobile": false}),
                    until,
                )
                .await?;
                Ok(json!({}))
            }
            ["set", "media", scheme] => {
                let tab = current()?;
                self.send(
                    &tab,
                    "Emulation.setEmulatedMedia",
                    json!({"features": [{"name": "prefers-color-scheme", "value": scheme}]}),
                    until,
                )
                .await?;
                Ok(json!({}))
            }
            ["screenshot", path] => {
                let tab = current()?;
                let shot = self
                    .send(
                        &tab,
                        "Page.captureScreenshot",
                        json!({"format": "png"}),
                        until,
                    )
                    .await?;
                let png = base64::engine::general_purpose::STANDARD
                    .decode(shot["data"].as_str().unwrap_or_default())
                    .map_err(|e| Stop::Failed(format!("the screenshot isn't an image: {e}")))?;
                std::fs::write(path, png).map_err(|e| {
                    Stop::Failed(format!("obid couldn't write the screenshot: {e}"))
                })?;
                Ok(json!({"path": path}))
            }
            ["click", r] | ["dblclick", r] => {
                let tab = current()?;
                let node = self.node_of(&tab, r)?;
                let at = self.center(&tab, node, until).await?;
                let clicks = if w[0] == "dblclick" { 2 } else { 1 };
                self.click_at(&tab, at, clicks, until).await
            }
            ["hover", r] => {
                let tab = current()?;
                let node = self.node_of(&tab, r)?;
                let at = self.center(&tab, node, until).await?;
                self.relay.pointer(&tab, PointerPhase::Move, at.0, at.1);
                self.mouse(&tab, "mouseMoved", at, 0, until).await?;
                Ok(json!({}))
            }
            ["type", r, text] | ["fill", r, text] => {
                let tab = current()?;
                let node = self.node_of(&tab, r)?;
                self.send(&tab, "DOM.focus", json!({"backendNodeId": node}), until)
                    .await?;
                if w[0] == "fill" {
                    self.on_node(
                        &tab,
                        node,
                        "function() { if ('value' in this) { this.value = ''; this.dispatchEvent(new Event('input', {bubbles: true})); } }",
                        json!(null),
                        until,
                    )
                    .await?;
                }
                self.send(&tab, "Input.insertText", json!({"text": text}), until)
                    .await?;
                Ok(json!({}))
            }
            ["select", r, values @ ..] => {
                let tab = current()?;
                let node = self.node_of(&tab, r)?;
                let chosen = self
                    .on_node(
                        &tab,
                        node,
                        "function(values) { const want = new Set(values); let n = 0; for (const o of this.options || []) { o.selected = want.has(o.value) || want.has(o.label); if (o.selected) n++; } this.dispatchEvent(new Event('input', {bubbles: true})); this.dispatchEvent(new Event('change', {bubbles: true})); return n; }",
                        json!(values),
                        until,
                    )
                    .await?;
                if chosen == json!(0) {
                    return Err(Stop::Failed(
                        "none of those values is one of its options".into(),
                    ));
                }
                Ok(json!({}))
            }
            ["drag", from, to] => {
                let tab = current()?;
                let a = self.center(&tab, self.node_of(&tab, from)?, until).await?;
                let b = self.center(&tab, self.node_of(&tab, to)?, until).await?;
                self.relay.pointer(&tab, PointerPhase::Move, a.0, a.1);
                self.mouse(&tab, "mouseMoved", a, 0, until).await?;
                self.mouse(&tab, "mousePressed", a, 1, until).await?;
                for step in 1..=5 {
                    let f = step as f64 / 5.0;
                    let p = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                    self.mouse(&tab, "mouseMoved", p, 0, until).await?;
                }
                self.mouse(&tab, "mouseReleased", b, 1, until).await?;
                Ok(json!({}))
            }
            ["upload", r, paths @ ..] => {
                let tab = current()?;
                let node = self.node_of(&tab, r)?;
                self.send(
                    &tab,
                    "DOM.setFileInputFiles",
                    json!({"files": paths, "backendNodeId": node}),
                    until,
                )
                .await?;
                Ok(json!({}))
            }
            ["press", chord] => {
                let tab = current()?;
                self.press(&tab, chord, until).await?;
                Ok(json!({}))
            }
            ["scroll", dir, px] => {
                let tab = current()?;
                let px = px.parse::<f64>().unwrap_or(100.0);
                let (dx, dy) = match *dir {
                    "up" => (0.0, -px),
                    "left" => (-px, 0.0),
                    "right" => (px, 0.0),
                    _ => (0.0, px),
                };
                let size = self
                    .eval_value(&tab, "({w: innerWidth, h: innerHeight})", until)
                    .await?;
                let at = (
                    size["w"].as_f64().unwrap_or(800.0) / 2.0,
                    size["h"].as_f64().unwrap_or(600.0) / 2.0,
                );
                self.send(
                    &tab,
                    "Input.dispatchMouseEvent",
                    json!({"type": "mouseWheel", "x": at.0, "y": at.1, "deltaX": dx, "deltaY": dy}),
                    until,
                )
                .await?;
                Ok(json!({}))
            }
            other => Err(Stop::Failed(format!(
                "obid's window browser doesn't know `{}`",
                other.join(" ")
            ))),
        }
    }
}

impl WindowDriver {
    /// The person's Reload, Back or answer to the page's dialog, on one of a job's tabs
    /// (BROWSER-26): said to the tab's page as the window would, the page let land, then told to
    /// the core (the agent's refs dead). A page that still shows the browser's own error page
    /// (`chrome-error://`) is refused, in its words.
    pub async fn person_page(
        &self,
        job: &str,
        tab: &str,
        verb: &str,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> Result<String, String> {
        let until = Until(tokio::time::Instant::now() + PERSON_LIMIT);
        let said: Step<String> = async {
            self.enable(job, tab, until).await?;
            let mut events = self.relay.page_events();
            let moves = match verb {
                crate::browser::contract::VERB_DIALOG => {
                    let mut p = json!({"accept": accept});
                    if let Some(t) = prompt_text {
                        p["promptText"] = json!(t);
                    }
                    self.send(tab, "Page.handleJavaScriptDialog", p, until)
                        .await?;
                    self.core
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .dialog(tab, None);
                    // An accepted dialog may send the page on.
                    accept
                }
                crate::browser::contract::VERB_BACK => {
                    let history = self
                        .send(tab, "Page.getNavigationHistory", json!({}), until)
                        .await?;
                    let at = history["currentIndex"].as_i64().unwrap_or(0);
                    let Some(entry) = (at > 0)
                        .then(|| history["entries"][(at - 1) as usize]["id"].as_i64())
                        .flatten()
                    else {
                        return Err(Stop::Failed("This tab has no page to go back to.".into()));
                    };
                    self.send(
                        tab,
                        "Page.navigateToHistoryEntry",
                        json!({"entryId": entry}),
                        until,
                    )
                    .await?;
                    true
                }
                _ => {
                    self.send(tab, "Page.reload", json!({}), until).await?;
                    true
                }
            };
            if moves {
                // The page lands: its load, or (a page that stays) a short while with none.
                let tab_id = tab.to_string();
                let landed = async {
                    loop {
                        match events.recv().await {
                            Ok(e)
                                if e.tab == tab_id
                                    && (e.method == "Page.loadEventFired"
                                        || e.method == "Page.navigatedWithinDocument") =>
                            {
                                break;
                            }
                            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }
                };
                let wait = if verb == crate::browser::contract::VERB_DIALOG {
                    LAND_AFTER_DIALOG
                } else {
                    until.left()
                };
                let _ = tokio::time::timeout(wait, landed).await;
            }
            self.refs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(tab);
            let info = self.page_info(tab, until).await?;
            let url = info["url"].as_str().unwrap_or_default().to_string();
            let title = info["title"].as_str().map(str::to_string);
            self.core
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .person_moved(tab, &url, title.as_deref());
            if url.starts_with("chrome-error://") {
                return Err(Stop::Failed(format!(
                    "{verb}: the page still doesn't load: the browser shows its own error page."
                )));
            }
            Ok(url)
        }
        .await;
        match said {
            Ok(url) => Ok(url),
            Err(Stop::Failed(e)) => Err(e),
            Err(Stop::TimedOut) => Err("The page didn't answer in time.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::browser::contract::{RelayCommand, RelayEvent};
    use crate::browser::relay::Relay;

    /// Answers the next `cdp` command with `result`: its method and whether it was marked.
    async fn answer(
        relay: &Relay,
        rx: &mut tokio::sync::broadcast::Receiver<RelayCommand>,
        result: Value,
    ) -> (String, bool) {
        let RelayCommand::Cdp {
            message,
            agent_code,
            ..
        } = rx.recv().await.unwrap()
        else {
            panic!("a cdp command");
        };
        let frame: Value = serde_json::from_str(&message).unwrap();
        relay.receive(vec![RelayEvent::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: json!({"id": frame["id"], "result": result}).to_string(),
        }]);
        (frame["method"].as_str().unwrap().to_string(), agent_code)
    }

    /// BROWSER-33: only the agent's own script (`eval`, a `javascript:` URL it opens) goes out
    /// marked; obid's own reads don't.
    #[tokio::test]
    async fn browser_33_only_evaluate_sends_agent_code() {
        let relay = Arc::new(Relay::new());
        let (mut rx, _following) = relay.follow();
        let home = std::env::temp_dir().join(format!("obid-page-{}", std::process::id()));
        let d = WindowDriver::new(
            relay.clone(),
            Arc::new(Mutex::new(crate::browser::engine::Browsers::new())),
            &home,
        );
        relay.expect("j1", "tab-1").unwrap();
        relay.receive(vec![RelayEvent::Attached {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
        }]);
        d.set_current("j1", "tab-1");
        let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let limit = Duration::from_secs(5);
        let (eval, url, snap) = (
            words(&["eval", "1+1"]),
            words(&["get", "url"]),
            words(&["snapshot"]),
        );

        let (called, sent) = tokio::join!(
            d.call("j1", &eval, limit),
            answer(&relay, &mut rx, json!({"result": {"value": 2}}))
        );
        assert_eq!(sent, ("Runtime.evaluate".to_string(), true));
        assert_eq!(called, Call::Ok(json!({"result": 2})));

        let (_, sent) = tokio::join!(
            d.call("j1", &url, limit),
            answer(&relay, &mut rx, json!({"result": {"value": "https://a/"}}))
        );
        assert_eq!(sent, ("Runtime.evaluate".to_string(), false));

        let (_, sent) = tokio::join!(
            d.call("j1", &snap, limit),
            answer(&relay, &mut rx, json!({"nodes": []}))
        );
        assert_eq!(sent, ("Accessibility.getFullAXTree".to_string(), false));

        // A `javascript:` URL the agent opens is its own script too; a page's address isn't.
        // The first open enables the tab's page (two frames), then navigates and reads the page.
        let (js, web) = (
            words(&["open", "javascript:1"]),
            words(&["open", "https://a/"]),
        );
        let (_, sent) = tokio::join!(d.call("j1", &js, limit), async {
            let mut sent = Vec::new();
            for _ in 0..4 {
                sent.push(answer(&relay, &mut rx, json!({})).await);
            }
            sent
        });
        assert!(
            sent.contains(&("Page.navigate".to_string(), true)),
            "{sent:?}"
        );
        let (_, sent) = tokio::join!(d.call("j1", &web, limit), async {
            let mut sent = Vec::new();
            for _ in 0..2 {
                sent.push(answer(&relay, &mut rx, json!({})).await);
            }
            sent
        });
        assert!(
            sent.contains(&("Page.navigate".to_string(), false)),
            "{sent:?}"
        );
        assert!(runs_script(" JavaScript:1") && runs_script("java\tscript:1"));
        assert!(!runs_script("https://a/") && !runs_script("about:blank"));
    }
}
