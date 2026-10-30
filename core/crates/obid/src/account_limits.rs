//! Durable owner decisions for jobs stopped by an account's usage limit (#1563).
//!
//! A decision is persisted before it is returned or acted upon. A revision changes when a job
//! joins; an answer against an older revision cannot accidentally apply to newly affected work.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Pending is a question; the other states are the owner's persisted answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    Pending,
    Resume,
    Stop,
    Move,
}

impl Choice {
    fn answer(value: &str) -> Result<Self, String> {
        match value {
            "resume" => Ok(Self::Resume),
            "stop" => Ok(Self::Stop),
            "move" => Ok(Self::Move),
            _ => Err("Choose an offered account-limit action.".into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub id: String,
    pub revision: u64,
    pub agent: String,
    pub account: String,
    pub reset_at: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
    pub jobs: Vec<String>,
    #[serde(default)]
    pub nonces: std::collections::HashMap<String, String>,
    pub choice: Choice,
    pub destination: Option<String>,
    #[serde(default)]
    pub snapshot: Option<String>,
    /// Job IDs whose answer was applied, persisted after each application.
    pub applied: Vec<String>,
}

pub struct Decisions {
    path: PathBuf,
    state: Mutex<Vec<Decision>>,
}

impl Decisions {
    pub fn open(home: &Path) -> std::io::Result<Self> {
        let path = home.join("limit-decisions.json");
        let state = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn edit<T>(
        &self,
        edit: impl FnOnce(&mut Vec<Decision>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = state.clone();
        let result = edit(&mut next)?;
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        crate::store::write_private(&self.path, &bytes).map_err(|e| e.to_string())?;
        *state = next;
        Ok(result)
    }

    pub fn list(&self) -> Vec<Decision> {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Multiple jobs stopped by the same account and reset join one pending decision.
    pub fn record(
        &self,
        agent: &str,
        account: &str,
        reset: Option<i64>,
        job: &str,
        nonce: &str,
    ) -> Result<Decision, String> {
        self.edit(|state| {
            if let Some(existing) = state
                .iter()
                .find(|d| d.nonces.get(job).is_some_and(|n| n == nonce))
            {
                return Ok(existing.clone());
            }
            // A fresh limit for the same job must invalidate any older pending scope.
            for old in state
                .iter_mut()
                .filter(|d| d.choice == Choice::Pending && d.jobs.iter().any(|j| j == job))
            {
                old.jobs.retain(|j| j != job);
                old.nonces.remove(job);
                old.revision += 1;
            }
            if let Some(existing) = state.iter_mut().find(|d| {
                d.choice == Choice::Pending
                    && d.agent == agent
                    && d.account == account
                    && !d.jobs.is_empty()
            }) {
                existing.jobs.push(job.to_owned());
                existing.nonces.insert(job.to_owned(), nonce.to_owned());
                existing.jobs.sort();
                existing.revision += 1;
                existing.reset_at = existing.reset_at.into_iter().chain(reset).max();
                return Ok(existing.clone());
            }
            let d = Decision {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                agent: agent.to_owned(),
                account: account.to_owned(),
                reset_at: reset,
                created_at: obi_protocol::time::now(),
                jobs: vec![job.to_owned()],
                nonces: [(job.to_owned(), nonce.to_owned())].into_iter().collect(),
                choice: Choice::Pending,
                destination: None,
                snapshot: None,
                applied: Vec::new(),
            };
            state.push(d.clone());
            Ok(d)
        })
    }

    /// Persist the displayed scope and options; stale signed choices fail when either changes.
    pub fn refresh(&self, id: &str, snapshot: &str) -> Result<Decision, String> {
        self.edit(|state| {
            let d = state
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or("Decision not found.")?;
            if d.choice != Choice::Pending {
                return Err("Decision is no longer pending.".into());
            }
            if d.snapshot.as_deref() != Some(snapshot) {
                d.snapshot = Some(snapshot.to_owned());
                d.revision += 1;
            }
            Ok(d.clone())
        })
    }

    /// Compare and set makes simultaneous answers and stale revisions harmless.
    pub fn answer(
        &self,
        id: &str,
        revision: u64,
        choice: &str,
        destination: Option<&str>,
    ) -> Result<Decision, String> {
        let choice = Choice::answer(choice)?;
        if (choice == Choice::Move && destination.is_none_or(|id| id.is_empty()))
            || (choice != Choice::Move && destination.is_some())
        {
            return Err("A move requires a destination; other actions take none.".into());
        }
        self.edit(|state| {
            let d = state
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or("Decision not found.")?;
            if d.revision != revision || d.choice != Choice::Pending {
                return Err("The decision changed; refresh and choose again.".into());
            }
            d.choice = choice;
            d.destination = destination.map(str::to_owned);
            d.revision += 1;
            Ok(d.clone())
        })
    }

    pub fn applied(&self, id: &str, job: &str) -> Result<(), String> {
        self.edit(|state| {
            let d = state
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or("Decision not found.")?;
            if !d.applied.iter().any(|j| j == job) {
                d.applied.push(job.to_owned());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_decision_survives_restart_and_rejects_stale_and_duplicate_answers() {
        let home = std::env::temp_dir().join(format!("oby-limit-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let decisions = Decisions::open(&home).unwrap();
        let first = decisions
            .record("claude", "a", Some(100), "one", "nonce-one")
            .unwrap();
        let both = decisions
            .record("claude", "a", Some(100), "two", "nonce-two")
            .unwrap();
        assert_eq!(first.id, both.id);
        assert_eq!(both.jobs, ["one", "two"]);
        assert!(
            decisions
                .answer(&both.id, first.revision, "resume", None)
                .is_err()
        );
        let shown = decisions.refresh(&both.id, "both and resume").unwrap();
        decisions
            .answer(&shown.id, shown.revision, "resume", None)
            .unwrap();
        assert!(
            decisions
                .answer(&shown.id, shown.revision, "resume", None)
                .is_err()
        );
        drop(decisions);
        let restored = Decisions::open(&home).unwrap();
        assert_eq!(restored.list()[0].choice, Choice::Resume);
        assert_eq!(restored.list()[0].jobs.len(), 2);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_new_limit_on_the_same_job_invalidates_the_old_decision() {
        let home = std::env::temp_dir().join(format!("oby-limit-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let decisions = Decisions::open(&home).unwrap();
        let first = decisions
            .record("claude", "a", Some(100), "one", "old")
            .unwrap();
        let second = decisions
            .record("claude", "a", Some(200), "one", "new")
            .unwrap();
        assert_ne!(first.id, second.id);
        assert!(
            decisions
                .answer(&first.id, first.revision, "resume", None)
                .is_err()
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_failed_write_does_not_accept_an_answer() {
        let home = std::env::temp_dir().join(format!("oby-limit-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let decisions = Decisions::open(&home).unwrap();
        let first = decisions
            .record("claude", "a", None, "one", "nonce")
            .unwrap();
        std::fs::remove_dir_all(&home).unwrap();
        assert!(
            decisions
                .answer(&first.id, first.revision, "resume", None)
                .is_err()
        );
        assert_eq!(decisions.list()[0].choice, Choice::Pending);
    }

    #[test]
    fn overlapping_windows_on_one_account_still_offer_one_pending_choice() {
        let home = std::env::temp_dir().join(format!("oby-limit-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&home).unwrap();
        let decisions = Decisions::open(&home).unwrap();
        let first = decisions
            .record("claude", "a", Some(100), "one", "one")
            .unwrap();
        let both = decisions
            .record("claude", "a", Some(200), "two", "two")
            .unwrap();
        assert_eq!(first.id, both.id);
        assert_eq!(both.reset_at, Some(200));
        assert_eq!(both.jobs.len(), 2);
        assert!(
            decisions
                .answer(&first.id, first.revision, "resume", None)
                .is_err()
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
