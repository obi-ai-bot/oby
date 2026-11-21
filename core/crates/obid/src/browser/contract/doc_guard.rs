//! The guard that keeps docs/api/browser.md and the contract's wire types alike (the lab's TWIN
//! test did the same for the TypeScript twin): every key a wire type serializes is named in the
//! doc, and every key of every ```json block in the doc is one of the contract's. Tests only: it
//! owns no state and writes nothing.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::*;

const DOC: &str = include_str!("../../../../../../docs/api/browser.md");

/// Keys whose values aren't the contract's own shapes: a tool's JSON Schema, a page's data.
const OPAQUE: &[&str] = &["inputSchema", "data"];

fn keys_of(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                out.insert(k.clone());
                if !OPAQUE.contains(&k.as_str()) {
                    keys_of(v, out);
                }
            }
        }
        Value::Array(xs) => xs.iter().for_each(|x| keys_of(x, out)),
        _ => {}
    }
}

fn sample_ask() -> Ask {
    Ask {
        id: "ask-1".into(),
        tab_id: "tab-1".into(),
        job: "j1".into(),
        kind: AskKind::Choice,
        reason: "r".into(),
        choices: vec!["a".into()],
        field: Some("#pw".into()),
        answered: true,
        choice: Some("a".into()),
    }
}

fn sample_tab() -> Tab {
    Tab {
        id: "tab-1".into(),
        owner: Owner::Agent {
            job: "j1".into(),
            agent: "codex".into(),
        },
        title: "t".into(),
        url: "u".into(),
        state: TabState::Ready,
        held_by: Some("d".into()),
        hold_generation: 1,
        computer: Computer::this_mac(),
        ask: Some(sample_ask()),
        dialog: Some("words".into()),
        partition: Some("persist:oby-agent-codex".into()),
    }
}

fn sample_download() -> Download {
    Download {
        id: "dl-1".into(),
        job: "j1".into(),
        tab_id: Some("tab-1".into()),
        name: "f.pdf".into(),
        started_by: StartedBy::Agent,
        place: Place::Profile,
        bytes: 1,
    }
}

fn sample_browser() -> JobBrowser {
    JobBrowser {
        job: "j1".into(),
        agent: "codex".into(),
        state: BrowserState::Failed,
        reason: Some("why".into()),
    }
}

/// A sample of every wire type, each optional key filled so it serializes.
fn samples() -> Vec<Value> {
    let outside = Outside {
        from: "u".into(),
        what: "page data".into(),
        note: "n".into(),
        data: json!("d"),
    };
    let bodies = vec![
        EventBody::Tab { tab: sample_tab() },
        EventBody::TabClosed {
            tab_id: "tab-1".into(),
        },
        EventBody::Action {
            tab_id: Some("tab-1".into()),
            job: "j1".into(),
            tool: "oby_browser_act".into(),
            text: "Clicking Sign in".into(),
            pointer: Some(Point { x: 1.0, y: 2.0 }),
            outcome: Outcome::Done,
            reason: Some("human_control".into()),
        },
        EventBody::Ask { ask: sample_ask() },
        EventBody::AskAnswered {
            ask_id: "ask-1".into(),
        },
        EventBody::Download {
            download: sample_download(),
        },
        EventBody::ScriptRun {
            tab_id: "tab-1".into(),
            job: "j1".into(),
            expression: "1".into(),
            result: outside.clone(),
        },
        EventBody::Settings {
            settings: Settings::default(),
        },
        EventBody::Browser {
            browser: sample_browser(),
        },
    ];
    let mut out: Vec<Value> = bodies
        .into_iter()
        .enumerate()
        .map(|(n, body)| {
            serde_json::to_value(Event {
                seq: n as u64,
                body,
            })
            .unwrap()
        })
        .collect();
    let mut more = vec![
        serde_json::to_value(TabsAnswer {
            tabs: vec![sample_tab()],
            settings: Settings::default(),
            browsers: vec![sample_browser()],
            seq: 1,
        })
        .unwrap(),
        serde_json::to_value(SettingsPatch {
            agents_use_browser: Some(true),
            agents_use_mac: Some(true),
        })
        .unwrap(),
        serde_json::to_value(DownloadsAnswer {
            downloads: vec![sample_download()],
            seq: 1,
        })
        .unwrap(),
        serde_json::to_value(SaveCopyAnswer {
            id: "dl-1".into(),
            saved_as: "f.pdf".into(),
        })
        .unwrap(),
        serde_json::to_value(AnswerBody {
            choice: Some("a".into()),
        })
        .unwrap(),
        serde_json::to_value(HoldAnswer {
            tab_id: "tab-1".into(),
            held_by: Some("d".into()),
            hold_generation: 1,
        })
        .unwrap(),
        serde_json::to_value(SecretAnswer {
            tab_id: "tab-1".into(),
            field: "#pw".into(),
            filled: true,
        })
        .unwrap(),
        serde_json::to_value(DialogBody {
            accept: true,
            prompt_text: Some("p".into()),
        })
        .unwrap(),
        serde_json::to_value(ReloadBody {}).unwrap(),
        // The window's relay (T3 Code's design).
        serde_json::to_value(RelayCommand::Cdp {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            message: "{}".into(),
        })
        .unwrap(),
        serde_json::to_value(RelayCommand::Pointer {
            thread_id: "j1".into(),
            tab_id: "tab-1".into(),
            phase: PointerPhase::Move,
            x: 1.0,
            y: 2.0,
        })
        .unwrap(),
        serde_json::to_value(RelayCommand::Turn {
            thread_id: "j1".into(),
            active: true,
        })
        .unwrap(),
        serde_json::to_value(RelayEvents {
            events: vec![
                RelayEvent::LoadFailed {
                    thread_id: "j1".into(),
                    tab_id: "tab-1".into(),
                    url: "u".into(),
                    code: -105,
                    description: "d".into(),
                },
                RelayEvent::Shown {
                    thread_id: "j1".into(),
                    tab_id: "tab-1".into(),
                    shown: true,
                },
            ],
        })
        .unwrap(),
        serde_json::to_value(BackBody {}).unwrap(),
        // The secret's body is only ever read: its keys as the route takes them.
        json!({"field": "#pw", "value": "v"}),
        serde_json::to_value(StreamFrame {
            tab_id: "tab-1".into(),
            seq: 1,
            data: "AA==".into(),
            mime: "image/jpeg".into(),
            width: 1,
            height: 1,
            dropped: 0,
        })
        .unwrap(),
        serde_json::to_value(StreamInput::Mouse {
            event_type: "mousePressed".into(),
            x: 1.0,
            y: 2.0,
            button: Some("left".into()),
            click_count: Some(1),
            delta_x: Some(0.0),
            delta_y: Some(0.0),
        })
        .unwrap(),
        serde_json::to_value(StreamInput::Key {
            event_type: "keyDown".into(),
            key: "a".into(),
            text: Some("a".into()),
            code: Some("KeyA".into()),
        })
        .unwrap(),
        serde_json::to_value(InputAnswer {
            tab_id: "tab-1".into(),
            delivered: vec!["f.pdf".into()],
        })
        .unwrap(),
        serde_json::to_value(ApiError {
            error: "e".into(),
            reason: "bad_request".into(),
        })
        .unwrap(),
        serde_json::to_value(outside).unwrap(),
        serde_json::to_value(mcp_tools()).unwrap(),
    ];
    out.append(&mut more);
    out
}

/// Whether `key` is named in `text` as a word of its own.
fn named(text: &str, key: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    text.match_indices(key).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + key.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

/// Every ```json block of the doc, parsed.
fn json_blocks(text: &str) -> Vec<Value> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(at) = rest.find("```json") {
        let after = &rest[at + "```json".len()..];
        let Some(end) = after.find("```") else { break };
        out.push(
            serde_json::from_str(&after[..end])
                .unwrap_or_else(|e| panic!("a json block in docs/api/browser.md: {e}")),
        );
        rest = &after[end + 3..];
    }
    out
}

#[test]
fn every_wire_key_is_in_the_api_doc_and_every_doc_key_is_the_contracts() {
    let mut keys = BTreeSet::new();
    for s in samples() {
        keys_of(&s, &mut keys);
    }
    let missing: Vec<&String> = keys.iter().filter(|k| !named(DOC, k)).collect();
    assert!(
        missing.is_empty(),
        "docs/api/browser.md never names these keys the contract sends: {missing:?}"
    );
    let blocks = json_blocks(DOC);
    assert!(!blocks.is_empty(), "the doc has json blocks");
    let mut doc_keys = BTreeSet::new();
    for b in &blocks {
        keys_of(b, &mut doc_keys);
    }
    let stray: Vec<&String> = doc_keys.difference(&keys).collect();
    assert!(
        stray.is_empty(),
        "docs/api/browser.md's json blocks have keys the contract doesn't: {stray:?}"
    );
}
