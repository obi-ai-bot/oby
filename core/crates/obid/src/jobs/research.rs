//! Research keeps its report and branch locally, without a repository check (PUBLISHING-28).

use crate::job::{Job, Status};
use serde_json::{Value, json};

impl Job {
    pub(crate) fn repository_check(&self) -> Option<&String> {
        self.check.as_ref().filter(|_| self.kind != "research")
    }

    pub(crate) fn has_report(&self) -> bool {
        self.kind == "research"
            && self.status == Status::Done
            && self.start_commit.is_some()
            && !self.summary.trim().is_empty()
    }

    /// The saved summary is the report; no second copy or agent-authored proposals are stored.
    pub(crate) fn saved_report(&self) -> Option<Value> {
        self.has_report().then(|| {
            json!({
                "job": self.id,
                "commit": self.start_commit,
                "markdown": self.summary,
                "proposals": [],
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Phase;

    #[test]
    fn publishing_28_research_check_stage_is_always_skipped() {
        let mut job = Job::new(
            "r".into(),
            "o/r".into(),
            "research".into(),
            "Read it".into(),
            "claude".into(),
            "a".into(),
        );
        job.check = Some("false".into());
        for phase in [Phase::Work, Phase::Check, Phase::Publish] {
            job.phase = Some(phase);
            for status in [Status::Running, Status::Done, Status::Interrupted] {
                job.status = status;
                assert!(job.repository_check().is_none());
                let pipeline = job.pipeline();
                let check = pipeline["stages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|stage| stage["id"] == "check")
                    .unwrap();
                assert_eq!(check["state"], "skipped");
                assert_eq!(check["detail"], "A research job changes no code");
            }
        }
        job.kind = "build".into();
        assert_eq!(job.repository_check().map(String::as_str), Some("false"));
    }

    #[test]
    fn publishing_28_only_finished_research_has_a_report() {
        let mut job = Job::new(
            "r".into(),
            "o/r".into(),
            "research".into(),
            "Read it".into(),
            "claude".into(),
            "a".into(),
        );
        job.summary = "# Findings\nSaved report".into();
        job.start_commit = Some("abc".into());
        assert!(job.saved_report().is_none());
        job.status = Status::Done;
        assert_eq!(job.saved_report().unwrap()["markdown"], job.summary);
        job.kind = "build".into();
        assert!(job.saved_report().is_none());
        job.kind = "research".into();
        job.summary = " \n".into();
        assert!(job.saved_report().is_none());
    }
}
