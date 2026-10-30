//! `POST /v1/plans`: one turn of the conversation with Oby. The conversation is a Claude Code (or
//! Codex) session on the conversation harness ([`harness`]), with Oby's own tools over MCP
//! ([`tools`]); every action a tool takes is checked as a typed plan ([`plan::Plan::problems`])
//! and carried out by [`actions`]. The turns nobody watches run there too: a stalled job's
//! ([`supervisor`]) and obid's own events' ([`orchestrator`]).

pub mod actions;
pub mod briefing;
pub mod calls;
pub mod find;
pub mod harness;
pub mod harness_codex;
pub mod orchestrator;
pub mod plan;
pub mod read;
pub mod recap;
pub mod shell;
pub mod supervisor;
pub mod tools;
pub mod turn;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::api::{ApiError, Shared};
use crate::conversation::{Target, Turn};
use actions::{Asked, Talk};
use plan::Context;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    said: String,
    answer: Option<Answer>,
    turns: Option<Vec<InputTurn>>,
    attachments: Option<Vec<String>>,
    /// The jobs the owner attached to the words (Sangam's Attach tasks), by id.
    jobs: Option<Vec<String>>,
    /// The conversation it joins; left out, the latest (#493).
    conversation: Option<String>,
    /// True: it starts a new conversation.
    new: Option<bool>,
    /// The id the app gave the user's words (a lower-case UUID): sent again with an id obid has,
    /// they're the same words, and nothing new happens.
    id: Option<String>,
    /// The teammate's Mac the iPhone may still send ("Run on another Mac", #808): taken and
    /// ignored, since every job runs on this Mac now.
    #[serde(rename = "runsOn")]
    _runs_on: Option<serde::de::IgnoredAny>,
    /// Folders on this Mac the conversation works in from now on (crate::folders): absolute paths,
    /// added to those it has. Only from the owner's Mac app: refused through the gateway or relay.
    folders: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    question: String,
    option: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputTurn {
    who: String,
    text: String,
}

pub async fn plan(
    State(shared): State<Arc<Shared>>,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    paired: Option<axum::Extension<crate::gateway::PairedDevice>>,
    relayed: Option<axum::Extension<crate::gateway::Relayed>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let req: Request = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't a plan request."))?;
    let words = req.said.trim();
    let id = req.id.clone();
    if id
        .as_deref()
        .is_some_and(|id| !crate::conversation::is_id(id))
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "id is a lower-case UUID.",
        ));
    }
    let attachments = validate_attachments(
        &shared.config.home,
        req.attachments.as_deref().unwrap_or(&[]),
    )?;
    let jobs = attached_jobs(&shared, req.jobs.unwrap_or_default())?;
    // No length of obid's own, as in Claude Code: the request's body is at most axum's 2 MB, and
    // the model says when words are more than it can read at once ([`TOO_LONG`]). Files or tasks
    // attached are a message without words too (Sangam's message box sends them alone).
    if words.is_empty() && attachments.is_empty() && jobs.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "There's no message to send.",
        ));
    }
    // An empty list is none. The verdict waits for the conversations' lock (begin): words sent
    // again with the same id are a repeat, whatever their folders.
    let folders = match req.folders.filter(|f| !f.is_empty()) {
        None => Ok(vec![]),
        Some(_) if paired.is_some() || relayed.is_some() => {
            Err("Folders are added only on the Mac.".to_owned())
        }
        Some(asked) => crate::folders::checked(&crate::folders::Places::of(&shared), &asked),
    };
    // Left out, a request joins the latest conversation, as it did when there was only one: the
    // apps from before #493 send none and show one thread.
    let target = match (req.conversation, req.new.unwrap_or(false)) {
        (Some(_), true) => {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Send conversation or new, not both.",
            ));
        }
        (Some(id), false) => Target::Id(id),
        (None, true) => Target::New,
        (None, false) => Target::Latest,
    };
    let turns = validate_turns(req.turns)?;
    // The owner's message: after more than 30 minutes away, the answer starts with a recap (#594).
    let now = obi_protocol::time::now();
    let back = shared.presence.seen(now);
    // A bare "never mind", "stop" or "cancel" never reaches the brain: it stops the turn under
    // way, after the action it's doing, and is answered with nothing (no job stops on a word).
    if req.answer.is_none() && turn::cancels(words) {
        // Refused folders: nothing of the request happens, a stop neither.
        if let Err(why) = &folders {
            return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why.clone()));
        }
        // The turn under way in the conversation the words name, else in the latest one; words
        // that start a new conversation have no turn to stop.
        let underway = match &target {
            Target::Id(id) => Some(id.clone()),
            Target::Latest => shared.turns.latest_conversation(),
            Target::New => None,
        };
        if let Some(id) = &underway {
            shared.turns.stop(Some(id));
        }
        // Its answer has no words: the recap waits for the next one.
        if let Some(since) = back {
            recap::keep(shared.clone(), since, now);
        }
        let conversation = underway.or_else(|| shared.conversations.latest_id());
        return Ok((
            StatusCode::OK,
            axum::Json(
                json!({"kind": "none", "jobs": [], "drafts": [], "conversation": conversation}),
            ),
        )
            .into_response());
    }
    // Oby's question the words answer: its text is that conversation's (crate::secretspec).
    let question = req
        .answer
        .as_ref()
        .map(|a| (a.question.clone(), a.option.clone()));
    let said = req
        .answer
        .map(|a| {
            format!(
                "{}\n\n(Oby asked: {} The user answered: {})",
                words, a.question, a.option
            )
        })
        .unwrap_or_else(|| words.to_owned());
    // What the conversation hears that isn't on obid's allowlist (crate::secretspec): words no app
    // of the owner's signed (oby-mcp's, a script's), turns the client wrote, files.
    let foreign = if !crate::routes::typed_by_owner(&shared, "POST", &uri, &headers, &body) {
        Some("words no app of the owner's signed".to_owned())
    } else if turns.is_some() {
        Some("turns its client wrote".to_owned())
    } else if !attachments.is_empty() {
        Some("files the owner attached".to_owned())
    } else {
        None
    };
    // The files' names go on the turn, for the tags under the owner's words.
    let names = attachments.iter().map(|p| file_name(p)).collect();
    let begun = shared.conversations.begin(
        target,
        said.clone(),
        jobs.clone(),
        names,
        id,
        foreign,
        folders,
    )?;
    // An answer to a question Oby asked elsewhere (the Watch answers in the latest conversation)
    // brings that conversation's words here.
    if let Some((question, option)) = &question
        && !shared
            .conversations
            .oby_asked(&begun.conversation, question, option)
    {
        shared.conversations.mark(
            &begun.conversation,
            "a question Oby asked in another conversation".into(),
        );
    }
    // The same words sent again: obid has them (and answered, or answers, them in its turn).
    if begun.repeated {
        return Ok((
            StatusCode::OK,
            axum::Json(
                json!({"kind": "none", "jobs": [], "drafts": [], "conversation": begun.conversation, "repeated": true}),
            ),
        )
            .into_response());
    }
    let words = turn::Words {
        turn: begun.turn,
        said,
        attachments,
        jobs,
    };
    // The turn goes on by itself: a client that stops waiting can't be told from a slow one, so
    // what the plan does still happens, and is kept (docs/runner-api.md, `POST /v1/plans`).
    let turn = tokio::spawn(async move {
        let conversation = begun.conversation;
        // One answer at a time in a conversation: words said while Oby answers wait for that turn
        // to end, and the next turn answers all that came meanwhile, as the latest's. The others',
        // and words a cancel dropped, answer nothing.
        let Some(next) = shared.turns.wait_turn(&conversation, words).await else {
            if let Some(since) = back {
                recap::keep(shared.clone(), since, now);
            }
            return respond(&shared, &conversation, Talk::None, None, None, vec![]);
        };
        let said = &next.words.said;
        // The conversation as it is now, Oby's answer to the turn they waited for included.
        let given =
            turns.unwrap_or_else(|| shared.conversations.turns_but(&conversation, &next.turns));
        let control = shared.turns.begin(&next.words.turn, &conversation);
        let asked = Asked {
            said,
            attachments: &next.words.attachments,
            jobs: &next.words.jobs,
            conversation: &conversation,
            turn: &control.turn,
        };
        // The recap is written while the brain thinks; a background call is bounded and
        // tried once, and when it fails the answer is the same as without it.
        let (answered, recap) = tokio::join!(
            harness::run(&shared, said, &given, &asked, &control),
            recap::back(&shared, back, now)
        );
        shared.turns.end(&control);
        let recap = recap.or_else(|| shared.presence.take());
        let made = shared.harness.take_made(&control.turn);
        respond(
            &shared,
            &conversation,
            answered.talk,
            answered.details,
            recap,
            made,
        )
    });
    turn.await.map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Oby's brain stopped before it answered.",
        )
    })?
}

/// Why words Oby's brain wrote in `conversation` aren't on obid's allowlist (crate::secretspec):
/// what the conversation read that isn't, or none when it read nothing else. A job they reach is
/// someone else's from then on.
pub fn written_after(shared: &Shared, conversation: &str) -> Option<String> {
    shared
        .conversations
        .foreign(conversation)
        .map(|why| format!("words Oby wrote after its conversation read {why} reached it"))
}

/// `new`, a job Oby's brain starts with words it wrote in `conversation` (crate::secretspec): the
/// owner's own words only while the conversation read nothing off obid's allowlist; otherwise
/// someone else's from the start, in a clone apart.
pub fn written_in(
    shared: &Shared,
    conversation: &str,
    new: crate::jobs::NewJob,
) -> crate::jobs::NewJob {
    match shared.conversations.foreign(conversation) {
        None => crate::jobs::NewJob {
            vetted: true,
            ..new
        },
        Some(why) => crate::jobs::NewJob {
            vetted: false,
            foreign: Some(format!(
                "Oby's conversation that wrote its words read {why}"
            )),
            ..new
        },
    }
}

/// Which conversation's turn Stop or cancel is for: `{"conversation": "<id>"}`, or, with no
/// body (the apps from before #493), the conversation of the latest turn under way.
fn underway(shared: &Shared, body: &Bytes) -> Result<Option<String>, ApiError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Which {
        conversation: Option<String>,
    }
    let named = if body.iter().all(u8::is_ascii_whitespace) {
        None
    } else {
        serde_json::from_slice::<Which>(body)
            .map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "The body is {\"conversation\": \"<id>\"}, or nothing.",
                )
            })?
            .conversation
    };
    Ok(named.or_else(|| shared.turns.latest_conversation()))
}

/// `POST /v1/conversation/stop`: the turn under way in one conversation ends after the action
/// it's doing, and its `POST /v1/plans` answers with what it did. Words said meanwhile then get
/// their turn. Turns in other conversations go on.
pub async fn stop(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    if let Some(id) = underway(&shared, &body)? {
        shared.turns.stop(Some(&id));
    }
    Ok((StatusCode::OK, axum::Json(json!({"stopped": true}))).into_response())
}

/// `POST /v1/plans/cancel`: the turn under way in one conversation is dropped: what its plan
/// would do doesn't happen, and its answer is `none`, as is the answer to words waiting for it.
/// What it did already stays done.
pub async fn cancel(State(shared): State<Arc<Shared>>, body: Bytes) -> Result<Response, ApiError> {
    if let Some(id) = underway(&shared, &body)? {
        shared.turns.cancel(Some(&id));
    }
    Ok((StatusCode::OK, axum::Json(json!({"cancelled": true}))).into_response())
}

/// The answer, as `POST /v1/plans` sends it, once Oby's words joined the conversation.
fn respond(
    shared: &Shared,
    conversation: &str,
    mut talk: Talk,
    details: Option<String>,
    recap: Option<String>,
    artifacts: Vec<Value>,
) -> Result<Response, ApiError> {
    if let Talk::Reply { text, jobs } = &mut talk
        && jobs.is_empty()
        && briefing::contradicts(text)
    {
        let running = shared.jobs.list(usize::MAX);
        if briefing::working(&running) > 0 {
            *text = briefing::working_line(&running, &shared.host);
        }
    }
    // The recap is Oby's first line, never a turn the user wrote; an answer with no words keeps
    // it for the next one.
    let recap = match (&talk, recap) {
        (Talk::None, Some(recap)) => {
            shared.presence.hold(recap);
            None
        }
        (_, recap) => recap,
    };
    let talk = recap::lead(talk, recap.as_deref());
    // Written from other jobs and the inbox: the conversation reads it in Oby's answer from now on.
    if recap.is_some() {
        shared
            .conversations
            .mark(conversation, "a recap of other jobs and the inbox".into());
    }
    let mut body = match &talk {
        Talk::None => json!({"kind": "none", "jobs": [], "drafts": []}),
        Talk::Reply { text, jobs } => {
            json!({"kind": "reply", "text": text, "jobs": jobs, "drafts": []})
        }
        Talk::Question { said, question } => json!({
            "kind": "question", "said": said, "jobs": [], "drafts": [],
            "question": {"text": question.text, "options": question.options,
                         "details": question.details, "recommended": question.recommended,
                         "plan": question.plan},
        }),
    };
    if let Some(details) =
        details.filter(|_| matches!(talk, Talk::Reply { .. } | Talk::Question { .. }))
    {
        body["details"] = json!(details);
    }
    let text = match &talk {
        Talk::None => None,
        Talk::Reply { text, .. } => Some(text.clone()),
        // A question keeps only its own words in the conversation: the recap goes before them.
        Talk::Question { question, .. } => Some(match &recap {
            Some(recap) => format!("{recap}\n\n{}", question.text),
            None => question.text.clone(),
        }),
    };
    if let Some(text) = text {
        let jobs = match &talk {
            Talk::Reply { jobs, .. } => jobs
                .iter()
                .filter_map(|j| j["id"].as_str().map(str::to_owned))
                .collect(),
            _ => vec![],
        };
        // A question keeps what it asked, so an answer's words are known to be this one's.
        let asked = match &talk {
            Talk::Question { question, .. } => Some(crate::conversation::Asked {
                question: question.text.clone(),
                options: question.options.clone(),
            }),
            _ => None,
        };
        match shared
            .conversations
            .append_asking(conversation, text, jobs, artifacts.clone(), asked)
        {
            Ok(_) => {}
            // Deleted while the brain thought: the answer still goes to the app that asked.
            Err(crate::conversation::Error::NoConversation) => {
                tracing::info!("conversation {conversation} was deleted during its turn");
            }
            Err(e) => return Err(e.into()),
        }
    }
    if let Some(recap) = recap {
        body["recap"] = json!(recap);
    }
    if !artifacts.is_empty() {
        body["artifacts"] = json!(artifacts);
    }
    body["conversation"] = json!(conversation);
    Ok((StatusCode::OK, axum::Json(body)).into_response())
}

/// A `thinking` event (#592): one call to the brain, as it begins and as its plan comes (`done`),
/// or doesn't (`failed`: the turn was cancelled, stopped or out of time). `index` is the step the
/// call plans, as the `step` events count them.
pub(super) fn thinking(
    shared: &Shared,
    control: &turn::Control,
    index: usize,
    text: &str,
    state: turn::State,
) {
    shared.events.send(
        "thinking",
        json!({"turn": control.turn, "conversation": control.conversation, "index": index,
               "text": text, "state": state.as_str(),
               "at": obi_protocol::time::format(obi_protocol::time::now())}),
    );
}

/// A `reply` event (#592): what Oby says, as soon as the plan that says it is checked and before
/// the jobs it starts are made. Never for a turn that's cancelled or stopped, nor for no words.
/// Whether it went out.
pub(super) fn acknowledge(shared: &Shared, control: &turn::Control, text: &str) -> bool {
    if text.trim().is_empty() || !control.live() {
        return false;
    }
    shared.events.send(
        "reply",
        json!({"turn": control.turn, "conversation": control.conversation, "text": text,
               "at": obi_protocol::time::format(obi_protocol::time::now())}),
    );
    true
}

/// What a tool's action may name (`Desk.brainContext`), as [`plan::Plan::problems`] checks it: the
/// accounts, the repositories and the ids each action may name, `conversation`'s jobs and inbox as
/// they are now.
pub(super) fn context(shared: &Shared, conversation: &str, turns: &[Turn], nonce: &str) -> Context {
    let config = &shared.config;
    let now = obi_protocol::time::now();
    let tools = tool_accounts(config);
    let mut accounts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut every = vec!["auto".to_owned()];
    let mut move_accounts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut told = vec![];
    for (tool, list) in &tools {
        let names: Vec<String> = list.iter().map(|a| a.name.clone()).collect();
        every.extend(names.iter().cloned());
        accounts.insert(
            (*tool).into(),
            std::iter::once("auto".to_owned())
                .chain(names.clone())
                .collect(),
        );
        move_accounts.insert((*tool).into(), names);
        told.extend(list.iter().map(|a| briefing::Account {
            id: a.id.clone(),
            agent: (*tool).into(),
            name: a.name.clone(),
            running: shared.jobs.running_on(&a.id),
        }));
    }
    accounts.insert("auto".into(), every);
    // What a job may move to: each tool's models (Engine/Move.swift, `Moves.models`).
    let mut models: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (tool, _) in &tools {
        let ids: &[&str] = match *tool {
            "claude" => &[
                "claude-fable-5-1",
                "claude-opus-5-5",
                "claude-sonnet-5-5",
                "claude-haiku-5-5",
            ],
            "codex" => &["gpt-6.1-sol", "gpt-6-astra"],
            _ => &[],
        };
        models.insert(
            (*tool).into(),
            ids.iter().map(|m| (*m).to_owned()).collect(),
        );
    }
    let briefing = briefing::Briefing {
        turns: turns.to_vec(),
        conversation: conversation.to_owned(),
        place: shared.host.clone(),
        jobs: shared.jobs.list(usize::MAX),
        inbox: shared.link.waiting_items(now),
        accounts: told,
        repos: config.repos.names(),
        max_running: shared.jobs.max_running(),
        default_mode: shared.jobs.default_mode().as_str(),
        failures: briefing::week(
            crate::failures::list(&config.home, crate::failures::MOST),
            now,
        ),
        now,
        nonce: nonce.to_owned(),
    };
    Context {
        accounts,
        repos: config.repos.names(),
        jobs: briefing.stoppable(),
        movable: briefing.jobs().iter().map(|j| j.id.clone()).collect(),
        models,
        move_accounts,
        permissions: briefing.permissions(),
        // A confirm item waits for the user's own answer: the brain may never answer it.
        inbox: briefing
            .inbox
            .iter()
            .filter(|i| i.kind != "confirm")
            .map(|i| i.id.clone())
            .collect(),
        ended: briefing
            .jobs()
            .into_iter()
            .filter(|job| left_unfinished(job, now))
            .map(|job| plan::Ended {
                id: job.id.clone(),
                repo: job.repo.clone(),
                issue: job.issue.map(|n| n as i64),
                pr: job.pr.map(|n| n as i64),
            })
            .collect(),
        ..Context::default()
    }
}

/// A job that failed, was stopped or ran out of rounds in the last two days, and wasn't let go:
/// its agent is the one to carry its work on (#541).
fn left_unfinished(job: &crate::job::Job, now: i64) -> bool {
    use crate::job::Status;
    let ended = matches!(
        job.status,
        Status::Failed | Status::Stopped | Status::Interrupted
    ) || (job.status == Status::Done && job.limit.is_some());
    ended
        && job.obi.is_none()
        && job.dismissed.is_none()
        && job
            .finished
            .is_some_and(|finished| now - finished < briefing::RECENT)
}

/// Each configured tool's accounts, as jobs are routed to them.
fn tool_accounts(
    config: &crate::config::Config,
) -> Vec<(&'static str, Vec<crate::config::Account>)> {
    let agents = config.agents.get();
    let mut out = vec![];
    if let Some(claude) = &agents.claude {
        out.push(("claude", crate::jobs::claude_accounts(claude)));
    }
    if let Some(codex) = &agents.codex {
        out.push((
            "codex",
            if codex.accounts.is_empty() {
                vec![crate::config::Account {
                    id: "codex".into(),
                    name: "Codex".into(),
                    config_dir: None,
                    payer: None,
                }]
            } else {
                codex.accounts.clone()
            },
        ));
    }
    if let Some(cursor) = &agents.cursor {
        out.push(("cursor", cursor.accounts.clone()));
    }
    out
}

/// The runner reads issue text with its own `gh` login. The body is data: this only extracts and
/// validates the issue's declared `Files:` paths; it never reaches the brain.
async fn issue_files(repo: &str, issue: u64) -> Option<Vec<String>> {
    let gh = crate::process::resolve_bin("gh")?;
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(gh)
            .args([
                "issue",
                "view",
                &issue.to_string(),
                "--repo",
                repo,
                "--json",
                "title,body",
            ])
            .env("GH_PAGER", "cat")
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let issue: Value = serde_json::from_slice(&output.stdout).ok()?;
    let title = issue["title"].as_str().unwrap_or_default();
    let body = issue["body"].as_str().unwrap_or_default();
    let line = body.lines().rev().find(|line| {
        let line = line.trim_start_matches([' ', '\t', '>', '*', '_']);
        line.get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("files"))
            && line[5..].trim_start_matches(['*', '_']).starts_with(':')
    });
    let mut paths = line.map(parse_issue_file_list).unwrap_or_default();
    if paths.is_empty() {
        paths = named_paths(&format!("{title}\n{body}"));
    }
    crate::jobs::claimed_files(&paths)
        .ok()
        .filter(|files| !files.is_empty())
}

fn parse_issue_file_list(line: &str) -> Vec<String> {
    let Some((_, rest)) = line.split_once(':') else {
        return Vec::new();
    };
    let mut listed = Vec::new();
    let mut rest = rest;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else { break };
        listed.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
    if listed.is_empty() {
        listed = rest
            .split([',', ';'])
            .flat_map(|part| part.split(" and "))
            .map(str::trim)
            .map(str::to_owned)
            .collect();
    }
    listed
}

fn named_paths(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let words = line.split(|c: char| c.is_whitespace() || ",;()[]<>\"'|".contains(c));
        for word in words {
            let candidate = word
                .trim_matches('`')
                .trim_matches(|c: char| ".:!?*_".contains(c));
            if candidate.contains('/')
                && !candidate.contains("//")
                && !candidate.starts_with('/')
                && !candidate.split('/').any(|part| part == "..")
                && candidate.chars().any(char::is_alphabetic)
            {
                found.push(candidate.to_owned());
            }
        }
    }
    found
}
fn validate_turns(input: Option<Vec<InputTurn>>) -> Result<Option<Vec<Turn>>, ApiError> {
    let Some(turns) = input else { return Ok(None) };
    if turns.len() > 50 {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "turns holds at most 50.",
        ));
    }
    let mut out = vec![];
    for t in turns {
        if !["user", "oby"].contains(&t.who.as_str()) || t.text.trim().is_empty() {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Each turn needs who user or oby and some text.",
            ));
        }
        out.push(Turn {
            id: String::new(),
            who: t.who,
            text: t.text,
            at: String::new(),
            jobs: vec![],
            conversation: String::new(),
            ended: false,
            artifacts: vec![],
            routine: None,
            asked: None,
            attachments: vec![],
        });
    }
    Ok(Some(out))
}
/// The most jobs one message attaches.
const ATTACHED_JOBS: usize = 20;

/// What a turn answers when Claude Code found the prompt more than its model can read at once
/// ([`too_long`]): a message that long, or a conversation grown that long. Trying again doesn't
/// help.
pub const TOO_LONG: &str = "That's more than Oby's model can read at once, so nothing was started. Attach long text as a file (+) instead, or send it in parts; if this conversation has grown long, start a new one.";

/// Whether Claude Code ended because the prompt didn't fit its model (its result's
/// `Prompt is too long`, 2.1.302).
pub(crate) fn too_long(said: &str) -> bool {
    said.contains("Prompt is too long")
}

/// The jobs the owner attached to their words, each once: only jobs obid has, which Oby's tools
/// read anyway (`oby_job`), so an id grants nothing.
fn attached_jobs(shared: &Shared, ids: Vec<String>) -> Result<Vec<String>, ApiError> {
    let mut out: Vec<String> = vec![];
    for id in ids {
        if shared.jobs.get(&id).is_none() {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "jobs names a job this runner doesn't have.",
            ));
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.len() > ATTACHED_JOBS {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A message attaches at most 20 jobs.",
        ));
    }
    Ok(out)
}

/// The most files one message attaches.
const ATTACHED_FILES: usize = 20;
/// The uploads obid keeps (`POST /v1/attachments`); past it the oldest go. ponytail: a count, not
/// a size; the rooms and jobs keep copies of their own, so only words not sent yet lose a file.
const UPLOADS: usize = 200;

/// A path's file name, as the tags under the owner's words show it.
fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment".into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    name: String,
}

/// `POST /v1/attachments?name=<file name>`: a file the owner picked or pasted in the Mac app, the
/// request's body as it is, kept in obid's attachments folder (`attachments/app/<uuid>/<name>`,
/// the name cleaned as the room's are) for `attachments` on `POST /v1/plans` and
/// `POST /v1/jobs/{id}/messages`. Only on the Mac: the gateway doesn't list it.
pub async fn upload(
    State(shared): State<Arc<Shared>>,
    axum::extract::Query(asked): axum::extract::Query<Upload>,
    body: Result<Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Response, ApiError> {
    let name = harness::clean(&asked.name);
    let too_big = || {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "{name} is larger than {} MB, so Oby can't take it.",
                harness::ATTACHMENT_CAP / 1_000_000
            ),
        )
    };
    let body = body.map_err(|_| too_big())?;
    if body.len() as u64 > harness::ATTACHMENT_CAP {
        return Err(too_big());
    }
    if body.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{name} is empty."),
        ));
    }
    let path =
        crate::github_work::keep_attachment(&shared.config, "app", &name, &body).map_err(|e| {
            tracing::warn!("an attachment couldn't be kept: {e}");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("{name} couldn't be kept on this Mac."),
            )
        })?;
    prune_uploads(&shared.config.home.join("attachments/app"), UPLOADS);
    Ok((
        StatusCode::CREATED,
        axum::Json(json!({"path": path.to_string_lossy(), "name": name})),
    )
        .into_response())
}

/// The bound on `POST /v1/attachments`'s body: a megabyte past the largest file, so a file just
/// over it is read whole and answered in obid's words.
pub fn upload_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(harness::ATTACHMENT_CAP as usize + (1 << 20))
}

/// Keeps the newest `keep` uploads in `folder` (one folder each), never following a link.
fn prune_uploads(folder: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut kept: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.path().symlink_metadata().ok()?;
            meta.is_dir().then_some((meta.modified().ok()?, e.path()))
        })
        .collect();
    kept.sort();
    let excess = kept.len().saturating_sub(keep);
    for (_, old) in kept.into_iter().take(excess) {
        let _ = std::fs::remove_dir_all(old);
    }
}

pub(crate) fn validate_attachments(home: &Path, paths: &[String]) -> Result<Vec<String>, ApiError> {
    if paths.len() > ATTACHED_FILES {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A message attaches at most 20 files.",
        ));
    }
    let path = home.join("attachments");
    let root = path.canonicalize().unwrap_or(path);
    let mut out = vec![];
    for p in paths {
        let file = PathBuf::from(p);
        let resolved = file.canonicalize().ok();
        if resolved
            .as_ref()
            .is_none_or(|f| !f.starts_with(&root) || f == &root || !f.is_file())
        {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("attachment");
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "{name} isn't one of the files the Oby app attached, so it isn't handed to the agent."
                ),
            ));
        }
        out.push(p.clone());
    }
    Ok(out)
}
