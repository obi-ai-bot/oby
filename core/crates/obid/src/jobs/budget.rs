//! How wide a build may be: what's spare of the machine's memory less what the other running jobs'
//! agents and checks may still take. Checks run one per repository at a time (PUBLISHING-25), so
//! each repository's may take its share beside the others'.

use super::*;

impl Jobs {
    /// [`process::build_env`] for a build of job `id` (its check's with `check`, else its agent's),
    /// as wide as the room memory leaves it: what's spare ([`spare`]) less what the other running
    /// agents here may still take ([`agents_held`]) and what one check of each repository may
    /// ([`check_held`]), each at least what its last build was told to build with. A check leaves
    /// out its own repository's, whose turn it holds; an agent's turn counts every other job's, any
    /// of which may start its check during the turn. Worked out at every turn and every check, so
    /// a build that starts next to others builds narrower. Nothing when memory can't be read: the
    /// build keeps its own defaults. Memory only narrows a build this way: it never holds a job or
    /// a check back (jobs start as soon as a slot is free, docs/runner-api.md).
    pub(super) fn build_env(&self, id: &str, check: bool) -> Vec<(&'static str, OsString)> {
        let Some(machine) = self.reading() else {
            return Vec::new();
        };
        // record()'s order: the jobs' lock, then the meter's.
        let state = self.lock();
        let mut measured = self.measured();
        let held = measured.held();
        let others: Vec<Load> = state
            .workers
            .keys()
            .filter(|w| *w != id)
            .filter_map(|w| state.jobs.get(w))
            .map(|job| {
                let load = Load::of(&state.jobs, job, &held);
                let (agent, check) = measured.granted.get(&job.id).copied().unwrap_or((0, 0));
                Load {
                    agent: load.agent.max(agent),
                    check: load.check.max(check),
                    ..load
                }
            })
            .collect();
        let own = state
            .jobs
            .get(id)
            .filter(|_| check)
            .map(|job| job.repo.as_str());
        let room = spare(&machine).saturating_sub(agents_held(&others) + check_held(&others, own));
        let jobs = process::build_jobs(room);
        let grant = jobs as u64 * process::BUILD_JOB_MEMORY;
        let grant = if check { (0, grant) } else { (grant, 0) };
        measured.granted.insert(id.to_owned(), grant);
        process::build_env(jobs)
    }
}

/// What a build's width weighs of a running job ([`Jobs::build_env`]): its repository, what its
/// agent and its check may hold at once, and what they hold now.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Load {
    repo: String,
    agent: u64,
    check: u64,
    agent_now: u64,
    check_now: u64,
}

impl Load {
    fn of(jobs: &HashMap<String, Job>, job: &Job, held: &HashMap<String, (u64, u64)>) -> Load {
        let (agent_now, check_now) = held.get(&job.id).copied().unwrap_or((0, 0));
        Load {
            repo: job.repo.clone(),
            agent: budget(jobs.values(), job),
            check: check_budget(jobs.values(), job),
            agent_now,
            check_now,
        }
    }
}

/// What the agents of `running` jobs here may still take: each one's budget less what it holds.
fn agents_held(running: &[Load]) -> u64 {
    running
        .iter()
        .map(|l| l.agent.saturating_sub(l.agent_now))
        .sum()
}

/// What the checks of `running` jobs may still take, one per repository at a time (PUBLISHING-25):
/// for each repository but `own` (the check building now holds its turn), the largest check budget
/// of its jobs less what its check running holds; summed, as two repositories' run side by side.
fn check_held(running: &[Load], own: Option<&str>) -> u64 {
    let mut repos: HashMap<&str, (u64, u64)> = HashMap::new();
    for load in running.iter().filter(|l| Some(l.repo.as_str()) != own) {
        let (most, now) = repos.entry(&load.repo).or_default();
        *most = (*most).max(load.check);
        *now += load.check_now;
    }
    repos
        .values()
        .map(|(most, now)| most.saturating_sub(*now))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn load(repo: &str, check: u64, check_now: u64) -> Load {
        Load {
            repo: repo.into(),
            check,
            check_now,
            ..Load::default()
        }
    }

    /// PUBLISHING-25 (docs/contracts/publishing.md): checks of two repositories run side by side,
    /// so a build makes room for one check of each repository, never only the largest; a check
    /// leaves out its own repository's, whose turn it holds, and what a check holds already counts.
    #[test]
    fn publishing_25_checks_of_two_repositories_share_the_machines_memory() {
        let running = [
            load("o/r", 4 * GB, 0),
            load("o/r", 2 * GB, 0),
            load("o/s", 3 * GB, 0),
        ];
        assert_eq!(check_held(&running, None), 7 * GB);
        assert_eq!(check_held(&running, Some("o/s")), 4 * GB);
        assert_eq!(check_held(&running, Some("o/r")), 3 * GB);
        let checking = [load("o/r", 4 * GB, GB), load("o/s", 3 * GB, 3 * GB)];
        assert_eq!(check_held(&checking, None), 3 * GB);
        assert_eq!(check_held(&[], None), 0);
    }
}
