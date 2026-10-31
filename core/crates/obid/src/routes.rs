//! The jobs' endpoints and the event stream (docs/runner-api.md, "POST /v1/jobs" to
//! "POST /v1/jobs/{id}/dismiss", and "GET /v1/events").

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{Map, Value, json};

use crate::api::{ApiError, Shared};
use crate::jobs::NewJob;

const NO_JOB: &str = "There's no such job.";

/// What a job sent to a teammate's Mac hears: compute sharing went with the strip to a dispatcher.
pub const ELSEWHERE: &str = "Jobs run only on this Mac now: runsOn is null or left out.";

/// Resolves once obid is stopping: a stream that would otherwise stay open ends.
async fn stopping(mut receiver: tokio::sync::watch::Receiver<bool>) {
    while !*receiver.borrow() {
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

fn json(status: StatusCode, value: Value) -> Response {
    (status, axum::Json(value)).into_response()
}

/// An explicit proof-of-possession migration for older endorsed iPhone keys.
/// Only the gateway supplies the authenticated pairing; the API token cannot invent one.
pub async fn device_pairing_challenge(
    State(shared): State<Arc<Shared>>,
    device: Option<axum::Extension<crate::gateway::PairedDevice>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let pairing = device
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "This request must come from your paired iPhone.",
            )
        })?
        .0
        .0;
    let nonce = shared
        .devices
        .pairing_challenge(&id, &pairing, obi_protocol::time::now())
        .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
    Ok(json(
        StatusCode::OK,
        json!({"pairing":pairing,"nonce":nonce}),
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevicePairingProof {
    pairing: String,
    nonce: String,
    approval: crate::approval::Approval,
}

pub async fn bind_device_pairing(
    State(shared): State<Arc<Shared>>,
    device: Option<axum::Extension<crate::gateway::PairedDevice>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let pairing = device
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "This request must come from your paired iPhone.",
            )
        })?
        .0
        .0;
    let proof: DevicePairingProof = serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A signed device-link proof is required.",
        )
    })?;
    if proof.pairing != pairing {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "This proof is for a different pairing.",
        ));
    }
    let key = shared.devices.public_key(&id);
    let now = obi_protocol::time::now();
    shared
        .jobs
        .approvals()
        .check_pairing_binding(
            &proof.approval,
            &id,
            &pairing,
            &proof.nonce,
            key.as_deref(),
            now,
        )
        .map_err(|why| ApiError::new(StatusCode::FORBIDDEN, why))?;
    shared
        .devices
        .bind_pairing(&id, &pairing, &proof.nonce, now)
        .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
    Ok(json(StatusCode::OK, json!({"id":id})))
}

/// The grouped, owner-decided account limit incidents (#1563).
pub async fn limit_decisions(State(shared): State<Arc<Shared>>) -> Response {
    json(
        StatusCode::OK,
        Value::Array(shared.jobs.limit_decisions().await),
    )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitDecisionAnswer {
    revision: u64,
    choice: String,
    account: Option<String>,
    approval: crate::approval::Approval,
}

/// Only an owner's registered device can choose the exact revision and destination it signed.
pub async fn answer_limit_decision(
    State(shared): State<Arc<Shared>>,
    device: Option<axum::Extension<crate::gateway::PairedDevice>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let answer: LimitDecisionAnswer = serde_json::from_slice(&body).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A limit decision requires a revision, choice and device approval.",
        )
    })?;
    if !matches!(answer.choice.as_str(), "move" | "resume" | "stop")
        || (answer.choice == "move") != answer.account.as_deref().is_some_and(|a| !a.is_empty())
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Choose an offered action and, for a move, its account.",
        ));
    }
    if device.is_some_and(|device| {
        !shared
            .devices
            .belongs_to_pairing(&answer.approval.device, &device.0.0)
    }) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "This approval belongs to a different device.",
        ));
    }
    let key = shared.devices.public_key(&answer.approval.device);
    shared
        .jobs
        .approvals()
        .check_limit_decision(
            &answer.approval,
            crate::approval::LimitChoice {
                id: &id,
                revision: answer.revision,
                choice: &answer.choice,
                account: answer.account.as_deref().unwrap_or_default(),
            },
            key.as_deref(),
            obi_protocol::time::now(),
        )
        .map_err(|why| ApiError::new(StatusCode::FORBIDDEN, why))?;
    let result = shared
        .jobs
        .answer_limit_decision(
            &id,
            answer.revision,
            &answer.choice,
            answer.account.as_deref(),
        )
        .await
        .map_err(|why| ApiError::new(StatusCode::CONFLICT, why))?;
    Ok(json(StatusCode::OK, result))
}

#[derive(serde::Deserialize)]
pub struct Limit {
    limit: Option<String>,
}

/// `GET /v1/jobs?limit=50`: the latest jobs, newest first.
pub async fn jobs(
    State(shared): State<Arc<Shared>>,
    Query(query): Query<Limit>,
) -> Result<Response, ApiError> {
    let limit = match query.limit.as_deref() {
        None => 50,
        Some(text) => text
            .parse::<usize>()
            .map_err(|_| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "limit is a number."))?,
    };
    let list: Vec<Value> = shared
        .jobs
        .list(limit.min(1000))
        .iter()
        .map(|j| j.to_json())
        .collect();
    Ok(json(StatusCode::OK, Value::Array(list)))
}

/// `GET /v1/jobs/{id}`.
pub async fn job(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let job = shared
        .jobs
        .get(&id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_JOB))?;
    Ok(json(StatusCode::OK, job.detail_json()))
}

#[derive(serde::Deserialize)]
pub struct After {
    after: Option<String>,
    limit: Option<String>,
}

/// `GET /v1/jobs/{id}/transcript?after=<n>&limit=<k>`: the entries that came or changed after
/// entry `n`, at most `k`, and the `n` to ask after next when there's more.
pub async fn transcript(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    Query(query): Query<After>,
) -> Result<Response, ApiError> {
    let number = |text: Option<&str>, default: u64, what: &str| match text {
        None => Ok(default),
        Some(text) => text.parse::<u64>().map_err(|_| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("{what} is a number."),
            )
        }),
    };
    let after = number(query.after.as_deref(), 0, "after")?;
    let limit = number(
        query.limit.as_deref(),
        crate::transcript::PAGE as u64,
        "limit",
    )?;
    let (entries, next) = shared
        .jobs
        .transcript(&id, after, usize::try_from(limit).unwrap_or(usize::MAX))
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_JOB))?;
    Ok(json(
        StatusCode::OK,
        json!({"entries": entries, "next": next}),
    ))
}

/// `GET /v1/jobs/{id}/transcript/{n}`: the entry `n` with its `detail` (crate::detail), masked.
pub async fn transcript_entry(
    State(shared): State<Arc<Shared>>,
    Path((id, n)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let n = n
        .parse::<u64>()
        .map_err(|_| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "n is a number."))?;
    if shared.jobs.get(&id).is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB));
    }
    let (entry, detail) = shared
        .jobs
        .transcript_entry(&id, n)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "The transcript has no such entry."))?;
    let mut value = json!(entry);
    if let Some(detail) = detail {
        value["detail"] = detail;
    }
    Ok(json(StatusCode::OK, value))
}

/// `POST /v1/jobs/{id}/stop`: a job that already ended comes back as it is.
pub async fn stop(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let job = shared
        .jobs
        .stop(&id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_JOB))?;
    Ok(json(StatusCode::OK, job.to_json()))
}

/// `POST /v1/jobs/{id}/start`: a job waiting for an account with room starts now (#872).
pub async fn start(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    match shared.jobs.start_now(&id) {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(refused)) => Err(ApiError::new(StatusCode::CONFLICT, refused.0)),
        Some(Ok(job)) => Ok(json(StatusCode::OK, job.to_json())),
    }
}

/// `POST /v1/stop-all`.
pub async fn stop_all(State(shared): State<Arc<Shared>>) -> Response {
    let stopped = shared.jobs.stop_all();
    json(
        StatusCode::OK,
        json!({"stopped": stopped.len(), "jobs": stopped.iter().map(|j| j.to_json()).collect::<Vec<_>>()}),
    )
}

/// `POST /v1/jobs/{id}/dismiss`.
pub async fn dismiss(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    match shared.jobs.dismiss(&id) {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(refused)) => Err(ApiError::new(StatusCode::CONFLICT, refused.0)),
        Some(Ok(job)) => Ok(json(StatusCode::OK, job.to_json())),
    }
}

/// `POST /v1/jobs/{id}/archive`: `{"archived": true}` (or no body) archives a job that ended and
/// needs nothing; `false` unarchives it.
pub async fn archive(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let archived = if body.iter().all(u8::is_ascii_whitespace) {
        true
    } else {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Archive {
            archived: bool,
        }
        serde_json::from_slice::<Archive>(&body)
            .map_err(|_| {
                ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "The body is {\"archived\": true} or {\"archived\": false}.",
                )
            })?
            .archived
    };
    match shared.jobs.archive(&id, archived, "by you") {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(refused)) => Err(ApiError::new(StatusCode::CONFLICT, refused.0)),
        Some(Ok(job)) => Ok(json(StatusCode::OK, job.to_json())),
    }
}

/// `PATCH /v1/jobs/{id}`: `{"title": "…"}` renames the job, one line of 1 to 60 characters once
/// trimmed (as a new job's `title`). `{"runsOn": null}` (this Mac, where every job runs now) is
/// taken and changes nothing; a teammate's Mac is `422`.
pub async fn rename(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let bad = || {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "The body is {\"title\": \"…\"} and/or {\"runsOn\": \"<Obi>\" | null}.",
        )
    };
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(&body) else {
        return Err(bad());
    };
    if map.is_empty() || map.keys().any(|k| k != "title" && k != "runsOn") {
        return Err(bad());
    }
    let title = match map.get("title") {
        None => None,
        Some(Value::String(t)) => Some(t.trim().to_owned()),
        Some(_) => return Err(bad()),
    };
    if let Some(title) = &title
        && (title.is_empty()
            || title.chars().count() > 60
            || title
                .chars()
                .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')))
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "title is one line of 1 to 60 characters.",
        ));
    }
    if map.get("runsOn").is_some_and(|place| !place.is_null()) {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, ELSEWHERE));
    }
    // Its title is what Oby's conversations read of it (`oby_jobs`, how it ended): one no app of
    // the owner's signed isn't the owner's own words (crate::secretspec), from before it lands.
    if title.is_some() && !typed_by_owner(&shared, "PATCH", &uri, &headers, &body) {
        shared
            .jobs
            .unvet(&id, "a title no app of the owner's signed");
    }
    let job = match title {
        Some(title) => shared.jobs.rename(&id, title),
        None => shared.jobs.get(&id),
    };
    match job {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(job) => Ok(json(StatusCode::OK, job.to_json())),
    }
}

/// `DELETE /v1/jobs/{id}`: a job that ended goes for good, its worktree and local branch with it;
/// nothing pushed is touched.
pub async fn delete(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    match shared.jobs.delete(&id).await {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(refused)) => Err(ApiError::new(StatusCode::CONFLICT, refused.0)),
        Some(Ok(_)) => {
            shared.link.settle_job(&id);
            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}

/// `POST /v1/jobs/{id}/delete` (obid only): the apps' Delete task. A job that ended with nothing
/// unpublished goes at once, as `DELETE /v1/jobs/{id}` does; one with commits that aren't published
/// waits for the owner's Confirm (`delete_job`, titled as `oby_delete_job`'s), which it answers.
pub async fn delete_asked(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    use crate::sphere::inbox::{Confirmed, action, confirm_item};
    let job = shared
        .jobs
        .get(&id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_JOB))?;
    if let Some(refused) = shared.jobs.undeletable(&id) {
        return Err(ApiError::new(StatusCode::CONFLICT, refused.0));
    }
    if job.unpublished() == 0 {
        delete(State(shared), Path(id)).await?;
        return Ok(json(StatusCode::OK, json!({"deleted": true, "ask": null})));
    }
    let title = crate::brain::tools::delete_title(&shared, &job);
    let mut item = confirm_item(
        &id,
        title.clone(),
        "Deleting a job can't be undone: it happens only if you confirm.".into(),
        // what the job was when asked: a Confirm after another round or commit is refused
        Confirmed {
            action: action::DELETE_JOB.into(),
            target: id.clone(),
            payload: json!({"snapshot": job.snapshot()}),
        },
        obi_protocol::time::now(),
    );
    item.details = Some(vec![format!("{title}."), "Keeps the job as it is.".into()]);
    let item = shared.link.add_confirm_instead(item);
    Ok(json(
        StatusCode::OK,
        json!({"deleted": false, "ask": shared.link.shown(&item)}),
    ))
}

/// `GET /v1/jobs/{id}/wait`: the job once it has ended; a space every 15 seconds until then.
pub async fn wait(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    if shared.jobs.get(&id).is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB));
    }
    enum Step {
        Waiting,
        Done,
    }
    let jobs = shared.jobs.clone();
    let stop = shared.stopping.clone();
    let ended = Box::pin(async move {
        tokio::select! {
            job = jobs.wait(&id) => job,
            // obid is stopping: the job as it is now.
            () = stopping(stop) => jobs.get(&id),
        }
    });
    let body = stream::unfold((Step::Waiting, Some(ended)), |(step, ended)| async move {
        match (step, ended) {
            (Step::Done, _) | (_, None) => None,
            (Step::Waiting, Some(mut ended)) => {
                tokio::select! {
                    job = &mut ended => {
                        let text = job.map(|j| j.to_json().to_string()).unwrap_or_else(|| "null".into());
                        Some((Ok::<Bytes, Infallible>(Bytes::from(text)), (Step::Done, None)))
                    }
                    () = tokio::time::sleep(Duration::from_secs(15)) => {
                        Some((Ok(Bytes::from_static(b" ")), (Step::Waiting, Some(ended))))
                    }
                }
            }
        }
    });
    // A space first, at once: the connection is open and the client knows to wait.
    let body =
        stream::once(async { Ok::<Bytes, Infallible>(Bytes::from_static(b" ")) }).chain(body);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(body),
    )
        .into_response())
}

/// `GET /v1/events`: every change as it happens, and a `: ping` comment every 15 seconds. A paired
/// device following it has Oby open: Oby's messages aren't pushed meanwhile (crate::push).
pub async fn events(
    State(shared): State<Arc<Shared>>,
    device: Option<axum::Extension<crate::gateway::PairedDevice>>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let following = device.map(|_| shared.push.followed());
    // An app opened: the accounts' usage, if stale, is asked of their tools (crate::jobs).
    shared.jobs.want_usage();
    let receiver = shared.events.subscribe();
    let transcripts = shared.events.subscribe_transcripts();
    let stream = stream::select(
        tokio_stream::wrappers::BroadcastStream::new(receiver),
        tokio_stream::wrappers::BroadcastStream::new(transcripts),
    )
    .filter_map(|event| async move {
        let event = event.ok()?;
        Some(Ok(SseEvent::default()
            .event(event.name)
            .data(event.data.to_string())))
    });
    let stream = stream
        .take_until(stopping(shared.stopping.clone()))
        // Kept as long as the stream is.
        .inspect(move |_| {
            let _ = &following;
        });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Whether the request carries words the owner typed in Oby's apps: signed by a registered
/// device's key ([`crate::approval::WORDS`]). Anything else that holds obid's token (oby-mcp, a
/// script, an agent, an older app) doesn't, whatever it sends (crate::secretspec).
pub fn typed_by_owner(
    shared: &Shared,
    method: &str,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> bool {
    signed_by(shared, method, uri, headers, body).is_some()
}

/// The registered device whose app signed the request ([`typed_by_owner`]), or none.
pub fn signed_by(
    shared: &Shared,
    method: &str,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> Option<String> {
    let path = uri.path();
    let header = headers
        .get(crate::approval::WORDS)
        .and_then(|v| v.to_str().ok())?;
    let checked = shared.jobs.approvals().check_words(
        header,
        method,
        path,
        body,
        |device| shared.devices.public_key(device),
        obi_protocol::time::now(),
    );
    if let Err(why) = &checked {
        tracing::info!("{method} {path}: not words the owner typed in Oby's apps: {why}");
    }
    checked
        .ok()
        .and_then(|()| header.split(' ').next().map(str::to_owned))
}

/// `POST /v1/jobs`: a job described whole (`{"job": {"repo", "kind", "task", …}}`).
pub async fn dispatch(
    State(shared): State<Arc<Shared>>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("The body isn't JSON: {e}")))?;
    let Some(job) = value.get("job").and_then(Value::as_object) else {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The body is {\"job\": {…}}.",
        ));
    };
    let whole = ["repo", "kind", "task"]
        .iter()
        .all(|k| job.contains_key(*k));
    if !whole {
        if job.contains_key("id") {
            return Err(ApiError::new(
                StatusCode::GONE,
                "Oby starts planned work by itself now: POST /v1/plans starts it and answers the jobs. POST /v1/jobs takes a whole job (repo, kind, task).",
            ));
        }
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A job needs repo, kind and task.",
        ));
    }
    let mut new =
        new_job(job).map_err(|why| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))?;
    new.vetted = typed_by_owner(&shared, "POST", &uri, &headers, &body);
    if job.get("runsOn").is_some_and(|place| !place.is_null()) {
        return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, ELSEWHERE));
    }
    // The owner's words start it, so they open its conversation (crate::task_conversation).
    let words = new.task.clone();
    let unsigned = !new.vetted;
    let foreign = unsigned.then(|| "words no app of the owner's signed".to_owned());
    let job = crate::task_conversation::create(
        &shared,
        new,
        Some(crate::task_conversation::Words {
            text: &words,
            foreign,
        }),
        None,
    )
    .map_err(|refused| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, refused.0))?;
    // Words no app of the owner's signed: two reviews merge its pull request (MERGING-14).
    let job = match unsigned {
        true => shared.jobs.mark_unsigned(&job.id).unwrap_or(job),
        false => job,
    };
    Ok(json(StatusCode::CREATED, job.to_json()))
}

/// The body's fields, by type; what they mean is checked when the job is made.
fn new_job(job: &Map<String, Value>) -> Result<NewJob, String> {
    let text = |key: &str| -> Result<Option<String>, String> {
        match job.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("{key} is a string.")),
        }
    };
    let number = |key: &str| -> Result<Option<u64>, String> {
        match job.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .filter(|n| *n >= 1)
                .map(Some)
                .ok_or_else(|| format!("{key} is a positive whole number.")),
        }
    };
    let files = match job.get("files") {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|f| f.as_str().map(str::to_owned).ok_or("files are paths."))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err("files is a list of paths.".into()),
    };
    match job.get("startWhen") {
        None | Some(Value::Null) => {}
        Some(Value::Array(items)) if items.is_empty() => {}
        Some(_) => return Err(
            "obid doesn't wait for a start condition yet: leave startWhen out to start at once."
                .into(),
        ),
    }
    if job.get("reads").is_some_and(|r| !r.is_null()) {
        return Err(
            "obid doesn't run research jobs with other repositories yet: leave reads out.".into(),
        );
    }
    let rounds = number("rounds")?;
    Ok(NewJob {
        repo: text("repo")?.unwrap_or_default(),
        kind: text("kind")?.unwrap_or_default(),
        task: text("task")?.unwrap_or_default(),
        title: text("title")?,
        request: text("request")?,
        pr: number("pr")?,
        issue: number("issue")?,
        account: text("account")?,
        agent: text("agent")?,
        branch: text("branch")?,
        base: text("base")?,
        rounds: rounds.map(|r| u32::try_from(r).unwrap_or(u32::MAX)),
        check: text("check")?,
        model: text("model")?,
        complex: false,
        files,
        attachments: None,
        mode: text("mode")?,
        conversation: None,
        turn: None,
        brief: None,
        follow_up_of: None,
        earlier: None,
        start_from: None,
        routine: None,
        untrusted: false,
        // Set by `dispatch` only for words an app of the owner's signed (crate::secretspec).
        vetted: false,
        foreign: None,
        split: false,
        adding: None,
        heard: Vec::new(),
    })
}

/// `POST /v1/clear`: Clear done, on any app. obid records now, on its own clock (never earlier than
/// the last clear), answers it, and every app following events hears it.
pub async fn clear(State(shared): State<Arc<Shared>>) -> Result<Response, ApiError> {
    let time = shared
        .cleared
        .clear(obi_protocol::time::now())
        .map_err(|e| {
            tracing::error!("can't keep the clear: {e}");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The clear couldn't be kept.",
            )
        })?;
    let body = json!({"clearedAt": obi_protocol::time::format(time)});
    shared.events.send("clear", body.clone());
    // What it cleared may be settled now: its worktrees go (docs/runner-api.md, `cleanup`).
    shared.jobs.clean_soon();
    Ok(json(StatusCode::OK, body))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewDevice {
    name: String,
    public_key: String,
    endorsement: Option<Value>,
}

/// `POST /v1/devices`: a device's publishing key, `{"id"}` (crate::devices). A paired phone's
/// request comes through the gateway, which says which phone it is and whether its pairing may add
/// a key now.
pub async fn register_device(
    State(shared): State<Arc<Shared>>,
    paired: Option<axum::Extension<crate::gateway::PairedDevice>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    let device: NewDevice = serde_json::from_value(value).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "A device is {\"name\", \"publicKey\"}, with an \"endorsement\" for a new key.",
        )
    })?;
    let now = std::time::SystemTime::now();
    let origin = match &paired {
        Some(paired) => crate::devices::Origin::Phone {
            pairing: &paired.0.0,
            may_add_key: shared.gateway.may_add_key(&paired.0.0, now),
        },
        None => crate::devices::Origin::Api,
    };
    let registered = shared.devices.register(
        &device.name,
        &device.public_key,
        device.endorsement.as_ref(),
        origin,
        now,
    );
    registered
        .map(|id| json(StatusCode::OK, json!({"id": id})))
        .map_err(refused)
}

/// `DELETE /v1/devices/{id}` with `{"approval": {"device", "signedAt", "signature"}}`: a
/// registered device's signature for forgetting this one (crate::devices).
pub async fn revoke_device(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let approval = value.get("approval").cloned().unwrap_or(Value::Null);
    shared.devices.revoke(&id, &approval).map_err(refused)?;
    Ok(json(StatusCode::OK, json!({"revoked": id})))
}

fn refused(refused: crate::devices::Refused) -> ApiError {
    use crate::devices::Refused;
    match refused {
        Refused::Malformed(why) => ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why),
        Refused::Unapproved(why) => ApiError::new(StatusCode::FORBIDDEN, why),
        Refused::Unknown => ApiError::new(StatusCode::NOT_FOUND, "That device isn't registered."),
        Refused::Unkept(why) => {
            tracing::error!("can't keep the devices: {why}");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The devices couldn't be kept.",
            )
        }
    }
}

/// The agent new conversation tasks go to (`status.defaultAgent`), kept across restarts.
pub async fn set_default_agent(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    let agent = value
        .get("defaultAgent")
        .and_then(Value::as_str)
        .filter(|agent| crate::settings::valid_default_agent(agent))
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "defaultAgent is auto, claude, codex or cursor.",
            )
        })?;
    crate::settings::update(&shared.config.home, |s| s.default_agent = agent.into()).map_err(
        |_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The setting couldn't be kept.",
            )
        },
    )?;
    Ok(json(StatusCode::OK, json!({"defaultAgent": agent})))
}

/// The jobs' settings as `GET` and `PUT /v1/settings/jobs` answer them.
fn job_settings_json(shared: &Shared) -> Value {
    json!({
        "maxRunning": shared.jobs.max_running(),
        "accountFailover": shared.jobs.account_failover(),
        "defaultAccount": crate::settings::load(&shared.config.home).default_account,
    })
}

/// `GET /v1/settings/jobs`: how many jobs run at once, and whether work moves on past an
/// account's usage limit by itself.
pub async fn job_settings(State(shared): State<Arc<Shared>>) -> Response {
    json(StatusCode::OK, job_settings_json(&shared))
}

/// `PUT /v1/settings/jobs`: `{"maxRunning": 1…20, "accountFailover": true|false, "defaultAccount":
/// "<account id>"|null}`, any of them, kept across restarts. Raising `maxRunning` starts waiting
/// jobs at once; lowering it stops nothing.
pub async fn set_job_settings(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    let bad = || {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "The body is {\"maxRunning\": 1 to 20, \"accountFailover\": true or false, \"defaultAccount\": an account's id or null}, any of them.",
        )
    };
    let max = match value.get("maxRunning") {
        None => None,
        Some(n) => Some(
            n.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(bad)?,
        ),
    };
    let failover = match value.get("accountFailover") {
        None => None,
        Some(on) => Some(on.as_bool().ok_or_else(bad)?),
    };
    // Some(None) clears it: routing's own order again.
    let default_account = match value.get("defaultAccount") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(id) => {
            let id = id.as_str().ok_or_else(bad)?;
            if crate::accounts::find(&shared.config.agents.get(), id).is_none() {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("There's no account {id}."),
                ));
            }
            Some(Some(id.to_owned()))
        }
    };
    if max.is_none() && failover.is_none() && default_account.is_none() {
        return Err(bad());
    }
    use crate::jobs::SettingsError;
    let kept = match max {
        Some(max) => shared.jobs.set_max_running(max).map(|_| ()),
        None => Ok(()),
    }
    .and_then(|()| match failover {
        Some(on) => crate::settings::update(&shared.config.home, |s| s.account_failover = Some(on))
            .map_err(SettingsError::Unkept),
        None => Ok(()),
    })
    .and_then(|()| match default_account {
        Some(id) => crate::settings::update(&shared.config.home, |s| s.default_account = id)
            .map_err(SettingsError::Unkept),
        None => Ok(()),
    });
    match kept {
        Ok(()) => Ok(json(StatusCode::OK, job_settings_json(&shared))),
        Err(SettingsError::Refused(why)) => {
            Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))
        }
        Err(e) => {
            tracing::error!("{e}");
            Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The setting couldn't be kept.",
            ))
        }
    }
}

/// `POST /v1/jobs/{id}/messages`: `{"text": "…"}`, the owner's words to the job's agent: `200`
/// with the job that heard them, or `201` with the follow-up a job that ended got.
pub async fn message(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if shared.jobs.get(&id).is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB));
    }
    let bad = || ApiError::new(StatusCode::BAD_REQUEST, "The body is {\"text\": \"…\"}.");
    let value = serde_json::from_slice::<Value>(&body).map_err(|_| bad())?;
    let text = value.get("text").and_then(Value::as_str).ok_or_else(bad)?;
    let attachments = match value.get("attachments") {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(items)) => {
            let paths = items
                .iter()
                .map(|p| p.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "attachments are paths."))?;
            crate::brain::validate_attachments(&shared.config.home, &paths)?
        }
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "attachments are paths.",
            ));
        }
    };
    let say = value.get("conversation").and_then(Value::as_bool) == Some(true);
    let first = shared.jobs.get(&id);
    let typed = typed_by_owner(&shared, "POST", &uri, &headers, &body);
    if first.as_ref().is_some_and(|job| job.account_limit_blocked) && !typed {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Continuing a limit-stopped job needs your app's signed choice.",
        ));
    }
    let answer = shared.jobs.message_attaching(
        &id,
        text,
        if typed {
            crate::jobs::Source::Owner
        } else {
            crate::jobs::Source::Api
        },
        &attachments,
    );
    let job = match answer {
        None => return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(refused)) => {
            return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, refused.0));
        }
        Some(Ok(job)) => job,
    };
    // Words no app of the owner's signed: the job that heard them (or the follow-up they started)
    // is no longer the owner's own words alone (crate::secretspec).
    let job = if typed {
        job
    } else {
        // Unsigned API follow-ups are born untrusted, before their scheduler can run. Keep
        // the same audit evidence even when there is no vetted state left to revoke.
        if !job.vetted {
            tracing::info!(
                "job {}: words no app of the owner's signed reached it",
                job.id
            );
        }
        shared
            .jobs
            .unvet(&job.id, "words no app of the owner's signed reached it");
        shared.jobs.mark_unsigned(&job.id).unwrap_or(job)
    };
    // Said on the task's page (Sangam's TaskView): the words and Oby's answer go on the end of the
    // job's conversation, which the app opens, as if said there. A task with none (one kept from
    // before every task had one) gets it now (crate::task_conversation, TASKCONV-4).
    let conversation = say
        .then(|| crate::task_conversation::of(&shared, &id))
        .flatten();
    let job = shared.jobs.get(&job.id).unwrap_or(job);
    if let Some((first, conversation)) = first.zip(conversation) {
        let said = shared
            .conversations
            .append(
                &conversation,
                "user",
                text.trim().to_owned(),
                vec![],
                (!typed).then(|| "words no app of the owner's signed".to_owned()),
            )
            .and_then(|_| {
                let line = crate::brain::actions::told_line(&shared, &first, &job);
                // It names the job: on obid's allowlist only as the job's own words are.
                let foreign = first
                    .check_refuses_secrets()
                    .map(|why| format!("the title of job {}, which {why}", first.id));
                shared.conversations.append(
                    &conversation,
                    "oby",
                    line,
                    vec![job.id.clone()],
                    foreign,
                )
            });
        if let Err(e) = said {
            tracing::info!("words for job {id}; conversation {conversation} wasn't told: {e:?}");
        }
    }
    // A follow-up it started; one already working heard the words, and is a 200.
    let status = if job.id != id && job.heard.is_empty() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok(json(status, job.to_json()))
}

/// `GET /v1/jobs/{id}/publish`: what publishing the job by hand would do, in obid's one sentence,
/// and its base: for a job whose own publish failed. `409` when there's nothing to publish.
pub async fn publish_action(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    match shared.jobs.offer_publish(&id) {
        None => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(why)) => Err(ApiError::new(StatusCode::CONFLICT, why)),
        Some(Ok((action, base))) => Ok(json(
            StatusCode::OK,
            json!({"action": action, "base": base}),
        )),
    }
}

/// `POST /v1/jobs/{id}/publish`: publishes by hand, once a registered device approved obid's own
/// sentence for it (crate::approval): `{"url"}` (null for a remote with no pull requests), `403` for an approval that doesn't hold, `409`
/// when there's nothing to publish or publishing failed (the line says why, never a secret).
/// Never a merge.
pub async fn publish(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    let approval: crate::approval::Approval = serde_json::from_value(value).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "An approval is {\"device\", \"action\", \"signedAt\", \"signature\"}.",
        )
    })?;
    let action = match shared.jobs.publish_action(&id) {
        None => return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Some(Err(why)) => return Err(ApiError::new(StatusCode::CONFLICT, why)),
        Some(Ok((action, _))) => action,
    };
    let key = shared.devices.public_key(&approval.device);
    shared
        .jobs
        .approvals()
        .check(
            &approval,
            &id,
            &action,
            key.as_deref(),
            obi_protocol::time::now(),
        )
        .map_err(|why| ApiError::new(StatusCode::FORBIDDEN, why))?;
    // Checked fresh by `check` above.
    let signed_at = obi_protocol::time::parse(&approval.signed_at).unwrap_or(i64::MAX);
    match shared.jobs.publish_by_hand(&id, &action, signed_at).await {
        Ok(url) => Ok(json(StatusCode::OK, json!({"url": url}))),
        Err(why) => Err(ApiError::new(StatusCode::CONFLICT, why)),
    }
}

/// `POST /v1/jobs/{id}/permission`: `{"id", "decision": "allow" | "always" | "deny", "message"}`,
/// the owner's answer to the job's waiting permission, or `{"id", "answer"}` to the agent's
/// question (#858). Returns the job, with the next waiting one or null.
pub async fn permission(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    uri: axum::http::Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if shared.jobs.get(&id).is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB));
    }
    // An answer's words, or a no's, that no app of the owner's signed (crate::secretspec).
    let unsigned = || !typed_by_owner(&shared, "POST", &uri, &headers, &body);
    let value: Value = serde_json::from_slice(&body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    let shape = "The body is {\"id\", \"decision\": \"allow\" | \"always\" | \"deny\", \"message\"}, or {\"id\", \"answer\"} for a question.";
    let permission = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, shape))?;
    use crate::jobs::PermissionRefused;
    let answered = match value.get("answer") {
        None | Some(Value::Null) => None,
        Some(Value::String(answer)) if value.get("decision").is_none() => {
            let unsigned = unsigned();
            if unsigned {
                shared
                    .jobs
                    .unvet(&id, "an answer no app of the owner's signed reached it");
            }
            Some(
                shared
                    .jobs
                    .answer_question_as(&id, permission, answer, !unsigned),
            )
        }
        Some(_) => return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, shape)),
    };
    if let Some(answered) = answered {
        return match answered {
            Ok(job) => Ok(json(StatusCode::OK, job.to_json())),
            Err(PermissionRefused::NoJob) => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
            Err(PermissionRefused::NotWaiting(why)) => {
                Err(ApiError::new(StatusCode::CONFLICT, why))
            }
            Err(PermissionRefused::Refused(why)) => {
                Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))
            }
        };
    }
    let decision = value
        .get("decision")
        .and_then(Value::as_str)
        .and_then(crate::permissions::Decision::parse)
        .ok_or_else(|| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, shape))?;
    let message = match value.get("message") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.as_str()),
        Some(_) => return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, shape)),
    };
    if message.is_some_and(|m| !m.trim().is_empty()) && unsigned() {
        shared
            .jobs
            .unvet(&id, "words no app of the owner's signed reached it");
    }
    match shared
        .jobs
        .answer_permission(&id, permission, decision, message)
    {
        Ok(job) => Ok(json(StatusCode::OK, job.to_json())),
        Err(PermissionRefused::NoJob) => Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB)),
        Err(PermissionRefused::NotWaiting(why)) => Err(ApiError::new(StatusCode::CONFLICT, why)),
        Err(PermissionRefused::Refused(why)) => {
            Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, why))
        }
    }
}

/// The mode a body names: `{"mode": "ask" | "acceptEdits" | "plan" | "bypass" | "sandbox"}`.
fn mode_of(body: &Bytes) -> Result<crate::permissions::Mode, ApiError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "That isn't JSON."))?;
    value
        .get("mode")
        .and_then(Value::as_str)
        .and_then(crate::permissions::Mode::parse)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "mode is ask, acceptEdits, plan, bypass or sandbox.",
            )
        })
}

/// `POST /v1/jobs/{id}/mode`: how much the job asks, from now on.
pub async fn mode(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    if shared.jobs.get(&id).is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, NO_JOB));
    }
    let mode = mode_of(&body)?;
    let job = shared
        .jobs
        .set_mode(&id, mode)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, NO_JOB))?;
    Ok(json(StatusCode::OK, job.to_json()))
}

/// `GET /v1/settings/permissions`: the mode new jobs get.
pub async fn permission_settings(State(shared): State<Arc<Shared>>) -> Response {
    json(
        StatusCode::OK,
        json!({"mode": shared.jobs.default_mode().as_str()}),
    )
}

/// `PUT /v1/settings/permissions`: `{"mode"}`, the mode new jobs get, kept across restarts. Jobs
/// already made keep theirs.
pub async fn set_permission_settings(
    State(shared): State<Arc<Shared>>,
    body: Bytes,
) -> Result<Response, ApiError> {
    let mode = mode_of(&body)?;
    shared.jobs.set_default_mode(mode).map_err(|e| {
        tracing::error!("can't keep the permission mode: {e}");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The setting couldn't be kept.",
        )
    })?;
    Ok(json(StatusCode::OK, json!({"mode": mode.as_str()})))
}

/// `GET /v1/permissions/rules`: the saved rules, oldest first.
pub async fn permission_rules(State(shared): State<Arc<Shared>>) -> Response {
    let rules: Vec<Value> = shared.jobs.rules().iter().map(|r| r.to_json()).collect();
    json(StatusCode::OK, Value::Array(rules))
}

/// `DELETE /v1/permissions/rules/{id}`: forgets one, and answers with the rules left.
pub async fn forget_permission_rule(
    State(shared): State<Arc<Shared>>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    match shared.jobs.forget_rule(&id) {
        None => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "There's no such rule.",
        )),
        Some(Err(e)) => {
            tracing::error!("can't keep the permission rules: {e}");
            Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "The rules couldn't be kept.",
            ))
        }
        Some(Ok(left)) => Ok(json(
            StatusCode::OK,
            Value::Array(left.iter().map(|r| r.to_json()).collect()),
        )),
    }
}
