//! Another Oby's review on this Mac (docs/contracts/teammate-review.md, TEAMREVIEW-3 and 4): what
//! this Oby has to start one, the job it starts, and the result of one with no check to run. It
//! owns the review job's start and that result; the reply and the publish gate are the exchange's
//! (`sphere::Link::review_ended`), and it never writes the exchange.

use super::*;

use crate::sphere::review_start::Can;

impl Jobs {
    /// What this Oby has for a review of `repo` (TEAMREVIEW-3): a repository the owner's jobs would
    /// take (listed, or one of GitHub's gh can push to), a Claude Code account routing would pick
    /// for background work, and the job sandbox. Reads only obid's memory, so it may run under the
    /// Link's lock.
    pub fn can_review(&self, repo: &str) -> Can {
        Can {
            pushable: self.repo(repo).is_some(),
            account: self
                .pick_for(Some(Tool::Claude.id()), None, &Work::new(None, None, true))
                .is_ok(),
            sandbox: self.sandbox.is_ok(),
        }
    }

    /// Starts the review another Obi asked for (runner-api.md, "obid only: Obi-to-Obi review jobs",
    /// step 1): a `review` job on an account routing picks, titled from typed fields only.
    pub fn start_obi_review(self: &Arc<Self>, request: ObiReview) -> Result<Job, NotMade> {
        let body: ReviewRequest = serde_json::from_value(request.body.clone())
            .map_err(|e| Refused(format!("The request isn't a review_request: {e}")))?;
        // A repository the owner's own jobs take: listed, or one of GitHub's gh can push to.
        let repo = self
            .repo(&body.repo)
            .ok_or_else(|| Refused(format!("{} isn't a repository obid knows.", body.repo)))?;
        // Claude Code's: the review's answer is its structured output (`--json-schema`).
        let (_, account) =
            self.pick_for(Some(Tool::Claude.id()), None, &Work::new(None, None, true))?;
        let id = self.new_id();
        let repo_name = body
            .repo
            .rsplit('/')
            .next()
            .unwrap_or(&body.repo)
            .to_owned();
        let task = format!(
            "Review {} for {}: {} at {} into {}.",
            body.repo,
            request.from_owner,
            body.head_branch,
            &body.head_sha[..7.min(body.head_sha.len())],
            body.base_branch
        );
        let mut job = Job::new(
            id,
            body.repo.clone(),
            "review".into(),
            task,
            "claude".into(),
            account.id,
        );
        job.title = Some(format!("Review for {}: {repo_name}", request.from_owner));
        job.branch = body.head_branch.clone();
        job.base = Some(body.base_branch.clone());
        job.pr = body.pr;
        job.check = repo.check.clone();
        job.mode = Some("bypass".into());
        job.round = Some(1);
        job.obi = Some(ObiRef {
            correlation_id: request.correlation_id.clone(),
            from: request.from.clone(),
            from_owner: request.from_owner.clone(),
            request_id: request.request_id.clone(),
        });
        job.obi_request = Some(json!({
            "from": request.from, "from_owner": request.from_owner, "type": "review_request", "body": request.body
        }));
        job.phase = Some(Phase::Prepare);
        job.step = format!("Getting {repo_name} ready");
        let job = self.insert(job).map_err(NotMade::Unkept)?;
        // The first review in one of GitHub's repositories makes it one of obid's (crate::repos).
        if let Err(why) = self.config.repos.keep(&repo) {
            tracing::warn!("{why}");
        }
        self.schedule();
        Ok(job)
    }

    /// A review with no check to run (TEAMREVIEW-4): it can't vouch that the tests pass, so no
    /// agent runs and the result is `failed`, saying so, through the exchange's publish gate.
    pub(super) async fn review_without_a_check(
        self: &Arc<Self>,
        id: &str,
        request: &ReviewRequest,
    ) {
        let body = review::no_check_result(request);
        self.update(id, |job| {
            job.review_result = Some(body.clone());
            job.approvable = false;
            job.step = "Scanning the result for secrets and sending it".into();
        });
        self.review_ended(id, true).await;
        self.update(id, |job| {
            job.status = Status::Done;
            job.step = String::new();
            job.commits = Some(0);
            job.summary = format!(
                "Review for {}: failed, 0 finding(s), no check to run.",
                job.obi
                    .as_ref()
                    .map(|o| o.from_owner.as_str())
                    .unwrap_or("another Oby")
            );
            job.review_result = Some(body);
            job.finished = Some(obi_protocol::time::now());
        });
    }
}
