//! The one table of what holds an account back for a usage limit, and until when (R08-07, LIMITS-21
//! to 24), kept with its usage in `home/usage.json`, so routing, moves, waits and talk read it alike.

use crate::usage::{UNKNOWN_RESET, Usages};
use tokio::sync::Notify;

/// How long after a limit's reset its work goes back by itself (LIMITS-5): the clocks may differ.
pub const RESUME_AFTER_RESET: i64 = 90;

/// What a hold keeps off an account: all its work (its `fullUntil`), or the work of one model
/// family whose own week its tool refused (USAGE-13), or one job, each under its key in `holds`.
pub enum Scope {
    Account,
    Family(String),
    Job(String),
}

impl Scope {
    fn key(&self) -> String {
        match self {
            Scope::Account => "account".into(),
            Scope::Family(family) => format!("family:{family}"),
            Scope::Job(id) => format!("job:{id}"),
        }
    }
}

/// Work an account is asked to take: its model's family, its job once made, and whether nobody's
/// watching it (LIMITS-20).
pub struct Work<'a> {
    pub family: Option<&'static str>,
    pub job: Option<&'a str>,
    pub background: bool,
}

impl<'a> Work<'a> {
    /// `model` a model's id or alias, or a family.
    pub fn new(model: Option<&str>, job: Option<&'a str>, background: bool) -> Self {
        let family = model.and_then(family);
        Work {
            family,
            job,
            background,
        }
    }
    pub fn of(job: &'a crate::job::Job) -> Self {
        Self::new(job.model.as_deref(), Some(&job.id), job.background())
    }
    /// The holds that keep it, and it alone, off an account: its family's and its job's.
    fn scopes(&self) -> impl Iterator<Item = Scope> {
        let family = self.family.map(|f| Scope::Family(f.into()));
        family
            .into_iter()
            .chain(self.job.map(|id| Scope::Job(id.into())))
    }
}

/// The family a model's id or alias, or a refusal's `rateLimitType`, names: `opus`, `sonnet`.
pub fn family(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    ["opus", "sonnet"].into_iter().find(|f| name.contains(f))
}

/// When work a hold kept off until `until` goes back by itself.
pub fn release_at(until: i64) -> i64 {
    until + RESUME_AFTER_RESET
}

/// The holds, read and written through each account's usage, and who hears that one changed.
pub struct Holds<'a>(pub &'a Usages, pub &'a Notify);

impl Holds<'_> {
    /// `account`'s tool refused work until `reset` (an hour from `now` when none is ahead): all of
    /// it, unless that reset is gone by (a stale one) or a later one is kept, or only the `scoped`
    /// work's (USAGE-13), replacing its earlier hold there.
    pub fn apply(&self, account: &str, reset: Option<i64>, scoped: Option<&Work>, now: i64) {
        let until = reset.filter(|at| *at > now).unwrap_or(now + UNKNOWN_RESET);
        let kept = self.0.get(account).full_until_at(now);
        let stale = reset.is_some_and(|at| at <= now) || kept.is_some_and(|at| at >= until);
        let scopes: Vec<Scope> =
            scoped.map_or(vec![Scope::Account], |work| work.scopes().collect());
        let (_, _, saved) = self.0.change(account, false, |usage| {
            for scope in scopes {
                match scope {
                    Scope::Account if stale => {}
                    Scope::Account => {
                        (usage.full_until, usage.full_until_inferred) =
                            (Some(until), reset.is_none())
                    }
                    scope => usage.holds.extend([(scope.key(), until)]),
                }
            }
        });
        if let Err(error) = saved {
            tracing::warn!("could not keep account usage: {error}");
        }
        self.1.notify_one();
    }

    /// Why `account` takes no `work` now, and until when as far as obid knows: what holds the whole
    /// account (crate::usage), a hold on the work's family or job read as its `fullUntil`.
    pub fn held(&self, account: &str, work: &Work, now: i64) -> Option<(String, Option<i64>)> {
        let mut usage = self.0.get(account);
        usage.full_until = usage.full_until.max(self.scoped(account, work, now));
        let why = usage.held_for(now, work.background)?;
        Some((why, usage.free_at(now, work.background)))
    }

    /// The latest hold still ahead on `account` for `work`'s family or its job.
    pub fn scoped(&self, account: &str, work: &Work, now: i64) -> Option<i64> {
        let holds = self.0.get(account).holds;
        let ahead = work
            .scopes()
            .filter_map(|scope| holds.get(&scope.key()).copied());
        ahead.filter(|at| *at > now).max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_or_a_model_names_its_family() {
        assert_eq!(family("seven_day_opus"), Some("opus"));
        assert_eq!(family("seven_day_sonnet"), Some("sonnet"));
        assert_eq!(family("seven_day_overage_included"), None);
        assert_eq!(family("five_hour"), None);
        assert_eq!(family("claude-opus-5-5"), Some("opus"));
        assert_eq!(family("Sonnet"), Some("sonnet"));
        assert_eq!(family("claude-haiku-5-5"), None);
        assert_eq!(family("gpt-6-astra"), None);
    }

    #[test]
    fn work_goes_back_90_seconds_after_its_hold() {
        assert_eq!(release_at(1_760_000_000), 1_760_000_090);
    }

    /// An older `usage.json` loads to the same whole-account hold, with no model or job holds; a
    /// model's hold holds only its family's work and its job, and goes once it's gone by.
    #[test]
    fn an_older_usage_file_keeps_its_hold() {
        let home = std::env::temp_dir().join(format!("obid-holds-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let now = obi_protocol::time::now();
        let older = serde_json::json!({"stub": {"fullUntil": now + 600, "fullUntilInferred": true},
            "other": {"holds": {"job:gone": now - 5}}});
        std::fs::write(home.join("usage.json"), older.to_string()).unwrap();
        let (usage, changed) = (Usages::open(&home), Notify::new());
        let holds = Holds(&usage, &changed);
        let kept = usage.get("stub");
        assert_eq!(
            (kept.full_until, kept.full_until_inferred),
            (Some(now + 600), true)
        );
        assert!(kept.holds.is_empty());
        let any = Work::new(None, None, false);
        assert_eq!(holds.held("stub", &any, now).unwrap().1, Some(now + 600));

        let refused = Work::new(Some("opus"), Some("j1"), false);
        holds.apply("other", Some(now + 60), Some(&refused), now);
        let opus = Work::new(Some("claude-opus-5-5"), None, false);
        let sonnet = Work::new(Some("claude-sonnet-5"), Some("j2"), false);
        assert_eq!(holds.held("other", &opus, now).unwrap().1, Some(now + 60));
        assert!(holds.held("other", &sonnet, now).is_none());
        let j1 = Work::new(None, Some("j1"), false);
        assert!(holds.held("other", &j1, now).is_some());
        assert_eq!(usage.get("other").full_until, None);
        // A hold gone by went when its account was saved.
        let kept: Vec<String> = usage.get("other").holds.into_keys().collect();
        assert_eq!(kept, ["family:opus", "job:j1"]);
        holds.apply("other", Some(now - 1), None, now);
        assert_eq!(
            usage.get("other").full_until,
            None,
            "a stale reset marks nothing"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
