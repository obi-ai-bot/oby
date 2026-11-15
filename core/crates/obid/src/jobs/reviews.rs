//! Oby's review of what a job published (docs/runner-api.md, "Oby's review"): who reviews, its
//! start, its rounds and its verdict, and MERGING-8's item when it still asks for changes. Its
//! reviewer reads the commit obid pushed, in Plan mode: it never writes the reviewed branch.

use super::*;

impl Jobs {
    /// The owner's own review, once it has ended done: the card that posts it, when it raises one
    /// (crate::review::ready_card). Another Oby's review is not this.
    pub(super) async fn offer_owner_review(&self, id: &str) {
        let Some(job) = self.get(id) else { return };
        // Oby's review of its own published work is never posted ([`Jobs::reviewed`]).
        if job.obi.is_some()
            || job.review_of.is_some()
            || job.kind != "review"
            || job.status != Status::Done
        {
            return;
        }
        let gh = crate::github::gh();
        let home = &self.config.home;
        if let Some(link) = self.link.get().and_then(std::sync::Weak::upgrade)
            && let Some(card) = crate::review::ready_card(home, gh.as_deref(), &job).await
        {
            link.add_items(vec![card]);
        }
    }

    /// Who reviews `author`'s published work (docs/runner-api.md, "Oby's review"): an account of
    /// another tool first, then another of its own, else the author's own in a session of the
    /// review's own; always one signed in, not being changed and with room, and never Cursor, nor
    /// one whose review of this head couldn't give a verdict. `Err` says why none fits: the review
    /// waits for one.
    pub(super) fn reviewer(&self, author: &Job) -> Result<(Tool, crate::config::Account), String> {
        let failed: Vec<&str> = author
            .review_failed_here()
            .into_iter()
            .flat_map(|f| f.reviews.iter().map(|(_, on)| on.as_str()))
            .collect();
        let home = login_home();
        let now = obi_protocol::time::now();
        let own = Tool::from_id(&author.agent);
        // A second review is another tool's than those that approved the head (MERGING-14).
        let approved: Vec<&str> = if author.second_review_due() {
            author.approving_tools()
        } else {
            Vec::new()
        };
        let tools = Tool::ALL
            .into_iter()
            .filter(|t| Some(*t) != own)
            .chain(own)
            .filter(|t| *t != Tool::Cursor && !approved.contains(&t.id()));
        let mut theirs = None;
        let mut held = Vec::new();
        // The tools with no room now, each with when its first account has, if known (MERGING-34).
        let mut full: Vec<(&str, Option<i64>)> = Vec::new();
        let mut not_yet = None;
        for tool in tools {
            for account in self.accounts(tool) {
                if self.being_changed(&account.id)
                    || !signed_in(tool, &account, home.as_deref())
                    || failed.contains(&reviewed_on(tool.id(), &account.id).as_str())
                {
                    continue;
                }
                if !self.takes_background(&account) {
                    not_yet.get_or_insert(account.name);
                    continue;
                }
                let usage = self.usage.get(&account.id);
                if let Some(why) = usage.held(now) {
                    held.push(format!("{} {why}", account.name));
                    let at = usage.free_at(now, true);
                    match full.iter_mut().find(|(t, _)| *t == tool.name()) {
                        Some((_, free)) => *free = [*free, at].into_iter().flatten().min(),
                        None => full.push((tool.name(), at)),
                    }
                } else if Some(tool) == own && account.id == author.account {
                    theirs = Some((tool, account));
                } else {
                    return Ok((tool, account));
                }
            }
        }
        theirs.ok_or_else(|| {
            if let Some(by) = approved.first().and_then(|a| Tool::from_id(a)) {
                let consent = not_yet.as_deref().map(consent_words);
                crate::merge::second_review_waits(by.name(), &full, consent)
            } else if let Some(name) = not_yet.filter(|_| held.is_empty()) {
                format!("My review waits for {}.", consent_words(&name))
            } else if held.is_empty() {
                "Oby's review waits for a Claude Code or Codex account signed in to review it."
                    .to_owned()
            } else {
                format!(
                    "Oby's review waits for an account with room: {}.",
                    held.join("; ")
                )
            }
        })
    }

    /// Starts Oby's review of what job `id` published by itself, when one is due
    /// ([`Job::review_due`]): a `review` job of its own on the [`Jobs::reviewer`]'s account, at
    /// the very commit obid pushed, in Plan mode. It never publishes; the scheduler starts it once
    /// a slot is free. With no account to run it on, the job says why (`noReviewer`), and
    /// [`Jobs::settle_reviews`] starts it once one fits.
    pub(super) fn start_review(&self, id: &str) {
        let Some((author, head)) = self.get(id).and_then(|job| {
            let head = job.review_due()?.to_owned();
            Some((job, head))
        }) else {
            return;
        };
        let (tool, account) = match self.reviewer(&author) {
            Ok(found) => found,
            Err(why) => {
                if author.no_reviewer.as_ref() != Some(&why) {
                    self.update(id, |job| job.no_reviewer = Some(why));
                }
                return;
            }
        };
        let review_id = self.new_id();
        let mut review = Job::new(
            review_id.clone(),
            author.repo.clone(),
            "review".into(),
            author.task.clone(),
            tool.id().into(),
            account.id,
        );
        review.request = author.request.clone();
        review.title = Some(crate::brain::briefing::line(
            &format!("Oby review: {}", author.short_title()),
            60,
        ));
        review.branch = format!("oby/review-{review_id}");
        review.base = author.base.clone();
        review.start_commit = Some(head);
        review.mode = Some(Mode::Plan.as_str().to_owned());
        review.round = Some(1);
        review.review_of = Some(id.to_owned());
        // It reads the commit obid pushed, which is in the reviewed job's own clone (crate::secretspec).
        review.apart = author.apart;
        review.clone_root = author.clone_root.clone();
        review.phase = Some(Phase::Prepare);
        review.step = format!("Getting {} ready", author.repo_name());
        // The review first: cut off between the two, it still runs, and its verdict still lands.
        let Ok(_) = self.insert(review) else { return };
        self.update(id, |job| {
            // Its summary of the round it worked on the last review is its reply (MERGING-33).
            if let Some((_, reply)) = job.replies.last_mut().filter(|(_, r)| r.is_empty()) {
                *reply = crate::secrets::known().redact(&job.summary);
            }
            job.reviewing = Some(review_id.clone());
            job.review_error = None;
            job.no_reviewer = None;
        });
        self.review_items(id, None);
    }

    /// The owner's Try again on Oby's review that no reviewer could give (its `failed` inbox item,
    /// [`crate::sphere::inbox::review_item`]): the head's review starts again, on the first
    /// reviewer that may take it, with one more to fall back on. `None` for a job obid doesn't
    /// have; `Err` when its review isn't waiting for the owner.
    pub fn review_again(self: &Arc<Self>, id: &str) -> Option<Result<Job, Refused>> {
        let job = self.get(id)?;
        if !job.review_needs_you() || job.current_review().is_some_and(|r| !r.approved()) {
            return Some(Err(Refused(
                "Oby's review of it isn't waiting for you.".into(),
            )));
        }
        self.update(id, |job| {
            job.review_failed = None;
            job.review_error = None;
        });
        self.start_review(id);
        self.schedule();
        self.get(id).map(Ok)
    }

    /// The owner's One more round on the job `id` whose Oby reviews ran out still asking for
    /// changes (`inbox::reviews_ran_out_item`): that review is quoted for its agent as a round past
    /// the limit, as one with reviews left is ([`Jobs::reviewed`]), and Oby reviews what it pushes.
    pub fn review_round_again(self: &Arc<Self>, id: &str) -> Option<Result<Job, Refused>> {
        let job = self.get(id)?;
        let refused = |why: &str| Some(Err(Refused(why.into())));
        let Some(review) = job.current_review().filter(|r| !r.approved()).cloned() else {
            return refused("Oby's review of it isn't waiting for you.");
        };
        let carried = chain::branch_carrier(&self.lock().jobs, &job, Holds::AtWork).is_some();
        if job.status != Status::Done || job.reviewing.is_some() || carried || past_safety(&job) {
            return refused("It can't take another round now.");
        }
        let Some((_, text)) = self
            .get(&review.job)
            .and_then(|r| review::verdict(&r.summary))
        else {
            return refused("Oby's review of it is gone: ask Oby to review it again.");
        };
        let by = format!(
            "{} · {}",
            Tool::from_id(&review.agent).map_or("Its agent", Tool::name),
            review.account
        );
        if let Err(why) = self.quote_review(id, review.round, &by, &text) {
            return Some(Err(Refused(format!(
                "Oby couldn't hand its review to the agent: {why}"
            ))));
        }
        self.update(id, |job| {
            job.heard.push(review::round_prompt(review.round));
            job.replies.push((review.round, String::new()));
            if let Ok(change) = transition(job, &crate::job::Command::NextRound) {
                change.apply(job);
            }
            job.step = "Working on Oby's review".into();
        });
        self.review_items(id, None);
        self.schedule();
        self.get(id).map(Ok)
    }

    /// The job `id`'s inbox item about Oby's review that couldn't run: `item` when there is one,
    /// and its other open ones done, with no answer.
    fn review_items(&self, id: &str, item: Option<crate::sphere::inbox::Item>) {
        if let Some(link) = self.link.get().and_then(std::sync::Weak::upgrade) {
            link.review_items(id, item);
        }
    }

    /// Oby's reviews left hanging, settled on each pass of [`Jobs::follow_pull_requests`] (so
    /// once obid starts too): a job whose `reviewing` names a review that's gone or ended without
    /// its verdict landing (a restart, a crash) takes it now, or a new review when it's gone; one
    /// waiting for an account (`noReviewer`) starts its review once one fits. Whether it settled
    /// any: a review it started waits for the scheduler.
    pub(super) fn settle_reviews(&self) -> bool {
        let (hanging, waiting): (Vec<(String, String)>, Vec<String>) = {
            let state = self.lock();
            let hanging = state
                .jobs
                .values()
                .filter_map(|job| {
                    let review = job.reviewing.as_ref()?;
                    let alive = state.workers.contains_key(review)
                        || state
                            .jobs
                            .get(review)
                            .is_some_and(|r| r.status.is_active() || r.resumes_at.is_some());
                    (!alive).then(|| (job.id.clone(), review.clone()))
                })
                .collect();
            // A second review that never started (a restart between the first's verdict and its
            // start, say) starts now too: none that gave no verdict, or that the owner ended.
            let waiting = state
                .jobs
                .values()
                .filter(|job| job.no_reviewer.is_some() || job.second_review_waits())
                .map(|job| job.id.clone())
                .collect();
            (hanging, waiting)
        };
        let settled = !hanging.is_empty() || !waiting.is_empty();
        // Changes left to the owner with no item for them (a round that left the head as it was,
        // one obid kept from before): theirs now. An item answered isn't asked again.
        let left: Vec<String> = self
            .lock()
            .jobs
            .values()
            .filter(|j| j.review_needs_you() && j.current_review().is_some_and(|r| !r.approved()))
            .map(|j| j.id.clone())
            .collect();
        for id in left {
            if let Some(item) = self.review_left_to_owner(&id) {
                self.review_items(&id, Some(item));
            }
        }
        for (author, review) in hanging {
            if self.get(&review).is_some() {
                self.reviewed(&review);
            } else {
                self.update(&author, |job| job.reviewing = None);
                self.start_review(&author);
            }
        }
        for id in waiting {
            self.start_review(&id);
        }
        settled
    }

    /// Oby's review job `id` ended (docs/runner-api.md, "Oby's review"): its verdict goes to the
    /// job it read, once. Approved, that job is reviewed. Changes requested, it takes them as its
    /// next round, the review quoted in its `.obi/inputs/review-<round>.md`, while it has reviews
    /// left ([`review::REVIEWS`]); otherwise they wait for the owner (`reviewNeedsYou`). A review
    /// that ended any other way says why in `reviewError`; one waiting for its usage reset comes
    /// back by itself; one of a head the job has pushed past counts for nothing, and the new head
    /// gets its own. Nothing goes to GitHub.
    pub(super) fn reviewed(&self, id: &str) {
        let Some(review) = self.get(id) else { return };
        let Some(of) = review.review_of.clone() else {
            return;
        };
        let Some(author) = self.get(&of) else { return };
        // A review its account's limit stopped waits for the owner's one decision (#1563), still
        // the author's review: never let go and retried on another account by itself.
        let waits_on_limit = review.account_limit_blocked
            && review.dismissed.is_none()
            && review.status == Status::Interrupted;
        if author.reviewing.as_deref().is_some_and(|r| r != id)
            || author.review.as_ref().is_some_and(|r| r.job == id)
            || review.resumes_at.is_some()
            || waits_on_limit
        {
            return;
        }
        // It read a head the job has pushed past, however it ended: the newer one gets a review
        // of its own, now unless the job's at work again (its publish starts one).
        let head = author.pushed_head().map(str::to_owned);
        if review.read_another_head(&author) {
            self.update(&of, |job| job.reviewing = None);
            self.start_review(&of);
            return;
        }
        // The owner took back their yes for its login (CHATGPT-23): that's no end for the review,
        // which goes on another account, or says what it waits for.
        if review.status == Status::Stopped
            && review.dismissed.is_none()
            && self.off_its_login(&review)
        {
            self.update(&of, |job| job.reviewing = None);
            self.start_review(&of);
            return;
        }
        let verdict = review::verdict(&review.summary);
        let why = match (review.status, &verdict) {
            (Status::Done, _) if review.commits.unwrap_or(0) > 0 => {
                Some("it made commits, so its verdict doesn't count".to_owned())
            }
            (Status::Done, _) if review.reason.as_deref() == Some(REVIEW_MOVED) => Some(
                "what it reviewed changed while it ran, so its verdict doesn't count".to_owned(),
            ),
            (Status::Done, None) => Some("it gave no verdict".to_owned()),
            (Status::Done, Some(_)) => None,
            (Status::Stopped, _) => Some("it was stopped".to_owned()),
            (status, _) => Some(
                review
                    .reason
                    .clone()
                    .unwrap_or_else(|| status.as_str().to_owned()),
            ),
        };
        // No verdict: the owner's (RECOVERY-15), in one item while its pull request is open, after
        // the next reviewer that may take it once, for a login refused or a crash; none they ended.
        let recorded = author
            .review_failed_here()
            .is_some_and(|f| f.reviews.iter().any(|(job, _)| job == id));
        if why.is_some() && (!review.ended_by_owner() || recorded) {
            let mut failed =
                author
                    .review_failed_here()
                    .cloned()
                    .unwrap_or_else(|| crate::job::ReviewFailed {
                        commit: head.clone().unwrap_or_default(),
                        reviews: Vec::new(),
                        needs_you: false,
                    });
            if !recorded {
                failed
                    .reviews
                    .push((id.to_owned(), reviewed_on(&review.agent, &review.account)));
            }
            let mut tried = author.clone();
            tried.review_failed = Some(failed.clone());
            let retry = matches!(review.status, Status::Failed | Status::Interrupted)
                && !failed.needs_you
                && failed.reviews.len() < 2
                && self.reviewer(&tried).is_ok();
            failed.needs_you = !retry;
            let why = why.unwrap_or_default();
            let number = author
                .published
                .as_deref()
                .and_then(|url| crate::github::pull_request_number(url, &author.repo))
                .map_or("its pull request".to_owned(), |n| format!("#{n}"));
            let error = (!retry).then(|| format!("{}{number}: {why}", crate::job::COULDNT_REVIEW));
            let author = self.update(&of, |job| {
                job.reviewing = None;
                job.review_failed = Some(failed);
                job.review_error = error.clone();
            });
            // Its end is its author's to tell: the review job itself needs no one.
            self.update(id, |review| {
                review.dismissed.get_or_insert_with(obi_protocol::time::now);
            });
            if retry {
                tracing::info!(
                    "job {of}: its review {id} ended before a verdict ({why}): another reviewer takes it"
                );
                self.start_review(&of);
            } else if let (Some(author), Some(error)) = (author.filter(pull_request_open), error) {
                self.review_items(
                    &of,
                    Some(crate::sphere::inbox::review_item(&author, id, &error)),
                );
            }
            return;
        }
        let Some((approved, text)) = verdict.filter(|_| why.is_none()) else {
            self.update(&of, |job| {
                job.reviewing = None;
                job.review_error = why.map(|why| format!("Oby's review didn't finish: {why}"));
            });
            return;
        };
        let round = author.review.as_ref().map_or(0, |r| r.round) + 1;
        let tool = Tool::from_id(&review.agent);
        let account = tool
            .and_then(|tool| {
                self.accounts(tool)
                    .into_iter()
                    .find(|a| a.id == review.account)
            })
            .map_or(review.account.clone(), |a| a.name);
        let by = format!("{} · {account}", tool.map_or("Its agent", Tool::name));
        let verdict = crate::job::Review {
            verdict: if approved {
                "approved"
            } else {
                "changesRequested"
            }
            .into(),
            job: id.to_owned(),
            agent: review.agent.clone(),
            account,
            same_account: review.agent == author.agent && review.account == author.account,
            round,
            date: obi_protocol::time::now(),
            commit: head.unwrap_or_default(),
            two_reviews: crate::merge::two_reviews(&author, review.security_paths.as_deref()),
        };
        // Its next round only with reviews left and nothing else moving its branch (Try again's).
        let (carried, busy) = {
            let state = self.lock();
            let carried = chain::branch_carrier(&state.jobs, &author, Holds::Any).is_some();
            (carried, branch_busy(&state.jobs, &author))
        };
        let quoted = (!approved
            && round < review::REVIEWS
            && after_pr::reviews_left(&author)
            && author.status == Status::Done
            && !carried
            && !past_safety(&author))
        .then(|| match &busy {
            // Its round would move a branch another job works on now (PUBLISHING-4).
            Some(why) => Err(why.clone()),
            None => self.quote_review(&of, round, &by, &text),
        });
        // Its round starts only while no job works on its branch, checked again as it starts: a job
        // started while the review was quoted holds it too (PUBLISHING-4).
        let busy = |jobs: &HashMap<String, Job>| jobs.get(&of).and_then(|j| branch_busy(jobs, j));
        self.update_after(&of, busy, |job, busy| {
            let quoted = match (quoted, busy) {
                (Some(Ok(())), Some(why)) => Some(Err(why)),
                (quoted, _) => quoted,
            };
            job.reviewing = None;
            job.review_failed = None;
            job.review_error = match &quoted {
                Some(Err(why)) => Some(format!(
                    "Oby's review asks for changes, and obid couldn't hand them to the agent: {why}"
                )),
                _ => None,
            };
            if approved {
                job.approvals.retain(|r| r.commit == verdict.commit);
                job.approvals.push(verdict.clone());
            }
            job.review = Some(verdict);
            if matches!(quoted, Some(Ok(()))) && job.status == Status::Done {
                job.heard.push(review::round_prompt(round));
                job.replies.push((round, String::new()));
                if let Ok(change) = transition(job, &crate::job::Command::NextRound) {
                    change.apply(job);
                }
                job.step = "Working on Oby's review".into();
            }
        });
        // Its changes left to the owner, its reviews run out or no round started: the owner hears
        // it, with One more round (MERGING-8). A round that started settles the old item.
        let left = self.review_left_to_owner(&of);
        self.review_items(&of, left);
        // An approval of a head that needs two asks another tool for the second (MERGING-14).
        if approved {
            self.start_review(&of);
        }
    }

    /// MERGING-8's item for the job `id`, when Oby's review of its head still asks for changes and
    /// nothing more happens by itself (`Job::review_needs_you`): its reviews ran out, a follow-up
    /// carries its branch, another job holds it, or the round asked for left the head as it was.
    /// None while a follow-up still works on its branch: what it pushes is the head to merge. Why
    /// obid couldn't hand the review to the agent, when it couldn't, is in its text.
    pub fn review_left_to_owner(&self, id: &str) -> Option<crate::sphere::inbox::Item> {
        let job = self.get(id)?;
        let review = job.current_review().filter(|r| !r.approved())?;
        // A follow-up that pushed to its pull request since carries it: its own review is the
        // one (MERGING-24).
        if self.carrier(&job).id != job.id {
            return None;
        }
        let following = chain::branch_carrier(&self.lock().jobs, &job, Holds::AtWork).is_some();
        if !job.review_needs_you() || following {
            return None;
        }
        let mut item = crate::sphere::inbox::reviews_ran_out_item(&job, &review.job);
        if let Some(why) = &job.review_error {
            item.text = format!("{} {why}", item.text);
        }
        Some(item)
    }

    /// Quotes Oby's review `round` of job `id`'s pull request in its worktree's
    /// `.obi/inputs/review-<round>.md`, as a check's output reaches agents: the vault's and obid's
    /// secrets taken out, fenced, data never in a prompt.
    fn quote_review(&self, id: &str, round: u32, by: &str, text: &str) -> Result<(), String> {
        let text = crate::secrets::known().redact(&crate::vault::for_agent(text)?);
        let quoted = review::quoted(round, by, &text, &crate::brain::briefing::nonce());
        let inputs =
            crate::obi_dir::folder(&self.worktree_path(id), "inputs").map_err(|e| e.to_string())?;
        crate::obi_dir::write_new(&inputs, &review::round_file(round), quoted.as_bytes())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
