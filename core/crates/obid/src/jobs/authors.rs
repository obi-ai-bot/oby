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
        Box::pin(self.authors_asking(crate::github::gh(), job, shown, when)).await;
    }

    /// [`Jobs::authors_as_it_starts`], asking `gh`, or none when obid has none.
    async fn authors_asking(
        &self,
        gh: Option<PathBuf>,
        job: &Job,
        shown: Option<&Value>,
        when: &str,
    ) {
        // Never asked again: a comment deleted in between would clear a job it reached. The
        // owner's review of a pull request anyone else wrote in reads someone else's code
        // (MERGING-36), a member's too.
        let found = match gh {
            None => Some((
                format!("obid had no gh to ask who wrote what it works from {when}"),
                false,
            )),
            Some(gh) => (self.others_wrote(&gh, job, shown).await)
                .map(|(why, members)| (format!("{when}, {why}"), members)),
        };
        let Some((why, members)) = found else { return };
        self.mark(&job.id, why, members && !job.read_only());
        // Oby's browser reads this start mark, never a page read's (BROWSER-18).
        if job.read_only() {
            self.update(&job.id, |job| job.someone_elses_code = true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BROWSER-18: the owner's review whose authors obid couldn't ask, with no gh, is someone
    /// else's code from its start, as one anyone else wrote in is: Oby's browser reads that mark.
    #[tokio::test]
    async fn browser_18_a_review_obid_had_no_gh_to_ask_about_is_someone_elses_code() {
        let root = std::env::temp_dir().join(format!("obid-authors-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        std::fs::create_dir_all(home.join("jobs")).unwrap();
        let mut job = Job::new(
            "aaaa1818".into(),
            "o/r".into(),
            "review".into(),
            "Review it".into(),
            "claude".into(),
            "c".into(),
        );
        job.pr = Some(90);
        std::fs::write(
            home.join("jobs/aaaa1818.json"),
            serde_json::to_vec(&job).unwrap(),
        )
        .unwrap();
        let config = Config::parse(
            &json!({"listen": "127.0.0.1:0", "home": home, "owner": "o",
                "repos": [{"repo": "o/r", "remote": "https://127.0.0.1:9/o/r.git"}]})
            .to_string(),
        )
        .unwrap();
        let jobs = Jobs::open(config, Events::new(), Err("no sandbox".into())).unwrap();
        jobs.authors_asking(None, &job, None, "as it started").await;
        let marked = jobs.get("aaaa1818").unwrap();
        assert!(marked.foreign.is_some(), "{marked:?}");
        assert!(marked.someone_elses_code, "{marked:?}");
        let _ = std::fs::remove_dir_all(root);
    }
}
