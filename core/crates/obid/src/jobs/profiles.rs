//! Which profile a job is made in, and runs on the accounts of (docs/contracts/profiles.md,
//! B00-40, PROFILES-34 and 36). It owns no state of its own: a job's `profile` is set once, as it
//! starts (`Job::start_in`), and accounts' profiles are crate::profile_scope's. It never writes a
//! job, a worktree or a branch.

use super::*;

impl Jobs {
    /// The profile a new job is made in: the one it names, else its conversation's, else the job
    /// it follows up's; none is the default's.
    pub(super) fn profile_for(&self, new: &NewJob) -> Option<String> {
        let home = &self.config.home;
        let followed = || {
            let id = new.follow_up_of.as_deref()?;
            self.lock().jobs.get(id).and_then(|job| job.profile.clone())
        };
        (new.profile.clone())
            .or_else(|| {
                (new.conversation.as_deref())
                    .and_then(|id| crate::conversation::kept_profile(home, id))
            })
            .or_else(followed)
    }

    /// The work a new job asks of an account: kept to the accounts of its profile.
    pub(super) fn work_for<'a>(&self, new: &NewJob, background: bool) -> Work<'a> {
        Work::new(new.model.as_deref(), None, background).within(self.profile_for(new))
    }

    /// The profile job `id` was made in (none: the default's), or none when obid has no such job.
    pub fn profile_of(&self, id: &str) -> Option<Option<String>> {
        self.lock().jobs.get(id).map(|job| job.profile.clone())
    }

    /// Whether `account` may take `work`: an account of its profile, or any for work no job's.
    pub(super) fn in_profile(&self, account: &crate::config::Account, work: &Work) -> bool {
        (work.profile.as_ref()).is_none_or(|tag| {
            crate::profile_scope::account_for(&self.config.home, tag.as_deref(), &account.id)
        })
    }
}
