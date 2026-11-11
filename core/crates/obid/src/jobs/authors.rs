//! Who wrote the issue or pull request a job works from (crate::secretspec, rule 5): the owner
//! alone leaves it clean, members of the owner's organizations make it a reach (ISSUETASK-5,
//! MERGING-31), and anyone else makes it someone else's code (ISSUETASK-4, MERGING-14).

use super::*;

impl Jobs {
    /// Why the issue or pull request `job` works from isn't the owner's alone, and whether only by
    /// members (crate::secretspec::others_wrote; a stranger's wins), or none. `shown`: its quote.
    pub(super) async fn others_wrote(
        &self,
        gh: &Path,
        job: &Job,
        shown: Option<&Value>,
    ) -> Option<(String, bool)> {
        let mut found = None;
        for (kind, number) in [("issue", job.issue), ("pr", job.pr)] {
            if let Some(number) = number
                && found.as_ref().is_none_or(|(_, members)| *members)
                && let Some(why) = crate::secretspec::others_wrote(
                    gh,
                    &job.repo,
                    kind,
                    number,
                    self.described_clean(&job.repo, number),
                    shown.filter(|_| kind == "issue"),
                )
                .await
            {
                found = Some(why);
            }
        }
        found
    }

    /// Who wrote the issue or pull request `job` works from, as it starts (crate::secretspec):
    /// what anyone else wrote there reached it then, even if it's gone by its check. `shown` is
    /// what obid quoted of its issue for this run.
    pub(super) async fn authors_as_it_starts(&self, job: &Job, shown: Option<&Value>, when: &str) {
        let Some(gh) = crate::github::gh() else {
            let why = format!("obid had no gh to ask who wrote what it works from {when}");
            self.mark(&job.id, why, false);
            return;
        };
        // Never asked again: a comment deleted in between would clear a job it reached. The
        // owner's review of a pull request anyone else wrote in reads someone else's code
        // (MERGING-36), a member's too.
        if let Some((why, members)) = self.others_wrote(&gh, job, shown).await {
            self.mark(
                &job.id,
                format!("{when}, {why}"),
                members && !job.read_only(),
            );
        }
    }
}
