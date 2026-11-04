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
/// shows them, needs the code host: obid offers what it holds itself (a job's status names the pull
/// request it opened).
pub const TELLS: [&str; 3] = [JOB_STATUS, JOB_PROGRESS, SHARED_RESULTS];
/// The title, stage and state of the owner's jobs, and the pull request each opened.
pub const JOB_STATUS: &str = "jobStatus";
/// On top of [`JOB_STATUS`]: each job's current step and latest report line, through the secret
/// scan (`answering::add_progress`). A rule has it only by the owner's yes to a card that lists it,
/// or their own list: a rule from before never starts to.
pub const JOB_PROGRESS: &str = "jobProgress";
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

/// `acme/app`, `acme/app and acme/web`, `acme/app, acme/web and acme/api`.
pub fn repo_names(repos: &[String]) -> String {
    match repos {
        [] => String::new(),
        [one] => one.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

impl Scope {
    fn every() -> Scope {
        Scope::Every("*".into())
    }

    pub fn is_every(&self) -> bool {
        matches!(self, Scope::Every(_))
    }

    /// The repositories it covers, as a card says them: `in every repository`, `in acme/app`.
    pub fn in_words(&self) -> String {
        match self {
            Scope::Every(_) => "in every repository".into(),
            Scope::These(list) => format!("in {}", repo_names(list)),
        }
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
    /// The rule of the owner's yes to the offer, or of a first join before SPH-8 ([`answers_who_asks`]):
    /// it answers who asks, and opens no room, which neither agreed to.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starter: bool,
    /// The rule of the owner's yes to posting in the room ([`Sharing::share_room`]). The room is that
    /// Sphere's, so it goes when they leave, as [`Rule::starter`] does. Not in the API's answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub room_yes: bool,
    /// [`JOB_PROGRESS`] is on it by the owner's yes to the offer, not their own list: that yes was for
    /// one Sphere too, so it comes off when they leave ([`Sharing::forget_the_offer`]). Not in the
    /// API's answer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub progress_yes: bool,
}

impl Rule {
    /// Whether it is for `owner` and covers each of `repos`. A question about no repository only a
    /// rule for every repository covers, or one made on the owner's yes, which answers who asks
    /// ("What's the progress?") from the repositories they chose ([`crate::sphere::answering::material`]).
    pub fn covers(&self, owner: &str, repos: &[String]) -> bool {
        self.for_owners.has_owner(owner)
            && if repos.is_empty() {
                self.repos.is_every() || self.for_this_sphere()
            } else {
                repos.iter().all(|r| self.repos.has_repo(r))
            }
    }

    /// Whether it lets the Obi tell `what`, one of [`TELLS`].
    pub fn has(&self, what: &str) -> bool {
        self.tells.iter().any(|t| t == what)
    }

    /// Whether it tells every member the status of the jobs in every repository.
    fn tells_everyone_status(&self) -> bool {
        self.for_owners.is_every() && self.repos.is_every() && self.has(JOB_STATUS)
    }

    /// Whether it tells every member the status of the jobs, in whichever repositories it covers.
    fn for_everyone(&self) -> bool {
        self.for_owners.is_every() && self.has(JOB_STATUS)
    }

    /// Whether obid made it on a yes that was for one Sphere: it goes when they leave.
    pub fn for_this_sphere(&self) -> bool {
        self.starter || self.room_yes
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
            "starter": self.starter,
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
    /// When the owner's say on sharing was asked for, or made needless by their removing a rule
    /// (PEER-16, PEER-18): obid asks at most once. A file from before has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offered: Option<i64>,
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

    /// Whether a question should ask the owner about sharing, the one time obid asks: they were never
    /// asked and never removed a rule, and share nothing, or share status with everyone and not yet
    /// each job's progress. A narrower rule is their own list, and asks nothing.
    pub fn to_offer(&self) -> bool {
        self.offered.is_none() && (self.rules.is_empty() || self.shares_only_status())
    }

    /// Whether a rule of their own tells everyone how their tasks are going in every repository, and
    /// nobody each job's progress: the yes asked of them is to add it.
    pub fn shares_only_status(&self) -> bool {
        self.rules.iter().any(Rule::tells_everyone_status)
            && !self.rules.iter().any(|r| r.has(JOB_PROGRESS))
    }

    /// The repositories the owner's yes last chose (PEER-38): none when they gave no such yes.
    pub fn chosen_repos(&self) -> Vec<String> {
        match self.rules.iter().find(|r| r.starter).map(|r| &r.repos) {
            Some(Scope::These(list)) => list.clone(),
            _ => Vec::new(),
        }
    }

    /// What the owner's yes to the card means, whose title names `chosen` and which lists all it
    /// tells: one rule that answers everyone about exactly those repositories, the one they have or a
    /// new one, and a room's yes never covers more than they now chose. Rules they wrote stay as they
    /// are.
    pub fn share_chosen(&mut self, now: i64, chosen: Vec<String>) -> Rule {
        self.offered.get_or_insert(now);
        for rule in self.rules.iter_mut().filter(|r| r.room_yes) {
            if let Scope::These(list) = &mut rule.repos {
                list.retain(|r| chosen.iter().any(|c| c.eq_ignore_ascii_case(r)));
            }
        }
        self.rules
            .retain(|r| !(r.room_yes && r.repos == Scope::These(Vec::new())));
        let at = self
            .rules
            .iter()
            .position(|r| r.starter)
            .unwrap_or_else(|| {
                self.rules.push(answers_who_asks(now, Scope::every()));
                self.rules.len() - 1
            });
        let rule = &mut self.rules[at];
        rule.repos = Scope::These(chosen);
        rule.tells = vec![JOB_STATUS.to_owned(), JOB_PROGRESS.to_owned()];
        rule.clone()
    }

    /// What their yes to the card that adds a job's progress means (PEER-23): it goes on every rule
    /// that tells status, since those people are among everyone. None with no rule of theirs for it.
    pub fn share_progress_too(&mut self, now: i64) -> Option<Rule> {
        let rule = self.rules.iter().position(Rule::tells_everyone_status)?;
        self.offered.get_or_insert(now);
        for rule in self.rules.iter_mut().filter(|r| r.has(JOB_STATUS)) {
            if !rule.has(JOB_PROGRESS) {
                rule.tells.push(JOB_PROGRESS.to_owned());
                rule.progress_yes = true;
            }
        }
        Some(self.rules[rule].clone())
    }

    /// What said "every repository" goes (PEER-39): a first join's rule, which nobody clicked; the
    /// offer's and the room's, whose yes was to those words. Each told about private work the owner
    /// didn't pick, so neither is narrowed by a guess: it tells nothing, and they're asked once more.
    /// Rules they wrote stay. Whether any went.
    pub fn retire_every_repository_yes(&mut self) -> bool {
        let before = self.rules.len();
        self.rules
            .retain(|r| !(r.for_this_sphere() && r.repos.is_every()));
        let retired = self.rules.len() != before;
        if retired {
            self.offered = None;
        }
        retired
    }

    /// Leaving a Sphere (PEER-19): what obid made on a yes goes, and that they were asked, so the next
    /// Sphere asks again. Rules they wrote stay, as they wrote them.
    pub fn forget_the_offer(&mut self) {
        self.rules.retain(|r| !r.for_this_sphere());
        self.offered = None;
        for rule in self.rules.iter_mut().filter(|r| r.progress_yes) {
            rule.tells.retain(|t| t != JOB_PROGRESS);
            rule.progress_yes = false;
        }
    }

    /// The repositories members may ask about: what the room's card builds on, and names. None while
    /// they share no status with everyone.
    pub fn status_repos(&self) -> Option<&Scope> {
        let everyone = self.rules.iter().filter(|r| r.for_everyone());
        // A rule of theirs first: it's the one the room goes by.
        let rule = everyone.min_by_key(|r| r.starter)?;
        Some(&rule.repos)
    }

    /// The rule the owner's yes to the room's card means (ROOMS-17): the one the room goes by, or a
    /// new one over the same repositories. None while they share no status, which this isn't the way
    /// to start.
    pub fn share_room(&mut self, now: i64) -> Option<Rule> {
        if let Some(rule) = self.rules.iter().find(|r| r.for_everyone() && !r.starter) {
            return Some(rule.clone());
        }
        let repos = self.status_repos()?.clone();
        let rule = Rule {
            starter: false,
            room_yes: true,
            ..answers_who_asks(now, repos)
        };
        self.rules.push(rule.clone());
        Some(rule)
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

/// With every member, the title, stage and state of its owner's jobs in `repos`, the repository each
/// is in and the pull request it opened, so their Obys can ask how the work goes without bothering
/// them. Never logs, diffs, prompts or files. The owner removes it like any rule, and it doesn't come
/// back.
fn answers_who_asks(now: i64, repos: Scope) -> Rule {
    Rule {
        id: new_id(),
        for_owners: Scope::every(),
        repos,
        tells: vec![JOB_STATUS.to_owned()],
        created: now,
        uses: 0,
        last_used: None,
        starter: true,
        room_yes: false,
        progress_yes: false,
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
    if tells.iter().any(|t| t == JOB_PROGRESS) && !tells.iter().any(|t| t == JOB_STATUS) {
        return Err(format!("{JOB_PROGRESS} goes with {JOB_STATUS}."));
    }
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
        room_yes: false,
        progress_yes: false,
    })
}

/// The repositories ticked on the card, from the body of `POST /v1/obid/sharing/offer`: none when it
/// names none, an empty list with every tick taken off. Never `*` (`Err` says why not).
pub fn chosen_from(body: &Value) -> Result<Option<Vec<String>>, String> {
    let Some(map) = body.as_object().filter(|m| m.contains_key("repos")) else {
        return Ok(None);
    };
    if map["repos"].as_array().is_some_and(Vec::is_empty) {
        return Ok(Some(Vec::new()));
    }
    if map["repos"].as_array().is_some_and(|list| list.len() > 50) {
        return Err("You can tick at most 50 repositories.".into());
    }
    let repo = |t: &str| obi_protocol::schema::is_valid("common#/$defs/repo", &json!(t));
    match scope(map, "repos", "repositories (owner/name)", repo) {
        Ok(Scope::These(list)) => Ok(Some(list)),
        Ok(Scope::Every(_)) | Err(_) => Err("That isn't a list of your repositories.".into()),
    }
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

    /// Whether what's kept reads, or there's nothing kept: obid writes nothing of its own over a file
    /// that doesn't, which shares nothing and stays as it is.
    pub fn reads(&self) -> bool {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice::<Sharing>(&bytes).is_ok(),
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
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
            ..Sharing::default()
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
            ..Sharing::default()
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
            ..Sharing::default()
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
            (
                json!({"for": "*", "repos": "*", "tells": ["jobProgress"]}),
                "progress with no job to tell it of",
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

    fn chosen() -> Vec<String> {
        vec!["acme/app".to_owned(), "acme/web".to_owned()]
    }

    #[test]
    fn ask_11_the_owners_yes_answers_who_asks_and_opens_no_room() {
        let owners = ["ernesto".to_owned()];
        let repos = ["acme/app".to_owned()];
        let mut sharing = Sharing::default();
        sharing.share_chosen(0, chosen());
        assert!(sharing.job_status_rule("ernesto", &repos).is_some());
        assert!(sharing.room_rule("ernesto", &repos).is_none());
        assert!(!sharing.job_status_for_all(&owners, &repos));
        // A rule the owner made does, and it's kept that way.
        sharing.rules.push(rule(
            json!({"for": "*", "repos": "*", "tells": ["jobStatus"]}),
        ));
        assert!(sharing.job_status_for_all(&owners, &repos));
        let kept: Sharing = serde_json::from_slice(&serde_json::to_vec(&sharing).unwrap()).unwrap();
        assert!(kept.rules[0].starter && !kept.rules[1].starter);
    }

    /// The owner's yes covers a question that names no repository and any in the list, and none about
    /// a repository outside it. A rule they wrote for some repositories answers as it always did.
    #[test]
    fn peer_37_a_rule_of_chosen_repositories_covers_only_them() {
        let mut sharing = Sharing::default();
        let yes = sharing.share_chosen(0, chosen());
        assert!(yes.covers("ernesto", &[]), "\"What's the progress?\"");
        assert!(yes.covers("ernesto", &["ACME/app".to_owned()]));
        assert!(!yes.covers("ernesto", &["acme/private".to_owned()]));
        assert!(
            !yes.covers(
                "ernesto",
                &["acme/app".to_owned(), "acme/private".to_owned()]
            ),
            "one outside is enough"
        );
        let own = rule(json!({"for": "*", "repos": ["acme/app"], "tells": ["jobStatus"]}));
        assert!(!own.covers("ernesto", &[]), "a rule of theirs didn't widen");
        let room = sharing.share_room(1).unwrap();
        assert!(
            room.covers("ernesto", &[]),
            "the room's yes answers who asks"
        );
    }

    /// One rule, whichever way they tick: exactly what's chosen, each time, never wider, and a room's
    /// yes follows it down, never up.
    #[test]
    fn peer_38_the_yes_covers_exactly_what_is_ticked() {
        let mut sharing = Sharing::default();
        let first = sharing.share_chosen(1, chosen());
        assert_eq!(first.repos, Scope::These(chosen()));
        assert_eq!(
            first.tells,
            [JOB_STATUS, JOB_PROGRESS],
            "what the card lists"
        );
        assert_eq!(sharing.chosen_repos(), chosen());
        assert_eq!(sharing.offered, Some(1));
        assert!(!sharing.to_offer(), "asked once");
        let room = sharing.share_room(2).unwrap();
        assert_eq!(room.repos, Scope::These(chosen()), "the room's goes as far");
        // Change: fewer, then others.
        let narrowed = sharing.share_chosen(3, vec!["acme/web".to_owned()]);
        assert_eq!(narrowed.id, first.id, "the same rule");
        assert_eq!(narrowed.repos, Scope::These(vec!["acme/web".to_owned()]));
        assert_eq!(sharing.offered, Some(1), "the first time asked stays");
        assert_eq!(
            sharing.rules[1].repos,
            Scope::These(vec!["acme/web".to_owned()]),
            "the room's never covers more"
        );
        sharing.share_chosen(4, chosen());
        assert_eq!(
            sharing.rules[1].repos,
            Scope::These(vec!["acme/web".to_owned()]),
            "and doesn't grow back by itself"
        );
        sharing.share_chosen(5, vec!["acme/app".to_owned()]);
        assert_eq!(sharing.rules.len(), 1, "a room's with nothing left goes");
        // A rule they wrote isn't touched.
        let mine = rule(json!({"for": ["ernesto"], "repos": "*", "tells": ["sharedResults"]}));
        sharing.rules.push(mine.clone());
        sharing.share_chosen(6, chosen());
        assert_eq!(sharing.rules[1], mine);
    }

    #[test]
    fn peer_38_what_the_card_takes() {
        assert_eq!(chosen_from(&json!({})), Ok(None));
        assert_eq!(chosen_from(&json!({"room": true})), Ok(None));
        assert_eq!(chosen_from(&json!({"repos": []})), Ok(Some(vec![])));
        assert_eq!(
            chosen_from(&json!({"repos": ["acme/app", "ACME/APP"]})),
            Ok(Some(vec!["acme/app".to_owned()]))
        );
        for bad in [
            json!({"repos": "*"}),
            json!({"repos": "acme/app"}),
            json!({"repos": ["not a repo"]}),
            json!({"repos": ["a/b\nConfirm"]}),
            json!({"repos": [1]}),
        ] {
            assert!(chosen_from(&bad).is_err(), "{bad}");
        }
        let many: Vec<String> = (0..51).map(|i| format!("acme/r{i}")).collect();
        let past = chosen_from(&json!({"repos": many})).unwrap_err();
        assert_eq!(past, "You can tick at most 50 repositories.", "a rule's 50");
        for bad in [json!({"repos": "*"}), json!({"repos": ["not a repo"]})] {
            assert!(!chosen_from(&bad).unwrap_err().contains('*'), "{bad}");
        }
        assert_eq!(repo_names(&chosen()), "acme/app and acme/web");
        assert_eq!(
            repo_names(&["a/a".to_owned(), "b/b".to_owned(), "c/c".to_owned()]),
            "a/a, b/b and c/c"
        );
        assert_eq!(Scope::These(vec!["a/a".to_owned()]).in_words(), "in a/a");
    }

    /// What said every repository goes at the next start and they're asked once more; what they wrote
    /// stays, and a yes to the card is left alone.
    #[test]
    fn peer_39_a_yes_to_every_repository_is_asked_again() {
        let before: Sharing = serde_json::from_value(json!({"offered": 7, "rules": [
            {"id": "s-000000000001", "for": "*", "repos": "*", "tells": ["jobStatus", "jobProgress"],
             "created": 0, "uses": 3, "lastUsed": null, "starter": true},
            {"id": "s-000000000002", "for": "*", "repos": "*", "tells": ["jobStatus"],
             "created": 0, "uses": 3, "lastUsed": null, "starter": false, "roomYes": true},
            {"id": "s-000000000003", "for": ["ernesto"], "repos": "*", "tells": ["jobStatus"],
             "created": 0, "uses": 3, "lastUsed": null, "starter": false},
        ]}))
        .unwrap();
        let mut after = before.clone();
        assert!(after.retire_every_repository_yes());
        assert_eq!(after.rules.len(), 1);
        assert_eq!(after.rules[0].id, "s-000000000003", "their own wider rule");
        assert_eq!(after.rules[0].tells, [JOB_STATUS], "not widened either");
        assert_eq!(after.offered, None, "asked once more");
        assert!(!after.retire_every_repository_yes(), "once");

        let mut own = Sharing {
            rules: vec![rule(
                json!({"for": "*", "repos": "*", "tells": ["jobStatus"]}),
            )],
            offered: Some(7),
            ..Sharing::default()
        };
        assert!(!own.retire_every_repository_yes(), "nothing of obid's here");
        assert_eq!(own.offered, Some(7));

        let mut chose = Sharing::default();
        chose.share_chosen(1, chosen());
        chose.share_room(2);
        assert!(!chose.retire_every_repository_yes(), "they chose these");
        assert_eq!(chose.rules.len(), 2);
    }

    /// Nothing about joining makes a rule: the file stays as it was, and a member from before and a
    /// new one reach the same card.
    #[test]
    fn peer_40_no_file_shares_nothing_and_is_asked() {
        let home = std::env::temp_dir().join(format!("obid-peer-27-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let kept = Kept::open(&home);
        let none = kept.load();
        assert!(none.rules.is_empty() && none.to_offer());
        assert!(kept.reads(), "nothing kept yet");
        // A file that doesn't read shares nothing.
        std::fs::write(home.join("sharing.json"), "not json").unwrap();
        assert!(Kept::open(&home).load().rules.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn peer_16_a_file_from_before_is_asked_once_and_an_unreadable_one_is_left_alone() {
        let old: Sharing = serde_json::from_value(json!({"rules": []})).unwrap();
        assert!(old.to_offer(), "an existing member's file: never asked");
        let mut asked = old;
        asked.offered = Some(1);
        assert!(!asked.to_offer());
        let kept: Sharing = serde_json::from_slice(&serde_json::to_vec(&asked).unwrap()).unwrap();
        assert!(!kept.to_offer(), "across a restart");
        assert!(
            !Sharing {
                rules: vec![rule(
                    json!({"for": ["ernesto"], "repos": "*", "tells": ["jobStatus"]})
                )],
                ..Sharing::default()
            }
            .to_offer(),
            "a rule of their own, for one person: nothing to ask"
        );

        let home = std::env::temp_dir().join(format!("obid-peer-16-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let kept = Kept::open(&home);
        assert!(kept.reads(), "nothing kept yet");
        std::fs::write(home.join("sharing.json"), "not json").unwrap();
        assert!(!kept.reads(), "obid writes nothing of its own over it");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn peer_17_the_owners_yes_is_one_rule_that_answers_who_asks_and_opens_no_room() {
        let owners = ["ernesto".to_owned()];
        let repos = ["acme/app".to_owned()];
        let mut sharing = Sharing::default();
        let rule = sharing.share_chosen(5, chosen());
        assert_eq!(sharing.offered, Some(5));
        assert_eq!(
            rule.tells,
            [JOB_STATUS, JOB_PROGRESS],
            "what the card lists, and no more"
        );
        assert!(sharing.job_status_rule("ernesto", &repos).is_some());
        assert!(!sharing.job_status_for_all(&owners, &repos), "not the room");
        assert_eq!(rule.to_json()["starter"], true);
        // Twice is still one, and the first time asked stays.
        assert_eq!(sharing.share_chosen(9, chosen()).id, rule.id);
        assert_eq!((sharing.rules.len(), sharing.offered), (1, Some(5)));
    }

    /// A yes isn't widened silently: a rule of theirs that told status keeps telling status, through
    /// a restart, until their yes to the one card that lists the rest.
    #[test]
    fn peer_23_only_a_yes_adds_a_jobs_progress_to_a_rule_from_before() {
        let before: Sharing = serde_json::from_value(json!({"rules": [{
            "id": "s-000000000001", "for": "*", "repos": "*", "tells": ["jobStatus"],
            "created": 0, "uses": 3, "lastUsed": null, "starter": false}]}))
        .unwrap();
        assert!(!before.rules[0].has(JOB_PROGRESS), "nothing widens it");
        assert!(before.to_offer(), "the one time obid asks");
        let mut no = before.clone();
        no.offered = Some(1);
        assert!(!no.to_offer() && !no.rules[0].has(JOB_PROGRESS), "a no");

        let mut yes = before.clone();
        let narrow = rule(json!({"for": ["ernesto"], "repos": ["a/b"], "tells": ["jobStatus"]}));
        let results = rule(json!({"for": "*", "repos": "*", "tells": ["sharedResults"]}));
        yes.rules.extend([narrow, results]);
        let widened = yes.share_progress_too(9).unwrap();
        assert_eq!(widened.id, "s-000000000001", "the rule they have");
        assert_eq!(widened.uses, 3);
        assert_eq!(yes.rules.len(), 3, "no new rule");
        assert!(yes.rules[1].has(JOB_PROGRESS), "they're among everyone");
        assert!(!yes.rules[2].has(JOB_PROGRESS), "it told no status");
        assert!(!yes.to_offer(), "not asked twice");
        assert_eq!(
            Sharing::default().share_progress_too(1),
            None,
            "no rule for it"
        );
        // With a yes behind it, a first-time owner isn't asked a second time either.
        let mut new = Sharing::default();
        new.share_chosen(1, chosen());
        new.offered = None;
        assert!(!new.to_offer(), "it has it already");
        assert_eq!(new.rules[0].tells, [JOB_STATUS, JOB_PROGRESS]);
        // Leaving takes back the yes, off a rule they wrote too, and their own list stays theirs.
        let mine = rule(json!({"for": "*", "repos": "*", "tells": ["jobStatus"]}));
        let listed =
            rule(json!({"for": ["ernesto"], "repos": "*", "tells": ["jobStatus", "jobProgress"]}));
        let mut left = Sharing {
            rules: vec![mine, listed],
            ..Sharing::default()
        };
        left.share_progress_too(1);
        assert!(left.rules[0].has(JOB_PROGRESS));
        let kept: Sharing = serde_json::from_slice(&serde_json::to_vec(&left).unwrap()).unwrap();
        assert!(kept.rules[0].progress_yes, "across a restart");
        left.forget_the_offer();
        assert_eq!(left.rules[0].tells, [JOB_STATUS]);
        assert_eq!(left.rules[1].tells, [JOB_STATUS, JOB_PROGRESS]);
        assert!(left.offered.is_none());
        // The room's has it not, on top of status: a post goes to everyone unasked.
        let mut room = Sharing::default();
        room.share_chosen(0, chosen());
        assert_eq!(room.share_room(1).unwrap().tells, [JOB_STATUS]);
    }

    #[test]
    fn rooms_17_the_owners_yes_to_the_room_is_one_rule_on_top_of_sharing_status() {
        let owners = ["ernesto".to_owned()];
        let repos = ["acme/app".to_owned()];
        let mut sharing = Sharing::default();
        assert!(sharing.share_room(1).is_none(), "no status shared yet");
        assert!(sharing.rules.is_empty());
        sharing.share_chosen(1, chosen());
        assert!(!sharing.job_status_for_all(&owners, &repos), "not yet");
        let rule = sharing.share_room(2).unwrap();
        assert!(sharing.job_status_for_all(&owners, &repos));
        assert!(
            !sharing.job_status_for_all(&owners, &["acme/private".to_owned()]),
            "only the repositories they chose"
        );
        assert!(rule.for_this_sphere());
        assert_eq!(rule.to_json()["starter"], false);
        assert_eq!(sharing.share_room(3).unwrap().id, rule.id, "one");
        assert_eq!(sharing.rules.len(), 2);
        let kept: Sharing = serde_json::from_slice(&serde_json::to_vec(&sharing).unwrap()).unwrap();
        assert!(kept.rules[1].room_yes, "kept across a restart");
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
