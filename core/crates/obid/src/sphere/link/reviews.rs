//! Review completion & publish gate, self-answered questions, GitHub host approval,
//! outgoing Sphere requests, Council seat checks, and teammate review dispatch.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use obi_protocol::client::ClientError;
use obi_protocol::time;
use serde_json::{Value, json};

use super::*;

impl Link {
    // -----------------------------------------------------------------------------------------
    // The review job's end: the publish gate and the result

    /// The review job of an exchange ended (before the job says so): `done` with its result body,
    /// or not. The publish gate scans every text of the result; clean, it goes; a finding, or a
    /// scanner that can't run, sends a failed result with no text. An approved review the request
    /// asks to approve on GitHub is approved there first, or asked about ([`Link::approve_on_host`]).
    /// Waits for the first try to send.
    pub async fn review_ended(&self, job: Job, done: bool) {
        let Some(obi) = job.obi.clone() else { return };
        let cid = obi.correlation_id;
        let (rule, request, from_owner) = {
            let state = self.lock();
            match state.exchanges.get(&cid) {
                Some(e) if e.direction == "incoming" && !e.ended() => (
                    e.rule.clone(),
                    e.request["body"].clone(),
                    e.peer_owner.clone().unwrap_or_default(),
                ),
                _ => return,
            }
        };
        let body = job
            .review_result
            .clone()
            .filter(|b| done && b["outcome"] != "failed");
        let Some(mut body) = body else {
            self.reply_failed(&cid, None).await;
            return;
        };
        let texts = every_text(&body);
        let scratch = self.config.home.join("tmp");
        let verdict = scan::scan(&texts, &scratch).await;
        let (scan_json, failure) = match &verdict {
            scan::Verdict::Clean => (json!({"ran": true, "clean": true}), None),
            scan::Verdict::Found(found) => {
                let places: Vec<String> = found.iter().take(3).map(ToString::to_string).collect();
                (
                    json!({"ran": true, "clean": false}),
                    Some(format!(
                        "The publish gate found a possible secret in {}: no text was sent.",
                        places.join(", ")
                    )),
                )
            }
            scan::Verdict::CouldNotRun(why) => (
                json!({"ran": false, "clean": false}),
                Some(format!(
                    "The secret scan couldn't run ({why}): no text was sent."
                )),
            ),
        };
        if let Some(why) = failure {
            tracing::warn!("sphere: exchange {cid}: {why}");
            self.reply_failed(&cid, Some((scan_json, why))).await;
            return;
        }
        if body["outcome"] == "approved" && approval::asked(&request) {
            // This Obi's own bar, whatever conditions the request listed: its own check passed and
            // its agent reported no blocker, kept or dropped. Below it, nothing is approved or asked.
            let host = if job.approvable {
                self.approve_on_host(&cid, &job.id, &obi.from, &from_owner, &request)
                    .await
            } else {
                "unable"
            };
            body["host_review"] = json!(host);
        }
        if let Some(rule) = rule {
            body["by_rule"] = json!(rule);
        }
        let queued = {
            let mut state = self.lock();
            // Settled while the gate scanned (a revoke): nothing more for it.
            let Some(mut exchange) = state.exchanges.get(&cid).filter(|e| !e.ended()).cloned()
            else {
                return;
            };
            let Some(result) = self.reply_locked(&state, &exchange, "result", body.clone()) else {
                return;
            };
            exchange.outbox.push(result);
            exchange.result = Some(body);
            exchange.publish_scan = Some(scan_json);
            exchange.state = "done".into();
            exchange.updated = time::now();
            state.exchanges.insert(cid.clone(), exchange.clone());
            exchange
        };
        self.save_exchange(&queued);
        self.send_now(&cid).await;
    }

    /// The Obi answers a question by itself under `rule` (protocol.md section 11, "Questions"): a
    /// model writes it from the rule's material only ([`answering`]), then the publish gate scans
    /// it. Clean, the `answer` goes, `by: obi` with the rule in `by_rule`; a possible secret, or a
    /// scanner that can't run, declines it `other` with no text. A model that says it can't
    /// answer, or that fails, sends the question to the owner.
    pub(super) async fn answer_by_itself(
        self: Arc<Self>,
        cid: String,
        rule: Rule,
        repos: Vec<String>,
        referenced: Vec<String>,
    ) {
        let Ok(_run) = self.answering.acquire().await else {
            return;
        };
        // A remembered decline the owner made while it waited covers it now: no run.
        if self.declined_meanwhile(&cid, &repos) {
            return;
        }
        // Once a run may start: the question still waits for it, unexpired, and the rule is still
        // there and still covers it (the owner may have changed it meanwhile). Else no run.
        let snapshot = {
            let state = self.lock();
            let Some(e) = state.exchanges.get(&cid).filter(|e| {
                e.state == "answering" && e.expires_at().is_none_or(|t| t > time::now())
            }) else {
                return;
            };
            let owner = e.peer_owner.clone().unwrap_or_default();
            state
                .sharing
                .rules
                .iter()
                .find(|r| r.id == rule.id && r.covers(&owner, &repos))
                .cloned()
                .map(|rule| {
                    let shared: Vec<Exchange> = state
                        .exchanges
                        .values()
                        .filter(|x| x.direction == "incoming" && x.peer_owner == e.peer_owner)
                        .cloned()
                        .collect();
                    let declined = decisions::declined_repos(&state.decisions, &owner);
                    (e.request["body"].clone(), owner, shared, declined, rule)
                })
        };
        let Some((question, owner, shared, declined, rule)) = snapshot else {
            self.to_owner(&cid);
            return;
        };
        let jobs = self.jobs.list(usize::MAX);
        let shared: Vec<&Exchange> = shared.iter().collect();
        let material = answering::material(
            &rule,
            &owner,
            &repos,
            &declined,
            &referenced,
            &jobs,
            &shared,
        );
        let dir = self.config.home.join("answers").join(&cid);
        let written = match answering::write(
            &self.config,
            &self.sandbox,
            &dir,
            &owner,
            &question,
            &material,
        )
        .await
        {
            Ok(Some(written)) => written,
            Ok(None) => {
                self.to_owner(&cid);
                return;
            }
            Err(why) => {
                // Why names checks, never the question's or the model's words.
                tracing::warn!("sphere: exchange {cid}: obid couldn't answer by itself ({why})");
                self.to_owner(&cid);
                return;
            }
        };
        let mut body = json!({"by": "obi", "text": written.text, "by_rule": rule.id});
        if !written.refs.is_empty() {
            body["refs"] = json!(written.refs);
        }
        if let Some(choice) = written.choice {
            body["choice"] = json!(choice);
        }
        if !obi_protocol::schema::is_valid("actions/answer", &body) {
            tracing::warn!("sphere: exchange {cid}: the answer obid wrote isn't the spec's");
            self.to_owner(&cid);
            return;
        }
        let scratch = self.config.home.join("tmp");
        let _ = std::fs::create_dir_all(&scratch);
        let verdict = scan::scan(&every_text(&body), &scratch).await;
        let (scan_json, failure) = match &verdict {
            scan::Verdict::Clean => (json!({"ran": true, "clean": true}), None),
            scan::Verdict::Found(found) => {
                let places: Vec<String> = found.iter().take(3).map(ToString::to_string).collect();
                (
                    json!({"ran": true, "clean": false}),
                    Some(format!(
                        "The publish gate found a possible secret in {}: no text was sent.",
                        places.join(", ")
                    )),
                )
            }
            scan::Verdict::CouldNotRun(why) => (
                json!({"ran": false, "clean": false}),
                Some(format!(
                    "The secret scan couldn't run ({why}): no text was sent."
                )),
            ),
        };
        let now = time::now();
        let queued = {
            let mut state = self.lock();
            let Some(mut exchange) = state
                .exchanges
                .get(&cid)
                .filter(|e| e.state == "answering" && e.expires_at().is_none_or(|t| t > now))
                .cloned()
            else {
                return;
            };
            // A remembered decline made while the model ran covers it now: declined, and nothing
            // the model wrote goes.
            if self.decline_remembered(&mut state, &mut exchange, &repos, now) {
                exchange.updated = now;
                state.exchanges.insert(cid.clone(), exchange.clone());
                let decisions = state.decisions.clone();
                drop(state);
                self.save_decisions(&decisions);
                self.save_exchange(&exchange);
                self.outbox.notify_one();
                return;
            }
            // The rule may have gone while the model ran: then nothing it wrote goes.
            let still = state
                .sharing
                .rules
                .iter()
                .any(|r| r.id == rule.id && r.covers(&owner, &repos));
            let reply = match &failure {
                _ if !still => None,
                None => self.reply_locked(&state, &exchange, "answer", body.clone()),
                Some(_) => {
                    self.reply_locked(&state, &exchange, "decline", json!({"reason": "other"}))
                }
            };
            let Some(reply) = reply else {
                drop(state);
                self.to_owner(&cid);
                return;
            };
            exchange.outbox.push(reply);
            match failure {
                None => {
                    exchange.result = Some(body);
                    exchange.state = "done".into();
                    if let Some(r) = state.sharing.rules.iter_mut().find(|r| r.id == rule.id) {
                        r.uses += 1;
                        r.last_used = Some(now);
                    }
                    // Kept under the lock, so an older copy never overwrites a newer one.
                    self.save_sharing(&state.sharing);
                }
                Some(why) => {
                    tracing::warn!("sphere: exchange {cid}: {why}");
                    exchange.state = "declined".into();
                    exchange.error = Some(why);
                }
            }
            exchange.publish_scan = Some(scan_json);
            exchange.updated = now;
            state.exchanges.insert(cid.clone(), exchange.clone());
            exchange
        };
        self.save_exchange(&queued);
        self.send_now(&cid).await;
    }

    /// A review whose request asks to approve it on GitHub (`approve_on_host`) came out approved.
    /// Under the owner's standing permission for the sender's owner and the repository (a
    /// remembered [`approval::HOST_APPROVAL`] decision, never anything received), obid approves it
    /// there now when the people who own every file it changes already approved that commit
    /// ([`Link::code_owners_approved`]); otherwise it asks the owner to confirm this one exchange
    /// with their own tap ([`approval::item`]). What the result's `host_review` says: `approved`,
    /// `unable` or `asked`.
    pub(super) async fn approve_on_host(
        &self,
        cid: &str,
        job: &str,
        from: &str,
        from_owner: &str,
        request: &Value,
    ) -> &'static str {
        let text = |key: &str| request[key].as_str().unwrap_or("");
        let (repo, base, sha) = (text("repo"), text("base_branch"), text("head_sha"));
        let Some(pr) = request["pr"].as_u64() else {
            return "unable";
        };
        // Revoked meanwhile (the exchange settled here), or out of the Sphere: nothing more is done
        // for its peer. The expiry, when the exchange is still open.
        let open = |state: &State| {
            state
                .exchanges
                .get(cid)
                .filter(|e| e.state == "accepted")
                .filter(|_| {
                    state
                        .membership
                        .as_ref()
                        .is_some_and(|m| m.state == "joined")
                })
                .map(|e| e.request["expires_at"].as_str().unwrap_or("").to_owned())
        };
        let (expires_at, permitted) = {
            let state = self.lock();
            let Some(expires_at) = open(&state) else {
                return "unable";
            };
            let permitted = decisions::covering(
                &state.decisions,
                approval::HOST_APPROVAL,
                from_owner,
                Some(repo),
            )
            .filter(|d| d.decision == "accept")
            .map(|d| d.id.clone());
            (expires_at, permitted)
        };
        let until = time::parse(&expires_at).unwrap_or(0);
        if let Some(id) = permitted {
            match self
                .code_owners_approved(cid, repo, pr, base, sha, &expires_at)
                .await
            {
                Ok(()) => {
                    let decisions = {
                        let mut state = self.lock();
                        if let Some(d) = state.decisions.iter_mut().find(|d| d.id == id) {
                            d.uses += 1;
                            d.last_used = Some(time::now());
                        }
                        state.decisions.clone()
                    };
                    self.save_decisions(&decisions);
                    let next_to = Some(expires_at.as_str());
                    return match self
                        .post_approval(cid, job, repo, pr, base, sha, until, next_to)
                        .await
                    {
                        Ok(()) => "approved",
                        Err(why) => {
                            tracing::warn!(
                                "sphere: exchange {cid}: obid didn't approve {repo}#{pr}: {why}"
                            );
                            "unable"
                        }
                    };
                }
                Err(why) => tracing::info!(
                    "sphere: exchange {cid}: no code owner's approval of {repo}#{pr} to stand next to ({why}): asking the owner"
                ),
            }
        }
        let asking = {
            let mut state = self.lock();
            if open(&state).is_none() {
                return "unable";
            }
            let item = approval::item(
                cid,
                from,
                from_owner,
                request,
                &expires_at,
                job,
                time::now(),
            );
            state.inbox.insert(item.id.clone(), item.clone());
            item
        };
        self.save_item(&asking);
        "asked"
    }

    /// Whether the people who own every file of pull request `pr` of `repo` approved `sha` on
    /// GitHub, so that obid's approval, under the owner's standing permission, stands next to
    /// theirs (`obi_protocol::host_approval`, the spec's rule): what the owner's `gh` reads
    /// ([`approval::code_host`]) right now. Why not, else.
    pub(super) async fn code_owners_approved(
        &self,
        cid: &str,
        repo: &str,
        pr: u64,
        base: &str,
        sha: &str,
        expires_at: &str,
    ) -> Result<(), String> {
        let gh = crate::github::gh().ok_or("gh isn't installed")?;
        let (code_host, owner_login) = approval::code_host(&gh, repo, pr, base, sha).await?;
        self.code_owners_decide(cid, repo, sha, expires_at, code_host, owner_login)
    }

    /// The spec's rule on what GitHub showed (`code_host`, read with `owner_login`'s `gh`), for
    /// exchange `cid` under the owner's standing permission.
    pub(super) fn code_owners_decide(
        &self,
        cid: &str,
        repo: &str,
        sha: &str,
        expires_at: &str,
        code_host: obi_protocol::host_approval::CodeHost,
        owner_login: String,
    ) -> Result<(), String> {
        use obi_protocol::host_approval::{Context, Input, Request, decide};
        let context = Context {
            now: time::format(time::now()),
            request: Request {
                correlation_id: cid.to_owned(),
                repo: repo.to_owned(),
                head_sha: sha.to_owned(),
                expires_at: expires_at.to_owned(),
            },
            member: self.is_joined(),
            owner_login,
            standing_permission: true,
        };
        let input = Input {
            owner_confirmations: Vec::new(),
            code_host,
        };
        decide(&context, &input).map_err(|r| r.message)
    }

    /// Whether obid's approval of `sha` on pull request `pr` of `repo` still stands, read again
    /// right after it was posted: the pull request is still the change reviewed, and, for one next
    /// to code owners' (`next_to`, the request's expiry), theirs still hold.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn recheck(
        &self,
        gh: &std::path::Path,
        cid: &str,
        repo: &str,
        pr: u64,
        base: &str,
        sha: &str,
        next_to: Option<&str>,
    ) -> Result<(), Recheck> {
        let view = approval::view(gh, repo, pr)
            .await
            .map_err(Recheck::Unread)?;
        approval::as_reviewed(&view, repo, pr, base, sha).map_err(Recheck::Changed)?;
        if let Some(expires_at) = next_to {
            let (host, own) = approval::code_host(gh, repo, pr, base, sha)
                .await
                .map_err(Recheck::Unread)?;
            self.code_owners_decide(cid, repo, sha, expires_at, host, own)
                .map_err(Recheck::Changed)?;
        }
        Ok(())
    }

    /// The owner confirmed approving a teammate's pull request on GitHub for one exchange, `item`
    /// ([`approval::item`]), with their device's signature made at `signed_at` (checked by the
    /// route, once). obid checks it as the spec's rule does (`obi_protocol::host_approval`): that
    /// exchange, still open to approve, at the commit its request named, a confirmation from the
    /// owner's device within two minutes; then approves it ([`Link::post_approval`]). The claim is
    /// on disk before GitHub is asked: obid stopping mid-post never approves it twice.
    pub(super) async fn confirmed_approval(
        &self,
        item: &str,
        payload: &Value,
        signed_at: Option<i64>,
    ) -> Result<(), ApiError> {
        use obi_protocol::host_approval::{Confirmation, Context, Input, Request, decide};
        let text = |key: &str| payload[key].as_str().unwrap_or("").to_owned();
        let (cid, repo, base, job) = (
            text("correlationId"),
            text("repo"),
            text("baseBranch"),
            text("job"),
        );
        let pr = payload["pr"].as_u64().unwrap_or(0);
        let lost = || {
            api(
                StatusCode::CONFLICT,
                "Nothing was approved: that request is gone.",
            )
        };
        if item != approval::item_id(&cid) || !crate::github::is_repo(&repo) || pr == 0 {
            return Err(lost());
        }
        // The commit and the expiry are the request's own, as obid keeps it.
        let (head, expires_at) = {
            let state = self.lock();
            let exchange = state.exchanges.get(&cid).ok_or_else(lost)?;
            let body = &exchange.request["body"];
            if exchange.direction != "incoming"
                || !approval::asked(body)
                || body["repo"] != json!(repo)
                || body["pr"].as_u64() != Some(pr)
            {
                return Err(lost());
            }
            (
                body["head_sha"].as_str().unwrap_or("").to_owned(),
                exchange.request["expires_at"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
            )
        };
        let context = Context {
            now: time::format(time::now()),
            request: Request {
                correlation_id: cid.clone(),
                repo: repo.clone(),
                head_sha: head.clone(),
                expires_at: expires_at.clone(),
            },
            member: self.is_joined(),
            owner_login: String::new(),
            standing_permission: false,
        };
        let input = Input {
            owner_confirmations: vec![Confirmation {
                correlation_id: cid.clone(),
                head_sha: text("headSha"),
                signed_at: signed_at.map(time::format).unwrap_or_default(),
                from_owner_device: signed_at.is_some(),
                used_before: false,
            }],
            ..Input::default()
        };
        if let Err(refused) = decide(&context, &input) {
            let why = if refused.code == "lapsed" {
                "That request lapsed: nothing was approved.".to_owned()
            } else {
                format!("Nothing was approved: {}.", refused.message)
            };
            return Err(api(StatusCode::CONFLICT, why));
        }
        // Kept before GitHub is asked: obid stopping mid-post never approves it twice.
        let claimed = self.item(item).ok_or_else(lost)?;
        if let Err(why) = self.keep_item(&claimed) {
            return Err(api(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Nothing was approved: obid couldn't keep the claim ({why})."),
            ));
        }
        let until = time::parse(&expires_at).unwrap_or(0);
        self.post_approval(&cid, &job, &repo, pr, &base, &head, until, None)
            .await
            .map_err(|why| {
                api(
                    StatusCode::BAD_GATEWAY,
                    format!("No approval stands: {why}."),
                )
            })
    }

    /// Approves `sha` on pull request `pr` of `repo`, into `base`, on GitHub with the owner's own
    /// `gh` ([`approval::post`]), for exchange `cid`, while obid is still in its Sphere and before
    /// `until`, the request's expiry, checked again right before GitHub is asked, and lists it in
    /// review job `job`'s outgoing actions. Right after, it reads the pull request again
    /// ([`Link::recheck`]; once more when `gh` couldn't read it): when its head moved, or, for an
    /// approval next to code owners' (`next_to`: the request's expiry), theirs no longer hold, or
    /// it still can't be read, obid dismisses its approval at once ([`approval::dismiss`]) and says
    /// why. One GitHub doesn't let it dismiss is the owner's to remove: an inbox item says so
    /// ([`approval::dismissal_item`]).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn post_approval(
        &self,
        cid: &str,
        job: &str,
        repo: &str,
        pr: u64,
        base: &str,
        sha: &str,
        until: i64,
        next_to: Option<&str>,
    ) -> Result<(), String> {
        let gh = crate::github::gh().ok_or("gh isn't installed")?;
        let scratch = self
            .config
            .home
            .join("tmp")
            .join(format!("approve-{}", uuid::Uuid::new_v4().simple()));
        // Not once its sender called the review off (protocol.md section 11).
        let still = || self.is_joined() && time::now() < until && self.not_called_off(cid);
        let posted = approval::post(&gh, &scratch, repo, pr, base, sha, still).await;
        let _ = std::fs::remove_dir_all(&scratch);
        let posted = posted?;
        self.jobs.approved_on_github(job, repo, &posted.url);
        // Kept with the exchange, for a cancel that comes after it; a cancel that came while it was
        // posted means it goes at once.
        let ours = json!({"job": job, "repo": repo, "pr": pr, "id": posted.id, "url": posted.url});
        let kept = {
            let mut state = self.lock();
            match state.exchanges.get_mut(cid) {
                Some(e) if e.state != "cancelled" => {
                    e.host_approval = Some(ours);
                    Some(e.clone())
                }
                _ => None,
            }
        };
        let why = match kept {
            None => "its sender called the review off".to_owned(),
            Some(exchange) => {
                self.save_exchange(&exchange);
                let mut checked = self.recheck(&gh, cid, repo, pr, base, sha, next_to).await;
                if let Err(Recheck::Unread(why)) = &checked {
                    tracing::info!("sphere: exchange {cid}: reading {repo}#{pr} again ({why})");
                    checked = self.recheck(&gh, cid, repo, pr, base, sha, next_to).await;
                }
                let why = match checked {
                    Ok(()) => return Ok(()),
                    Err(Recheck::Changed(why)) => why,
                    Err(Recheck::Unread(why)) => format!("obid couldn't read it again ({why})"),
                };
                // A cancel that came meanwhile took it, and dismisses it.
                let mine = self
                    .lock()
                    .exchanges
                    .get_mut(cid)
                    .and_then(|e| e.host_approval.take())
                    .is_some();
                if !mine {
                    return Err(format!("{why} right after obid approved it"));
                }
                why
            }
        };
        let dismissed = match posted.id {
            Some(id) => approval::dismiss(&gh, repo, pr, id).await,
            None => Err("GitHub didn't say which review it was".into()),
        };
        match dismissed {
            Ok(()) => {
                self.jobs.dismissed_on_github(job, repo, &posted.url);
                Err(format!(
                    "{why} right after obid approved it, so obid dismissed its approval"
                ))
            }
            Err(failed) => {
                tracing::error!(
                    "sphere: exchange {cid}: obid approved {repo}#{pr} and couldn't dismiss it ({failed}) though {why}: the owner removes {} on GitHub",
                    posted.url
                );
                let item = approval::dismissal_item(cid, job, repo, pr, &posted.url, time::now());
                self.lock().inbox.insert(item.id.clone(), item.clone());
                self.save_item(&item);
                Err(format!(
                    "{why} right after obid approved it, and GitHub didn't let obid dismiss its approval: remove it on GitHub"
                ))
            }
        }
    }

    /// Settles the open asks to approve a teammate's pull request on GitHub ([`approval::item`],
    /// and the `obiRequest` ones from before it) that `due` picks: done with no answer, `why` at
    /// the end of their text. Nothing is approved.
    pub(super) fn settle_host_approvals(&self, due: impl Fn(&Item) -> bool, why: &str) {
        let now = time::now();
        let settled: Vec<Item> = {
            let mut state = self.lock();
            state
                .inbox
                .values_mut()
                .filter(|i| i.done.is_none() && approval::ask_expiry(i).is_some() && due(i))
                .map(|i| {
                    i.done = Some(now);
                    i.text = format!("{} {why}", i.text);
                    i.clone()
                })
                .collect()
        };
        for item in &settled {
            self.save_item(item);
        }
    }

    /// `POST /v1/inbox/{id}/answer` on an `obiRequest` item asking whether to approve a teammate's
    /// pull request on GitHub, as obid made them before the owner's Confirm was needed: an answer
    /// approves nothing. `Accept` (or with remember) turns it into the `confirm` item that asks the
    /// same ([`approval::item`], the same id), and is refused (`403`): only the owner's tap on it
    /// approves. `Decline` or done close it; later puts it off.
    pub(super) async fn answer_host_approval(
        &self,
        mut item: Item,
        answer: Option<String>,
        later: Option<i64>,
    ) -> Result<Value, ApiError> {
        let now = time::now();
        let obi = item.obi.clone().unwrap_or_default();
        let approving = matches!(
            answer.as_deref(),
            Some(inbox::ACCEPT | inbox::ACCEPT_REMEMBER)
        );
        if let Some(at) = later {
            item.later = Some(at);
        } else if answer
            .as_deref()
            .is_some_and(|a| !approving && a != inbox::DECLINE)
        {
            return Err(api(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The answer is one of: {}.", item.options.join("; ")),
            ));
        } else if approving {
            let text = |key: &str| obi[key].as_str().unwrap_or("").to_owned();
            let request = json!({
                "repo": text("repo"), "pr": obi["pr"], "base_branch": text("baseBranch"),
                "head_sha": text("headSha"),
            });
            let asking = approval::item(
                &text("correlationId"),
                &text("from"),
                &text("fromOwner"),
                &request,
                &text("expiresAt"),
                &text("job"),
                now,
            );
            self.lock().inbox.insert(asking.id.clone(), asking.clone());
            self.save_item(&asking);
            return Err(api(StatusCode::FORBIDDEN, NEEDS_YOUR_TAP));
        } else {
            item.done = Some(now);
            item.answer = answer;
        }
        self.lock().inbox.insert(item.id.clone(), item.clone());
        self.save_item(&item);
        Ok(item.to_json())
    }

    /// Sends a failed result with no text (and `by_rule` when a decision accepted it): the work, or
    /// the publish gate (`scan`: what it found, and why nothing with text went), failed.
    pub(super) async fn reply_failed(&self, cid: &str, scan: Option<(Value, String)>) {
        let queued = {
            let mut state = self.lock();
            let Some(mut exchange) = state.exchanges.get(cid).cloned() else {
                return;
            };
            if exchange.direction != "incoming" || exchange.ended() {
                return;
            }
            let mut body = failed_result(&exchange.request);
            if let Some(rule) = &exchange.rule {
                body["by_rule"] = json!(rule);
            }
            let Some(result) = self.reply_locked(&state, &exchange, "result", body.clone()) else {
                return;
            };
            exchange.outbox.push(result);
            exchange.result = Some(body);
            exchange.state = "failed".into();
            if let Some((scan, why)) = scan {
                exchange.publish_scan = Some(scan);
                exchange.error = Some(why);
            }
            exchange.updated = time::now();
            state.exchanges.insert(cid.to_owned(), exchange.clone());
            exchange
        };
        self.save_exchange(&queued);
        self.send_now(cid).await;
    }

    /// Tries to send the exchange's waiting replies now, for at most 30 seconds; the sender tries
    /// again later whatever is left.
    pub(super) async fn send_now(&self, cid: &str) {
        let sent = tokio::time::timeout(Duration::from_secs(30), self.flush(cid)).await;
        if !matches!(sent, Ok(true)) {
            self.outbox.notify_one();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Outgoing requests and exchanges

    /// `POST /v1/obid/sphere/requests`: the owner's Obi asks another Obi.
    pub async fn request(&self, body: &[u8]) -> Result<Value, ApiError> {
        let map = object(body, &["to", "type", "body", "expiresIn"])?;
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        let joined = self.joined()?;
        let to = text_field(&map, "to")?.to_owned();
        let kind = text_field(&map, "type")?.to_owned();
        // run_request and share_offer went with compute sharing (refused below, once the Obi is
        // known, as before).
        // Introductions and follow-ups are additions obid reads but doesn't send in this version:
        // one goes only once every active member takes it (protocol.md section 5).
        if !obi_protocol::REQUEST_TYPES.contains(&kind.as_str()) || kind == "introduction" {
            return Err(invalid(format!(
                "type is review_request, question or handoff, not {kind}."
            )));
        }
        let action = map
            .get("body")
            .ok_or_else(|| invalid("body is the action's fields, and it's required.".into()))?
            .clone();
        if action.get("follows").is_some() {
            return Err(invalid(
                "obid doesn't send follow-ups yet: body has no follows.".into(),
            ));
        }
        let errors = obi_protocol::schema::errors(&format!("actions/{kind}"), &action, 5);
        if !errors.is_empty() {
            return Err(invalid(format!(
                "body doesn't match the spec's {kind}: {}",
                errors.join("; ")
            )));
        }
        let expires_in = map
            .get("expiresIn")
            .and_then(Value::as_i64)
            .filter(|n| (60..=obi_protocol::MAX_LIFETIME_SECONDS).contains(n))
            .ok_or_else(|| invalid("expiresIn is 60 to 2,592,000 seconds.".into()))?;
        if to == joined.obi_id {
            return Err(invalid("An Obi doesn't ask itself.".into()));
        }
        self.scan_outgoing(&action).await?;
        self.send_scanned(&joined, to, kind, action, Expiry::In(expires_in))
            .await
    }

    /// Whether this obid may convene a Council review panel: the `council` setting.
    pub(crate) fn council_enabled(&self) -> bool {
        self.config.council
    }

    /// This Obi's membership, when it has joined: `409` otherwise.
    pub(crate) fn joined(&self) -> Result<Membership, ApiError> {
        self.lock()
            .membership
            .clone()
            .filter(|m| m.state == "joined")
            .ok_or_else(|| api(StatusCode::CONFLICT, "obid hasn't joined a Sphere."))
    }

    /// Scans every text field of an action going out for secrets, before anything is signed or sent
    /// (#478): `422` for a finding, or a scanner that can't run.
    pub(crate) async fn scan_outgoing(&self, action: &Value) -> Result<(), ApiError> {
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        let texts = every_text(action);
        let scratch = self.config.home.join("tmp");
        std::fs::create_dir_all(&scratch).map_err(|e| {
            api(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("obid couldn't scan the request: {e}"),
            )
        })?;
        match scan::scan(&texts, &scratch).await {
            scan::Verdict::Clean => {}
            scan::Verdict::Found(found) => {
                let places: Vec<String> = found.iter().take(3).map(ToString::to_string).collect();
                return Err(invalid(format!(
                    "The secret scan found a possible secret in {}: nothing was sent.",
                    places.join(", ")
                )));
            }
            scan::Verdict::CouldNotRun(why) => {
                return Err(invalid(format!(
                    "The secret scan couldn't run ({why}): nothing was sent."
                )));
            }
        }
        Ok(())
    }

    /// The Council's seats not sent their request yet, checked before any is sent
    /// (crate::sphere::council): each an active member whose card accepts `review_request` and whose
    /// seal key obid trusts when it seals, none this Obi's owner's, and no two with one owner, nor with
    /// the owner of a seat already sent this motion's request (`seated`; rules I2 and I3). `422` (or
    /// `409` for a seal key obid doesn't trust) with nothing sent. Reads the directory first.
    pub(crate) async fn check_seats(
        &self,
        seats: &[String],
        seated: HashSet<String>,
    ) -> Result<(), ApiError> {
        if let Err(e) = self.read_directory().await {
            self.note_error(&e);
            return Err(from_client(&e));
        }
        let kind = "review_request".to_owned();
        let mine = self.owner();
        let state = self.lock();
        let mut owners = seated;
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        for obi in seats {
            let member = state
                .directory
                .members
                .get(obi)
                .ok_or_else(|| invalid(format!("{obi} isn't a member: nothing was sent.")))?;
            if !member.active {
                return Err(invalid(format!(
                    "{obi} has been revoked: nothing was sent."
                )));
            }
            if !member.accepts.contains(&kind) {
                return Err(invalid(format!(
                    "{obi}'s card doesn't accept {kind}: nothing was sent."
                )));
            }
            if member.seal.is_none() && self.pins.seals(obi) {
                return Err(api(
                    StatusCode::CONFLICT,
                    format!(
                        "{obi} seals, and obid trusts no seal key of its now: nothing was sent."
                    ),
                ));
            }
            if member.owner == mine {
                return Err(invalid(format!(
                    "{obi} acts for this Obi's own owner, who never sits on their own panel: nothing was sent."
                )));
            }
            if !owners.insert(member.owner.clone()) {
                return Err(invalid(format!(
                    "{obi} has the same owner as another seat: one seat per owner, nothing was sent."
                )));
            }
        }
        Ok(())
    }

    /// The outgoing `review_request` exchange to `to` for `repo` (ignoring ASCII case) at `head_sha`,
    /// expiring at `expires_at`, when obid sent one, with that request's body (its words opened) and
    /// its receiver's owner: the Council's convener answers it again rather than send a seat a second
    /// request for one motion (its deadline says which motion).
    pub(crate) fn sent_review(
        &self,
        to: &str,
        repo: &str,
        head_sha: &str,
        expires_at: i64,
    ) -> Option<(Value, Value, Option<String>)> {
        let at = time::format(expires_at);
        self.lock()
            .exchanges
            .values()
            .find(|x| {
                x.direction == "outgoing"
                    && x.kind == "review_request"
                    && x.peer == to
                    && x.request["expires_at"] == at.as_str()
                    && x.request["body"]["head_sha"] == head_sha
                    && x.request["body"]["repo"]
                        .as_str()
                        .is_some_and(|r| r.eq_ignore_ascii_case(repo))
            })
            .map(|x| (x.to_json(), x.request["body"].clone(), x.peer_owner.clone()))
    }

    /// The owners of every member this Obi sent a `review_request` for `repo` (ignoring ASCII case) at
    /// `head_sha` expiring at `expires_at`: the seats of one Council motion so far, declined or not.
    pub(crate) fn motion_owners(
        &self,
        repo: &str,
        head_sha: &str,
        expires_at: i64,
    ) -> HashSet<String> {
        let at = time::format(expires_at);
        self.lock()
            .exchanges
            .values()
            .filter(|x| {
                x.direction == "outgoing"
                    && x.kind == "review_request"
                    && x.request["expires_at"] == at.as_str()
                    && x.request["body"]["head_sha"] == head_sha
                    && x.request["body"]["repo"]
                        .as_str()
                        .is_some_and(|r| r.eq_ignore_ascii_case(repo))
            })
            .filter_map(|x| x.peer_owner.clone())
            .collect()
    }

    /// Posts again, byte for byte, a request whose post didn't answer ([`Link::send_scanned`] with an
    /// instant expiry): `None` when `cid` isn't one; otherwise the exchange once the Obysphere has it,
    /// or the error again.
    pub(crate) async fn resend_unconfirmed(&self, cid: &str) -> Option<Result<Value, ApiError>> {
        let envelope = self.lock().unconfirmed.get(cid).cloned()?;
        let sent = match self.token().await {
            Ok(token) => match self.client() {
                Ok(client) => client.send(&token, &envelope).await,
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        Some(match sent {
            Ok(sent) => {
                // Under the resent envelope's own id: Client::send took the receipt because it names
                // that envelope, never because its id happens to be `cid`.
                let id = envelope["id"].as_str().unwrap_or_default();
                self.keep_receipt(id, &sent);
                self.lock().unconfirmed.remove(cid);
                self.lock().seen.insert(cid.to_owned());
                let updated = self.update_exchange(cid, |x| {
                    x.error = None;
                    let kept = x.request.clone();
                    x.messages.insert(
                        0,
                        Message {
                            direction: "sent".into(),
                            seq: Some(sent.seq),
                            at: sent.recorded_at.clone(),
                            envelope: kept,
                        },
                    );
                });
                Ok(updated.map(|x| x.to_json()).unwrap_or(Value::Null))
            }
            Err(e) => {
                self.refused_token(&e);
                Err(from_client(&e))
            }
        })
    }

    /// Sends one request already scanned ([`Link::scan_outgoing`]): sealed for the receiver when its
    /// card has a seal key, signed, checked as the receiver will check it, and kept as an outgoing
    /// exchange. `expiry` is a lifetime from now, or an instant (the Council's convener gives every seat
    /// the same one). Answers the exchange.
    pub(crate) async fn send_scanned(
        &self,
        joined: &Membership,
        to: String,
        kind: String,
        action: Value,
        expiry: Expiry,
    ) -> Result<Value, ApiError> {
        let invalid = |m: String| api(StatusCode::UNPROCESSABLE_ENTITY, m);
        // Built, sealed and sent; once more after a seal refusal, with the directory read again.
        let mut attempt = 0;
        loop {
            attempt += 1;
            if let Err(e) = self.read_directory().await {
                self.note_error(&e);
                return Err(from_client(&e));
            }
            let (envelope, peer_owner) = {
                let state = self.lock();
                let member =
                    state.directory.members.get(&to).ok_or_else(|| {
                        invalid(format!("{to} isn't a member of {}.", joined.sphere))
                    })?;
                if !member.active {
                    return Err(invalid(format!("{to} has been revoked.")));
                }
                if !member.accepts.contains(&kind) {
                    return Err(invalid(format!("{to}'s card doesn't accept {kind}.")));
                }
                // An Obi known to seal gets nothing in the clear: not when the Sphere serves an older
                // card, or a key list its card doesn't sign (protocol.md section 9.1).
                if member.seal.is_none() && self.pins.seals(&to) {
                    return Err(api(
                        StatusCode::CONFLICT,
                        format!(
                            "{to} seals, and obid trusts no seal key of its now: nothing was sent."
                        ),
                    ));
                }
                if matches!(kind.as_str(), "share_offer" | "run_request") {
                    return Err(invalid(
                        "Sharing Macs and API accounts went with compute sharing.".into(),
                    ));
                }
                let key = state
                    .key
                    .clone()
                    .ok_or_else(|| api(StatusCode::CONFLICT, "There's no Obi key."))?;
                let id = new_uuid();
                let now = time::now();
                let expires_at = match expiry {
                    Expiry::In(seconds) => now + seconds,
                    Expiry::At(at) => at,
                };
                if expires_at <= now || expires_at - now > obi_protocol::MAX_LIFETIME_SECONDS {
                    return Err(invalid(
                        "The request would expire in the past, or more than 2,592,000 seconds from now: nothing was sent.".into(),
                    ));
                }
                let unsigned = json!({
                    "protocol": obi_protocol::PROTOCOL,
                    "id": id,
                    "type": kind,
                    "from": joined.obi_id,
                    "to": to,
                    "sphere": joined.sphere,
                    "correlation_id": id,
                    "created_at": time::format(now),
                    "expires_at": time::format(expires_at),
                    "key_id": key.key_id(),
                    "body": action.clone(),
                });
                let Value::Object(mut unsigned) = unsigned else {
                    unreachable!("an envelope is an object")
                };
                // Scanned above, the words are sealed for the receiver when its card has a seal key,
                // then signed (protocol.md section 9.1).
                if let Some(recipient) = &member.seal {
                    obi_protocol::seal::seal(&mut unsigned, recipient).map_err(|e| {
                        api(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("obid couldn't seal it: {e}"),
                        )
                    })?;
                }
                let envelope = obi_protocol::envelope::sign(&unsigned, &key).map_err(|e| {
                    api(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("obid couldn't sign it: {e}"),
                    )
                })?;
                // What the receiver will check, checked here first.
                let context = obi_protocol::envelope::Context {
                    now: Some(now),
                    sphere: Some(joined.sphere.clone()),
                    receiver: Some(to.clone()),
                    keys: state.directory.keys.clone(),
                    seq: None,
                    seen_ids: Vec::new(),
                    // Sealed for the receiver, its words are checked by the schema alone here.
                    seal_keys: None,
                    sender_seals: false,
                };
                obi_protocol::envelope::verify(&envelope, &context).map_err(|r| {
                    invalid(format!("That message wouldn't pass the spec's checks: {r}"))
                })?;
                (envelope, member.owner.clone())
            };
            let cid = envelope["id"].as_str().unwrap_or("").to_owned();
            let kept_envelope = if envelope.get("seal").is_some() {
                kept(envelope.clone(), action.clone())
            } else {
                envelope.clone()
            };
            let mut exchange = Exchange::new(
                "outgoing",
                kept_envelope.clone(),
                to.clone(),
                Some(peer_owner),
            );
            exchange.state = "sent".into();
            // Kept before it goes, so a reply that comes back at once finds it.
            self.lock().exchanges.insert(cid.clone(), exchange.clone());
            let sent = match self.token().await {
                Ok(token) => match self.client() {
                    Ok(client) => client.send(&token, &envelope).await,
                    Err(e) => Err(e),
                },
                Err(e) => Err(e),
            };
            match sent {
                Ok(sent) => {
                    self.keep_receipt(&cid, &sent);
                    self.lock().seen.insert(cid.clone());
                    let updated = self.update_exchange(&cid, |e| {
                        e.messages.insert(
                            0,
                            Message {
                                direction: "sent".into(),
                                seq: Some(sent.seq),
                                at: sent.recorded_at.clone(),
                                envelope: kept_envelope.clone(),
                            },
                        );
                    });
                    return Ok(updated.unwrap_or(exchange).to_json());
                }
                Err(e) => {
                    // A post that didn't answer may have been recorded. For an instant expiry (the
                    // Council's convener), the exchange and the signed envelope stay, so a repeated
                    // call posts the same bytes again rather than send the seat a second request.
                    if matches!(e, ClientError::Unreachable(_)) && matches!(expiry, Expiry::At(_)) {
                        self.lock()
                            .unconfirmed
                            .insert(cid.clone(), envelope.clone());
                        self.update_exchange(&cid, |x| {
                            x.error = Some(format!("Not confirmed yet: {e}"));
                        });
                        return Err(from_client(&e));
                    }
                    self.lock().exchanges.remove(&cid);
                    self.exchanges.remove(&cid);
                    // The Sphere kept nothing: `to`'s card gained a seal key, or another, since the
                    // directory was read. Built once more for the card it has now.
                    let sealing = match &e {
                        ClientError::Refused { code, .. } if code == "not_sealed" => {
                            self.pins.mark_seals(&to);
                            true
                        }
                        ClientError::Refused { code, .. } => {
                            code == "invalid_envelope" && envelope.get("seal").is_some()
                        }
                        _ => false,
                    };
                    if sealing && attempt == 1 {
                        continue;
                    }
                    self.refused_token(&e);
                    return Err(from_client(&e));
                }
            }
        }
    }

    /// The owner's published pull request waits for an approving review from another person that
    /// Oby's own reviewer can't give (the merge step decides that, crate::merge): asks a
    /// teammate's Obi over the Sphere to review exactly `change.head` and, when its review passes,
    /// approve it on GitHub as that teammate (spec `protocol.md` section 11, "Approving on the code
    /// host"). Call it again whenever the merge step looks: one teammate at a time, the next when
    /// one can't, and never another after a review that asked for changes. A new head asks again.
    /// The merge itself stays the merge step's, once GitHub shows the approval of that head.
    pub async fn ask_teammate(&self, change: &approval::Change<'_>) -> approval::Teammate {
        use approval::{Asked, Teammate};
        // The directory as the Sphere has it now; the one read last when it can't be reached.
        if let Err(e) = self.read_directory().await {
            self.note_error(&e);
        }
        let now = time::now();
        let next = {
            let state = self.lock();
            let Some(me) = state
                .membership
                .as_ref()
                .filter(|m| m.state == "joined")
                .map(|m| m.obi_id.clone())
            else {
                return Teammate::NeedsYou(format!(
                    "Oby isn't in an Obysphere, so no teammate's Oby can approve it. {}",
                    approval::instead(change.repo, true)
                ));
            };
            let mut asked = Vec::new();
            for e in approval::requests(
                state.exchanges.values(),
                change.repo,
                change.pr,
                change.head,
            ) {
                let owner = e.peer_owner.as_deref().unwrap_or(&e.peer);
                match approval::asked_state(e, now) {
                    Asked::Waiting => {
                        return Teammate::Waiting(format!(
                            "Waiting for {owner}'s Oby to review it and approve it on GitHub."
                        ));
                    }
                    Asked::ChangesRequested => {
                        let checks_failed = e
                            .result
                            .as_ref()
                            .and_then(|r| r["acceptance"].as_array())
                            .is_some_and(|a| {
                                a.iter()
                                    .any(|c| c["kind"] == "tests_pass" && c["met"] == false)
                            });
                        return Teammate::NeedsYou(if checks_failed {
                            format!(
                                "{owner}'s Oby didn't approve it: its checks didn't pass on it, or it has none for {}.",
                                change.repo
                            )
                        } else {
                            format!("{owner}'s Oby reviewed it and asked for changes.")
                        });
                    }
                    Asked::Spent => asked.push(e.peer.clone()),
                }
            }
            let members: Vec<(String, String)> = state
                .directory
                .members
                .iter()
                .filter(|(obi, m)| {
                    m.active && **obi != me && m.accepts.iter().any(|a| a == "review_request")
                })
                .map(|(obi, m)| (obi.clone(), m.owner.clone()))
                .collect();
            let all: Vec<&Exchange> = state.exchanges.values().collect();
            let next = approval::teammates(&members, change.repo, &all, &asked)
                .into_iter()
                .next();
            match next {
                Some(obi) => {
                    let owner = members
                        .iter()
                        .find(|(o, _)| *o == obi)
                        .map(|(_, owner)| owner.clone())
                        .unwrap_or_default();
                    (obi, owner)
                }
                None if asked.is_empty() => {
                    return Teammate::NeedsYou(format!(
                        "No teammate's Oby in the Obysphere takes reviews. {}",
                        approval::instead(change.repo, false)
                    ));
                }
                None => {
                    return Teammate::NeedsYou(format!(
                        "No teammate's Oby could approve it on GitHub. {}",
                        approval::instead(change.repo, false)
                    ));
                }
            }
        };
        let (to, owner) = next;
        let ask = json!({"to": to, "type": "review_request", "expiresIn": approval::REQUEST_LIFETIME,
                         "body": approval::request_body(change)});
        match self.request(ask.to_string().as_bytes()).await {
            Ok(_) => Teammate::Waiting(format!(
                "Asked {owner}'s Oby to review it and approve it on GitHub."
            )),
            // The Sphere can't be reached now: asked again next time.
            Err(e) if e.status.is_server_error() => Teammate::Waiting(format!(
                "Oby couldn't reach the Obysphere to ask {owner}'s Oby: {}",
                e.message
            )),
            Err(e) => Teammate::NeedsYou(format!(
                "Oby couldn't ask {owner}'s Oby to approve it: {}",
                e.message
            )),
        }
    }

    /// `GET /v1/obid/sphere/exchanges?limit=50`, the latest first.
    pub fn exchanges_json(&self, limit: usize) -> Value {
        let state = self.lock();
        let mut all: Vec<&Exchange> = state.exchanges.values().collect();
        all.sort_by(|a, b| {
            b.created
                .cmp(&a.created)
                .then_with(|| b.correlation_id.cmp(&a.correlation_id))
        });
        Value::Array(all.into_iter().take(limit).map(Exchange::to_json).collect())
    }

    /// `GET /v1/obid/sphere/exchanges/{correlationId}`.
    pub fn exchange_json(&self, cid: &str) -> Option<Value> {
        self.lock().exchanges.get(cid).map(Exchange::to_json)
    }
}
