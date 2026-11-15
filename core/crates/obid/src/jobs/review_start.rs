//! Oby's review of a published head starts once, however many callers find it due at once
//! (MERGING-55): whether it's still due, the review job and its author naming it are one step.

use super::*;

impl Jobs {
    /// Starts `review`, built for job `id`'s head `head`, when that job's review is still due at
    /// `head` ([`Job::review_due`]), under one hold of the lock: the review is kept, then its author
    /// names it (`reviewing`) and replies with its summary (MERGING-33). Whether it started: one
    /// that lost to another caller's, or wasn't kept, starts nothing and leaves no job behind.
    pub(super) fn claim_review(&self, id: &str, head: &str, mut review: Job) -> bool {
        let review_id = review.id.clone();
        let (review, author) = {
            let mut state = self.lock();
            let Some(mut author) = state
                .jobs
                .get(id)
                .filter(|job| job.review_due() == Some(head))
                .cloned()
            else {
                return false;
            };
            // The review first: cut off between the two, it still runs, and its verdict still lands.
            let review = match self.keep(&mut state, &review_id, review.revised()) {
                Ok(review) => review,
                Err(_) => {
                    state.jobs.remove(&review_id);
                    state.unkept.remove(&review_id);
                    return false;
                }
            };
            // Its summary of the round it worked on the last review is its reply (MERGING-33).
            if let Some((_, reply)) = author.replies.last_mut().filter(|(_, r)| r.is_empty()) {
                *reply = crate::secrets::known().redact(&author.summary);
            }
            author.reviewing = Some(review_id);
            author.review_error = None;
            author.no_reviewer = None;
            stamp(&state, &mut author);
            (review, self.keep(&mut state, id, author.revised()).ok())
        };
        self.events.send("job", review.to_json());
        if let Some(author) = author {
            crate::failures::observe(&self.config.home, &author);
            self.events.send("job", author.to_json());
        }
        self.changed.notify_waiters();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A done job whose pull request obid pushed `head` to, with no review under way.
    fn published(head: &str) -> Job {
        let mut job = Job::new(
            "aaaa0001".into(),
            "o/r".into(),
            "fix".into(),
            "Fix it".into(),
            "claude".into(),
            "c".into(),
        );
        job.status = Status::Done;
        job.branch = "oby/fix".into();
        job.base = Some("dev".into());
        job.published = Some("https://github.com/o/r/pull/7".into());
        let push = crate::publish::Outgoing::new("push", "o/r", head, Some("oby/fix"));
        job.outgoing.push(push);
        job
    }

    /// What a caller of [`Jobs::start_review`] builds from its copy of `author`: a review of the
    /// head it found due, before it claims it.
    fn review_for(author: &Job, id: &str) -> (String, Job) {
        let head = author.review_due().expect("a review is due").to_owned();
        let mut review = Job::new(
            id.into(),
            author.repo.clone(),
            "review".into(),
            author.task.clone(),
            "codex".into(),
            "default".into(),
        );
        review.start_commit = Some(head.clone());
        review.review_of = Some(author.id.clone());
        (head, review)
    }

    /// Two callers that both saw `author`'s review due claim it: the first starts it, the second
    /// starts nothing and leaves no job. Returns the review jobs of `author` obid has after.
    fn claim_twice(jobs: &Jobs, author: &str) -> Vec<Job> {
        let snapshot = jobs.get(author).unwrap();
        let (head, first) = review_for(&snapshot, "bbbb0001");
        let (again, second) = review_for(&snapshot, "bbbb0002");
        assert!(jobs.claim_review(author, &head, first));
        assert!(!jobs.claim_review(author, &again, second));
        assert_eq!(
            jobs.get(author).unwrap().reviewing.as_deref(),
            Some("bbbb0001")
        );
        assert!(jobs.get("bbbb0002").is_none());
        jobs.lock()
            .jobs
            .values()
            .filter(|job| job.review_of.as_deref() == Some(author))
            .cloned()
            .collect()
    }

    /// MERGING-55: two callers that found the same head's first review due at once start one
    /// review, which the job names; the other leaves no review job behind.
    #[tokio::test]
    async fn merging_55_two_starts_for_one_head_start_one_review() {
        let root = std::env::temp_dir().join(format!("obid-review-once-{}", uuid::Uuid::new_v4()));
        let job = published(&"a".repeat(40));
        let jobs = super::super::tests::jobs_with_kept(&root.join("home"), &[&job]);
        let reviews = claim_twice(&jobs, "aaaa0001");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].id, "bbbb0001");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// MERGING-55: a head whose first review approved it and that needs two (MERGING-14) starts
    /// its second review once, however many callers found it due.
    #[tokio::test]
    async fn merging_55_a_second_review_starts_once_too() {
        let root = std::env::temp_dir().join(format!("obid-second-once-{}", uuid::Uuid::new_v4()));
        let head = "a".repeat(40);
        let mut job = published(&head);
        job.review = Some(crate::job::Review {
            verdict: "approved".into(),
            job: "r1".into(),
            agent: "claude".into(),
            account: "default".into(),
            same_account: false,
            round: 1,
            date: 0,
            commit: head,
            two_reviews: Some("it changes a security path".into()),
        });
        assert!(job.second_review_due());
        let jobs = super::super::tests::jobs_with_kept(&root.join("home"), &[&job]);
        let reviews = claim_twice(&jobs, "aaaa0001");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].id, "bbbb0001");
        let _ = std::fs::remove_dir_all(&root);
    }
}
