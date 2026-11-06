//! What the brain may decide (Engine/Brain.swift, `Plan`): the actions Oby's tools (super::tools)
//! take, each with its fields, and why obid refuses one (`Plan.problems`, the same refusal texts).
//! There is no merge, push, approve or publish action, and obid checks every plan before anything
//! happens for it.
//! before anything happens for it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How long a reply may be: a few short sentences, which the Mac also speaks.
pub const REPLY_LENGTH: usize = 400;
/// The most options a question offers.
pub const MOST_OPTIONS: usize = 3;
/// How many jobs one plan starts, at most.
pub const MOST_DISPATCHES: usize = 10;
/// The most findings one job is handed (#540).
pub const MOST_FINDINGS: usize = 10;
/// How long a job's `done` may be: one line.
pub const DONE_LENGTH: usize = 300;
/// How long one finding's note may be.
pub const NOTE_LENGTH: usize = 500;

/// The actions, as the schema names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Dispatch,
    StopAgent,
    /// The chat acts on jobs and the inbox (#300).
    RetryJob,
    AnswerInbox,
    /// The user tells a job something (#542).
    MessageJob,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Dispatch => "dispatch",
            Action::StopAgent => "stop_agent",
            Action::RetryJob => "retry_job",
            Action::AnswerInbox => "answer_inbox",
            Action::MessageJob => "message_job",
        }
    }
}

/// The kinds of work. A kind the model makes up is an investigation, as in Swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", from = "String")]
pub enum Kind {
    Review,
    Fix,
    Build,
    Test,
    Investigate,
    Research,
}

impl From<String> for Kind {
    fn from(raw: String) -> Kind {
        match raw.as_str() {
            "review" => Kind::Review,
            "fix" => Kind::Fix,
            "build" => Kind::Build,
            "test" => Kind::Test,
            "research" => Kind::Research,
            _ => Kind::Investigate,
        }
    }
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Review,
        Kind::Fix,
        Kind::Build,
        Kind::Test,
        Kind::Investigate,
        Kind::Research,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Review => "review",
            Kind::Fix => "fix",
            Kind::Build => "build",
            Kind::Test => "test",
            Kind::Investigate => "investigate",
            Kind::Research => "research",
        }
    }

    /// Work that commits (`Job.committing`).
    pub fn commits(self) -> bool {
        matches!(self, Kind::Fix | Kind::Build | Kind::Test)
    }
}

/// How hard the work looks: a complex job gets a stronger model offered, never applied on its own,
/// and runs at its account's complex-task effort when the owner set one (#1574).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    #[default]
    Routine,
    Complex,
}

/// A job's permission mode, as a plan names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[serde(rename = "ask")]
    Ask,
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    #[serde(rename = "plan")]
    Plan,
    #[serde(rename = "bypass")]
    Bypass,
}

impl Mode {
    pub fn to_permissions(self) -> crate::permissions::Mode {
        match self {
            Mode::Ask => crate::permissions::Mode::Ask,
            Mode::AcceptEdits => crate::permissions::Mode::AcceptEdits,
            Mode::Plan => crate::permissions::Mode::Plan,
            Mode::Bypass => crate::permissions::Mode::Bypass,
        }
    }

    pub fn as_str(self) -> &'static str {
        self.to_permissions().as_str()
    }
}

fn auto() -> String {
    "auto".into()
}

fn one() -> i64 {
    1
}

/// One job of a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dispatch {
    #[serde(default = "auto")]
    pub agent: String,
    #[serde(default = "auto")]
    pub account: String,
    pub repo: String,
    #[serde(default, alias = "baseBranch")]
    pub base_branch: Option<String>,
    /// The branch the work goes on, when the owner or the repository's instructions named one.
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub pr: Option<i64>,
    /// The issue the work is about. obid reads it for the agent, who has no GitHub login.
    #[serde(default)]
    pub issue: Option<i64>,
    /// An epic: one job per open sub-issue.
    #[serde(default)]
    pub epic: Option<i64>,
    pub kind: Kind,
    /// A short title for the job, 3 to 6 words; none from rules.
    #[serde(default)]
    pub title: Option<String>,
    pub task: String,
    #[serde(default)]
    pub inputs: Vec<String>,
    /// How many rounds the agent may take: 1 unless the user asked to keep going until done. 1 is
    /// obid's own default: 3 for a fix, build or test with a check (#958), else 1.
    #[serde(default = "one")]
    pub rounds: i64,
    /// A one-line command that must succeed for the work to count as done.
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub complexity: Complexity,
    /// What must happen before the job starts (#216). obid can't wait yet (#455): it refuses any.
    #[serde(default, alias = "startWhen")]
    pub start_when: Option<Vec<Value>>,
    /// How the job asks (#134): only when the user said; none for Settings' default.
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Other repositories a research job must read (never writable).
    #[serde(default)]
    pub reads: Option<Vec<String>>,
    /// What this turn's lookups showed, for the agent to start from (#540): up to 10, each a path
    /// a lookup showed, its lines and a note.
    #[serde(default)]
    pub findings: Option<Vec<Finding>>,
    /// One line saying what must be true when the job is finished (#540).
    #[serde(default)]
    pub done: Option<String>,
}

pub use crate::job::Finding;

impl Dispatch {
    /// What the job's agent is handed from the turn's research, or none.
    pub fn brief(&self) -> Option<crate::job::Brief> {
        let findings = self.findings.clone().unwrap_or_default();
        let done = self
            .done
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_owned);
        (!findings.is_empty() || done.is_some()).then_some(crate::job::Brief { findings, done })
    }
}

/// A question for the user: at most 3 options, and the user may answer in words of their own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub text: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub details: Option<Vec<String>>,
    /// The index of the option the asker recommends, or none.
    #[serde(default)]
    pub recommended: Option<i64>,
    #[serde(default)]
    pub plan: Option<String>,
}

/// One plan: one action and its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub action: Action,
    pub reply: String,
    /// One job per independent piece of work, up to 10; empty unless the action is dispatch.
    pub dispatches: Vec<Dispatch>,
    /// The jobs an action names, by id from the briefing.
    pub jobs: Option<Vec<String>>,
    /// Stop everything, for stop_agent: only when the user clearly asked for all of it.
    pub all: Option<bool>,
    pub message: Option<String>,
    /// answer_inbox's item by id, and its answer: an option by its number from 1, or words.
    pub item: Option<String>,
    pub option: Option<i64>,
    pub answer: Option<String>,
}

impl Plan {
    /// A plan of `action` with nothing else in it.
    pub fn of(action: Action, reply: &str) -> Plan {
        Plan {
            action,
            reply: reply.into(),
            dispatches: vec![],
            jobs: None,
            all: None,
            message: None,
            item: None,
            option: None,
            answer: None,
        }
    }

    pub fn named(&self) -> &[String] {
        self.jobs.as_deref().unwrap_or(&[])
    }
}

/// What the brain may choose from (`BrainContext`): the accounts of each tool, the repositories,
/// the briefing and the quoted file, and the ids each action may name.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Each tool's accounts by name, `auto` first, and `auto` for any tool.
    pub accounts: BTreeMap<String, Vec<String>>,
    pub repos: Vec<String>,
    /// Handles for untrusted inputs. None yet.
    pub handles: Vec<String>,
    /// The jobs the brain may stop by id: the briefing's queued and running ones.
    pub jobs: Vec<String>,
    /// The jobs the brain may move or read by id: every job in the briefing.
    pub movable: Vec<String>,
    /// What a job may move to: each tool's models, and its accounts by name.
    pub models: BTreeMap<String, Vec<String>>,
    pub move_accounts: BTreeMap<String, Vec<String>>,
    /// The jobs whose permission prompt waits for the user, by id.
    pub permissions: Vec<String>,
    /// Pull requests Oby authored (`owner/name#n`) and the jobs that own their review.
    pub review_authors: BTreeMap<String, String>,
    /// The inbox items waiting for the user, by id.
    pub inbox: Vec<String>,
    /// The user's own words this turn, where "new job", "from scratch" or "start over" is read
    /// (#541). Never the brain's restatement.
    pub said: String,
    /// What this turn's lookups found, their titles and results: a finding's path must be in it
    /// (#540).
    pub found: String,
    /// The briefing's jobs that failed, were stopped or ran out of rounds in the last two days:
    /// more work on their issue or pull request goes on in them (#541).
    pub ended: Vec<Ended>,
}

/// A job of the briefing that ended without finishing its work, and what it was about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ended {
    pub id: String,
    pub repo: String,
    pub issue: Option<i64>,
    pub pr: Option<i64>,
}

/// Whether the user asked for a fresh job in so many words (#541): then work on the issue of a
/// job that failed is a new job, not that job's next round.
pub fn starts_over(said: &str) -> bool {
    let words = said
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    [
        "new job",
        "from scratch",
        "start over",
        "start it over",
        "start again from",
        "fresh job",
    ]
    .iter()
    .any(|phrase| format!(" {words} ").contains(&format!(" {phrase} ")))
}

fn count(text: &str) -> usize {
    text.chars().count()
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// A pull request number is only unique inside its repository.
pub fn review_key(repo: &str, pr: i64) -> String {
    format!("{repo}#{pr}")
}

impl Plan {
    /// Why obid refuses this plan; empty when it's fine. `canary` is the turn's own word, which
    /// no plan may repeat.
    pub fn problems(&self, context: &Context, canary: &str) -> Vec<String> {
        let mut problems: Vec<String> = vec![];
        let action = self.action;
        if count(&self.reply) > REPLY_LENGTH {
            problems.push(format!("reply is longer than {REPLY_LENGTH} characters"));
        }
        let mut texts: Vec<&str> = vec![&self.reply];
        texts.extend(self.dispatches.iter().map(|d| d.task.as_str()));
        texts.extend(self.dispatches.iter().filter_map(|d| d.title.as_deref()));
        texts.extend(self.dispatches.iter().filter_map(|d| d.done.as_deref()));
        texts.extend(
            self.dispatches
                .iter()
                .flat_map(|d| d.findings.iter().flatten())
                .map(|f| f.note.as_str()),
        );
        if !canary.is_empty() && texts.iter().any(|t| t.contains(canary)) {
            problems.push("the plan repeats the canary".into());
        }

        if action == Action::Dispatch {
            if self.dispatches.is_empty() {
                problems.push("dispatch needs at least one job in dispatches".into());
            }
            if self.dispatches.len() > MOST_DISPATCHES {
                problems.push("dispatches holds at most 10 jobs".into());
            }
            for dispatch in &self.dispatches {
                for problem in dispatch_problems(dispatch, context) {
                    if !problems.contains(&problem) {
                        problems.push(problem);
                    }
                }
            }
        } else if !self.dispatches.is_empty() {
            problems.push(format!("{} has no dispatches", action.as_str()));
        }
        // Only some actions name jobs, and only ones the briefing listed. Stopping everything is
        // said in so many words, never by naming none.
        let named = self.named();
        match action {
            Action::StopAgent => {
                for id in named.iter().filter(|id| !context.jobs.contains(id)) {
                    problems.push(format!("{id} isn't a job Oby can stop"));
                }
                if named.is_empty() && self.all != Some(true) {
                    problems.push(
                        "stop_agent names the jobs to stop, or all is true for everything".into(),
                    );
                }
                if !named.is_empty() && self.all == Some(true) {
                    problems.push("stop_agent names jobs or stops all, not both".into());
                }
            }
            Action::MessageJob => {
                if named.len() != 1 {
                    problems.push("message_job names one job".into());
                }
                for id in named.iter().filter(|id| !context.movable.contains(id)) {
                    problems.push(format!("{id} isn't a job Oby can message"));
                }
                if self.message.as_deref().is_none_or(blank) {
                    problems.push("message_job needs what Oby found and what to change".into());
                }
            }
            Action::RetryJob => {}
            Action::Dispatch | Action::AnswerInbox => {
                if !named.is_empty() {
                    problems.push("only stop_agent, retry_job and message_job name jobs".into());
                }
            }
        }
        problems.extend(self.job_action_problems(context));
        if action != Action::StopAgent && self.all == Some(true) {
            problems.push("only stop_agent stops all".into());
        }
        if count(self.message.as_deref().unwrap_or("")) > 2000 {
            problems.push("message is longer than 2,000 characters".into());
        }
        problems
    }

    /// Why obid refuses the job actions' fields (Engine/JobActions.swift).
    fn job_action_problems(&self, context: &Context) -> Vec<String> {
        let mut problems: Vec<String> = vec![];
        let named = self.named();
        let action = self.action;
        match action {
            Action::RetryJob | Action::MessageJob => {
                if named.is_empty() || named.len() > 10 {
                    problems.push(format!("{} names 1 to 10 jobs", action.as_str()));
                }
                for id in named.iter().filter(|id| !context.movable.contains(id)) {
                    problems.push(format!("{id} isn't a job in the briefing"));
                }
                if action == Action::MessageJob {
                    if named.len() != 1 {
                        problems.push("message_job names one job".into());
                    }
                    if self.message.as_deref().is_none_or(blank) {
                        problems.push("message_job needs a message".into());
                    }
                    if count(self.message.as_deref().unwrap_or("")) > 2000 {
                        problems.push("message_job is longer than 2,000 characters".into());
                    }
                }
            }
            Action::AnswerInbox => {
                if !named.is_empty() {
                    problems.push("answer_inbox names an inbox item, not jobs".into());
                }
                if !context
                    .inbox
                    .contains(self.item.as_ref().unwrap_or(&String::new()))
                {
                    problems.push("answer_inbox names an item waiting in the inbox".into());
                }
                let words = self.answer.as_deref().unwrap_or("").trim();
                if self.option.is_none() && words.is_empty() {
                    problems.push("answer_inbox gives an option or the user's words".into());
                }
                if self.option.is_some() && !words.is_empty() {
                    problems
                        .push("answer_inbox gives an option or the user's words, not both".into());
                }
                if let Some(option) = self.option
                    && option < 1
                {
                    problems.push("option counts from 1".into());
                }
            }
            Action::Dispatch | Action::StopAgent => {}
        }
        if action != Action::AnswerInbox
            && (self.item.is_some() || self.option.is_some() || self.answer.is_some())
        {
            problems.push("only answer_inbox has an item, an option or an answer".into());
        }
        if action != Action::MessageJob && self.message.as_deref().is_some_and(|m| !m.is_empty()) {
            problems.push("only message_job has a message".into());
        }
        if count(self.answer.as_deref().unwrap_or("")) > 2000 {
            problems.push("answer is longer than 2,000 characters".into());
        }
        problems
    }
}

/// What's wrong with one job of a plan.
fn dispatch_problems(dispatch: &Dispatch, context: &Context) -> Vec<String> {
    let mut problems = vec![];
    match context.accounts.get(&dispatch.agent) {
        Some(accounts) => {
            if !accounts.contains(&dispatch.account) {
                problems.push(format!(
                    "{} isn't an account of {}",
                    dispatch.account, dispatch.agent
                ));
            }
        }
        None => problems.push(format!("{} isn't connected", dispatch.agent)),
    }
    if !context.repos.contains(&dispatch.repo) {
        problems.push(format!("{} isn't a known repository", dispatch.repo));
    }
    if blank(&dispatch.task) {
        problems.push("task is empty".into());
    }
    if count(&dispatch.task) > 2000 {
        problems.push("task is longer than 2,000 characters".into());
    }
    if let Some(title) = &dispatch.title
        && !is_title(title)
    {
        problems.push("title must be one line of 1 to 60 characters".into());
    }
    if dispatch.pr.is_some_and(|pr| pr <= 0) {
        problems.push("pr must be a positive number".into());
    }
    if dispatch.kind == Kind::Review
        && let Some(pr) = dispatch.pr
        && let Some(author) = context.review_authors.get(&review_key(&dispatch.repo, pr))
    {
        problems.push(format!(
            "PR #{pr} has Oby author job {author}; use ask_review_again"
        ));
    }
    if dispatch.issue.is_some_and(|issue| issue <= 0) {
        problems.push("issue must be a positive number".into());
    }
    // The agent that wrote the code is the one to fix it (#541): it knows what it tried. A
    // review stays a fresh pair of eyes.
    if dispatch.kind.commits()
        && !starts_over(&context.said)
        && let Some(ended) = context.ended.iter().find(|e| {
            e.repo == dispatch.repo
                && ((dispatch.issue.is_some() && e.issue == dispatch.issue)
                    || (dispatch.pr.is_some() && e.pr == dispatch.pr))
        })
    {
        let about = match (dispatch.issue, dispatch.pr) {
            (Some(issue), _) if ended.issue == Some(issue) => format!("issue #{issue}"),
            (_, Some(pr)) => format!("PR #{pr}"),
            _ => "that work".to_owned(),
        };
        problems.push(format!(
            "{about} has job {id}, which failed, was stopped or ran out of rounds; use retry_job on {id} (a new job only when the user says new job, from scratch or start over)",
            id = ended.id
        ));
    }
    // Both is an issue's work started from a pull request's head (#219): work that commits.
    if dispatch.pr.is_some() && dispatch.issue.is_some() && !dispatch.kind.commits() {
        problems.push("a job is about a pr or an issue, not both, unless it's a fix, build or test of the issue that starts from the pr".into());
    }
    if let Some(epic) = dispatch.epic {
        if epic <= 0 {
            problems.push("epic must be a positive number".into());
        }
        if dispatch.pr.is_some() || dispatch.issue.is_some() {
            problems.push("an epic job has no pr or issue: Oby finds its sub-issues".into());
        }
    }
    let most = i64::from(crate::settings::limits().rounds());
    if !(1..=most).contains(&dispatch.rounds) {
        problems.push(format!("rounds must be 1 to {most}"));
    }
    if let Some(check) = &dispatch.check
        && !is_check(check)
    {
        problems.push("check must be one line of 1 to 200 characters".into());
    }
    if let Some(base) = &dispatch.base_branch
        && !is_branch_name(base)
    {
        problems.push("base_branch isn't a branch name".into());
    }
    if let Some(branch) = &dispatch.branch
        && !is_branch_name(branch)
    {
        problems.push("branch isn't a branch name".into());
    }
    if dispatch.kind == Kind::Research {
        if let Some(reads) = &dispatch.reads {
            if reads.len() > 5 {
                problems.push("reads holds at most 5 repositories".into());
            }
            if reads.iter().collect::<BTreeSet<_>>().len() != reads.len() {
                problems.push("reads has duplicate repositories".into());
            }
            if reads.contains(&dispatch.repo) {
                problems.push("reads lists the job's repository".into());
            }
            if let Some(unknown) = reads.iter().find(|r| !context.repos.contains(r)) {
                problems.push(format!("{unknown} isn't a known repository"));
            }
        }
    } else if dispatch.reads.as_ref().is_some_and(|r| !r.is_empty()) {
        problems.push("reads is only available for research jobs".into());
    }
    if let Some(unknown) = dispatch
        .inputs
        .iter()
        .find(|i| !context.handles.contains(i))
    {
        problems.push(format!("input {unknown} doesn't exist"));
    }
    // What obid's jobs can't carry yet is refused, never dropped (#546).
    if dispatch.epic.is_some() {
        problems.push(
            "epic is null: obid can't make one job per sub-issue yet; name the issues instead"
                .into(),
        );
    }
    if dispatch.reads.as_ref().is_some_and(|r| !r.is_empty()) {
        problems
            .push("reads is null: obid's research jobs can't read other repositories yet".into());
    }
    problems.extend(brief_problems(dispatch, context));
    // obid can't hold a job until something happens yet (#455).
    if dispatch.start_when.as_ref().is_some_and(|c| !c.is_empty()) {
        problems.push("start_when is empty: obid can't wait to start a job yet".into());
    }
    problems
}

/// What's wrong with what a job is handed from the turn's research (#540).
fn brief_problems(dispatch: &Dispatch, context: &Context) -> Vec<String> {
    let mut problems = vec![];
    let findings = dispatch.findings.as_deref().unwrap_or(&[]);
    if findings.len() > MOST_FINDINGS {
        problems.push(format!("findings holds at most {MOST_FINDINGS}"));
    }
    for finding in findings {
        let path = finding.path.as_str();
        if !is_relative_path(path) {
            problems.push("a finding's path must be relative, at most 300 characters, and contain no .. segment or leading slash or hyphen".into());
        } else if !context.found.contains(path) {
            problems.push(format!("path {path} isn't in what you read"));
        }
        if let Some(lines) = &finding.lines
            && !is_line_range(lines)
        {
            problems.push("a finding's lines are a line or a range, like 120 or 120-134".into());
        }
        let note = finding.note.trim();
        if note.is_empty() || count(note) > NOTE_LENGTH || note.contains(['\n', '\r']) {
            problems.push(format!(
                "a finding's note is one line of 1 to {NOTE_LENGTH} characters"
            ));
        }
    }
    if let Some(done) = &dispatch.done {
        let done = done.trim();
        if done.is_empty() || count(done) > DONE_LENGTH || done.contains(['\n', '\r']) {
            problems.push(format!("done is one line of 1 to {DONE_LENGTH} characters"));
        }
    }
    problems
}

/// A path inside a repository, as lookups take them.
fn is_relative_path(path: &str) -> bool {
    !path.is_empty()
        && count(path) <= 300
        && !path.starts_with('/')
        && !path.starts_with('-')
        && !path.split('/').any(|piece| piece == "..")
        && !path.contains(['\n', '\r'])
}

/// "120" or "120-134".
fn is_line_range(lines: &str) -> bool {
    let number = |n: &str| !n.is_empty() && n.len() <= 7 && n.bytes().all(|b| b.is_ascii_digit());
    match lines.split_once('-') {
        Some((start, end)) => number(start) && number(end),
        None => number(lines),
    }
}

/// A branch name git accepts and Oby is happy to create: no option-looking or odd names.
pub fn is_branch_name(name: &str) -> bool {
    name.len() <= 100
        && !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("/.")
}

/// A command Oby will run to check the work: one line, short enough to read on the card, with no
/// control character but a tab (NUL among them).
pub fn is_check(command: &str) -> bool {
    let trimmed = command.trim_matches([' ', '\t']);
    !trimmed.is_empty()
        && count(trimmed) <= 200
        && !trimmed.chars().any(|c| c.is_control() && c != '\t')
}

/// A job's title: one line, short enough for a row on the iPhone and the notch.
pub fn is_title(title: &str) -> bool {
    let trimmed = title.trim_matches([' ', '\t']);
    !trimmed.is_empty() && count(trimmed) <= 60 && !trimmed.contains(['\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A stop of the jobs named, or of everything when none are.
    fn stop(ids: &[&str]) -> Plan {
        let mut plan = Plan::of(Action::StopAgent, "");
        plan.jobs = Some(ids.iter().map(|id| (*id).to_owned()).collect());
        plan.all = Some(ids.is_empty());
        plan
    }

    fn context() -> Context {
        Context {
            accounts: BTreeMap::from([("claude".into(), vec!["default".into()])]),
            repos: vec!["deskree-inc/oby".into()],
            jobs: vec!["ab12cd34".into()],
            ..Context::default()
        }
    }

    /// ConversationTests.plansNamingJobsAreChecked
    #[test]
    fn plans_naming_jobs_are_checked() {
        let context = context();
        assert!(stop(&["ab12cd34"]).problems(&context, "zq9").is_empty());
        assert_eq!(
            stop(&["zz"]).problems(&context, "zq9"),
            ["zz isn't a job Oby can stop"]
        );
        // Stopping everything is said in so many words, never by naming no job.
        assert!(stop(&[]).problems(&context, "zq9").is_empty());
        let mut bare = stop(&[]);
        bare.all = Some(false);
        assert_eq!(
            bare.problems(&context, "zq9"),
            ["stop_agent names the jobs to stop, or all is true for everything"]
        );
        let mut both = stop(&["ab12cd34"]);
        both.all = Some(true);
        assert_eq!(
            both.problems(&context, "zq9"),
            ["stop_agent names jobs or stops all, not both"]
        );
        let mut answer = Plan::of(Action::AnswerInbox, "");
        answer.all = Some(true);
        assert!(
            answer
                .problems(&context, "zq9")
                .contains(&"only stop_agent stops all".to_owned())
        );
        let mut long = stop(&[]);
        long.reply = "x".repeat(REPLY_LENGTH + 1);
        assert_eq!(
            long.problems(&context, "zq9"),
            ["reply is longer than 400 characters"]
        );
    }

    /// The canary of the turn never comes back in a plan.
    #[test]
    fn a_plan_repeating_the_canary_is_refused() {
        let mut plan = stop(&[]);
        plan.reply = "canary zq9".into();
        assert_eq!(
            plan.problems(&context(), "zq9"),
            ["the plan repeats the canary"]
        );
    }

    /// ConversationTests.reviewingAnObyAuthoredPullRequestAgainNamesItsAuthorJob
    #[test]
    fn reviewing_an_oby_authored_pull_request_again_names_its_author_job() {
        let context = Context {
            accounts: BTreeMap::from([("claude".into(), vec!["default".into()])]),
            repos: vec!["deskree-inc/oby".into(), "deskree-inc/another".into()],
            movable: vec!["author".into()],
            review_authors: BTreeMap::from([("deskree-inc/oby#80".into(), "author".into())]),
            ..Context::default()
        };
        let dispatch: Dispatch = serde_json::from_value(json!({
            "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "base_branch": null,
            "pr": 80, "issue": null, "epic": null, "kind": "review", "title": null, "task": "Review PR #80",
            "inputs": [], "rounds": 1, "check": null, "complexity": "routine", "start_when": null, "mode": null}))
        .unwrap();
        let mut standalone = Plan::of(Action::Dispatch, "");
        standalone.dispatches = vec![dispatch.clone()];
        assert_eq!(
            standalone.problems(&context, "zq9"),
            ["PR #80 has Oby author job author; use ask_review_again"]
        );
        let mut other = dispatch;
        other.repo = "deskree-inc/another".into();
        standalone.dispatches = vec![other];
        assert!(standalone.problems(&context, "zq9").is_empty());
    }

    /// JobActionTests.swiftChecksWhatTheBrainNames, with Swift's refusal texts.
    #[test]
    fn obid_checks_what_the_brain_names() {
        let context = Context {
            movable: vec!["fa11ed00".into()],
            inbox: vec!["fa11ed00-failed-1".into()],
            ..Context::default()
        };
        let retry = |ids: &[&str]| {
            let mut plan = Plan::of(Action::RetryJob, "");
            plan.jobs = Some(ids.iter().map(|id| (*id).to_owned()).collect());
            plan
        };
        let answer = |item: &str, option: Option<i64>, words: Option<&str>| {
            let mut plan = Plan::of(Action::AnswerInbox, "");
            plan.item = Some(item.into());
            plan.option = option;
            plan.answer = words.map(str::to_owned);
            plan
        };
        assert!(retry(&["fa11ed00"]).problems(&context, "c4n4ry").is_empty());
        assert_eq!(
            retry(&["nobody"]).problems(&context, "c4n4ry"),
            ["nobody isn't a job in the briefing"]
        );
        assert_eq!(
            retry(&[]).problems(&context, "c4n4ry"),
            ["retry_job names 1 to 10 jobs"]
        );
        assert!(
            answer("fa11ed00-failed-1", Some(1), None)
                .problems(&context, "c4n4ry")
                .is_empty()
        );
        assert!(
            answer("fa11ed00-failed-1", None, Some("try again"))
                .problems(&context, "c4n4ry")
                .is_empty()
        );
        assert_eq!(
            answer("nothing-1", Some(1), None).problems(&context, "c4n4ry"),
            ["answer_inbox names an item waiting in the inbox"]
        );
        assert_eq!(
            answer("fa11ed00-failed-1", None, None).problems(&context, "c4n4ry"),
            ["answer_inbox gives an option or the user's words"]
        );
        assert_eq!(
            answer("fa11ed00-failed-1", Some(0), Some("x")).problems(&context, "c4n4ry"),
            [
                "answer_inbox gives an option or the user's words, not both",
                "option counts from 1"
            ]
        );
        // The fields belong to answer_inbox alone.
        let mut stray = retry(&["fa11ed00"]);
        stray.item = Some("fa11ed00-failed-1".into());
        assert_eq!(
            stray.problems(&context, "c4n4ry"),
            ["only answer_inbox has an item, an option or an answer"]
        );
    }

    /// What obid's jobs can't carry is refused, never dropped (review on #551).
    #[test]
    fn epics_and_reads_are_refused_not_dropped() {
        let context = context();
        let mut plan = Plan::of(Action::Dispatch, "");
        plan.dispatches = vec![
            serde_json::from_value(json!({
            "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "epic": 30,
            "kind": "build", "task": "Work on epic #30"}))
            .unwrap(),
        ];
        assert_eq!(
            plan.problems(&context, "zq9"),
            ["epic is null: obid can't make one job per sub-issue yet; name the issues instead"]
        );
        plan.dispatches = vec![serde_json::from_value(json!({
            "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "reads": ["deskree-inc/oby"],
            "kind": "research", "task": "Compare"}))
        .unwrap()];
        assert!(plan.problems(&context, "zq9").contains(
            &"reads is null: obid's research jobs can't read other repositories yet".to_owned()
        ));
    }

    /// FinishFromPRTests.finishingAnIssueFromAPullRequestIsAValidFixBuildOrTest and
    /// theBrainIsToldToDispatchFromThePullRequestsHead.
    #[test]
    fn finishing_an_issue_from_a_pull_request_is_a_valid_fix_build_or_test() {
        let context = context();
        let finish = |kind: &str| {
            let mut plan = Plan::of(Action::Dispatch, "");
            plan.dispatches = vec![
                serde_json::from_value(json!({
                "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "pr": 124,
                "issue": 19, "kind": kind, "task": "Finish #19 from PR #124"}))
                .unwrap(),
            ];
            plan.problems(&context, "zq9")
        };
        for kind in ["fix", "build", "test"] {
            assert!(finish(kind).is_empty(), "{kind}");
        }
        assert_eq!(
            finish("review"),
            [
                "a job is about a pr or an issue, not both, unless it's a fix, build or test of the issue that starts from the pr"
            ]
        );
        assert!(!finish("investigate").is_empty());
    }

    /// #541: work on the issue or pull request of a job that failed, stopped or ran out of rounds
    /// goes on in that job, unless the user starts over; a review is still a fresh one.
    #[test]
    fn work_a_job_left_unfinished_goes_on_in_that_job() {
        let mut context = context();
        context.ended = vec![Ended {
            id: "va11e7".into(),
            repo: "deskree-inc/oby".into(),
            issue: Some(12),
            pr: Some(33),
        }];
        let dispatch = |kind: &str, fields: Value| {
            let mut job = json!({"agent": "claude", "account": "default", "repo": "deskree-inc/oby",
                                 "kind": kind, "task": "Fix the vault tests"});
            for (key, value) in fields.as_object().unwrap() {
                job[key] = value.clone();
            }
            let mut plan = Plan::of(Action::Dispatch, "");
            plan.dispatches = vec![serde_json::from_value(job).unwrap()];
            plan
        };
        context.said = "fix the vault job's failing tests".into();
        for kind in ["fix", "build", "test"] {
            assert_eq!(
                dispatch(kind, json!({"issue": 12})).problems(&context, "zq9"),
                [
                    "issue #12 has job va11e7, which failed, was stopped or ran out of rounds; use retry_job on va11e7 (a new job only when the user says new job, from scratch or start over)"
                ],
                "{kind}"
            );
        }
        assert_eq!(
            dispatch("fix", json!({"pr": 33})).problems(&context, "zq9"),
            [
                "PR #33 has job va11e7, which failed, was stopped or ran out of rounds; use retry_job on va11e7 (a new job only when the user says new job, from scratch or start over)"
            ]
        );
        // Someone else's review of the job's pull request, other issues and other repositories.
        assert!(
            dispatch("review", json!({"pr": 33}))
                .problems(&context, "zq9")
                .is_empty()
        );
        assert!(
            dispatch("investigate", json!({"issue": 12}))
                .problems(&context, "zq9")
                .is_empty()
        );
        assert!(
            dispatch("fix", json!({"issue": 13}))
                .problems(&context, "zq9")
                .is_empty()
        );
        assert!(
            dispatch("fix", json!({}))
                .problems(&context, "zq9")
                .is_empty()
        );
        context.repos.push("deskree-inc/other".into());
        assert!(
            dispatch("fix", json!({"issue": 12, "repo": "deskree-inc/other"}))
                .problems(&context, "zq9")
                .is_empty()
        );
        for words in [
            "Start the vault fix over from scratch",
            "make a new job for #12",
            "start over on the vault fix",
            "Start it over.",
        ] {
            context.said = words.into();
            assert!(
                dispatch("fix", json!({"issue": 12}))
                    .problems(&context, "zq9")
                    .is_empty(),
                "{words}"
            );
        }
        for words in [
            "fix the vault job",
            "renew jobs",
            "restart overnight",
            "start overflow fix",
        ] {
            assert!(!starts_over(words), "{words}");
        }
    }

    /// #540: a job is handed what this turn's lookups showed, and only that.
    #[test]
    fn findings_cite_only_what_the_turn_read() {
        let mut context = context();
        let briefed = |findings: Value, done: Value| {
            let mut plan = Plan::of(Action::Dispatch, "");
            plan.dispatches = vec![
                serde_json::from_value(json!({
                "agent": "claude", "account": "default", "repo": "deskree-inc/oby",
                "kind": "fix", "task": "Fix the vault", "findings": findings, "done": done}))
                .unwrap(),
            ];
            plan
        };
        let vault = json!([{"path": "Engine/Vault.swift", "lines": "120-134",
                            "note": "the key is read before unlock"}]);
        // Nothing read this turn: no path can be cited.
        assert_eq!(
            briefed(vault.clone(), json!(null)).problems(&context, "zq9"),
            ["path Engine/Vault.swift isn't in what you read"]
        );
        context.found =
            "Engine/Vault.swift\nread_file deskree-inc/oby@origin/dev Engine/Vault.swift (sha abc1234)\n120: let key".into();
        let plan = briefed(vault.clone(), json!("the vault tests pass"));
        assert!(plan.problems(&context, "zq9").is_empty());
        assert_eq!(
            plan.dispatches[0].brief(),
            Some(crate::job::Brief {
                findings: vec![Finding {
                    path: "Engine/Vault.swift".into(),
                    lines: Some("120-134".into()),
                    note: "the key is read before unlock".into(),
                }],
                done: Some("the vault tests pass".into()),
            })
        );
        assert_eq!(
            briefed(
                json!([{"path": "Engine/Keys.swift", "lines": null, "note": "n"}]),
                json!(null)
            )
            .problems(&context, "zq9"),
            ["path Engine/Keys.swift isn't in what you read"]
        );
        assert_eq!(
            briefed(
                json!([{"path": "../Vault.swift", "lines": "x", "note": " "}]),
                json!("two\nlines")
            )
            .problems(&context, "zq9"),
            [
                "a finding's path must be relative, at most 300 characters, and contain no .. segment or leading slash or hyphen",
                "a finding's lines are a line or a range, like 120 or 120-134",
                "a finding's note is one line of 1 to 500 characters",
                "done is one line of 1 to 300 characters",
            ]
        );
        let eleven: Vec<Value> = (0..11)
            .map(|_| json!({"path": "Engine/Vault.swift", "lines": "1", "note": "n"}))
            .collect();
        assert_eq!(
            briefed(json!(eleven), json!(null)).problems(&context, "zq9"),
            ["findings holds at most 10"]
        );
        assert_eq!(
            briefed(json!([]), json!("x".repeat(301))).problems(&context, "zq9"),
            ["done is one line of 1 to 300 characters"]
        );
        assert_eq!(
            briefed(json!([]), json!("canary zq9")).problems(&context, "zq9"),
            ["the plan repeats the canary"]
        );
        // A dispatch with no research keeps working as before, with nothing to hand over.
        let plain = briefed(json!(null), json!(null));
        assert!(
            plain
                .problems(
                    &Context {
                        found: String::new(),
                        ..context.clone()
                    },
                    "zq9"
                )
                .is_empty()
        );
        assert_eq!(plain.dispatches[0].brief(), None);
    }

    /// #542: the user may tell a job of the briefing something, one whose pull request merged too
    /// (it gets a follow-up, the owner's decision of 2026-10-18).
    #[test]
    fn the_user_may_message_a_job_of_the_briefing() {
        let mut context = context();
        context.movable = vec!["ab12cd34".into(), "33aa33aa".into()];
        let message = |ids: &[&str], text: Option<&str>| {
            let mut plan = Plan::of(Action::MessageJob, "");
            plan.jobs = Some(ids.iter().map(|id| (*id).to_owned()).collect());
            plan.message = text.map(str::to_owned);
            plan
        };
        assert!(
            message(&["ab12cd34"], Some("skip the Watch target"))
                .problems(&context, "zq9")
                .is_empty()
        );
        assert!(
            !message(&["ab12cd34"], None)
                .problems(&context, "zq9")
                .is_empty()
        );
        assert!(
            message(&["zz"], Some("hi"))
                .problems(&context, "zq9")
                .contains(&"zz isn't a job Oby can message".to_owned())
        );
        assert!(
            message(&["ab12cd34", "33aa33aa"], Some("hi"))
                .problems(&context, "zq9")
                .contains(&"message_job names one job".to_owned())
        );
        assert!(
            message(&["33aa33aa"], Some("one more thing"))
                .problems(&context, "zq9")
                .is_empty()
        );
    }

    #[test]
    fn unknown_kinds_are_investigations_and_branch_names_are_checked() {
        let dispatch: Dispatch =
            serde_json::from_value(json!({"repo": "r", "kind": "audit", "task": "t"})).unwrap();
        assert_eq!(dispatch.kind, Kind::Investigate);
        for good in ["dev", "oby/fix-12", "release-1.2"] {
            assert!(is_branch_name(good), "{good}");
        }
        for bad in [
            "-x", "/x", ".x", "x/", "x.", "x.lock", "a..b", "a//b", "a/.b", "a b",
        ] {
            assert!(!is_branch_name(bad), "{bad}");
        }
    }
}
