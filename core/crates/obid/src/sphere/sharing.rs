//! Sharing rules and per-sender limits (docs/runner-api.md, "obid only: sharing rules"; spec
//! `protocol.md` section 11, "Questions" and "Rate limits"): what the owner lets their Obi tell
//! some people without being asked, and how much each sender's owner can make it do. Only the
//! owner changes them, through the runner API; nothing another Obi sends can. Kept in
//! `home/sharing.json` (0600).

use std::path::{Path, PathBuf};

use obi_protocol::time;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::store;

/// What a rule may let the Obi tell. The spec's third, published pull requests as the code host
/// shows them, needs the code host: obid offers the two it holds itself (a job's status names
/// the pull request it opened).
pub const TELLS: [&str; 2] = [JOB_STATUS, SHARED_RESULTS];
/// The title, stage and state of the owner's jobs, and the pull request each opened.
pub const JOB_STATUS: &str = "jobStatus";
/// What obid already sent that person's Obis: the results of reviews they asked for.
pub const SHARED_RESULTS: &str = "sharedResults";

/// The most rules obid keeps.
pub const MAX_RULES: usize = 100;

/// Who a rule is for, or which repositories it covers: a list, or every one (`*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scope {
    Every(String),
    These(Vec<String>),
}

impl Scope {
    fn every() -> Scope {
        Scope::Every("*".into())
    }

    fn is_every(&self) -> bool {
        matches!(self, Scope::Every(_))
    }

    /// Whether it names `owner` exactly, or is every one.
    fn has_owner(&self, owner: &str) -> bool {
        match self {
            Scope::Every(_) => true,
            Scope::These(list) => list.iter().any(|o| o == owner),
        }
    }

    /// Whether it covers `repo`, ignoring ASCII case, or is every one.
    pub fn has_repo(&self, repo: &str) -> bool {
        match self {
            Scope::Every(_) => true,
            Scope::These(list) => list.iter().any(|r| r.eq_ignore_ascii_case(repo)),
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Scope::Every(_) => json!("*"),
            Scope::These(list) => json!(list),
        }
    }
}

/// One sharing rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// `s-` and 12 lower-case hex: a `rule_id` the spec allows in `answer.by_rule`.
    pub id: String,
    /// Owners from the Sphere's directory, or every member (`*`).
    #[serde(rename = "for")]
    pub for_owners: Scope,
    pub repos: Scope,
    /// What it lets the Obi tell: some of [`TELLS`].
    pub tells: Vec<String>,
    pub created: i64,
    /// Answers the Obi sent under it.
    pub uses: u64,
    pub last_used: Option<i64>,
    /// The rule obid made at the first join ([`starter`]), not the owner: it answers who asks, and
    /// opens no room. Never in the API's answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starter: bool,
}

impl Rule {
    /// Whether it is for `owner` and covers each of `repos` (none: only a rule for every
    /// repository covers a question about no repository).
    pub fn covers(&self, owner: &str, repos: &[String]) -> bool {
        self.for_owners.has_owner(owner)
            && if repos.is_empty() {
                self.repos.is_every()
            } else {
                repos.iter().all(|r| self.repos.has_repo(r))
            }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "for": self.for_owners.to_json(),
            "repos": self.repos.to_json(),
            "tells": self.tells,
            "created": time::format(self.created),
            "uses": self.uses,
            "lastUsed": self.last_used.map(time::format),
        })
    }
}

/// How much each sender's owner, and every sender together, can make the Obi do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    /// Questions an hour from one sender's owner the Obi takes on to answer by itself (a model run
    /// on the owner's account).
    pub self_answers_per_hour: u32,
    /// The same, from every sender together: what all of them can spend of the owner's account.
    pub self_answers_per_hour_total: u32,
    /// Requests of every type waiting in the owner's inbox at once.
    pub inbox_per_owner: u32,
}

impl Default for Limits {
    /// The spec's recommended defaults.
    fn default() -> Limits {
        Limits {
            self_answers_per_hour: 30,
            self_answers_per_hour_total: 60,
            inbox_per_owner: 5,
        }
    }
}

impl Limits {
    pub fn to_json(self) -> Value {
        json!({"selfAnswersPerHour": self.self_answers_per_hour,
               "selfAnswersPerHourTotal": self.self_answers_per_hour_total,
               "inboxPerOwner": self.inbox_per_owner})
    }
}

/// The rules and the limits, as kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sharing {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub limits: Limits,
}

impl Sharing {
    pub fn to_json(&self) -> Value {
        json!({
            "rules": self.rules.iter().map(Rule::to_json).collect::<Vec<_>>(),
            "limits": self.limits.to_json(),
        })
    }

    /// The one rule an answer by the Obi would be made under, for a question from an Obi of
    /// `owner` about `repos` (none: a question about no repository): the oldest rule for that
    /// owner that covers every one of them. A question about no repository only a rule for every
    /// repository covers.
    pub fn covering(&self, owner: &str, repos: &[String]) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.covers(owner, repos))
    }

    /// The oldest rule that lets the Obi tell `owner` the status of its jobs in `repos`.
    pub fn job_status_rule(&self, owner: &str, repos: &[String]) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| rule.covers(owner, repos) && rule.tells.iter().any(|t| t == JOB_STATUS))
    }

    /// The oldest rule the room goes by for `owner`, as [`Self::job_status_rule`]: one the owner made.
    /// A post goes to every member unasked, which joining a Sphere doesn't agree to.
    pub fn room_rule(&self, owner: &str, repos: &[String]) -> Option<&Rule> {
        self.rules.iter().find(|rule| {
            !rule.starter && rule.covers(owner, repos) && rule.tells.iter().any(|t| t == JOB_STATUS)
        })
    }

    /// Whether the room may hear it: a rule lets the Obi tell each of `owners` (at least one) the
    /// status of its jobs in `repos`, since a post goes to every member.
    pub fn job_status_for_all(&self, owners: &[String], repos: &[String]) -> bool {
        !owners.is_empty()
            && owners
                .iter()
                .all(|owner| self.room_rule(owner, repos).is_some())
    }
}

/// What an obid shares once it first joins a Sphere (ASK-11, docs/contracts/ask-an-oby.md): with every
/// member, the title, stage and state of its owner's jobs, the repository each is in and the pull
/// request it opened, so their Obys can ask how the work goes without bothering them. Never logs,
/// diffs, prompts or files. The owner removes it like any rule, and it doesn't come back.
fn starter(now: i64) -> Sharing {
    Sharing {
        rules: vec![Rule {
            id: new_id(),
            for_owners: Scope::every(),
            repos: Scope::every(),
            tells: vec![JOB_STATUS.to_owned()],
            created: now,
            uses: 0,
            last_used: None,
            starter: true,
        }],
        limits: Limits::default(),
    }
}

/// A new rule id: `s-` and 12 lower-case hex.
pub fn new_id() -> String {
    let mut bytes = [0u8; 6];
    getrandom::fill(&mut bytes).expect("the system's random source works");
    format!("s-{}", hex::encode(bytes))
}

/// `"*"` or a list of 1 to 50 distinct values, each passing `valid`.
fn scope(
    map: &Map<String, Value>,
    key: &str,
    what: &str,
    valid: impl Fn(&str) -> bool,
) -> Result<Scope, String> {
    let shape = format!("{key} is \"*\" or a list of 1 to 50 {what}.");
    match map.get(key) {
        Some(Value::String(s)) if s == "*" => Ok(Scope::every()),
        Some(Value::Array(list)) if (1..=50).contains(&list.len()) => {
            let mut out: Vec<String> = Vec::new();
            for item in list {
                let text = item.as_str().ok_or_else(|| shape.clone())?;
                if !valid(text) {
                    return Err(format!("{text:?} isn't one of the {what}."));
                }
                if !out.iter().any(|o| o.eq_ignore_ascii_case(text)) {
                    out.push(text.to_owned());
                }
            }
            Ok(Scope::These(out))
        }
        _ => Err(shape),
    }
}

/// A new rule from the body of `POST /v1/obid/sharing/rules`: `for`, `repos` and `tells`, nothing
/// else (`Err` says why not).
pub fn rule_from(body: &Value, now: i64) -> Result<Rule, String> {
    let map = body.as_object().ok_or("The body is a JSON object.")?;
    if let Some(unknown) = map
        .keys()
        .find(|k| !["for", "repos", "tells"].contains(&k.as_str()))
    {
        return Err(format!("{unknown} isn't a field this endpoint takes."));
    }
    let owner = |t: &str| obi_protocol::schema::is_valid("common#/$defs/owner_id", &json!(t));
    let repo = |t: &str| obi_protocol::schema::is_valid("common#/$defs/repo", &json!(t));
    let for_owners = scope(map, "for", "owners", owner)?;
    let repos = scope(map, "repos", "repositories (owner/name)", repo)?;
    let tells: Vec<String> = match map.get("tells") {
        Some(Value::Array(list)) if !list.is_empty() => list
            .iter()
            .map(|t| {
                t.as_str()
                    .filter(|t| TELLS.contains(t))
                    .map(str::to_owned)
                    .ok_or_else(|| format!("tells lists some of {}.", TELLS.join(", ")))
            })
            .collect::<Result<_, _>>()?,
        _ => return Err(format!("tells lists some of {}.", TELLS.join(", "))),
    };
    let mut distinct = tells.clone();
    distinct.sort();
    distinct.dedup();
    if distinct.len() != tells.len() {
        return Err("tells names each at most once.".into());
    }
    Ok(Rule {
        id: new_id(),
        for_owners,
        repos,
        tells,
        created: now,
        uses: 0,
        last_used: None,
        starter: false,
    })
}

/// New limits from the body of `PUT /v1/obid/sharing/limits` (`Err` says why not).
pub fn limits_from(body: &Value) -> Result<Limits, String> {
    let map = body.as_object().ok_or("The body is a JSON object.")?;
    let shape = "The body is {\"selfAnswersPerHour\": 0 to 1,000, \"selfAnswersPerHourTotal\": 0 to 10,000, \"inboxPerOwner\": 0 to 100}.";
    if map.len() != 3 {
        return Err(shape.into());
    }
    let number = |key: &str, max: u64| {
        map.get(key)
            .and_then(Value::as_u64)
            .filter(|n| *n <= max)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(shape)
    };
    Ok(Limits {
        self_answers_per_hour: number("selfAnswersPerHour", 1000)?,
        self_answers_per_hour_total: number("selfAnswersPerHourTotal", 10_000)?,
        inbox_per_owner: number("inboxPerOwner", 100)?,
    })
}

/// Where they are kept.
pub struct Kept {
    path: PathBuf,
}

impl Kept {
    pub fn open(home: &Path) -> Kept {
        Kept {
            path: home.join("sharing.json"),
        }
    }

    /// The rules and limits kept, or none. A file that doesn't read, or can't be read, shares
    /// nothing, and stays as it is.
    pub fn load(&self) -> Sharing {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::error!(
                    "{} doesn't read, so nothing is shared: {e}",
                    self.path.display()
                );
                Sharing::default()
            }),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::error!(
                        "{} can't be read, so nothing is shared: {e}",
                        self.path.display()
                    );
                }
                Sharing::default()
            }
        }
    }

    /// Whether there is no file yet, for certain: a file that can't be read is one.
    fn missing(&self) -> bool {
        matches!(std::fs::symlink_metadata(&self.path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    }

    /// The owner's first Sphere: the starter rule, kept, when they have no file yet (ASK-11).
    pub fn seed_at_joining(&self) -> Option<Sharing> {
        if !self.missing() {
            return None;
        }
        let sharing = starter(time::now());
        match self.save(&sharing) {
            Ok(()) => Some(sharing),
            Err(e) => {
                tracing::warn!("can't keep {}: {e}", self.path.display());
                None
            }
        }
    }

    /// A membership from before the starter rule: it never agreed to share, and an upgrade doesn't
    /// start to. No rules, kept, so that a later join doesn't seed one either.
    pub fn keep_none_when_missing(&self) {
        if self.missing()
            && let Err(e) = self.save(&Sharing::default())
        {
            tracing::warn!("can't keep {}: {e}", self.path.display());
        }
    }

    pub fn save(&self, sharing: &Sharing) -> std::io::Result<()> {
        let text = serde_json::to_vec_pretty(sharing).map_err(std::io::Error::other)?;
        store::write_private(&self.path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(body: Value) -> Rule {
        rule_from(&body, 0).unwrap()
    }

    #[test]
    fn rules_cover_their_owners_and_every_repository_asked_about() {
        let sharing = Sharing {
            rules: vec![
                rule(json!({"for": ["ernesto"], "repos": ["Acme/App"], "tells": ["jobStatus"]})),
                rule(json!({"for": "*", "repos": "*", "tells": ["sharedResults"]})),
            ],
            limits: Limits::default(),
        };
        let repos = |list: &[&str]| list.iter().map(|r| (*r).to_owned()).collect::<Vec<_>>();
        let first = &sharing.rules[0].id;
        let second = &sharing.rules[1].id;
        assert_eq!(
            &sharing
                .covering("ernesto", &repos(&["acme/app"]))
                .unwrap()
                .id,
            first
        );
        // Another repository too: only the rule for every repository covers both.
        assert_eq!(
            &sharing
                .covering("ernesto", &repos(&["acme/app", "acme/web"]))
                .unwrap()
                .id,
            second
        );
        // A question about nothing: only a rule for every repository.
        assert_eq!(&sharing.covering("ernesto", &[]).unwrap().id, second);
        assert_eq!(
            &sharing
                .covering("giulia", &repos(&["acme/app"]))
                .unwrap()
                .id,
            second
        );
        let narrow = Sharing {
            rules: vec![sharing.rules[0].clone()],
            limits: Limits::default(),
        };
        assert!(narrow.covering("ernesto", &[]).is_none());
        assert!(narrow.covering("giulia", &repos(&["acme/app"])).is_none());
    }

    #[test]
    fn the_room_hears_job_status_only_under_a_rule_for_every_member() {
        let repo = vec!["acme/app".to_owned()];
        let owners = |list: &[&str]| list.iter().map(|o| (*o).to_owned()).collect::<Vec<_>>();
        let of = |rules: Vec<Rule>| Sharing {
            rules,
            limits: Limits::default(),
        };
        let everyone = of(vec![rule(
            json!({"for": "*", "repos": ["acme/app"], "tells": ["jobStatus"]}),
        )]);
        assert!(everyone.job_status_for_all(&owners(&["ernesto", "giulia"]), &repo));
        assert!(!everyone.job_status_for_all(&owners(&["ernesto"]), &["acme/web".to_owned()]));
        assert!(!everyone.job_status_for_all(&[], &repo), "nobody to tell");
        let one = of(vec![rule(
            json!({"for": ["ernesto"], "repos": "*", "tells": ["jobStatus"]}),
        )]);
        assert!(one.job_status_for_all(&owners(&["ernesto"]), &repo));
        assert!(!one.job_status_for_all(&owners(&["ernesto", "giulia"]), &repo));
        // Another thing to tell isn't job status, even for everyone.
        let results = of(vec![rule(
            json!({"for": "*", "repos": "*", "tells": ["sharedResults"]}),
        )]);
        assert!(!results.job_status_for_all(&owners(&["ernesto"]), &repo));
    }

    #[test]
    fn a_rule_takes_only_what_it_may_be() {
        for (body, why) in [
            (
                json!({"for": "ernesto", "repos": "*", "tells": ["jobStatus"]}),
                "for",
            ),
            (
                json!({"for": ["Ernesto Smith"], "repos": "*", "tells": ["jobStatus"]}),
                "owner",
            ),
            (
                json!({"for": "*", "repos": ["not a repo"], "tells": ["jobStatus"]}),
                "repo",
            ),
            (
                json!({"for": "*", "repos": "*", "tells": ["files"]}),
                "tells",
            ),
            (json!({"for": "*", "repos": "*", "tells": []}), "no tells"),
            (
                json!({"for": "*", "repos": "*", "tells": ["jobStatus", "jobStatus"]}),
                "twice",
            ),
            (
                json!({"for": "*", "repos": "*", "tells": ["jobStatus"], "prompt": "x"}),
                "field",
            ),
            (
                json!({"for": [], "repos": "*", "tells": ["jobStatus"]}),
                "empty",
            ),
        ] {
            assert!(rule_from(&body, 0).is_err(), "{why}");
        }
        let made =
            rule(json!({"for": ["ernesto", "ernesto"], "repos": "*", "tells": ["jobStatus"]}));
        assert_eq!(made.for_owners, Scope::These(vec!["ernesto".into()]));
        assert!(made.id.starts_with("s-") && made.id.len() == 14);
        assert!(obi_protocol::schema::is_valid(
            "common#/$defs/rule_id",
            &json!(made.id)
        ));
        let shown = made.to_json();
        assert_eq!(shown["repos"], "*");
        assert_eq!(shown["for"], json!(["ernesto"]));
    }

    #[test]
    fn ask_11_the_rule_obid_starts_with_answers_who_asks_and_opens_no_room() {
        let owners = ["ernesto".to_owned()];
        let repos = ["acme/app".to_owned()];
        let sharing = starter(0);
        assert!(sharing.job_status_rule("ernesto", &repos).is_some());
        assert!(sharing.room_rule("ernesto", &repos).is_none());
        assert!(!sharing.job_status_for_all(&owners, &repos));
        // A rule the owner made does, and it's kept that way.
        let mut sharing = starter(0);
        sharing.rules.push(rule(
            json!({"for": "*", "repos": "*", "tells": ["jobStatus"]}),
        ));
        assert!(sharing.job_status_for_all(&owners, &repos));
        let kept: Sharing = serde_json::from_slice(&serde_json::to_vec(&sharing).unwrap()).unwrap();
        assert!(kept.rules[0].starter && !kept.rules[1].starter);
    }

    #[test]
    fn ask_11_the_first_join_shares_job_status_until_the_owner_says_otherwise() {
        let home = std::env::temp_dir().join(format!("obid-ask-11-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let kept = Kept::open(&home);
        assert!(kept.load().rules.is_empty(), "nothing before joining");
        assert!(!home.join("sharing.json").exists(), "and nothing kept");
        let first = kept.seed_at_joining().unwrap();
        assert_eq!(first.rules.len(), 1);
        let rule = &first.rules[0];
        assert_eq!(rule.tells, vec![JOB_STATUS.to_owned()], "status only");
        assert!(rule.covers("anyone", &[]) && rule.covers("anyone", &["a/b".to_owned()]));
        assert_eq!(kept.load(), first, "the same rule, under the same id");
        assert_eq!(kept.seed_at_joining(), None, "once");
        // Removed by the owner: it stays removed, across a restart and another join.
        kept.save(&Sharing {
            rules: vec![],
            ..first
        })
        .unwrap();
        assert_eq!(kept.seed_at_joining(), None);
        assert!(Kept::open(&home).load().rules.is_empty());
        // A file that doesn't read shares nothing, and is never replaced by the starter.
        std::fs::write(home.join("sharing.json"), "not json").unwrap();
        assert!(Kept::open(&home).load().rules.is_empty());
        assert_eq!(kept.seed_at_joining(), None);
        assert_eq!(
            std::fs::read_to_string(home.join("sharing.json")).unwrap(),
            "not json"
        );
        std::fs::remove_file(home.join("sharing.json")).unwrap();
        // A membership from before: no rules, kept, so that no later join seeds one.
        kept.keep_none_when_missing();
        assert!(kept.load().rules.is_empty());
        assert_eq!(kept.seed_at_joining(), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn limits_and_their_defaults() {
        assert_eq!(
            Limits::default().to_json(),
            json!({"selfAnswersPerHour": 30, "selfAnswersPerHourTotal": 60, "inboxPerOwner": 5})
        );
        assert_eq!(
            limits_from(
                &json!({"selfAnswersPerHour": 0, "selfAnswersPerHourTotal": 10000, "inboxPerOwner": 100})
            )
            .unwrap(),
            Limits {
                self_answers_per_hour: 0,
                self_answers_per_hour_total: 10_000,
                inbox_per_owner: 100
            }
        );
        for bad in [
            json!({"selfAnswersPerHour": 1001, "selfAnswersPerHourTotal": 1, "inboxPerOwner": 1}),
            json!({"selfAnswersPerHour": 1, "selfAnswersPerHourTotal": 10001, "inboxPerOwner": 1}),
            json!({"selfAnswersPerHour": 1, "selfAnswersPerHourTotal": 1, "inboxPerOwner": 101}),
            json!({"selfAnswersPerHour": 1.5, "selfAnswersPerHourTotal": 1, "inboxPerOwner": 1}),
            json!({"selfAnswersPerHour": 1, "inboxPerOwner": 1}),
            json!({"selfAnswersPerHour": 1, "inboxPerOwner": 1, "other": 1}),
        ] {
            assert!(limits_from(&bad).is_err(), "{bad}");
        }
        // A file from before the overall cap keeps its limits and gets the cap's default.
        let kept: Sharing = serde_json::from_value(json!({"rules": []})).unwrap();
        assert_eq!(kept.limits, Limits::default());
        let older: Sharing = serde_json::from_value(
            json!({"rules": [], "limits": {"selfAnswersPerHour": 3, "inboxPerOwner": 2}}),
        )
        .unwrap();
        assert_eq!(older.limits.self_answers_per_hour, 3);
        assert_eq!(older.limits.self_answers_per_hour_total, 60);
    }
}
