//! The owner's merge (crate::merge): the card that asks, their Merge or Merge and remember, Ask a
//! teammate, and what's still open at the head they confirm. It merges only on the owner's word.

use super::*;

impl Jobs {
    /// What waits on the owner about merging the job `id` now: `card`, unless they answered it
    /// already (Leave it open, Dismiss): then not again for this head and reason. A card the
    /// follower settled by itself (the state went and came back) is offered again. Its words are
    /// the Merged stage's (`mergeError`).
    pub(super) fn offer_merge_card(
        &self,
        id: &str,
        link: Option<&crate::sphere::Link>,
        card: Option<crate::sphere::inbox::Item>,
    ) {
        let card = self.one_more_card(id, card);
        let answered = card.as_ref().is_some_and(|c| {
            link.and_then(|l| l.item(&c.id))
                .is_some_and(|i| i.done.is_some() && i.answer.is_some())
        });
        // Merged for good, nothing waits on the owner about merging it: a card made from a read
        // that started before the merge (the follower's, while the owner's Merge went) doesn't
        // come back, here or in the inbox (MERGING-20). Decided under the lock that writes it.
        let mut merged = false;
        self.update(id, |job| {
            merged = crate::merge::merged_for_good(job);
            let open = card.is_some() && !answered && !merged;
            job.merge_needs_you = open;
            job.merge_error = card.as_ref().filter(|_| open).map(|c| c.text.clone());
        });
        let card = card.filter(|_| !merged);
        // MERGING-24: the jobs before it on its pull request ask nothing about merging from now
        // on, as their card settles with this one (TASKS-27), not on their own next read.
        let superseded: Vec<String> = {
            let state = self.lock();
            (state.jobs.values())
                .filter(|j| j.id != id && (j.merge_needs_you || j.merge_error.is_some()))
                .filter(|j| carrier_of(&state.jobs, j).id == id)
                .map(|j| j.id.clone())
                .collect()
        };
        for earlier in superseded {
            self.update(&earlier, |job| {
                job.merge_needs_you = false;
                job.merge_error = None;
            });
        }
        if let Some(link) = link {
            link.merge_items(id, card);
        }
    }

    /// Right before obid sends `action` for the job `id` about `head` (crate::merge): whether the
    /// job is still one to carry on at that head, not merged already, and if so its mark
    /// (`Job::sending`), saved before it's sent.
    pub(super) fn about_to_send(
        &self,
        id: &str,
        head: &str,
        action: &str,
        by: Option<crate::job::MergedBy>,
        login: Option<String>,
    ) -> bool {
        let mut going = false;
        // The owner's own Merge goes at the head their card showed, wherever it came from: GitHub
        // refuses it if the head moved since (`sha`). Never another Obi's.
        let owners = by.as_ref().is_some_and(|b| b.by == "owner");
        self.update(id, |job| {
            // Never the branch of a job still at work.
            let owners = owners
                && job.published.is_some()
                && job.obi.is_none()
                && !(action == "branchDelete" && job.status.is_active());
            if (carries(job) && job.pushed_head() == Some(head)) || owners {
                going = crate::merge::mark(
                    job,
                    crate::job::Sending {
                        action: action.into(),
                        head: head.into(),
                        by,
                        login,
                    },
                );
            }
        });
        going
    }

    /// Each repository obid works in, with its default branch: what a workflow memory is checked
    /// against.
    fn repo_branches(&self) -> Vec<(String, String)> {
        self.config
            .repos
            .get()
            .iter()
            .map(|r| (r.repo.clone(), r.default_branch.clone()))
            .collect()
    }

    /// The owner's Ask a teammate, and remember, on the job `id`'s card about `head` (crate::merge,
    /// `teammate_card`; the owner's decision of 2026-10-25): their yes to "Can I remember that?" is
    /// about who reviews, never how it merges. Oby keeps a `review` memory for the repository
    /// (`memory::remember_review`), or, where the owner taught it a workflow since, changes only
    /// that workflow's review to a teammate's Oby; then a teammate's Oby is asked right away
    /// (`Link::ask_teammate`). What came of the ask, in Oby's words; why not, otherwise: nothing
    /// was asked, nor remembered. `signed`: the owner's app signed the answer (a workflow it
    /// changes is theirs, else it's kept as a Remember no app of theirs signed).
    pub async fn ask_teammate_and_remember(
        &self,
        id: &str,
        head: &str,
        signed: bool,
    ) -> Result<String, String> {
        let job = self.get(id).ok_or("There's no such job.")?;
        // A follow-up that pushed to its pull request since carries it (MERGING-23).
        let job = self.carrier(&job);
        let id = job.id.as_str();
        // Only a job Oby still carries, at the head Oby pushed (crate::merge::ready's own rule).
        if !carries(&job) || job.pushed_head() != Some(head) {
            return Err("It isn't one Oby carries on at that commit any more.".into());
        }
        let gh = crate::github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
        let number = job
            .published
            .as_deref()
            .and_then(|url| job.pull_request_number(url))
            .ok_or("The job has no pull request of its own.")?;
        let link = self
            .link
            .get()
            .and_then(std::sync::Weak::upgrade)
            .ok_or("Oby isn't in an Obysphere.")?;
        let home = self.config.home.clone();
        let (pr, _) =
            crate::github::view_merging(&gh, &job.repo, number, obi_protocol::time::now()).await?;
        if !pr.is_open() || pr.head.as_deref() != Some(head) {
            return Err("It moved on since: a new card comes if it still needs a teammate.".into());
        }
        let name = job.repo.rsplit('/').next().unwrap_or(&job.repo).to_owned();
        let from = format!("You asked a teammate's Oby to review {name}#{number}");
        let remembered = match crate::memory::workflow(&home, &job.repo).and_then(|m| m.workflow) {
            Some(mut learned) => {
                learned.review = crate::memory::Approval::Teammate;
                crate::memory::remember_workflow(
                    &home,
                    &job.repo,
                    learned,
                    &from,
                    &self.repo_branches(),
                    signed,
                )
                .await
            }
            None => {
                crate::memory::remember_review(&home, &job.repo, &from, &self.repo_branches()).await
            }
        };
        remembered.map_err(|why| {
            tracing::warn!("job {id}: the teammate review wasn't remembered: {why:?}");
            "Oby couldn't remember it: try again.".to_owned()
        })?;
        let goal = job.short_title();
        let change = crate::sphere::approval::Change {
            repo: &job.repo,
            pr: number,
            base: &pr.base,
            head_branch: &job.branch,
            head,
            goal: &goal,
        };
        Ok(match link.ask_teammate(&change).await {
            crate::sphere::approval::Teammate::Waiting(said)
            | crate::sphere::approval::Teammate::NeedsYou(said) => said,
        })
    }

    /// The owner's Merge, or Merge and remember, on the job `id`'s "Ready to merge?" card about
    /// `head` (crate::sphere::link): GitHub is read again, the pull request must still be at that
    /// head and hold by the workflow the card proposes (or the one the owner taught Oby since),
    /// then it merges, `mergedBy` the owner. `remember` then keeps that workflow, the owner's yes
    /// to the card's "Can I remember that?". Why not, otherwise: nothing was sent. `lift`: what
    /// their answer lifts (crate::merge::Lift): an answer their app signed lifts what its card
    /// named and remembers as theirs; any other, only the holds of before (MERGING-20). While
    /// GitHub is still working out whether it can merge, their Merge is kept and tried again by
    /// itself ([`Jobs::keep_merge`], MERGING-35): `Ok` with [`crate::merge::KEPT`].
    pub async fn merge_by_owner(
        self: &Arc<Self>,
        id: &str,
        head: &str,
        remember: bool,
        lift: crate::merge::Lift<'_>,
    ) -> Result<Option<String>, String> {
        match self.merge_by_owner_now(id, head, remember, &lift).await {
            Err(blocked) if blocked.why == crate::merge::WORKING_OUT => {
                self.keep_merge(id, head, remember, crate::merge::KeptLift::of(&lift));
                Ok(Some(crate::merge::KEPT.into()))
            }
            merged => merged.map_err(|b| b.why),
        }
    }

    /// The owner's Merge kept while GitHub works out whether it can merge ([`crate::merge::keep`],
    /// MERGING-35): what comes back to them is a card. Kept in memory only: a restart drops it.
    fn keep_merge(
        self: &Arc<Self>,
        id: &str,
        head: &str,
        remember: bool,
        lift: crate::merge::KeptLift,
    ) {
        let (jobs, id, head) = (Arc::clone(self), id.to_owned(), head.to_owned());
        tokio::spawn(async move {
            let (on, id_, head_, lift_) = (&jobs, id.as_str(), head.as_str(), &lift);
            let why = crate::merge::keep(move || async move {
                let lift = lift_.lift();
                on.merge_by_owner_now(id_, head_, remember, &lift)
                    .await
                    .map(drop)
            })
            .await;
            tracing::info!("job {id}: the owner's kept Merge ended: {why:?}");
            if let (Some(why), Some(job)) = (why, jobs.get(&id)) {
                let link = jobs.link.get().and_then(std::sync::Weak::upgrade);
                let card = crate::merge::stopped_card(&job, &head, &why);
                jobs.offer_merge_card(&id, link.as_deref(), Some(card));
            }
        });
    }

    /// [`Jobs::merge_by_owner`], once, now.
    async fn merge_by_owner_now(
        &self,
        id: &str,
        head: &str,
        remember: bool,
        lift: &crate::merge::Lift<'_>,
    ) -> Result<Option<String>, crate::merge::Blocked> {
        let signed = !matches!(lift, crate::merge::Lift::Before);
        let OwnersMerge {
            job,
            gh,
            home,
            pr,
            merging,
            reviewers,
            learned,
            workflow,
            number,
        } = self
            .owners_merge(id, head)
            .await
            .map_err(crate::merge::unread)?;
        let id = job.id.as_str();
        // What their word goes over is said, never a reason to refuse (MERGING-27).
        let open = matches!(lift, crate::merge::Lift::Word)
            .then(|| crate::merge::still_open(&job, &workflow, &pr, &merging, &reviewers))
            .flatten();
        crate::merge::on_owners_word(
            true,
            crate::merge::ready_as(
                Some(lift),
                &job,
                Some(&workflow),
                &pr,
                &merging,
                Some(&reviewers),
            ),
        )?;
        let by = crate::job::MergedBy {
            by: "owner".into(),
            memory: None,
            at: obi_protocol::time::now(),
        };
        let go = |action: &str| self.about_to_send(id, head, action, Some(by.clone()), None);
        let outcome =
            crate::merge::merge(&gh, &home, &job, &workflow, &reviewers, Some(lift), &go).await;
        self.note_merge(id, &outcome, by);
        let merged = outcome?;
        for sent in &merged.outgoing {
            tracing::info!("job {id}: sent {} {} {}", sent.action, sent.repo, sent.id);
        }
        if remember && learned.is_none() {
            let name = job.repo.rsplit('/').next().unwrap_or(&job.repo).to_owned();
            let from = format!("You merged {name}#{number} this way");
            if let Err(why) = crate::memory::remember_workflow(
                &home,
                &job.repo,
                workflow,
                &from,
                &self.repo_branches(),
                signed,
            )
            .await
            {
                tracing::warn!("job {id}: merged, but its workflow wasn't remembered: {why:?}");
                return Ok(Some(
                    "Merged, but Oby couldn't remember how: it asks again next time.".into(),
                ));
            }
        }
        Ok(open)
    }

    /// What's still open on the job `id`'s pull request at `head`, for the Confirm their words
    /// make to show before their click goes over it (MERGING-27); none when it can't be read, and
    /// the merge reads it all again.
    pub async fn merge_still_open(&self, id: &str, head: &str) -> Option<String> {
        let read = self.owners_merge(id, head).await.ok()?;
        crate::merge::still_open(
            &read.job,
            &read.workflow,
            &read.pr,
            &read.merging,
            &read.reviewers,
        )
    }

    /// What the owner's own Merge of the job `id`'s pull request at `head` goes on, read fresh.
    async fn owners_merge(&self, id: &str, head: &str) -> Result<OwnersMerge, String> {
        let job = self.get(id).ok_or("There's no such job.")?;
        // A follow-up that pushed to its pull request since carries it, its push Oby's own: the
        // merge goes on that job's record (MERGING-23).
        let mut job = self.carrier(&job);
        job.unfollowed = self.unfollowed(&job);
        let gh = crate::github::gh().ok_or("GitHub's CLI (gh) isn't installed.")?;
        let number = job
            .published
            .as_deref()
            .and_then(|url| job.pull_request_number(url))
            .ok_or("The job has no pull request of its own.")?;
        let home = self.config.home.clone();
        let (pr, merging) =
            crate::github::view_merging(&gh, &job.repo, number, obi_protocol::time::now()).await?;
        let merging = merging?;
        if pr.head.as_deref() != Some(head) {
            return Err(crate::merge::HEAD_MOVED.into());
        }
        let owner = crate::github_login::read_status(&gh)
            .await
            .user
            .ok_or("obid's gh isn't signed in to GitHub.")?;
        let reviewer = crate::github_login::reviewer(&gh, &home, &owner).await;
        let learned = crate::memory::workflow(&home, &job.repo).and_then(|m| m.workflow);
        let reviewers = crate::merge::accounts(&home, &owner, &reviewer, learned.as_ref())
            .ok_or("Oby couldn't tell who its reviewer accounts are.")?;
        let workflow = learned
            .clone()
            .filter(|w| w.base == pr.base)
            .or_else(|| crate::merge::proposed(&job, &pr, &merging, &reviewers))
            .ok_or("It isn't one Oby may merge.")?;
        Ok(OwnersMerge {
            job,
            gh,
            home,
            pr,
            merging,
            reviewers,
            learned,
            workflow,
            number,
        })
    }
}

/// [`Jobs::owners_merge`]'s reading.
struct OwnersMerge {
    job: Job,
    gh: std::path::PathBuf,
    home: std::path::PathBuf,
    pr: crate::github::PullRequest,
    merging: crate::github::Merging,
    reviewers: crate::merge::Reviewers,
    learned: Option<crate::memory::Workflow>,
    workflow: crate::memory::Workflow,
    number: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MERGING-20: the owner's Merge lands while the follower is still carrying a read of the pull
    /// request from before it (open, its card "Merge … into main?"): when that read ends, its card
    /// doesn't come back on the merged job, so nothing waits on the owner about merging it.
    #[tokio::test]
    async fn merging_20_a_read_from_before_the_owners_merge_brings_no_card_back() {
        let root = std::env::temp_dir().join(format!("obid-merged-read-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let head = "a".repeat(40);
        let mut job = Job::new(
            "aaaa0017".into(),
            "o/r".into(),
            "build".into(),
            "Do it".into(),
            "claude".into(),
            "c".into(),
        );
        job.status = Status::Done;
        job.branch = "oby/build-aaaa0017".into();
        job.base = Some("main".into());
        job.published = Some("https://github.com/o/r/pull/7".into());
        job.pull_request = Some(crate::github::PullRequest {
            state: "OPEN".into(),
            review_decision: None,
            base: "main".into(),
            merged_at: None,
            checked_at: 0,
            head: Some(head.clone()),
        });
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        std::fs::write(
            home.join("jobs/aaaa0017.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let config = crate::config::Config::parse(
            &serde_json::json!({"listen": "127.0.0.1:0", "home": home, "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://example.com/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        let events = crate::events::Events::new();
        let jobs = Jobs::open(config.clone(), events.clone(), Err("no sandbox".into())).unwrap();
        let (_stop, stopping) = tokio::sync::watch::channel(false);
        let link = crate::sphere::Link::open(config, Arc::clone(&jobs), events, stopping);
        let id = "aaaa0017";
        // The follower's read, while the pull request was still open: its card, in the inbox.
        let card = crate::merge::stopped_card(
            &jobs.get(id).unwrap(),
            &head,
            "Its base is main, which Oby merges into only on your word.",
        );
        jobs.offer_merge_card(id, Some(&link), Some(card.clone()));
        assert!(link.item(&card.id).is_some_and(|i| i.done.is_none()));
        // Meanwhile the owner's Merge went through, and obid noted it.
        let by = crate::job::MergedBy {
            by: "owner".into(),
            memory: None,
            at: 1,
        };
        let merged = Ok(crate::merge::Merged {
            method: crate::memory::Method::Squash,
            outgoing: vec![],
        });
        jobs.note_merge(id, &merged, by);
        // Then the read that started before it ends.
        jobs.offer_merge_card(id, Some(&link), Some(card.clone()));
        let job = jobs.get(id).unwrap();
        assert_eq!(job.merged_by.as_ref().map(|b| b.by.as_str()), Some("owner"));
        assert!(
            !job.merge_needs_you,
            "the merged job waits on the owner again"
        );
        assert_eq!(job.merge_error, None);
        let item = link.item(&card.id).unwrap();
        assert!(
            item.done.is_some() && item.answer.is_none(),
            "its inbox item stays open"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
