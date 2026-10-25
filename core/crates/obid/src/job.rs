//! A job: one piece of work, where it runs and how it went (docs/runner-api.md, "Job"), as obid
//! keeps it and as the apps read it. The JSON carries every key the Swift runner's does
//! (Runner/API.swift, `JobJSON`), null where nothing is known, plus obid's own `obi` and `worktree`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `queued`, `running`, `done`, `failed`, `stopped` or `interrupted`. On obid, `interrupted` is a
/// job that needs the owner: it stopped part way, its work kept on its branch (`reason`, `nextStep`);
/// `failed` is a dead end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
    Interrupted,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Queued => "queued",
            Status::Running => "running",
            Status::Done => "done",
            Status::Failed => "failed",
            Status::Stopped => "stopped",
            Status::Interrupted => "interrupted",
        }
    }

    /// Queued or running.
    pub fn is_active(self) -> bool {
        matches!(self, Status::Queued | Status::Running)
    }
}

/// A next step: another round of a job that stopped at one of obid's limits.
pub const ONE_MORE_ROUND: &str = "One more round";
/// A next step: the agent's session picked up again where it stopped.
pub const TRY_AGAIN: &str = "Try again";
/// How the reason of a job obid's own bug (a panic in its task) ended starts (#914).
pub const CRASHED: &str = "obid hit a bug while running this job, so it stopped it: ";
/// The reason of a job obid kept going down with (#915): it isn't picked up again by itself.
pub const LOOPED: &str = "obid went down 3 times in 30 minutes while this ran, so Oby left it here. Try again carries it on.";

/// The Review stage once Oby's review approved the pull request's head.
pub const APPROVED_BY_OBY: &str = "Approved by Oby";

/// The kinds of work.
pub const KINDS: [&str; 6] = ["review", "fix", "build", "test", "investigate", "research"];

/// The kinds that only read: they run in Plan mode, whatever the default mode. An investigation
/// isn't one: it may run a test or a build to reproduce a bug (the owner's decision of 2026-10-18).
pub const READ_ONLY: [&str; 1] = ["review"];

/// The permission modes (docs/runner-api.md, "Permission modes").
pub const MODES: [&str; 5] = ["ask", "acceptEdits", "plan", "bypass", "sandbox"];

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The pipeline stage a job is in, as it last was (Engine/Pipeline.swift, `JobStage.ID`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Prepare,
    Work,
    Check,
    Publish,
}

/// How long what obid said about publishing a job by hand counts: the two minutes an approval is
/// fresh.
const OFFERS_KEPT: i64 = 120;

/// What obid said a publish by hand would do: when, and of which run (its `finished`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub run: Option<i64>,
    pub at: i64,
}

/// Why a queued job isn't running yet (`waiting`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub kind: String,
    pub text: String,
    /// Waiting for room while only these accounts' weekly reserves stand in the way (#872), by id:
    /// the apps offer "Use Personal anyway" on each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved: Option<Vec<String>>,
    /// The tool a job waiting for room asked for (`claude`, `codex`, `cursor`), or none: then any
    /// tool's account with room starts it, wherever it waits now (#872).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

impl Waiting {
    /// Whether it waits for an account with room (#872): it takes no slot meanwhile.
    pub fn for_room(&self) -> bool {
        self.reserved.is_some()
    }
}

/// On a review job another Obi asked for: the exchange it answers (runner-api.md, "obid only:
/// Obi-to-Obi review jobs").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObiRef {
    pub correlation_id: String,
    pub from: String,
    pub from_owner: String,
    pub request_id: String,
}

/// One thing Oby found before the job started (#540): a file a lookup showed, the lines, and
/// what Oby saw there or ruled out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub path: String,
    #[serde(default)]
    pub lines: Option<String>,
    pub note: String,
}

/// What the brain's research handed the job (#540): where to look and what done means. Written
/// from what repositories, GitHub and the web said, so its agent reads it as quoted data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brief {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<String>,
}

/// Why and where a stalled job stopped, with its handoff note (docs/runner-api.md, `stalled`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stalled {
    /// `quiet`, `loop`, `turns`, `ceiling` or `budget`.
    pub why: String,
    /// The same as the job's `limit`.
    pub text: String,
    pub at: i64,
    pub round: u32,
    pub branch: String,
    /// The branch's head when it stalled, and the job's own commits.
    pub commit: Option<String>,
    pub commits: u64,
    /// A session is kept to carry on in (its id stays obid's).
    pub session: bool,
    /// The handoff note, quoted as data, secrets taken out; `agent` or `obid` wrote it.
    pub note: String,
    pub note_by: String,
}

/// A teammate's Mac a job's agent runs on: their Obi, and its owner as the directory says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsOn {
    pub obi: String,
    pub owner: String,
    /// On a share of that teammate's API account (#809): its grant. The job's model calls are
    /// theirs, on their Mac.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<String>,
}

/// Oby's latest verdict on a job's published work (docs/runner-api.md, "Oby's review").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// `approved` or `changesRequested`.
    pub verdict: String,
    /// The review job.
    pub job: String,
    /// The reviewer's tool and account name.
    pub agent: String,
    pub account: String,
    /// It ran on the author's own account: no other had room.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub same_account: bool,
    /// Which of Oby's reviews of the job it is, from 1.
    pub round: u32,
    pub date: i64,
    /// The head it read: the pull request's, as obid pushed it.
    pub commit: String,
}

impl Review {
    pub fn approved(&self) -> bool {
        self.verdict == "approved"
    }

    fn to_json(&self) -> Value {
        let mut value = json!({"verdict": self.verdict, "job": self.job, "agent": self.agent,
                               "account": self.account, "round": self.round,
                               "date": obi_protocol::time::format(self.date), "commit": self.commit});
        if self.same_account {
            value["sameAccount"] = json!(true);
        }
        value
    }
}

/// What deleting a job's build outputs freed, in bytes, and when, in seconds since the epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildFreed {
    pub bytes: u64,
    pub at: i64,
}

/// A job as obid keeps it (`home/jobs/<id>.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    /// The user's own words.
    pub request: String,
    pub agent: String,
    pub account: String,
    pub repo: String,
    pub branch: String,
    pub base: Option<String>,
    pub pr: Option<u64>,
    pub kind: String,
    pub task: String,
    pub title: Option<String>,
    pub status: Status,
    pub step: String,
    pub summary: String,
    pub session: Option<String>,
    /// Seconds since the epoch.
    pub created: i64,
    pub started: Option<i64>,
    pub finished: Option<i64>,
    pub commits: Option<u64>,
    /// The pull request's URL once published.
    #[serde(default)]
    pub published: Option<String>,
    /// How many of `commits` the pull request has, or none before it's published.
    #[serde(default)]
    pub pushed: Option<u64>,
    pub publish_error: Option<String>,
    /// What publishing is doing now, while it publishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_step: Option<crate::publish::Step>,
    /// Everything obid sent out for the job, oldest first: ids only.
    #[serde(default)]
    pub outgoing: Vec<crate::publish::Outgoing>,
    /// The published pull request as obid last read it from GitHub.
    #[serde(default)]
    pub pull_request: Option<crate::github::PullRequest>,
    /// Why the last read of it failed, or none.
    #[serde(default)]
    pub pull_request_error: Option<String>,
    /// Why no review ran or none gave a verdict, or none.
    #[serde(default)]
    pub review_error: Option<String>,
    /// Oby's review job of its published work while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewing: Option<String>,
    /// Oby's latest verdict on its published work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
    /// On Oby's review job: the job whose published work it reviews.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_of: Option<String>,
    /// Why Oby's review of its published work waits: no account fits to run it on (none signed in
    /// with room, never Cursor). It starts once one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_reviewer: Option<String>,
    pub summaries: Option<Vec<String>>,
    pub issue: Option<u64>,
    pub rounds: Option<u32>,
    pub round: Option<u32>,
    pub check: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub notes: Option<Vec<String>>,
    pub files: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
    pub limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stalled: Option<Stalled>,
    pub start_commit: Option<String>,
    pub mode: Option<String>,
    pub waiting: Option<Waiting>,
    pub phase: Option<Phase>,
    /// The phase a restart cut it short in, when it was checking or publishing: its next run picks
    /// up there, its check run again rather than its agent's turn. Kept in its file, never in the
    /// API; taken when that run starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<Phase>,
    /// When obid picked it up again after going down while it ran, other than by a clean stop
    /// (#915): the last 30 minutes' only. Kept in its file, never in the API; emptied when the job
    /// ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crash_resumes: Vec<i64>,
    pub dismissed: Option<i64>,
    /// The job's worktree, an absolute path, once it exists.
    pub worktree: Option<String>,
    pub obi: Option<ObiRef>,
    /// The teammate's Mac its agent runs on (crate::sphere::guest), or none: this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs_on: Option<RunsOn>,
    /// The conversation whose plan started it, or none (#493). It keeps the id when that
    /// conversation is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    /// The user's turn of `conversation` whose words started it, so the conversation shows its
    /// card under them the moment it starts, before Oby's answer lists it. None for a job no turn
    /// started directly (a follow-up told from its page, a Start, an event turn's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    /// What an Obi-to-Obi review job made: the result body for its exchange
    /// (runner-api.md, "obid only: Obi-to-Obi review jobs", step 7). Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_result: Option<Value>,
    /// On an Obi-to-Obi review: whether this Obi's own bar for approving it on GitHub passed,
    /// whatever conditions the request listed: its own check passed and its agent reported no
    /// blocker, kept in the result or dropped (crate::sphere::approval). Never in the API's job.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub approvable: bool,
    /// On an Obi-to-Obi review: the request as quoted to the agent, `{from, from_owner, type,
    /// body}`, the body exactly as received. Data, never in a prompt. Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obi_request: Option<Value>,
    /// The permission its agent waits on now, for the owner's OK (docs/runner-api.md, "Permission
    /// modes").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<crate::permissions::Permission>,
    /// The agent's question call `permission` asks from (#858), with the owner's answers so far:
    /// what a restart keeps. Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_call: Option<crate::permissions::QuestionCall>,
    /// The latest Mac checks, kept on the job and sent on its JSON/event (`runner-api.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_check: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apple_check: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apple_ran: Option<Value>,
    /// Per-job bearer for `POST` and `GET /v1/agent/apple-check`; never sent to the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apple_token: Option<String>,
    /// What obid answered for the owner by its own rules (permissions.rs `Answered`): the latest
    /// few, with `answered_count` for all of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answered: Vec<crate::permissions::Answered>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub answered_count: u32,
    /// What obid saw of the agent and Tetrix, through obid's own Tetrix server (crate::tetrix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tetrix: Option<crate::tetrix::Use>,
    /// Teammates' open pull requests on the job's files (crate::teammates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teammates: Option<crate::teammates::Teammates>,
    /// Calls an agent made through the user's connected apps; names only, never secret values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connection_uses: Vec<crate::connections::ConnectionUse>,
    /// The connected apps the job started with (names, URLs and secret names, never a value):
    /// what its broker offers for as long as the job lives, across rounds, messages and restarts.
    /// Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<crate::connections::Connection>>,
    /// The owner's words (`POST /v1/jobs/{id}/messages`) the agent hasn't heard yet: its next
    /// turn's prompt. Never in the API's job (`notes` has them all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heard: Vec<String>,
    /// When obid said what publishing it by hand would do (`GET /v1/jobs/{id}/publish`), and
    /// which run it described (that run's `finished`): the last two minutes' worth. Never in the
    /// API's job.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub offers: Vec<Offer>,
    /// This round is one the owner asked for with their words: it may end `done` with nothing
    /// committed. Never in the API's job.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asked: bool,
    /// What Oby found before it started, for the first prompt (#540). Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<Brief>,
    /// What its `.obi/artifacts/` was published as, one per entry (crate::artifacts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<JobArtifact>,
    /// Why an entry of its `.obi/artifacts/` wasn't published, as of its last end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_problems: Vec<String>,
    /// The job this one follows up: the owner told that one something after it ended (Sangam's
    /// design, the owner's decision of 2026-10-18).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up_of: Option<String>,
    /// Its activity at a glance, from its transcript (`crate::activity`, #730).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<Value>,
    /// The agent's latest words, shortened: what it's doing now and why (#730).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Why it stopped without ending done, in obid's words, apart from the agent's own summary;
    /// or, ended done, what it didn't do (nothing committed). None while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The one next step of a job that needs the owner (`interrupted`), as its button says it:
    /// [`ONE_MORE_ROUND`] or [`TRY_AGAIN`]; both send Try again's words. None when only the owner's
    /// own words, or Dismiss, can take it on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
    /// When obid tries a job its account's usage limit stopped again by itself (`interrupted`),
    /// in seconds since the epoch: a minute after the reset its tool named. None otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumes_at: Option<i64>,
    /// What the job it follows up did, for its first prompt, quoted there. Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earlier: Option<String>,
    /// The branch in obid's clone its worktree starts from instead of its base: the first job's,
    /// for a follow-up of work that isn't merged. Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_from: Option<String>,
    /// What the conversation that owns it did by itself each time it stalled
    /// (crate::brain::supervisor), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supervised: Vec<Supervision>,
    /// Its change against `start_commit` after each round, oldest first (#736): `diff` in the API.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<crate::facts::Diff>,
    /// Each run of its own check, oldest first, the running one live (#734).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<crate::facts::CheckRecord>,
    /// How its last publish's secret scan went (#735).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<crate::publish::Scanned>,
    /// Each stage's times, as the job went through it (#733). Finished jobs' are the history
    /// estimates read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_times: Vec<crate::facts::StageTime>,
    /// What it should take, from jobs like it (#733).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<crate::facts::Estimate>,
    /// What its turns used, in tokens and cost, as its tool reported them (#731).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::usage::Tokens>,
    /// The most resident memory its processes (agent, check and what they started) held at
    /// once, in bytes, as obid's meter read it while it ran (crate::meter); none before a reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_rss: Option<u64>,
    /// Of that, its agent's tree at its most, and its check's (they're learned apart: checks run
    /// one at a time); none before a reading. Kept as they grow, not only at the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_peak_rss: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_peak_rss: Option<u64>,
    /// What deleting its worktree's build outputs freed once it was done with, and when
    /// (crate::jobs, `free_build_outputs`); none before that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_freed: Option<BuildFreed>,
    /// Claude Code's session and its counts at its last result, which the next turn's resumed
    /// counts include (crate::jobs). Never in the API's job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_seen: Option<(String, crate::usage::Tokens)>,
    /// Its estimated share of its account's windows (#732).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_share: Option<crate::usage::WindowShare>,
    /// How long its agent has run, in seconds, every run and restart included, since the owner
    /// last let it past its ceiling: what `jobCeilingHours` holds (crate::jobs). Never in the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ran_secs: Option<u64>,
    /// What it had spent (crate::jobs, `spent`) when the owner last let it past its token budget:
    /// `jobTokenBudget` counts from there. Never in the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_from: Option<u64>,
    /// When it was archived (`POST /v1/jobs/{id}/archive`, `oby_archive_job`), or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<i64>,
    /// What Oby or the owner changed about it after it started, oldest first: its report's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
    /// Every time its tool or account was changed (`oby_change_job`), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<Move>,
    /// The owner named its account (`POST /v1/jobs`, `oby_change_job`): it stays there past a
    /// usage limit, never moved by Oby (#871). Kept in its file, never in the API.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub named: bool,
    /// The routine whose run started it (crate::routines), or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<RoutineMark>,
    /// The standing rule that started it with no tap (crate::rules, #1314), or none. Such a job
    /// runs in Sandbox mode at most, and publishes by itself as any job does (the owner's
    /// decision of 2026-10-24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<RuleMark>,
    /// The conversation that started it had read something it didn't write (a file, a command's
    /// output, the web, an attachment): it runs in Sandbox mode at most, as a standing rule's
    /// job, and publishes by itself as any job does (the owner's decision of 2026-10-24,
    /// replacing 2026-10-23's by hand).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub untrusted: bool,
    /// Why obid didn't merge its pull request, when only the owner can take it on from here
    /// (crate::merge); none otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_error: Option<String>,
    /// The merge stopped where the owner takes over: `merge_error` says why.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merge_needs_you: bool,
    /// Who had obid merge its pull request, and when; none until obid merged it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_by: Option<MergedBy>,
    /// What obid is sending for its merge (crate::merge), one mark per kind (an approval, the
    /// merge, the branch's deletion), saved before it's sent and cleared once obid knows how it
    /// went: a restart in between reads GitHub to settle each. Never in the API's job.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sending: Vec<Sending>,
}

/// Something obid sends for a job's merge, kept until it knows how it went (`Job::sending`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sending {
    /// `pullRequestApproval`, `merge` or `branchDelete`.
    pub action: String,
    /// The head it's about.
    pub head: String,
    /// For a merge: who had it merged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<MergedBy>,
    /// For an approval: the reviewer account's login it goes as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
}

/// Who had obid merge a job's pull request (crate::merge): the owner, with a tap, or the
/// repository's workflow the owner taught it (a `workflow` memory, #854).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedBy {
    /// `owner` or `memory`.
    pub by: String,
    /// The workflow memory it followed, for `memory`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// Seconds since the epoch.
    pub at: i64,
}

/// Which standing rule started a job (#1314): the rule's id and type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleMark {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
}

/// Which routine's run a job is (docs/runner-api.md, "obid only: routines"): the routine, its
/// name when the run started, the time it was due, and whether it ran late.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineMark {
    pub id: String,
    pub name: String,
    /// When the run was due, in seconds since the epoch.
    pub scheduled: i64,
    pub catch_up: bool,
}

/// One decision of the conversation about a job of its that stalled (crate::brain::supervisor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supervision {
    pub at: i64,
    /// `continue`, `restart` or `split`; `asked` when it left the next step to the owner.
    pub decision: String,
    /// The jobs it started: the restart, or the split's parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
    /// Its turn in the conversation.
    pub turn: String,
    /// The automatic steps across the job's tree with this one; for `asked`, those taken before.
    pub count: usize,
}

/// One thing changed about a job after it started, in obid's own words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub what: String,
    pub date: i64,
}

/// A job moved to another tool or account: each "Claude Code · Personal".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub from: String,
    pub to: String,
    pub why: Option<String>,
    pub date: i64,
}

/// One artifact a job published: the entry of `.obi/artifacts/` it came from, so the job's next
/// end gives it a new version rather than a new artifact. The API shows `{id, version}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobArtifact {
    pub id: String,
    pub version: u32,
    pub entry: String,
}

impl Job {
    /// Whether its pull request was merged, as the job's `pullRequest` says: it's finished for
    /// good. obid follows no pull request yet, so none reads merged until it does.
    /// Whether its check keeps loopback (`sandbox::Spec::loopback`): only an owner's job whose
    /// agent ran here, so its check runs nothing that agent couldn't reach. Never another Obi's
    /// review, nor a job that ran on a teammate's Mac, whose commits came from there.
    pub fn check_keeps_loopback(&self) -> bool {
        self.obi.is_none() && self.runs_on.is_none()
    }

    pub fn merged(&self) -> bool {
        self.to_json()["pullRequest"]["state"] == "MERGED"
    }

    /// A new job, queued now.
    pub fn new(
        id: String,
        repo: String,
        kind: String,
        task: String,
        agent: String,
        account: String,
    ) -> Job {
        Job {
            id,
            request: task.clone(),
            agent,
            account,
            repo,
            branch: String::new(),
            base: None,
            pr: None,
            kind,
            task,
            title: None,
            status: Status::Queued,
            step: String::new(),
            summary: String::new(),
            session: None,
            created: obi_protocol::time::now(),
            started: None,
            finished: None,
            commits: None,
            published: None,
            pushed: None,
            publish_error: None,
            publish_step: None,
            outgoing: Vec::new(),
            pull_request: None,
            pull_request_error: None,
            review_error: None,
            reviewing: None,
            review: None,
            review_of: None,
            no_reviewer: None,
            summaries: None,
            issue: None,
            rounds: None,
            round: None,
            check: None,
            model: None,
            effort: None,
            notes: None,
            files: None,
            attachments: None,
            limit: None,
            stalled: None,
            start_commit: None,
            mode: None,
            waiting: None,
            phase: None,
            cut: None,
            crash_resumes: Vec::new(),
            dismissed: None,
            worktree: None,
            obi: None,
            runs_on: None,
            conversation: None,
            turn: None,
            review_result: None,
            approvable: false,
            obi_request: None,
            permission: None,
            question_call: None,
            mac_check: None,
            apple_check: None,
            apple_ran: None,
            apple_token: None,
            answered: Vec::new(),
            answered_count: 0,
            connection_uses: Vec::new(),
            tetrix: None,
            teammates: None,
            connections: None,
            heard: Vec::new(),
            offers: Vec::new(),
            asked: false,
            brief: None,
            artifacts: Vec::new(),
            artifact_problems: Vec::new(),
            follow_up_of: None,
            activity: None,
            context: None,
            reason: None,
            next_step: None,
            resumes_at: None,
            earlier: None,
            start_from: None,
            supervised: Vec::new(),
            diffs: Vec::new(),
            checks: Vec::new(),
            scan: None,
            stage_times: Vec::new(),
            estimate: None,
            archived: None,
            changes: Vec::new(),
            moves: Vec::new(),
            named: false,
            usage: None,
            peak_rss: None,
            agent_peak_rss: None,
            check_peak_rss: None,
            build_freed: None,
            usage_seen: None,
            window_share: None,
            routine: None,
            rule: None,
            untrusted: false,
            ran_secs: None,
            budget_from: None,
            merge_error: None,
            merge_needs_you: false,
            merged_by: None,
            sending: Vec::new(),
        }
    }

    /// What a Confirm about it is proposed against: its round, status, commits, mode, tool and
    /// account. Confirmed after any of them changed, it's refused.
    pub fn snapshot(&self) -> Value {
        json!({"round": self.current_round(), "status": self.status.as_str(),
               "commits": self.commits, "unpublished": self.unpublished(),
               "mode": self.mode, "agent": self.agent, "account": self.account})
    }

    /// Adds `what` to its changes, dated now.
    pub fn changed(&mut self, what: String) {
        self.changes.push(Change {
            what,
            date: obi_protocol::time::now(),
        });
    }

    /// obid said, at `now`, what publishing its latest run by hand would do.
    pub fn offered(&mut self, now: i64) {
        self.offers.retain(|offer| now - offer.at < OFFERS_KEPT);
        self.offers.push(Offer {
            run: self.finished,
            at: now,
        });
    }

    /// Whether a device's approval signed at `signed_at` can only be of the job's latest run: the
    /// run ended before it was signed, and nothing obid said in the two minutes before it was of
    /// an earlier run (one device may have shown that one while another asked again). The
    /// sentence a device signs names the branch, not its commits, so an approval that could be of
    /// either is refused; two minutes on, it can't be.
    pub fn approvable_at(&self, signed_at: i64) -> bool {
        self.finished.is_none_or(|ended| ended <= signed_at)
            && !self.offers.iter().any(|offer| {
                offer.at <= signed_at
                    && signed_at - offer.at < OFFERS_KEPT
                    && offer.run != self.finished
            })
    }

    /// The owner's review job: always in Plan mode. A review another Obi asked for
    /// runs its own way (runner-api.md, "obid only: Obi-to-Obi review jobs").
    pub fn read_only(&self) -> bool {
        READ_ONLY.contains(&self.kind.as_str()) && self.obi.is_none()
    }

    pub fn max_rounds(&self) -> u32 {
        self.rounds.unwrap_or(1).max(1)
    }

    pub fn current_round(&self) -> u32 {
        self.round.unwrap_or(1).max(1)
    }

    /// The last part of `owner/name`.
    pub fn repo_name(&self) -> &str {
        self.repo.rsplit('/').next().unwrap_or(&self.repo)
    }

    /// What the apps call it (Engine/Jobs.swift, `shortTitle`): its own title, else
    /// `<Kind> <repo name> #<number>`, else the task's first line, at most 60 characters.
    pub fn short_title(&self) -> String {
        if let Some(title) = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            return title.to_owned();
        }
        if let Some(number) = self.pr.or(self.issue) {
            let mut kind = self.kind.clone();
            if let Some(first) = kind.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            return format!("{kind} {} #{number}", self.repo_name());
        }
        let source = if self.task.is_empty() {
            &self.request
        } else {
            &self.task
        };
        let line = source.split('\n').next().unwrap_or("");
        if line.chars().count() > 60 {
            format!("{}…", line.chars().take(59).collect::<String>())
        } else {
            line.to_owned()
        }
    }

    /// Commits beyond its base that aren't on its pull request yet: all of them before it's
    /// published, then the ones later rounds made (Engine/Jobs.swift, `unpublished`). On a remote
    /// with no pull requests, the ones that aren't pushed; one whose pull request failed after the
    /// push has it still to open, so all of them (#627).
    pub fn unpublished(&self) -> u64 {
        let commits = self.commits.unwrap_or(0);
        let on_it = match (&self.published, &self.publish_error) {
            (Some(_), _) => self.pushed.unwrap_or(commits),
            (None, None) => self.pushed.unwrap_or(0),
            (None, Some(_)) => 0,
        };
        commits.saturating_sub(on_it)
    }

    /// Pushed, with no pull request to open: its remote has none, and it has every commit.
    pub fn pushed_only(&self) -> bool {
        self.published.is_none()
            && self.publish_error.is_none()
            && self.pushed.is_some_and(|n| n > 0)
            && self.unpublished() == 0
    }

    /// Whether its pull request, if it has one, is open as far as obid knows.
    pub fn pull_request_open(&self) -> bool {
        self.pull_request.as_ref().is_none_or(|pr| pr.is_open())
    }

    /// Done, with commits that aren't on its pull request yet: obid publishes it when it ends, and
    /// offers Publish when that failed. Not once dismissed, never on a review or research job, and
    /// never once its pull request was merged or closed.
    pub fn can_publish(&self) -> bool {
        self.status == Status::Done
            && self.kind != "research"
            && self.kind != "review"
            && self.base.is_some()
            && self.unpublished() > 0
            && self.dismissed.is_none()
            && self.pull_request_open()
    }

    /// Done with commits its pull request will never take: it was merged or closed before they
    /// went out.
    pub fn stranded(&self) -> bool {
        self.status == Status::Done
            && self.published.is_some()
            && self.unpublished() > 0
            && !self.pull_request_open()
    }

    /// The last commit obid pushed to the job's branch: its pull request's head, as obid left it.
    pub fn pushed_head(&self) -> Option<&str> {
        self.outgoing
            .iter()
            .rev()
            .find(|o| o.action == "push" && o.branch.as_deref() == Some(self.branch.as_str()))
            .map(|o| o.id.as_str())
    }

    /// It just published to `url`: an observation of another pull request no longer holds, nor why
    /// an earlier head got no review.
    pub fn was_published(&mut self, url: String) {
        if self.published.as_deref() != Some(url.as_str()) {
            self.pull_request = None;
        }
        self.pull_request_error = None;
        self.published = Some(url);
        self.publish_error = None;
        self.review_error = None;
    }

    /// The head Oby's review reads next, if one is due: done and published, its pull request
    /// open, no review under way, and none read this head yet (one review per published head).
    /// Never for a review.
    pub fn review_due(&self) -> Option<&str> {
        let head = self.pushed_head()?;
        (self.status == Status::Done
            && self.published.is_some()
            && self.kind != "review"
            && self.reviewing.is_none()
            && self.pull_request_open()
            && self.review.as_ref().is_none_or(|r| r.commit != head))
        .then_some(head)
    }

    /// Oby's verdict on the head its pull request has now, if it has one: an approval here is
    /// what posting it and merging go on from.
    pub fn current_review(&self) -> Option<&Review> {
        self.review
            .as_ref()
            .filter(|r| self.pushed_head() == Some(r.commit.as_str()))
    }

    /// Oby's review still asks for changes on the pull request's head, and nothing more happens by
    /// itself: its reviews ran out ([`crate::review::REVIEWS`]), or the round they asked for left
    /// the head as it was.
    pub fn review_needs_you(&self) -> bool {
        self.status == Status::Done
            && self.dismissed.is_none()
            && self.reviewing.is_none()
            && self.pull_request_open()
            && self.current_review().is_some_and(|r| !r.approved())
    }

    /// Whether its worktree's build outputs may still be there: none deleted since it last ended.
    pub fn builds_left(&self) -> bool {
        self.build_freed
            .as_ref()
            .is_none_or(|freed| self.finished.is_some_and(|end| end > freed.at))
    }

    /// Waiting for Publish (its own publish failed), with commits its pull request won't take,
    /// failed or interrupted, Oby's review left for the owner, or its merge stopped where only
    /// the owner takes it on (crate::merge), and not dismissed.
    pub fn needs_you(&self) -> bool {
        self.dismissed.is_none()
            && (self.can_publish()
                || self.stranded()
                || self.status == Status::Failed
                || self.status == Status::Interrupted
                || self.review_needs_you()
                || self.merge_needs_you)
    }

    /// The job as the runner API shows it.
    pub fn to_json(&self) -> Value {
        let date = |t: Option<i64>| t.map(obi_protocol::time::format);
        // Out of the object below, which is as big as json! takes.
        let supervised: Vec<Value> = self
            .supervised
            .iter()
            .map(|s| {
                json!({"at": obi_protocol::time::format(s.at), "decision": s.decision,
                       "jobs": s.jobs, "turn": s.turn, "count": s.count,
                       "of": crate::brain::supervisor::MOST})
            })
            .collect();
        // `title` is the derived one, always a string, as the Swift runner sends it.
        let mut value = json!({
            "id": self.id,
            "request": self.request,
            "agent": self.agent,
            "account": self.account,
            "repo": self.repo,
            "branch": self.branch,
            "base": self.base,
            "pr": self.pr,
            "kind": self.kind,
            "task": self.task,
            "status": self.status.as_str(),
            "step": self.step,
            "summary": self.summary,
            "session": self.session,
            "created": obi_protocol::time::format(self.created),
            "started": date(self.started),
            "finished": date(self.finished),
            "commits": self.commits,
            "published": self.published,
            "pushed": self.pushed,
            "publishError": self.publish_error,
            "summaries": self.summaries,
            "issue": self.issue,
            "fromPR": null,
            "rounds": self.max_rounds(),
            "round": self.current_round(),
            "check": self.check,
            "model": self.model,
            "effort": self.effort,
            "complexity": null,
            "notes": self.notes,
            "pullRequest": self.pull_request.as_ref().map(crate::github::PullRequest::to_json),
            "pullRequestError": self.pull_request_error,
            "dismissed": date(self.dismissed),
            "reviewing": self.reviewing,
            "review": self.review.as_ref().map(Review::to_json),
            "reviewError": self.review_error,
            "reviewOf": self.review_of,
            "files": self.files,
            "lead": null,
            "claim": null,
            "outgoing": self.outgoing.iter().map(crate::publish::Outgoing::to_json).collect::<Vec<_>>(),
            "question": null,
            "limit": self.limit,
            "stalled": self.stalled.as_ref().map(|s| json!({
                "why": s.why, "text": s.text, "at": date(Some(s.at)), "round": s.round,
                "branch": s.branch, "commit": s.commit, "commits": s.commits,
                "session": s.session, "note": s.note, "noteBy": s.note_by,
            })),
            "startCommit": self.start_commit,
            "firstSwiftBuildSeconds": null,
            "macCheck": mac_check_json(self.mac_check.clone(), "merge"),
            "mergeError": self.merge_error,
            "appleCheck": mac_check_json(self.apple_check.clone(), "apple"),
            "appleRan": mac_check_json(self.apple_ran.clone(), "apple"),
            "tetrix": self.tetrix.as_ref().map(crate::tetrix::Use::to_json),
            "moves": self.moves.iter().map(|m| json!({"from": m.from, "to": m.to, "why": m.why, "date": obi_protocol::time::format(m.date)})).collect::<Vec<_>>(),
            "archived": date(self.archived),
            "changes": self.changes.iter().map(|c| json!({"what": c.what, "date": obi_protocol::time::format(c.date)})).collect::<Vec<_>>(),
            "xcodeCloud": null,
            "xcodeCloudError": null,
            // The one start condition obid sets, by itself: the apps' "Use … anyway" shows on a job
            // waiting to start (#872).
            "startWhen": self.waiting.as_ref().filter(|w| self.status == Status::Queued && w.for_room())
                .map(|_| json!([{"room": true, "agent": self.agent}])),
            "waiting": self.waiting.as_ref().filter(|_| self.status == Status::Queued),
            "mode": self.mode.as_deref().unwrap_or("bypass"),
            "permission": self.permission,
            "answered": self.answered.iter().map(crate::permissions::Answered::to_json).collect::<Vec<_>>(),
            "answeredCount": self.answered_count,
            "connectionUses": self.connection_uses,
            "pipeline": self.pipeline(),
            "title": self.short_title(),
            "mergeNeedsYou": self.merge_needs_you,
            "reviewNeedsYou": self.review_needs_you(),
            "cloudNeedsYou": false,
            "report": false,
            "obi": self.obi,
            "worktree": self.worktree,
            "conversation": self.conversation,
            "turn": self.turn,
            "artifacts": self.artifacts.iter().map(|a| json!({"id": a.id, "version": a.version})).collect::<Vec<_>>(),
            "artifactProblems": self.artifact_problems,
            "followUpOf": self.follow_up_of,
            "routine": self.routine.as_ref().map(|r| json!({"id": r.id, "name": r.name,
                "scheduled": obi_protocol::time::format(r.scheduled), "catchUp": r.catch_up})),
            "supervised": supervised,
        });
        if let Some(activity) = &self.activity {
            value["activity"] = activity.clone();
        }
        // A standing rule's job (#1314) says which rule started it.
        if let Some(rule) = &self.rule {
            value["rule"] = json!({"id": rule.id, "type": rule.kind});
        }
        if self.untrusted {
            value["untrusted"] = json!(true);
        }
        if let Some(merged) = &self.merged_by {
            value["mergedBy"] = json!({"by": merged.by, "memory": merged.memory,
                                       "at": obi_protocol::time::format(merged.at)});
        }
        if let Some(why) = &self.no_reviewer {
            value["noReviewer"] = json!(why);
        }
        value["teammates"] = json!(self.teammates);
        if let Some(context) = &self.context {
            value["context"] = json!(context);
            // The agent's own words, quoted: never obid's.
            value["contextBy"] = json!("agent");
        }
        // Why it stopped and its one next step (#725), out of the literal above: json!'s recursion
        // limit.
        value["reason"] = json!(self.reason);
        // "on Wellington's Mac" on its card (Sangam's design, #808).
        value["runsOn"] = json!(self.runs_on.as_ref().map(|r| {
            let mut place = json!({"obi": r.obi, "owner": r.owner});
            if let Some(grant) = &r.grant {
                place["grant"] = json!(grant);
            }
            place
        }));
        value["nextStep"] = json!(self.next_step);
        value["resumesAt"] = json!(date(self.resumes_at));
        if let Some(step) = self.publish_step {
            value["publishStep"] = json!(step.as_str());
        }
        if let Some(usage) = &self.usage {
            value["usage"] = json!(usage);
        }
        if let Some(peak) = self.peak_rss {
            value["peakRss"] = json!(peak);
        }
        if let Some(peak) = self.agent_peak_rss {
            value["agentPeakRss"] = json!(peak);
        }
        if let Some(peak) = self.check_peak_rss {
            value["checkPeakRss"] = json!(peak);
        }
        if let Some(freed) = &self.build_freed {
            value["buildFreed"] = json!({"bytes": freed.bytes, "at": date(Some(freed.at))});
        }
        if let Some(share) = self.window_share.as_ref().and_then(|s| s.to_json()) {
            value["windowShare"] = share;
        }
        // The latest round's (#736); every round's is in `detail_json`. Absent before the first
        // round ends.
        if let Some(last) = self.diffs.last() {
            value["diff"] = json!(last);
        }
        // The latest run only, on every event (#734); every run's is in `detail_json`.
        if let Some(last) = self.checks.last() {
            value["checks"] = json!([last]);
        }
        // The scan's own record; else why it never ran: its checks failed, or publishing stopped
        // before it (#735).
        // A check that never passed ends needing the owner (`interrupted`, #725) or failed.
        let scan = if matches!(self.status, Status::Failed | Status::Interrupted)
            && self.phase == Some(Phase::Check)
        {
            Some(json!({"ran": false, "reason": "checks failed"}))
        } else if let Some(scan) = &self.scan {
            Some(json!(scan))
        } else {
            self.publish_error
                .as_ref()
                .map(|why| json!({"ran": false, "reason": why}))
        };
        if let Some(scan) = scan {
            value["scan"] = scan;
        }
        if let Some(estimate) = &self.estimate {
            value["estimate"] = json!({
                "low": estimate.low,
                "high": estimate.high,
                "basis": estimate.basis,
                "samples": estimate.samples,
                "firstGiven": estimate.first_given,
            });
        }
        value
    }

    /// The job as `GET /v1/jobs/{id}` shows it: [`Self::to_json`] with what's too big to send on
    /// every event: each round's diff stat (`diff.rounds`, #736) and every check run (#734).
    pub fn detail_json(&self) -> Value {
        let mut value = self.to_json();
        if value.get("diff").is_some() {
            value["diff"]["rounds"] = json!(self.diffs);
        }
        if value.get("checks").is_some() {
            value["checks"] = json!(self.checks);
        }
        value
    }

    /// Stamps its stages' times at `now` (#733): how the time since the last stamp counts follows
    /// the job as it was then. Whether a stage started or finished.
    pub fn stamp(&mut self, now: i64) -> bool {
        let pipeline = self.pipeline();
        let stages: Vec<(String, String)> = pipeline["stages"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                (
                    s["id"].as_str().unwrap_or("").to_owned(),
                    s["state"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        use crate::facts::Clock;
        let clock = if self.permission.is_some() {
            Clock::Owner
        } else if self.status == Status::Queued && self.waiting.is_some() {
            Clock::Idle
        } else if self.mode.as_deref() == Some("plan") && !self.read_only() {
            Clock::Planning
        } else {
            Clock::Running
        };
        crate::facts::stamp(&mut self.stage_times, &stages, clock, now)
    }

    /// The stages the apps draw (Engine/Pipeline.swift): prepare, work, the check, publish,
    /// review, and the merge, which obid leaves to a person (runner-api.md, "obid only: what obid
    /// leaves out"): the Merged stage is `done` only once GitHub says a person merged it.
    pub fn pipeline(&self) -> Value {
        let active = self.status.is_active();
        let stopped = matches!(
            self.status,
            Status::Failed | Status::Stopped | Status::Interrupted
        );
        let phase = self.phase.unwrap_or(
            if self.status == Status::Queued
                || (self.started.is_none()
                    && self.status != Status::Done
                    && self.published.is_none())
            {
                Phase::Prepare
            } else {
                Phase::Work
            },
        );
        let working = active && phase == Phase::Work;
        let failure = if self.summary.is_empty() {
            let mut status = self.status.as_str().to_owned();
            if let Some(first) = status.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            status
        } else {
            self.summary.clone()
        };
        let mut stages: Vec<Value> = vec![
            json!({"id": "prepare", "title": "Prepare", "state": "upcoming", "detail": "Clone and worktree"}),
            json!({"id": "work", "title": "Work", "state": "upcoming",
                   "detail": format!("Round {} of {}{}", self.current_round(), self.current_round().max(self.max_rounds()),
                                     if working && !self.step.is_empty() { format!(" · {}", self.step) } else { String::new() })}),
        ];
        if let Some(check) = self.check.as_deref().filter(|c| !c.trim().is_empty()) {
            stages.push(
                json!({"id": "check", "title": "Check", "state": "upcoming", "detail": check}),
            );
        }
        let merge_base = self
            .pull_request
            .as_ref()
            .map(|pr| pr.base.as_str())
            .or(self.base.as_deref())
            .unwrap_or("dev");
        stages.push(json!({"id": "publish", "title": "Publish", "state": "upcoming", "detail": "Secret scan, push and pull request"}));
        stages.push(json!({"id": "review", "title": "Review", "state": "upcoming", "detail": "Waiting for review"}));
        stages.push(json!({"id": "merged", "title": format!("Merged into {merge_base}"), "state": "upcoming"}));
        let id_of = |p: Phase| match p {
            Phase::Prepare => "prepare",
            Phase::Work => "work",
            Phase::Check => "check",
            Phase::Publish => "publish",
        };
        // A stage's state, its detail when given, and its reason (none unless given).
        fn set(
            stages: &mut [Value],
            id: &str,
            state: &str,
            detail: Option<&str>,
            reason: Option<&str>,
        ) {
            if let Some(stage) = stages.iter_mut().find(|s| s["id"] == id) {
                stage["state"] = json!(state);
                if let Some(detail) = detail {
                    stage["detail"] = json!(detail);
                }
                match reason {
                    Some(reason) => stage["reason"] = json!(reason),
                    None => {
                        if let Some(object) = stage.as_object_mut() {
                            object.remove("reason");
                        }
                    }
                }
            }
        }
        let phase_id = id_of(phase);
        let phase_index = stages.iter().position(|s| s["id"] == phase_id).unwrap_or(1);
        for stage in stages.iter_mut().take(phase_index) {
            stage["state"] = json!("done");
        }
        let can_publish = self.can_publish();
        if active {
            let detail =
                (phase == Phase::Prepare && !self.step.is_empty()).then_some(self.step.as_str());
            set(&mut stages, phase_id, "current", detail, None);
            if phase == Phase::Publish {
                let detail = self
                    .publish_step
                    .map_or("Secret scan", crate::publish::Step::detail);
                set(&mut stages, "publish", "current", Some(detail), None);
            }
        } else if self.status == Status::Interrupted && self.dismissed.is_none() {
            // It needs the owner where it stopped: the rest is still to come.
            let reason = self.reason.as_deref().unwrap_or(&failure);
            set(&mut stages, phase_id, "current", None, Some(reason));
        } else if stopped {
            set(&mut stages, phase_id, "failed", None, Some(&failure));
            for stage in stages.iter_mut().skip(phase_index + 1) {
                stage["state"] = json!("skipped");
            }
        } else {
            for id in ["prepare", "work", "check"] {
                set(&mut stages, id, "done", None, None);
            }
            if can_publish {
                let detail = self
                    .publish_step
                    .map_or("Ready to publish", crate::publish::Step::detail);
                let state = if self.publish_error.is_some() {
                    "failed"
                } else {
                    "current"
                };
                set(
                    &mut stages,
                    "publish",
                    state,
                    Some(detail),
                    self.publish_error.as_deref(),
                );
            } else if self.pushed_only() {
                set(
                    &mut stages,
                    "publish",
                    "done",
                    Some("Secret scan clean · pushed · no pull request for this remote"),
                    None,
                );
                for id in ["review", "merged"] {
                    set(&mut stages, id, "skipped", None, None);
                }
            } else if self.published.is_none() {
                for id in ["publish", "review", "merged"] {
                    set(&mut stages, id, "skipped", None, None);
                }
            }
        }
        // A pull request from an earlier round doesn't complete the current work, check or publish.
        if self.published.is_some() && !active && !stopped && !can_publish {
            set(
                &mut stages,
                "publish",
                "done",
                Some("Secret scan clean · pushed · pull request opened"),
                None,
            );
            let decision = self
                .pull_request
                .as_ref()
                .and_then(|pr| pr.review_decision.as_deref());
            let oby = self.current_review();
            // The Merged stage while it's open: why the merge waits for the owner, else GitHub.
            let merge_line = self
                .merge_error
                .as_deref()
                .filter(|_| self.merge_needs_you)
                .unwrap_or(crate::publish::MERGE_ON_GITHUB);
            // Merged by a workflow the owner taught Oby: it says so (#854).
            let as_you_like = self
                .merged_by
                .as_ref()
                .filter(|m| m.by == "memory")
                .map(|_| format!("Merged into {merge_base}, as you like"));
            match self.pull_request.as_ref().map(|pr| pr.state.as_str()) {
                Some("MERGED") => {
                    let detail = match (decision, oby) {
                        (Some("APPROVED"), _) => "Approved",
                        (Some("CHANGES_REQUESTED"), _) => "Review complete",
                        (_, Some(review)) if review.approved() => APPROVED_BY_OBY,
                        (_, Some(_)) => "Oby requested changes",
                        _ => "Merged without review",
                    };
                    set(&mut stages, "review", "done", Some(detail), None);
                    set(&mut stages, "merged", "done", as_you_like.as_deref(), None);
                }
                Some("CLOSED") => {
                    set(
                        &mut stages,
                        "review",
                        "failed",
                        None,
                        Some("Pull request closed without merging."),
                    );
                    set(&mut stages, "merged", "skipped", None, None);
                }
                _ => match decision {
                    Some("APPROVED") => {
                        set(&mut stages, "review", "done", Some("Approved"), None);
                        set(&mut stages, "merged", "current", Some(merge_line), None);
                    }
                    Some("CHANGES_REQUESTED") => {
                        set(
                            &mut stages,
                            "review",
                            "failed",
                            Some("Changes requested"),
                            Some("The reviewer requested changes."),
                        );
                    }
                    // No decision on GitHub: Oby's own review, else the owner's on GitHub.
                    _ => match (&self.reviewing, &self.review_error, oby) {
                        (Some(_), _, _) => {
                            set(
                                &mut stages,
                                "review",
                                "current",
                                Some("Oby is reviewing"),
                                None,
                            );
                        }
                        (None, Some(error), _) => {
                            let detail = format!("Waiting for review · {error}");
                            set(&mut stages, "review", "current", Some(&detail), None);
                        }
                        (None, None, Some(review)) if review.approved() => {
                            set(&mut stages, "review", "done", Some(APPROVED_BY_OBY), None);
                            set(&mut stages, "merged", "current", Some(merge_line), None);
                        }
                        (None, None, Some(_)) => {
                            set(
                                &mut stages,
                                "review",
                                "failed",
                                Some("Changes requested"),
                                Some("Oby requested changes."),
                            );
                        }
                        (None, None, None) => {
                            let detail = match &self.no_reviewer {
                                Some(why) => format!("Waiting for review · {why}"),
                                None => crate::publish::YOUR_REVIEW.into(),
                            };
                            set(&mut stages, "review", "current", Some(&detail), None);
                        }
                    },
                },
            }
        }
        // The owner let it go: nothing is published, reviewed or merged. A pull request from an
        // earlier round keeps its own progress; a failed publish keeps its reason.
        if self.dismissed.is_some()
            && !active
            && (self.published.is_none() || stopped)
            && let Some(publish) = stages.iter().position(|s| s["id"] == "publish")
        {
            stages[publish]["state"] = json!("skipped");
            stages[publish]["detail"] = json!("Dismissed");
            if (phase != Phase::Publish || !stopped)
                && let Some(object) = stages[publish].as_object_mut()
            {
                object.remove("reason");
            }
            for stage in stages.iter_mut().skip(publish + 1) {
                stage["state"] = json!("skipped");
            }
        }
        let position = stages
            .iter()
            .position(|s| s["state"] == "current" || s["state"] == "failed")
            .or_else(|| {
                stages
                    .iter()
                    .rposition(|s| s["state"] == "done" || s["state"] == "skipped")
            })
            .unwrap_or(0);
        let total = stages.len();
        for stage in stages.iter_mut() {
            let id = stage["id"].as_str().unwrap_or("").to_owned();
            if let Some(time) = self.stage_times.iter().find(|t| t.id == id) {
                stage["started"] = json!(obi_protocol::time::format(time.started));
                // Once it's no longer current: what a current one has taken changes every second,
                // so the apps count it from `started`.
                if let Some(finished) = time.finished.filter(|_| time.open.is_none()) {
                    stage["finished"] = json!(obi_protocol::time::format(finished));
                    stage["seconds"] = json!(time.seconds);
                }
                stage["waited"] = json!(time.waited);
            }
            let ahead = stage["state"] == "upcoming" || stage["state"] == "current";
            if let Some((_, range)) = self
                .estimate
                .iter()
                .flat_map(|e| e.stages.iter())
                .find(|(s, _)| *s == id)
                .filter(|_| ahead)
            {
                stage["estimate"] = json!(range);
            }
        }
        json!({"stages": stages, "number": position + 1, "total": total})
    }
}

fn mac_check_json(check: Option<Value>, kind: &str) -> Value {
    let Some(mut check) = check else {
        return Value::Null;
    };
    if check["kind"].is_null() {
        check["kind"] = json!(kind);
    }
    for key in [
        "started",
        "finished",
        "mac",
        "steps",
        "failures",
        "checkRun",
        "comment",
        "postError",
    ] {
        if check.get(key).is_none() {
            check[key] = Value::Null;
        }
    }
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> Job {
        let mut job = Job::new(
            "9f08fd10".into(),
            "deskree-inc/oby".into(),
            "review".into(),
            "Review pull request #33 in deskree-inc/oby…".into(),
            "claude".into(),
            "default".into(),
        );
        job.pr = Some(33);
        job.created = obi_protocol::time::parse("2026-10-09T21:42:53Z").unwrap();
        job
    }

    /// Only an owner's job whose agent ran here keeps loopback in its check: not another Obi's
    /// review, nor a job that ran on a teammate's Mac.
    #[test]
    fn only_an_owners_job_run_here_keeps_loopback_in_its_check() {
        let mut job = job();
        assert!(job.check_keeps_loopback());
        job.runs_on = Some(RunsOn {
            obi: "ernesto-mac".into(),
            owner: "ernesto".into(),
            grant: None,
        });
        assert!(!job.check_keeps_loopback());
        let mut review = self::job();
        review.obi = Some(ObiRef {
            correlation_id: "c".into(),
            from: "ernesto-mac".into(),
            from_owner: "ernesto".into(),
            request_id: "r".into(),
        });
        assert!(!review.check_keeps_loopback());
    }

    /// Ported from RunnerTests/RunnerTests.swift, `jobsHaveISODatesAndEveryKey`: every key the
    /// Swift runner sends, plus obid's `obi` and `worktree`.
    #[test]
    fn jobs_have_iso_dates_and_every_key() {
        let mut job = job();
        job.started = Some(job.created + 2);
        let value = job.to_json();
        let object = value.as_object().unwrap();
        assert_eq!(value["created"], "2026-10-09T21:42:53Z");
        assert_eq!(value["started"], "2026-10-09T21:42:55Z");
        for key in [
            "base",
            "session",
            "finished",
            "commits",
            "published",
            "pushed",
            "publishError",
            "summaries",
            "issue",
            "fromPR",
            "check",
            "model",
            "effort",
            "complexity",
            "notes",
            "pullRequest",
            "pullRequestError",
            "dismissed",
            "reviewing",
            "review",
            "reviewError",
            "reviewOf",
            "files",
            "lead",
            "claim",
            "question",
            "limit",
            "stalled",
            "startCommit",
            "firstSwiftBuildSeconds",
            "macCheck",
            "mergeError",
            "appleCheck",
            "appleRan",
            "tetrix",
            "xcodeCloud",
            "xcodeCloudError",
            "startWhen",
            "waiting",
            "permission",
            "conversation",
            "turn",
            "followUpOf",
            "reason",
            "nextStep",
            "resumesAt",
            "routine",
            "runsOn",
            "teammates",
        ] {
            assert_eq!(object.get(key), Some(&Value::Null), "{key}");
        }
        // The Swift runner's 65 keys, obid's eighteen, and `supervised`.
        assert_eq!(object.len(), 84);
        assert_eq!(value["supervised"], json!([]));
        assert_eq!(value["archived"], Value::Null);
        assert_eq!(value["changes"], json!([]));
        assert_eq!(value["artifacts"], json!([]));
        assert_eq!(value["artifactProblems"], json!([]));
        assert_eq!(value["answered"], json!([]));
        assert_eq!(value["answeredCount"], 0);
        assert_eq!(value["report"], false);
        assert_eq!(value["cloudNeedsYou"], false);
        assert_eq!(value["mode"], "bypass");
        assert_eq!(value["outgoing"], json!([]));
        assert_eq!(value["moves"], json!([]));
        assert_eq!(value["connectionUses"], json!([]));
        assert!(value["pipeline"].is_object());
        assert_eq!(value["title"], "Review oby #33");
        assert_eq!(value["rounds"], 1);
        assert_eq!(value["round"], 1);
        assert_eq!(value["kind"], "review");
        assert_eq!(value["pr"], 33);
        assert_eq!(value["status"], "queued");
        assert_eq!(value["obi"], Value::Null);
        assert_eq!(value["worktree"], Value::Null);
    }

    /// Each stage is timed as the job goes (#733): every round's time added up, and the time
    /// it waited on the owner kept apart, not in its seconds.
    #[test]
    fn stages_are_timed_and_waiting_on_the_owner_isnt_counted() {
        let mut job = job();
        job.kind = "fix".into();
        job.pr = None;
        job.check = Some("node --test".into());
        let t0 = job.created;
        job.stamp(t0);
        job.status = Status::Running;
        job.started = Some(t0 + 10);
        job.phase = Some(Phase::Work);
        assert!(job.stamp(t0 + 10));
        job.permission = Some(crate::permissions::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            title: "Run it".into(),
            detail: "it".into(),
            rule: None,
            always: None,
            why: None,
            question: None,
        });
        assert!(!job.stamp(t0 + 20));
        job.permission = None;
        job.stamp(t0 + 50);
        job.phase = Some(Phase::Check);
        job.stamp(t0 + 70);
        job.status = Status::Done;
        job.finished = Some(t0 + 80);
        job.stamp(t0 + 80);
        let time = |id: &str| job.stage_times.iter().find(|t| t.id == id).unwrap();
        assert_eq!((time("prepare").started, time("prepare").seconds), (t0, 10));
        let work = time("work");
        assert_eq!((work.started, work.finished), (t0 + 10, Some(t0 + 70)));
        assert_eq!((work.seconds, work.waited), (30, 30));
        assert_eq!(
            (time("check").seconds, time("check").finished),
            (10, Some(t0 + 80))
        );
        assert!(job.stage_times.iter().all(|t| t.id != "publish"));
        let stages = job.pipeline()["stages"].clone();
        assert_eq!(stages[1]["started"], obi_protocol::time::format(t0 + 10));
        assert_eq!(stages[1]["finished"], obi_protocol::time::format(t0 + 70));
        assert_eq!(
            (stages[1]["seconds"].clone(), stages[1]["waited"].clone()),
            (json!(30), json!(30))
        );
        assert!(stages[3].get("started").is_none());
    }

    #[test]
    fn titles() {
        let mut job = job();
        assert_eq!(job.short_title(), "Review oby #33");
        job.pr = None;
        job.task = format!("{}\nmore", "x".repeat(70));
        assert_eq!(job.short_title(), format!("{}…", "x".repeat(59)));
        job.title = Some("Review for ernesto: fixture-app".into());
        assert_eq!(job.short_title(), "Review for ernesto: fixture-app");
    }

    #[test]
    fn pipelines_follow_the_job() {
        let mut job = job();
        job.check = Some("node --test".into());
        let stages = |job: &Job| -> Vec<(String, String)> {
            job.pipeline()["stages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| {
                    (
                        s["id"].as_str().unwrap().to_owned(),
                        s["state"].as_str().unwrap().to_owned(),
                    )
                })
                .collect()
        };
        assert_eq!(stages(&job)[0], ("prepare".into(), "current".into()));
        job.status = Status::Running;
        job.started = Some(job.created);
        job.phase = Some(Phase::Check);
        assert_eq!(stages(&job)[2], ("check".into(), "current".into()));
        job.status = Status::Done;
        let done = stages(&job);
        assert!(done[..3].iter().all(|(_, s)| s == "done"));
        assert!(done[3..].iter().all(|(_, s)| s == "skipped"));
        job.status = Status::Failed;
        job.summary = "the check failed".into();
        let failed = job.pipeline();
        assert_eq!(failed["stages"][2]["state"], "failed");
        assert_eq!(failed["stages"][2]["reason"], "the check failed");
        assert_eq!(failed["number"], 3);
        // Needing the owner, it waits where it stopped: why, and the rest still to come.
        job.status = Status::Interrupted;
        job.reason = Some("`node --test` still fails after 1 round(s)".into());
        job.next_step = Some(ONE_MORE_ROUND.into());
        let needs = job.pipeline();
        assert_eq!(needs["stages"][2]["state"], "current");
        assert_eq!(
            needs["stages"][2]["reason"],
            "`node --test` still fails after 1 round(s)"
        );
        assert_eq!(needs["stages"][3]["state"], "upcoming");
        assert_eq!(job.to_json()["nextStep"], ONE_MORE_ROUND);
        assert!(job.needs_you());
    }

    /// Engine/Pipeline.swift for a published job, with obid's own rule: the Merged stage is done
    /// only once a person merged it (runner-api.md, "obid only: what obid leaves out").
    #[test]
    fn a_published_pipeline_follows_github_and_never_merges() {
        let mut job = job();
        job.kind = "fix".into();
        job.pr = None;
        job.base = Some("dev".into());
        job.status = Status::Done;
        job.started = Some(job.created);
        job.commits = Some(1);
        let state = |job: &Job, id: &str| -> (String, String) {
            let stage = job.pipeline()["stages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == id)
                .cloned()
                .unwrap();
            (
                stage["state"].as_str().unwrap().to_owned(),
                stage["detail"].as_str().unwrap_or("").to_owned(),
            )
        };
        // Done with a commit it couldn't publish: Publish failed, with why.
        job.publish_error = Some("Couldn't push".into());
        assert!(job.can_publish() && job.needs_you());
        assert_eq!(state(&job, "publish").0, "failed");
        assert_eq!(job.pipeline()["stages"][2]["reason"], "Couldn't push");
        // Pushed, then the pull request failed: still publishable, for the pull request (#627).
        job.pushed = Some(1);
        job.publish_error = Some("Pushed, but couldn't open the pull request: gh broke".into());
        assert!(job.can_publish() && !job.pushed_only());
        // Pushed to a remote with no pull requests: nothing left to publish.
        job.publish_error = None;
        assert!(job.pushed_only() && !job.can_publish() && !job.needs_you());
        assert_eq!(
            state(&job, "publish"),
            (
                "done".into(),
                "Secret scan clean · pushed · no pull request for this remote".into()
            )
        );
        assert_eq!(state(&job, "review").0, "skipped");
        // A new commit on it isn't pushed yet.
        job.commits = Some(2);
        assert!(job.can_publish() && !job.pushed_only());
        job.commits = Some(1);
        job.pushed = None;
        // Published: Review waits, Merged is upcoming.
        job.was_published("https://github.com/deskree-inc/oby/pull/42".into());
        job.pushed = Some(1);
        assert!(!job.can_publish() && !job.needs_you());
        assert_eq!(
            state(&job, "publish"),
            (
                "done".into(),
                "Secret scan clean · pushed · pull request opened".into()
            )
        );
        assert_eq!(
            state(&job, "review"),
            (
                "current".into(),
                "Pull request open · waiting for your review on GitHub".into()
            )
        );
        assert_eq!(state(&job, "merged").0, "upcoming");
        let pr = |state: &str, decision: Option<&str>| crate::github::PullRequest {
            state: state.into(),
            review_decision: decision.map(str::to_owned),
            base: "dev".into(),
            merged_at: (state == "MERGED").then_some(job.created),
            checked_at: job.created,
            head: None,
        };
        job.pull_request = Some(pr("OPEN", Some("APPROVED")));
        assert_eq!(state(&job, "review"), ("done".into(), "Approved".into()));
        assert_eq!(
            state(&job, "merged"),
            ("current".into(), "Merge it on GitHub".into())
        );
        job.pull_request = Some(pr("OPEN", Some("CHANGES_REQUESTED")));
        assert_eq!(
            state(&job, "review"),
            ("failed".into(), "Changes requested".into())
        );
        job.pull_request = Some(pr("MERGED", None));
        assert_eq!(
            state(&job, "review"),
            ("done".into(), "Merged without review".into())
        );
        assert_eq!(state(&job, "merged").0, "done");
        job.pull_request = Some(pr("CLOSED", None));
        assert_eq!(state(&job, "review").0, "failed");
        assert_eq!(state(&job, "merged").0, "skipped");
        // A later round's commits the closed pull request won't take: stranded, and it needs you.
        job.commits = Some(2);
        assert!(job.stranded() && job.needs_you() && !job.can_publish());
        // Its last push is its pull request's head.
        job.outgoing.push(crate::publish::Outgoing::new(
            "push",
            &job.repo,
            "abc",
            Some("oby/other"),
        ));
        job.branch = "oby/fix-42".into();
        job.outgoing.push(crate::publish::Outgoing::new(
            "push",
            &job.repo,
            "def",
            Some("oby/fix-42"),
        ));
        assert_eq!(job.pushed_head(), Some("def"));
    }

    /// Oby's review is due once per published head, and never while one runs (#525). Its verdict shows on the head it read, and changes it
    /// still asks for there, with nothing more under way, need the owner.
    #[test]
    fn oby_reviews_each_published_head_once() {
        let mut job = job();
        job.kind = "fix".into();
        job.pr = None;
        job.base = Some("dev".into());
        job.branch = "oby/fix-42".into();
        job.status = Status::Done;
        job.started = Some(job.created);
        job.commits = Some(1);
        job.pushed = Some(1);
        assert_eq!(job.review_due(), None);
        let review = |job: &Job| {
            job.pipeline()["stages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["id"] == "review")
                .cloned()
                .unwrap()
        };
        let push = |job: &mut Job, head: &str| {
            job.outgoing.push(crate::publish::Outgoing::new(
                "push",
                &job.repo,
                head,
                Some("oby/fix-42"),
            ));
        };
        push(&mut job, "abc");
        job.was_published("https://github.com/deskree-inc/oby/pull/42".into());
        assert_eq!(job.review_due(), Some("abc"));
        job.reviewing = Some("r1".into());
        assert_eq!(job.review_due(), None);
        assert_eq!(review(&job)["detail"], "Oby is reviewing");
        job.reviewing = None;
        let created = job.created;
        let verdict = |verdict: &str, commit: &str| Review {
            verdict: verdict.into(),
            job: "r1".into(),
            agent: "codex".into(),
            account: "Work".into(),
            same_account: false,
            round: 1,
            date: created,
            commit: commit.into(),
        };
        job.review = Some(verdict("changesRequested", "abc"));
        assert_eq!(job.review_due(), None);
        assert!(job.review_needs_you() && job.needs_you());
        assert_eq!(job.to_json()["reviewNeedsYou"], true);
        assert_eq!(review(&job)["state"], "failed");
        // Working on it, it doesn't need the owner; its new head is due.
        job.status = Status::Running;
        assert!(!job.review_needs_you());
        job.status = Status::Done;
        push(&mut job, "def");
        assert!(!job.review_needs_you());
        assert_eq!(job.review_due(), Some("def"));
        assert_eq!(
            review(&job)["detail"],
            "Pull request open · waiting for your review on GitHub"
        );
        // No account fits to run it on: it says why it waits.
        job.no_reviewer = Some("Oby's review waits for an account with room: Work is full.".into());
        assert_eq!(
            review(&job)["detail"],
            "Waiting for review · Oby's review waits for an account with room: Work is full."
        );
        assert!(job.to_json()["noReviewer"].is_string() && !job.needs_you());
        job.no_reviewer = None;
        job.review = Some(verdict("approved", "def"));
        assert_eq!(job.review_due(), None);
        assert!(!job.needs_you());
        assert_eq!(review(&job)["detail"], APPROVED_BY_OBY);
        assert_eq!(job.to_json()["review"]["date"], "2026-10-09T21:42:53Z");
        assert!(job.to_json()["review"].get("sameAccount").is_none());
    }
}
