//! A Fix CI task that pushed onto the owner's own pull request (TASKCONV-31) follows it as its own
//! (docs/contracts/autofix.md, AUTOFIX-14, 17 to 20): it opened no pull request, so the follower of
//! `published` ones never reads it. This reads it every follow interval while the task is done and
//! the pull request open at the head it pushed, and carries it on: a conflict with its base gets a
//! round merging the base in (never a rebase: the branch is the owner's), red checks get AUTOFIX's
//! rounds, green is said once. Past that, one line and a card, and the feed's `CI failed` card
//! comes back only for a head someone else pushed. It writes the job's `pullRequest` and its
//! `autofix` lines; never the branch (a round's publish pushes) and never a merge: the owner's pull
//! request is theirs to merge.

use super::*;

/// The commit the job last pushed to the branch it works on: the owner's pull request's branch it
/// pushed onto ([`Job::onto`], TASKCONV-31), else its own ([`Job::pushed_head`]).
pub(crate) fn head(job: &Job) -> Option<&str> {
    let Some(onto) = job.onto.as_deref() else {
        return job.pushed_head();
    };
    (job.outgoing.iter().rev())
        .find(|o| o.action == "push" && o.branch.as_deref() == Some(onto))
        .map(|o| o.id.as_str())
}

/// Whether `job` follows the owner's own pull request it pushed onto (AUTOFIX-14): a Fix CI task
/// (`onto` its branch, `pr` its number, none it opened), done, not another Oby's, not let go, that
/// pushed there, while the pull request is open as far as obid knows.
pub(crate) fn follows(job: &Job) -> bool {
    job.kind == "fix"
        && job.onto.is_some()
        && job.pr.is_some()
        && job.published.is_none()
        && job.status == Status::Done
        && job.obi.is_none()
        && job.dismissed.is_none()
        && head(job).is_some()
        && job.pull_request_open()
}

/// The job that follows `repo`'s pull request `number` at `head`, if one does: the feed offers it
/// no `CI failed` card (AUTOFIX-14), since that job takes its red checks itself.
pub(crate) fn follower<'a>(jobs: &'a [Job], repo: &str, number: u64, at: &str) -> Option<&'a Job> {
    jobs.iter().find(|j| {
        follows(j)
            && j.repo.eq_ignore_ascii_case(repo)
            && j.pr == Some(number)
            && head(j) == Some(at)
    })
}

/// Whether GitHub says the pull request conflicts with its base, so it runs none of its checks.
fn conflicts(merging: &crate::github::Merging) -> bool {
    merging.mergeable.as_deref() == Some("CONFLICTING")
        || merging.merge_state.as_deref() == Some("DIRTY")
}

impl Jobs {
    /// One pass over the jobs that follow the owner's pull request they pushed onto ([`follows`]),
    /// each read when due as a published one is ([`by_github::follow_every`]).
    pub(super) async fn follow_onto(self: &Arc<Self>) {
        let now = obi_protocol::time::now();
        let due: Vec<Job> = {
            let state = self.lock();
            (state.jobs.values())
                .filter(|job| follows(job))
                .filter(|job| {
                    let fresh = |at: i64| now - at < by_github::follow_every(job, now);
                    let url = url(job);
                    let tried = (state.followed.get(&job.id))
                        .is_some_and(|(u, at)| *u == url && fresh(*at));
                    let read = (job.pull_request.as_ref()).is_some_and(|pr| fresh(pr.checked_at));
                    !tried && !read
                })
                .cloned()
                .collect()
        };
        let Some(gh) = crate::github::gh() else {
            return;
        };
        for job in due {
            let Some(number) = job.pr else { continue };
            self.lock()
                .followed
                .insert(job.id.clone(), (url(&job), now));
            match crate::github::view_checks(&gh, &job.repo, number, now).await {
                Ok((pr, merging)) => {
                    let read = pr.clone();
                    self.update(&job.id, |job| {
                        job.pull_request = Some(read);
                        job.pull_request_error = None;
                    });
                    if let Some(merging) = merging {
                        self.carry_onto(&job.id, &gh, &pr, &merging).await;
                    }
                }
                Err(why) => {
                    self.update(&job.id, |job| job.pull_request_error = Some(why));
                }
            }
        }
    }

    /// What a read of the owner's pull request the job `id` pushed onto leads to: nothing once it
    /// is closed or someone else pushed (theirs, MERGING-25; the feed's card is back for it); a
    /// conflict, a merge round ([`Self::onto_conflict`]); red checks, AUTOFIX's round or its
    /// give-up and card; green, its line once (AUTOFIX-17). Pending checks wait.
    async fn carry_onto(
        self: &Arc<Self>,
        id: &str,
        gh: &Path,
        pr: &crate::github::PullRequest,
        merging: &crate::github::Merging,
    ) {
        let Some(job) = self.get(id) else { return };
        let link = self.link.get().and_then(std::sync::Weak::upgrade);
        let card = |card: Option<crate::sphere::inbox::Item>| {
            self.offer_merge_card(id, link.as_deref(), card);
        };
        let Some(at) = pr
            .head
            .as_deref()
            .filter(|at| pr.is_open() && head(&job) == Some(at))
        else {
            return card(None);
        };
        if job.status.is_active() {
            return;
        }
        if conflicts(merging) {
            return card(self.onto_conflict(&job, at, &pr.base).await);
        }
        match merging.checks {
            Some("FAILURE") => {
                // `autoFixChecks` off: the card at once (AUTOFIX-4).
                let waits = self.auto_fix_checks() && self.autofix(&job, gh, at, merging).await;
                let job = self.get(id).unwrap_or(job);
                card(
                    (!waits)
                        .then(|| crate::merge::stopped_card(&job, at, crate::merge::RED_CHECKS)),
                );
            }
            Some("SUCCESS") => {
                self.update(id, |job| {
                    if job.autofix.green_at(at) {
                        job.autofix.say("Its checks pass on GitHub now.");
                    }
                });
                card(None);
            }
            _ => {}
        }
    }

    /// The owner's pull request the job pushed onto conflicts with `base` at `head` (AUTOFIX-18,
    /// 19): `base` fetched into its clone and one round of its agent merging it in, said in its
    /// conversation; past [`crate::autofix::MERGES`], or when that can't start, one line, said
    /// once, and the card it answers.
    async fn onto_conflict(
        self: &Arc<Self>,
        job: &Job,
        head: &str,
        base: &str,
    ) -> Option<crate::sphere::inbox::Item> {
        let id = job.id.as_str();
        // GitHub's name for it, said only when it's a branch name obid would use.
        let base = Some(base)
            .filter(|b| git::is_branch_name(b))
            .unwrap_or("its base");
        if self.may_merge(id, head) && self.fetch_to_merge(job, base).await {
            self.update(id, |job| {
                job.autofix.say(crate::autofix::conflict_line(base))
            });
            if self.merge_round(id, head) {
                return None;
            }
        }
        let why = crate::autofix::still_conflicts(base);
        self.update(id, |job| {
            if job.autofix.still_conflicts(head) {
                let line = why.split_once(". ").map_or(why.as_str(), |(said, _)| said);
                job.autofix.say(format!("{line}. Over to you."));
            }
        });
        let job = self.get(id)?;
        Some(crate::merge::stopped_card(&job, head, &why))
    }

    /// `base` fetched into the job's clone as `origin/<base>`, where its agent merges it from:
    /// whether it was. A base that isn't the default branch and holds someone else's commits marks
    /// the job, as a rebase's fetch does.
    async fn fetch_to_merge(&self, job: &Job, base: &str) -> bool {
        let Some(repo) = self.repo(&job.repo).filter(|_| git::is_branch_name(base)) else {
            return false;
        };
        let clone = self.clone_for(job);
        let tracking = format!("refs/remotes/origin/{base}");
        let preparing = self.preparing(&repo.repo);
        let _fetching = preparing.lock().await;
        if let Err(e) = git::fetch(
            &clone,
            &repo.remote,
            &[format!("+refs/heads/{base}:{tracking}")],
        )
        .await
        {
            tracing::info!(
                "job {}: {base} didn't fetch, so it isn't merged in: {e}",
                job.id
            );
            return false;
        }
        let what = format!("the branch {base}, fetched to merge into a pull request");
        Box::pin(self.taint_unless_ours(&clone, &repo, &tracking, Some(base), &what)).await;
        true
    }
}

/// The pull request's address, as the follower's `followed` keeps it.
fn url(job: &Job) -> String {
    format!(
        "https://github.com/{}/pull/{}",
        job.repo,
        job.pr.unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onto_job() -> Job {
        let mut job = Job::new(
            "a".into(),
            "acme/app".into(),
            "fix".into(),
            "Fix CI".into(),
            "claude".into(),
            "default".into(),
        );
        job.status = Status::Done;
        job.onto = Some("fix-login".into());
        job.pr = Some(5);
        job.outgoing.push(crate::publish::Outgoing::new(
            "push",
            "acme/app",
            "abc",
            Some("fix-login"),
        ));
        job
    }

    /// AUTOFIX-14: a Fix CI task that pushed onto the owner's pull request follows it at the head
    /// it pushed there; not once someone else's job, a let go one, or one that opened its own.
    #[test]
    fn autofix_14_a_fix_ci_push_onto_the_owners_pull_request_is_followed() {
        let job = onto_job();
        assert_eq!(head(&job), Some("abc"));
        assert!(follows(&job));
        let jobs = [job.clone()];
        assert!(follower(&jobs, "acme/app", 5, "abc").is_some());
        assert!(
            follower(&jobs, "acme/app", 5, "def").is_none(),
            "someone else's head"
        );
        assert!(follower(&jobs, "acme/app", 6, "abc").is_none());
        let running = Job {
            status: Status::Running,
            ..job.clone()
        };
        assert!(!follows(&running));
        let opened = Job {
            published: Some("https://github.com/acme/app/pull/9".into()),
            ..job.clone()
        };
        assert!(!follows(&opened));
        let own = Job {
            onto: None,
            ..job.clone()
        };
        assert!(
            !follows(&own),
            "its own branch's pull request is the published follower's"
        );
        let mut closed = job.clone();
        closed.pull_request = Some(crate::github::PullRequest {
            state: "MERGED".into(),
            review_decision: None,
            base: "main".into(),
            merged_at: Some(1),
            checked_at: 1,
            head: Some("abc".into()),
        });
        assert!(!follows(&closed));
    }

    /// AUTOFIX-20 (#2486): its publish stage names the pull request it pushed onto, never "no pull
    /// request for this remote".
    #[test]
    fn autofix_20_the_publish_stage_names_the_pull_request_it_pushed_onto() {
        let mut job = onto_job();
        (job.commits, job.pushed) = (Some(1), Some(1));
        let detail = |job: &Job| {
            let pipeline = job.pipeline();
            (pipeline["stages"].as_array().unwrap().iter())
                .find(|s| s["id"] == "publish")
                .map(|s| s["detail"].clone())
                .unwrap()
        };
        assert_eq!(
            detail(&job),
            "Secret scan clean · pushed onto pull request #5"
        );
        job.onto = None;
        assert_eq!(
            detail(&job),
            "Secret scan clean · pushed · no pull request for this remote"
        );
    }

    #[test]
    fn autofix_18_conflicting_is_githubs_conflict_or_dirty() {
        let merging = |mergeable: Option<&str>, state: Option<&str>| crate::github::Merging {
            mergeable: mergeable.map(str::to_owned),
            merge_state: state.map(str::to_owned),
            ..crate::github::Merging::default()
        };
        assert!(conflicts(&merging(Some("CONFLICTING"), None)));
        assert!(conflicts(&merging(Some("UNKNOWN"), Some("DIRTY"))));
        assert!(!conflicts(&merging(Some("MERGEABLE"), Some("BLOCKED"))));
        assert!(
            !conflicts(&merging(Some("UNKNOWN"), None)),
            "GitHub still working it out"
        );
    }
}
