//! The findings ledger (docs/runner-api.md, "obid only: the ledger"): the one place obid writes
//! GitHub issues. Agents' findings, review follow-ups and failures go to a private repository the
//! configuration names (`ledger.repo`), through a GitHub App of its own (the Ledger App), never to
//! the repositories obid works on, which may be public. Nothing else in obid creates, labels or
//! edits an issue.
//!
//! Every write asks for an installation token covering the ledger repository only, with
//! `issues: write` and `metadata: read` and nothing more, and first checks that the repository is
//! still private and still the one named (a renamed or transferred repository isn't followed).
//! Each write passes the secret scan; a finding, or a scan that can't run, sends nothing.
//!
//! What keeps the ledger readable:
//! - **A cap per job** (`ledger.mostPerJob`, 5 when left out): what a job finds past it is kept
//!   in `home/ledger/held.jsonl` (0600), never thrown away, and nothing more goes to GitHub.
//! - **Dedupe**: a finding whose title (lower case, letters and digits only) or fingerprint matches
//!   an open finding, or one closed in the last `ledger.recentlyClosedDays` (30), isn't written
//!   again; the existing one is answered.
//! - **Grouping**: a finding with a group goes under that group's open epic (`ledger:epic`, made
//!   when there is none) as a sub-issue, and says `Part of #N`.
//! - **Expiry**: every finding carries `expires:<n>d` for its kind; [`Ledger::sweep`] closes, as
//!   not planned and labelled `ledger:expired`, the open ones untouched for that long, unless
//!   someone labelled them `ledger:keep`. Closing archives: nothing is deleted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The start of every variable that would carry the Ledger App's settings: never in an agent's or
/// a check's environment (crate::process::is_forbidden).
pub const VARIABLE_PREFIX: &str = "OBY_LEDGER";

/// The vault item holding the Ledger App's private key when `ledger.key` names no file.
pub const VAULT_KEY: &str = "oby.ledger.app-key";

/// Every finding obid writes carries it.
pub const FINDING_LABEL: &str = "ledger:finding";
/// The parent issues findings are grouped under.
pub const EPIC_LABEL: &str = "ledger:epic";
/// A person's label that keeps a finding from expiring.
pub const KEEP_LABEL: &str = "ledger:keep";
/// What an expired finding is closed with.
pub const EXPIRED_LABEL: &str = "ledger:expired";
/// The status label a claimed ledger issue gets (Engine/IssueSync.swift's).
pub const CLAIM_LABEL: &str = "status:in-progress";

const FINDING_MARK: &str = "<!-- oby-finding: ";
const EPIC_MARK: &str = "<!-- oby-ledger-epic: ";

/// GitHub's longest issue title.
const MOST_TITLE: usize = 256;
/// Well under GitHub's 65,536-character body.
const MOST_BODY: usize = 60_000;
/// The most pages of 100 issues one dedupe reads, open and recently closed each.
const MOST_PAGES: u32 = 10;
/// How long a check that the ledger is private holds.
const PRIVATE_FOR: i64 = 3600;

/// `ledger` in `config.json`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    /// The private repository, `owner/name`.
    pub repo: String,
    /// The Ledger App's App ID (or Client ID): the JWT's issuer.
    pub app_id: String,
    /// Its installation on `repo`; asked of GitHub when left out.
    #[serde(default)]
    pub installation: Option<u64>,
    /// The App's private key (PEM), a file only obid's user may read; left out, the vault's
    /// [`VAULT_KEY`].
    #[serde(default)]
    pub key: Option<PathBuf>,
    /// The most issues one job writes: 1 to 50, [`MOST_PER_JOB`] when left out.
    #[serde(default = "most_per_job")]
    pub most_per_job: usize,
    /// How far back a closed finding still counts as a duplicate, in days: 1 to 365, 30 when left
    /// out.
    #[serde(default = "recently_closed_days")]
    pub recently_closed_days: u32,
}

/// The most issues one job writes when `ledger.mostPerJob` doesn't say.
pub const MOST_PER_JOB: usize = 5;

fn most_per_job() -> usize {
    MOST_PER_JOB
}

fn recently_closed_days() -> u32 {
    30
}

impl Settings {
    /// Why these settings can't be used, or none.
    pub fn problem(&self) -> Option<String> {
        if !crate::github::is_repo(&self.repo) {
            return Some(format!(
                "ledger.repo is {:?}: it must be owner/name.",
                self.repo
            ));
        }
        if self.app_id.is_empty() || !self.app_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Some("ledger.appId must be the Ledger App's App ID or Client ID.".into());
        }
        if self.key.as_ref().is_some_and(|k| !k.is_absolute()) {
            return Some("ledger.key must be an absolute path.".into());
        }
        if !(1..=50).contains(&self.most_per_job) {
            return Some(format!(
                "ledger.mostPerJob is {}: it is 1 to 50.",
                self.most_per_job
            ));
        }
        if !(1..=365).contains(&self.recently_closed_days) {
            return Some(format!(
                "ledger.recentlyClosedDays is {}: it is 1 to 365.",
                self.recently_closed_days
            ));
        }
        None
    }

    /// Whether `repo` is the ledger.
    pub fn is_ledger(&self, repo: &str) -> bool {
        self.repo.eq_ignore_ascii_case(repo)
    }
}

/// Whether obid may write an issue in `repo`: only in the ledger, and only when there is one.
pub fn may_write_issue(ledger: Option<&Settings>, repo: &str) -> bool {
    ledger.is_some_and(|l| l.is_ledger(repo))
}

/// What a finding is, which decides its label and how long it lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Something Oby's review found worth doing later.
    ReviewFollowUp,
    /// A failure Oby studied.
    Failure,
    /// An issue a research job proposed.
    Proposal,
    /// Anything else an agent noticed.
    Finding,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::ReviewFollowUp => "kind:review-follow-up",
            Kind::Failure => "kind:failure",
            Kind::Proposal => "kind:proposal",
            Kind::Finding => "kind:finding",
        }
    }

    /// How long an untouched finding of this kind stays open, in days.
    pub fn lifetime_days(self) -> u32 {
        match self {
            Kind::Failure => 14,
            Kind::ReviewFollowUp | Kind::Finding => 30,
            Kind::Proposal => 90,
        }
    }
}

/// The expiry label for a lifetime: `expires:30d`.
pub fn expiry_label(days: u32) -> String {
    format!("expires:{days}d")
}

/// The lifetime an expiry label names.
fn expiry_days(label: &str) -> Option<i64> {
    label
        .strip_prefix("expires:")?
        .strip_suffix('d')?
        .parse::<i64>()
        .ok()
        .filter(|d| *d > 0)
}

/// The epic a finding goes under: findings with the same key share one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// Stable: `review:owner/repo`, `failures`.
    pub key: String,
    /// The epic's title when it's made.
    pub title: String,
}

/// One thing an agent found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub title: String,
    /// What it is, in Markdown; quoted data from agents, signed `by Oby:` when it's written.
    pub body: String,
    pub kind: Kind,
    #[serde(default)]
    pub group: Option<Group>,
}

/// What became of a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// A new ledger issue, under `epic` when it has a group.
    Created {
        number: u64,
        url: String,
        epic: Option<u64>,
    },
    /// An open or recently closed finding already says it.
    Duplicate { number: u64, url: String },
    /// The job reached its cap: kept in `home/ledger/held.jsonl`, not sent.
    Held,
}

/// Why nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("{0} isn't the ledger: obid writes issues only in its private ledger")]
    NotTheLedger(String),
    #[error(
        "the ledger {0} isn't private (or isn't that repository any more): nothing was written"
    )]
    NotPrivate(String),
    #[error("a finding has a title of 1 to 256 characters")]
    BadTitle,
    #[error("the secret scan found something: nothing was written")]
    Secret,
    #[error("the secret scan couldn't run ({0}): nothing was written")]
    ScanCouldNotRun(String),
    #[error("the Ledger App's token: {0}")]
    Token(String),
    #[error("GitHub: {0}")]
    GitHub(String),
    #[error("the ledger's records: {0}")]
    Store(String),
}

/// One call to GitHub's REST API.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub method: &'static str,
    /// From the API's root: `/repos/o/r/issues?state=open`.
    pub path: String,
    pub bearer: String,
    pub body: Option<Value>,
}

/// GitHub's answer: its status and JSON body (null when there was none).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

pub type Transport = Arc<dyn Fn(Call) -> BoxFuture<'static, Result<Answer, String>> + Send + Sync>;
pub type Signer = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, String> + Send + Sync>;
pub type Scanner =
    Arc<dyn Fn(Vec<(String, String)>) -> BoxFuture<'static, crate::scan::Verdict> + Send + Sync>;
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// What the ledger keeps across restarts: how many issues each job wrote (`home/ledger/jobs.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    written: BTreeMap<String, usize>,
}

#[derive(Default)]
struct Live {
    /// The installation token and when obid stops using it.
    token: Option<(String, i64)>,
    installation: Option<u64>,
    /// When the ledger was last seen private.
    private_at: Option<i64>,
    kept: Kept,
}

/// The ledger writer. One write at a time, so the cap and dedupe hold across jobs.
pub struct Ledger {
    settings: Settings,
    folder: PathBuf,
    transport: Transport,
    sign: Signer,
    scan: Scanner,
    now: Clock,
    live: tokio::sync::Mutex<Live>,
}

/// An issue as the REST API lists it.
#[derive(Debug, Clone)]
struct Listed {
    number: u64,
    id: u64,
    url: String,
    title: String,
    body: String,
    labels: Vec<String>,
    updated: i64,
    closed: Option<i64>,
}

impl Listed {
    fn read(row: &Value) -> Option<Listed> {
        if row.get("pull_request").is_some() {
            return None;
        }
        let time = |key: &str| {
            row.get(key)
                .and_then(Value::as_str)
                .and_then(|t| t.parse::<jiff::Timestamp>().ok())
                .map(|t| t.as_second())
        };
        Some(Listed {
            number: row.get("number")?.as_u64()?,
            id: row.get("id")?.as_u64()?,
            url: row.get("html_url")?.as_str()?.to_owned(),
            title: row.get("title")?.as_str()?.to_owned(),
            body: row
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            labels: row
                .get("labels")
                .and_then(Value::as_array)
                .map(|labels| {
                    labels
                        .iter()
                        .filter_map(|l| l.get("name").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            updated: time("updated_at").unwrap_or_default(),
            closed: time("closed_at"),
        })
    }
}

/// A title as dedupe compares it: lower case, letters and digits, single spaces.
pub fn normalized(title: &str) -> String {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A finding's fingerprint: its kind, group and normalized title, FNV-1a as IssueSync's markers.
pub fn fingerprint(finding: &Finding) -> String {
    let text = format!(
        "{}\n{}\n{}",
        finding.kind.label(),
        finding.group.as_ref().map(|g| g.key.as_str()).unwrap_or(""),
        normalized(&finding.title)
    );
    let hash = text.bytes().fold(14_695_981_039_346_656_037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
    });
    format!("{hash:016x}")
}

fn marked(body: &str, mark: &str) -> Option<String> {
    let start = body.find(mark)? + mark.len();
    let end = body[start..].find(" -->")?;
    Some(body[start..start + end].to_owned())
}

fn cut(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(most - 1).collect();
    out.push('…');
    out
}

fn query(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Ledger {
    /// The ledger for `settings`, its key read from the file or the vault: none when the settings
    /// or the key won't do, and why.
    pub fn open(home: &Path, settings: Settings) -> Result<Ledger, String> {
        if let Some(problem) = settings.problem() {
            return Err(problem);
        }
        let pem = match &settings.key {
            Some(path) => read_key_file(path)?,
            None => crate::vault::value(VAULT_KEY)
                .map_err(|e| format!("the vault couldn't be read: {e}"))?
                .ok_or_else(|| format!("no {VAULT_KEY} in the vault"))?,
        };
        let key = parse_key(&pem)?;
        crate::secrets::known().learn("ledger app key", pem.trim());
        let sign: Signer = Arc::new(move |input: &[u8]| {
            let mut signature = vec![0; key.public_modulus_len()];
            key.sign(
                &aws_lc_rs::signature::RSA_PKCS1_SHA256,
                &aws_lc_rs::rand::SystemRandom::new(),
                input,
                &mut signature,
            )
            .map_err(|_| "the key couldn't sign".to_owned())?;
            Ok(signature)
        });
        let folder = home.join("ledger");
        let scratch = folder.clone();
        let scan: Scanner = Arc::new(move |texts| {
            let scratch = scratch.join(format!("scan-{}", uuid::Uuid::new_v4().simple()));
            Box::pin(async move {
                if let Err(e) = crate::store::make_private_dir(&scratch) {
                    return crate::scan::Verdict::CouldNotRun(format!("no scratch folder: {e}"));
                }
                let verdict = crate::scan::scan(&texts, &scratch).await;
                let _ = std::fs::remove_dir_all(&scratch);
                verdict
            })
        });
        Ok(Ledger::with(
            settings,
            folder,
            github_transport(),
            sign,
            scan,
            Arc::new(|| jiff::Timestamp::now().as_second()),
        ))
    }

    /// A ledger with its parts given: tests hand it a GitHub, a signer, a scanner and a clock.
    pub fn with(
        settings: Settings,
        folder: PathBuf,
        transport: Transport,
        sign: Signer,
        scan: Scanner,
        now: Clock,
    ) -> Ledger {
        let kept = std::fs::read(folder.join("jobs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Ledger {
            settings,
            folder,
            transport,
            sign,
            scan,
            now,
            live: tokio::sync::Mutex::new(Live {
                kept,
                ..Live::default()
            }),
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Writes `finding` from job `job` in the ledger, under the cap, unless it's already there.
    pub async fn record(&self, job: &str, finding: Finding) -> Result<Recorded, Refused> {
        let title = finding.title.trim();
        if title.is_empty() || title.chars().count() > MOST_TITLE {
            return Err(Refused::BadTitle);
        }
        let finding = Finding {
            title: title.to_owned(),
            body: cut(&finding.body, MOST_BODY),
            ..finding
        };
        let mut live = self.live.lock().await;
        let written = live.kept.written.get(job).copied().unwrap_or(0);
        if written >= self.settings.most_per_job {
            self.hold(job, &finding)?;
            return Ok(Recorded::Held);
        }
        let mut texts = vec![
            ("finding title".to_owned(), finding.title.clone()),
            ("finding body".to_owned(), finding.body.clone()),
        ];
        if let Some(group) = &finding.group {
            texts.push(("finding group".to_owned(), group.title.clone()));
        }
        self.scanned(texts).await?;
        let token = self.ready(&mut live).await?;

        let print = fingerprint(&finding);
        let wanted = normalized(&finding.title);
        let since = (self.now)() - i64::from(self.settings.recently_closed_days) * 86_400;
        let mut known = self.list(&token, "open", FINDING_LABEL, None).await?;
        known.extend(
            self.list(&token, "closed", FINDING_LABEL, Some(since))
                .await?
                .into_iter()
                .filter(|i| i.closed.is_some_and(|c| c >= since)),
        );
        if let Some(same) = known.iter().find(|i| {
            marked(&i.body, FINDING_MARK).as_deref() == Some(print.as_str())
                || normalized(&i.title) == wanted
        }) {
            return Ok(Recorded::Duplicate {
                number: same.number,
                url: same.url.clone(),
            });
        }

        let mut made = 0;
        let epic = match &finding.group {
            None => None,
            Some(group) => {
                let epics = self.list(&token, "open", EPIC_LABEL, None).await?;
                match epics
                    .into_iter()
                    .find(|e| marked(&e.body, EPIC_MARK).as_deref() == Some(group.key.as_str()))
                {
                    Some(epic) => Some((epic.number, epic.id)),
                    None => {
                        if written + 2 > self.settings.most_per_job {
                            self.hold(job, &finding)?;
                            return Ok(Recorded::Held);
                        }
                        let body = format!(
                            "by Oby: findings grouped under `{}`. Each one is a sub-issue.\n\n{EPIC_MARK}{} -->",
                            group.key, group.key
                        );
                        let epic = self
                            .create(
                                &token,
                                &cut(&group.title, MOST_TITLE),
                                &body,
                                &[EPIC_LABEL.to_owned()],
                            )
                            .await?;
                        made += 1;
                        Some((epic.number, epic.id))
                    }
                }
            }
        };

        let mut body = format!(
            "by Oby: a finding from job {job}, quoted as the agent wrote it.\n\n{}",
            finding.body
        );
        if let Some((number, _)) = epic {
            body.push_str(&format!("\n\nPart of #{number}"));
        }
        body.push_str(&format!("\n\n{FINDING_MARK}{print} -->"));
        let labels = vec![
            FINDING_LABEL.to_owned(),
            finding.kind.label().to_owned(),
            expiry_label(finding.kind.lifetime_days()),
        ];
        let created = self.create(&token, &finding.title, &body, &labels).await;
        let created = match created {
            Ok(created) => created,
            Err(e) => {
                if made > 0 {
                    self.count(&mut live, job, made)?;
                }
                return Err(e);
            }
        };
        made += 1;
        self.count(&mut live, job, made)?;
        if let Some((number, _)) = epic {
            // `Part of #N` already says it when GitHub won't take the sub-issue.
            let _ = self
                .call(
                    "POST",
                    format!("/repos/{}/issues/{number}/sub_issues", self.settings.repo),
                    &token,
                    Some(json!({"sub_issue_id": created.id})),
                )
                .await;
        }
        Ok(Recorded::Created {
            number: created.number,
            url: created.url,
            epic: epic.map(|(n, _)| n),
        })
    }

    /// Claims ledger issue `number` of `repo` for job `id`, as IssueSync does: `status:in-progress`
    /// and one hidden claim line at the end of its body. An issue anywhere but the ledger is
    /// [`Refused::NotTheLedger`]: obid writes nothing there.
    pub async fn claim(&self, repo: &str, number: u64, id: &str) -> Result<(), Refused> {
        if !self.settings.is_ledger(repo) {
            return Err(Refused::NotTheLedger(repo.to_owned()));
        }
        let mut live = self.live.lock().await;
        let token = self.ready(&mut live).await?;
        let path = format!("/repos/{}/issues/{number}", self.settings.repo);
        let issue = self.call("GET", path.clone(), &token, None).await?;
        let body = issue
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let body = crate::brain::lead::claimed(body, id);
        self.scanned(vec![("issue body".to_owned(), body.clone())])
            .await?;
        self.call("PATCH", path.clone(), &token, Some(json!({"body": body})))
            .await?;
        self.call(
            "POST",
            format!("{path}/labels"),
            &token,
            Some(json!({"labels": [CLAIM_LABEL]})),
        )
        .await?;
        Ok(())
    }

    /// Closes the open findings untouched for longer than their `expires:<n>d` label says, as not
    /// planned and labelled `ledger:expired`, leaving those labelled `ledger:keep` and the epics.
    /// Answers the numbers it closed.
    pub async fn sweep(&self) -> Result<Vec<u64>, Refused> {
        let mut live = self.live.lock().await;
        let token = self.ready(&mut live).await?;
        let now = (self.now)();
        let mut closed = Vec::new();
        for issue in self.list(&token, "open", FINDING_LABEL, None).await? {
            if issue
                .labels
                .iter()
                .any(|l| l == KEEP_LABEL || l == EPIC_LABEL)
            {
                continue;
            }
            let Some(days) = issue.labels.iter().find_map(|l| expiry_days(l)) else {
                continue;
            };
            if issue.updated + days * 86_400 > now {
                continue;
            }
            let path = format!("/repos/{}/issues/{}", self.settings.repo, issue.number);
            self.call(
                "POST",
                format!("{path}/labels"),
                &token,
                Some(json!({"labels": [EXPIRED_LABEL]})),
            )
            .await?;
            self.call(
                "PATCH",
                path,
                &token,
                Some(json!({"state": "closed", "state_reason": "not_planned"})),
            )
            .await?;
            closed.push(issue.number);
        }
        Ok(closed)
    }

    /// What the job found past its cap, kept for good in `held.jsonl`.
    fn hold(&self, job: &str, finding: &Finding) -> Result<(), Refused> {
        use std::io::Write as _;
        crate::store::make_private_dir(&self.folder).map_err(|e| Refused::Store(e.to_string()))?;
        let mut options = std::fs::OpenOptions::new();
        options.append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let line = json!({"job": job, "at": (self.now)(), "finding": finding});
        options
            .open(self.folder.join("held.jsonl"))
            .and_then(|mut f| writeln!(f, "{line}"))
            .map_err(|e| Refused::Store(e.to_string()))
    }

    fn count(&self, live: &mut Live, job: &str, made: usize) -> Result<(), Refused> {
        *live.kept.written.entry(job.to_owned()).or_default() += made;
        crate::store::make_private_dir(&self.folder).map_err(|e| Refused::Store(e.to_string()))?;
        let bytes = serde_json::to_vec(&live.kept).map_err(|e| Refused::Store(e.to_string()))?;
        crate::store::write_private(&self.folder.join("jobs.json"), &bytes)
            .map_err(|e| Refused::Store(e.to_string()))
    }

    async fn scanned(&self, texts: Vec<(String, String)>) -> Result<(), Refused> {
        match (self.scan)(texts).await {
            crate::scan::Verdict::Clean => Ok(()),
            crate::scan::Verdict::Found(_) => Err(Refused::Secret),
            crate::scan::Verdict::CouldNotRun(why) => Err(Refused::ScanCouldNotRun(why)),
        }
    }

    /// A token for the ledger, and the ledger checked private and still itself.
    async fn ready(&self, live: &mut Live) -> Result<String, Refused> {
        let now = (self.now)();
        let token = match &live.token {
            Some((token, until)) if *until > now => token.clone(),
            _ => {
                let (token, until) = self.mint(live).await?;
                live.token = Some((token.clone(), until));
                token
            }
        };
        if live.private_at.is_none_or(|at| now - at >= PRIVATE_FOR) {
            let repo = self
                .call(
                    "GET",
                    format!("/repos/{}", self.settings.repo),
                    &token,
                    None,
                )
                .await?;
            let private = repo.get("private").and_then(Value::as_bool) == Some(true)
                && repo.get("visibility").and_then(Value::as_str) != Some("public");
            let same = repo
                .get("full_name")
                .and_then(Value::as_str)
                .is_some_and(|name| self.settings.is_ledger(name));
            if !private || !same {
                live.private_at = None;
                return Err(Refused::NotPrivate(self.settings.repo.clone()));
            }
            live.private_at = Some(now);
        }
        Ok(token)
    }

    /// An installation token for the ledger repository alone, with `issues: write` and
    /// `metadata: read`, and when obid stops using it (5 minutes before GitHub does).
    async fn mint(&self, live: &mut Live) -> Result<(String, i64), Refused> {
        let now = (self.now)();
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = b64.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = json!({"iat": now - 60, "exp": now + 540, "iss": self.settings.app_id});
        let input = format!("{header}.{}", b64.encode(claims.to_string()));
        let signature = (self.sign)(input.as_bytes()).map_err(Refused::Token)?;
        let jwt = format!("{input}.{}", b64.encode(signature));
        let installation = match self.settings.installation.or(live.installation) {
            Some(id) => id,
            None => {
                let found = self
                    .call(
                        "GET",
                        format!("/repos/{}/installation", self.settings.repo),
                        &jwt,
                        None,
                    )
                    .await
                    .map_err(|e| Refused::Token(format!("no installation on the ledger ({e})")))?;
                let id = found
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| Refused::Token("GitHub named no installation".into()))?;
                live.installation = Some(id);
                id
            }
        };
        let name = self
            .settings
            .repo
            .split_once('/')
            .map(|(_, name)| name)
            .unwrap_or_default();
        let answer = self
            .call(
                "POST",
                format!("/app/installations/{installation}/access_tokens"),
                &jwt,
                Some(json!({
                    "repositories": [name],
                    "permissions": {"issues": "write", "metadata": "read"},
                })),
            )
            .await
            .map_err(|e| Refused::Token(e.to_string()))?;
        let token = answer
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| Refused::Token("GitHub sent no token".into()))?
            .to_owned();
        // A token that could do more than the ledger needs isn't used.
        let wider = answer
            .get("permissions")
            .and_then(Value::as_object)
            .is_none_or(|granted| {
                granted.iter().any(|(name, level)| {
                    !matches!(
                        (name.as_str(), level.as_str()),
                        ("issues", Some("write" | "read")) | ("metadata", Some("read"))
                    )
                })
            });
        let repositories: Vec<&str> = answer
            .get("repositories")
            .and_then(Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(|r| r.get("full_name")?.as_str())
                    .collect()
            })
            .unwrap_or_default();
        if wider || repositories.len() != 1 || !self.settings.is_ledger(repositories[0]) {
            return Err(Refused::Token(
                "GitHub's token reaches more than the ledger's issues: not used".into(),
            ));
        }
        crate::secrets::known().learn("ledger token", &token);
        let until = answer
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|t| t.parse::<jiff::Timestamp>().ok())
            .map(|t| t.as_second())
            .unwrap_or(now + 3600)
            - 300;
        Ok((token, until))
    }

    async fn create(
        &self,
        token: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<Listed, Refused> {
        let row = self
            .call(
                "POST",
                format!("/repos/{}/issues", self.settings.repo),
                token,
                Some(json!({"title": title, "body": body, "labels": labels})),
            )
            .await?;
        Listed::read(&row)
            .ok_or_else(|| Refused::GitHub("it made an issue it didn't describe".into()))
    }

    /// The issues with `label` in `state`, updated since `since` when given, newest first.
    async fn list(
        &self,
        token: &str,
        state: &str,
        label: &str,
        since: Option<i64>,
    ) -> Result<Vec<Listed>, Refused> {
        let mut path = format!(
            "/repos/{}/issues?state={state}&labels={}&per_page=100",
            self.settings.repo,
            query(label)
        );
        if let Some(since) = since.and_then(|s| jiff::Timestamp::from_second(s).ok()) {
            path.push_str(&format!("&since={}", query(&since.to_string())));
        }
        let mut all = Vec::new();
        for page in 1..=MOST_PAGES {
            let rows = self
                .call("GET", format!("{path}&page={page}"), token, None)
                .await?;
            let rows = rows.as_array().cloned().unwrap_or_default();
            let full = rows.len() == 100;
            all.extend(rows.iter().filter_map(Listed::read));
            if !full {
                break;
            }
        }
        Ok(all)
    }

    async fn call(
        &self,
        method: &'static str,
        path: String,
        bearer: &str,
        body: Option<Value>,
    ) -> Result<Value, Refused> {
        let answer = (self.transport)(Call {
            method,
            path: path.clone(),
            bearer: bearer.to_owned(),
            body,
        })
        .await
        .map_err(Refused::GitHub)?;
        if !(200..300).contains(&answer.status) {
            let message = answer
                .body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message");
            return Err(Refused::GitHub(format!(
                "{method} {} answered {}: {}",
                path.split('?').next().unwrap_or_default(),
                answer.status,
                cut(message, 200)
            )));
        }
        Ok(answer.body)
    }
}

/// The key file, refused when anyone but its owner may read it.
fn read_key_file(path: &Path) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|e| format!("ledger.key {}: {e}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "ledger.key {} may be read by others: make it 0600",
                path.display()
            ));
        }
    }
    std::fs::read_to_string(path).map_err(|e| format!("ledger.key {}: {e}", path.display()))
}

/// A GitHub App key as GitHub hands it out (PKCS#1) or as PKCS#8.
fn parse_key(pem: &str) -> Result<aws_lc_rs::signature::RsaKeyPair, String> {
    let pem = pem.trim();
    let (body, pkcs8) = if let Some(rest) = pem.strip_prefix("-----BEGIN RSA PRIVATE KEY-----") {
        (rest.strip_suffix("-----END RSA PRIVATE KEY-----"), false)
    } else if let Some(rest) = pem.strip_prefix("-----BEGIN PRIVATE KEY-----") {
        (rest.strip_suffix("-----END PRIVATE KEY-----"), true)
    } else {
        (None, false)
    };
    let body = body.ok_or("the Ledger App's key isn't a PEM RSA key")?;
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.split_whitespace().collect::<String>())
        .map_err(|_| "the Ledger App's key isn't a PEM RSA key")?;
    if pkcs8 {
        aws_lc_rs::signature::RsaKeyPair::from_pkcs8(&der)
    } else {
        aws_lc_rs::signature::RsaKeyPair::from_der(&der)
    }
    .map_err(|_| "the Ledger App's key isn't an RSA key".into())
}

/// GitHub's REST API over obid's own TLS, no redirects followed.
fn github_transport() -> Transport {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("reqwest client");
    Arc::new(move |call: Call| {
        let http = http.clone();
        Box::pin(async move {
            let method = reqwest::Method::from_bytes(call.method.as_bytes())
                .map_err(|_| "no such method".to_owned())?;
            let mut request = http
                .request(method, format!("https://api.github.com{}", call.path))
                .bearer_auth(&call.bearer)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "obid-ledger");
            if let Some(body) = &call.body {
                request = request.json(body);
            }
            let response = request
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?;
            let status = response.status().as_u16();
            let bytes = response
                .bytes()
                .await
                .map_err(|e| e.without_url().to_string())?;
            let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            Ok(Answer { status, body })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const LEDGER: &str = "acme/work";
    const NOW: i64 = 1_800_000_000;

    fn settings() -> Settings {
        Settings {
            repo: LEDGER.into(),
            app_id: "123".into(),
            installation: Some(9),
            key: None,
            most_per_job: 3,
            recently_closed_days: 30,
        }
    }

    fn stamp(at: i64) -> String {
        jiff::Timestamp::from_second(at).unwrap().to_string()
    }

    /// A GitHub of its own: the ledger's issues, what the repository says of itself, and every call.
    struct Fake {
        issues: Vec<Value>,
        repo: Value,
        token: Value,
        calls: Vec<Call>,
        next: u64,
    }

    impl Fake {
        fn new() -> Fake {
            Fake {
                issues: vec![],
                repo: json!({"full_name": LEDGER, "private": true, "visibility": "private"}),
                token: json!({
                    "token": "ghs_ledgertoken",
                    "expires_at": stamp(NOW + 3600),
                    "permissions": {"issues": "write", "metadata": "read"},
                    "repositories": [{"full_name": LEDGER}],
                }),
                calls: vec![],
                next: 1,
            }
        }

        fn issue(&mut self, title: &str, body: &str, labels: &[&str], open: bool, at: i64) {
            let number = self.next;
            self.next += 1;
            self.issues.push(json!({
                "number": number, "id": 1000 + number,
                "html_url": format!("https://github.com/{LEDGER}/issues/{number}"),
                "title": title, "body": body,
                "labels": labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>(),
                "state": if open { "open" } else { "closed" },
                "updated_at": stamp(at),
                "closed_at": if open { Value::Null } else { json!(stamp(at)) },
            }));
        }

        fn answer(&mut self, call: Call) -> Answer {
            self.calls.push(call.clone());
            let ok = |body: Value| Answer { status: 200, body };
            let (path, params) = call.path.split_once('?').unwrap_or((&call.path, ""));
            let param = |name: &str| {
                params
                    .split('&')
                    .find_map(|p| p.strip_prefix(&format!("{name}=")))
                    .map(|v| v.replace("%3A", ":"))
            };
            let issues = format!("/repos/{LEDGER}/issues");
            match (call.method, path) {
                ("POST", "/app/installations/9/access_tokens") => ok(self.token.clone()),
                ("GET", p) if p == format!("/repos/{LEDGER}") => ok(self.repo.clone()),
                ("GET", p) if p == issues => {
                    if param("page").as_deref() != Some("1") {
                        return ok(json!([]));
                    }
                    let state = param("state").unwrap();
                    let label = param("labels").unwrap();
                    ok(Value::Array(
                        self.issues
                            .iter()
                            .filter(|i| i["state"] == state.as_str())
                            .filter(|i| {
                                i["labels"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .any(|l| l["name"] == label.as_str())
                            })
                            .cloned()
                            .collect(),
                    ))
                }
                ("POST", p) if p == issues => {
                    let body = call.body.unwrap();
                    let labels: Vec<String> = body["labels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|l| l.as_str().unwrap().to_owned())
                        .collect();
                    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                    self.issue(
                        body["title"].as_str().unwrap(),
                        body["body"].as_str().unwrap(),
                        &labels,
                        true,
                        NOW,
                    );
                    Answer {
                        status: 201,
                        body: self.issues.last().unwrap().clone(),
                    }
                }
                ("GET", p) if p.starts_with(&issues) => {
                    let n: u64 = p.rsplit('/').next().unwrap().parse().unwrap();
                    ok(self
                        .issues
                        .iter()
                        .find(|i| i["number"] == n)
                        .cloned()
                        .unwrap())
                }
                _ => ok(json!({})),
            }
        }
    }

    struct Rig {
        fake: Arc<Mutex<Fake>>,
        ledger: Ledger,
        scanned: Arc<Mutex<Vec<String>>>,
        folder: PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    fn rig_with(settings: Settings, fake: Fake, verdict: fn() -> crate::scan::Verdict) -> Rig {
        let fake = Arc::new(Mutex::new(fake));
        let scanned = Arc::new(Mutex::new(vec![]));
        let folder =
            std::env::temp_dir().join(format!("obid-ledger-{}", uuid::Uuid::new_v4().simple()));
        let transport: Transport = {
            let fake = fake.clone();
            Arc::new(move |call| {
                let answer = fake.lock().unwrap().answer(call);
                Box::pin(async move { Ok(answer) })
            })
        };
        let scan: Scanner = {
            let scanned = scanned.clone();
            Arc::new(move |texts: Vec<(String, String)>| {
                scanned
                    .lock()
                    .unwrap()
                    .extend(texts.into_iter().map(|(_, t)| t));
                Box::pin(async move { verdict() })
            })
        };
        let ledger = Ledger::with(
            settings,
            folder.clone(),
            transport,
            Arc::new(|_: &[u8]| Ok(vec![1, 2, 3])),
            scan,
            Arc::new(|| NOW),
        );
        Rig {
            fake,
            ledger,
            scanned,
            folder,
        }
    }

    fn rig(fake: Fake) -> Rig {
        rig_with(settings(), fake, || crate::scan::Verdict::Clean)
    }

    fn finding(title: &str) -> Finding {
        Finding {
            title: title.into(),
            body: "The cache is never evicted.".into(),
            kind: Kind::ReviewFollowUp,
            group: None,
        }
    }

    fn writes(rig: &Rig) -> Vec<Call> {
        rig.fake
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|c| c.method != "GET" && !c.path.starts_with("/app/"))
            .cloned()
            .collect()
    }

    #[tokio::test]
    async fn a_finding_goes_to_the_ledger_with_its_labels_and_fingerprint() {
        let rig = rig(Fake::new());
        let made = rig
            .ledger
            .record("job-1", finding("Evict the cache"))
            .await
            .unwrap();
        assert_eq!(
            made,
            Recorded::Created {
                number: 1,
                url: format!("https://github.com/{LEDGER}/issues/1"),
                epic: None
            }
        );
        let writes = writes(&rig);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].path, format!("/repos/{LEDGER}/issues"));
        assert_eq!(writes[0].bearer, "ghs_ledgertoken");
        let body = writes[0].body.clone().unwrap();
        assert_eq!(
            body["labels"],
            json!(["ledger:finding", "kind:review-follow-up", "expires:30d"])
        );
        let text = body["body"].as_str().unwrap();
        assert!(text.starts_with("by Oby: a finding from job job-1"));
        assert!(text.contains(&format!(
            "<!-- oby-finding: {} -->",
            fingerprint(&finding("Evict the cache"))
        )));
        assert!(
            rig.scanned
                .lock()
                .unwrap()
                .contains(&"Evict the cache".to_owned())
        );
    }

    #[tokio::test]
    async fn the_token_reaches_only_the_ledgers_issues() {
        let rig = rig(Fake::new());
        rig.ledger.record("job-1", finding("One")).await.unwrap();
        let fake = rig.fake.lock().unwrap();
        let mint = fake
            .calls
            .iter()
            .find(|c| c.path.starts_with("/app/"))
            .unwrap();
        assert_eq!(
            mint.body,
            Some(
                json!({"repositories": ["work"], "permissions": {"issues": "write", "metadata": "read"}})
            )
        );
        // The JWT, never the installation token, asks for it.
        assert_ne!(mint.bearer, "ghs_ledgertoken");
        assert_eq!(mint.bearer.split('.').count(), 3);
    }

    #[tokio::test]
    async fn a_token_that_reaches_more_is_not_used() {
        for token in [
            json!({"token": "t", "permissions": {"issues": "write", "contents": "write"}, "repositories": [{"full_name": LEDGER}]}),
            json!({"token": "t", "permissions": {"issues": "write"}, "repositories": [{"full_name": LEDGER}, {"full_name": "acme/oby"}]}),
            json!({"token": "t", "permissions": {"issues": "write"}, "repositories": [{"full_name": "acme/oby"}]}),
        ] {
            let mut fake = Fake::new();
            fake.token = token;
            let rig = rig(fake);
            let refused = rig
                .ledger
                .record("job-1", finding("One"))
                .await
                .unwrap_err();
            assert!(matches!(refused, Refused::Token(_)), "{refused:?}");
            assert!(writes(&rig).is_empty());
        }
    }

    #[tokio::test]
    async fn a_public_or_moved_ledger_gets_nothing() {
        for repo in [
            json!({"full_name": LEDGER, "private": false, "visibility": "public"}),
            json!({"full_name": "acme/oby", "private": true, "visibility": "private"}),
            json!({"full_name": LEDGER}),
        ] {
            let mut fake = Fake::new();
            fake.repo = repo;
            let rig = rig(fake);
            assert_eq!(
                rig.ledger.record("job-1", finding("One")).await,
                Err(Refused::NotPrivate(LEDGER.into()))
            );
            assert_eq!(
                rig.ledger.claim(LEDGER, 1, "job-1").await,
                Err(Refused::NotPrivate(LEDGER.into()))
            );
            assert!(writes(&rig).is_empty());
        }
    }

    #[tokio::test]
    async fn the_cap_holds_what_comes_after_it_without_losing_it() {
        let rig = rig(Fake::new());
        for n in 0..3 {
            assert!(matches!(
                rig.ledger
                    .record("job-1", finding(&format!("Finding {n}")))
                    .await,
                Ok(Recorded::Created { .. })
            ));
        }
        assert_eq!(
            rig.ledger.record("job-1", finding("Finding 3")).await,
            Ok(Recorded::Held)
        );
        assert_eq!(
            rig.ledger.record("job-1", finding("Finding 4")).await,
            Ok(Recorded::Held)
        );
        // Another job has its own cap.
        assert!(matches!(
            rig.ledger.record("job-2", finding("Finding 5")).await,
            Ok(Recorded::Created { .. })
        ));
        assert_eq!(writes(&rig).len(), 4);
        let held = std::fs::read_to_string(rig.folder.join("held.jsonl")).unwrap();
        assert_eq!(held.lines().count(), 2);
        assert!(held.contains("Finding 3") && held.contains("Finding 4"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(rig.folder.join("held.jsonl"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn the_cap_survives_a_restart() {
        let rig = rig(Fake::new());
        for n in 0..3 {
            rig.ledger
                .record("job-1", finding(&format!("Finding {n}")))
                .await
                .unwrap();
        }
        let again = Ledger::with(
            settings(),
            rig.folder.clone(),
            rig.ledger.transport.clone(),
            rig.ledger.sign.clone(),
            rig.ledger.scan.clone(),
            Arc::new(|| NOW),
        );
        assert_eq!(
            again.record("job-1", finding("Finding 9")).await,
            Ok(Recorded::Held)
        );
    }

    #[tokio::test]
    async fn dedupe_matches_open_and_recently_closed_titles_and_fingerprints() {
        let mut fake = Fake::new();
        fake.issue(
            "Evict the cache!",
            "",
            &[FINDING_LABEL],
            true,
            NOW - 400 * 86_400,
        );
        fake.issue(
            "Close the socket",
            "",
            &[FINDING_LABEL],
            false,
            NOW - 3 * 86_400,
        );
        fake.issue(
            "Old and closed",
            "",
            &[FINDING_LABEL],
            false,
            NOW - 90 * 86_400,
        );
        let print = fingerprint(&finding("Renamed since"));
        fake.issue(
            "A person retitled it",
            &format!("x\n{FINDING_MARK}{print} -->"),
            &[FINDING_LABEL],
            true,
            NOW,
        );
        let rig = rig(fake);
        let url = |n: u64| format!("https://github.com/{LEDGER}/issues/{n}");
        assert_eq!(
            rig.ledger
                .record("job-1", finding("evict  the CACHE"))
                .await,
            Ok(Recorded::Duplicate {
                number: 1,
                url: url(1)
            })
        );
        assert_eq!(
            rig.ledger
                .record("job-1", finding("Close the socket"))
                .await,
            Ok(Recorded::Duplicate {
                number: 2,
                url: url(2)
            })
        );
        assert_eq!(
            rig.ledger.record("job-1", finding("Renamed since")).await,
            Ok(Recorded::Duplicate {
                number: 4,
                url: url(4)
            })
        );
        // Closed longer ago than recentlyClosedDays: written again.
        assert!(matches!(
            rig.ledger.record("job-1", finding("Old and closed")).await,
            Ok(Recorded::Created { .. })
        ));
        // Duplicates cost the job nothing.
        assert_eq!(writes(&rig).len(), 1);
        let since = rig
            .fake
            .lock()
            .unwrap()
            .calls
            .iter()
            .find(|c| c.path.contains("state=closed"))
            .unwrap()
            .path
            .clone();
        assert!(
            since.contains(&format!("since={}", query(&stamp(NOW - 30 * 86_400)))),
            "{since}"
        );
    }

    #[tokio::test]
    async fn grouped_findings_share_one_epic_as_sub_issues() {
        let rig = rig(Fake::new());
        let group = Group {
            key: "review:acme/oby".into(),
            title: "Review follow-ups in acme/oby".into(),
        };
        let one = Finding {
            group: Some(group.clone()),
            ..finding("First")
        };
        let two = Finding {
            group: Some(group),
            ..finding("Second")
        };
        assert_eq!(
            rig.ledger.record("job-1", one).await,
            Ok(Recorded::Created {
                number: 2,
                url: format!("https://github.com/{LEDGER}/issues/2"),
                epic: Some(1)
            })
        );
        assert_eq!(
            rig.ledger.record("job-2", two).await,
            Ok(Recorded::Created {
                number: 3,
                url: format!("https://github.com/{LEDGER}/issues/3"),
                epic: Some(1)
            })
        );
        let writes = writes(&rig);
        let epics: Vec<_> = writes
            .iter()
            .filter(|c| {
                c.body
                    .as_ref()
                    .is_some_and(|b| b["labels"] == json!([EPIC_LABEL]))
            })
            .collect();
        assert_eq!(epics.len(), 1, "one epic for the group");
        assert_eq!(
            epics[0].body.as_ref().unwrap()["title"],
            "Review follow-ups in acme/oby"
        );
        let subs: Vec<_> = writes
            .iter()
            .filter(|c| c.path.ends_with("/issues/1/sub_issues"))
            .collect();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].body, Some(json!({"sub_issue_id": 1002})));
        let second = writes
            .iter()
            .filter(|c| c.path == format!("/repos/{LEDGER}/issues"))
            .nth(2)
            .unwrap();
        assert!(
            second.body.as_ref().unwrap()["body"]
                .as_str()
                .unwrap()
                .contains("Part of #1")
        );
    }

    #[tokio::test]
    async fn a_new_epic_counts_against_the_cap() {
        let rig = rig(Fake::new());
        rig.ledger.record("job-1", finding("One")).await.unwrap();
        rig.ledger.record("job-1", finding("Two")).await.unwrap();
        let grouped = Finding {
            group: Some(Group {
                key: "g".into(),
                title: "G".into(),
            }),
            ..finding("Three")
        };
        // One left, and the epic and the finding would be two.
        assert_eq!(
            rig.ledger.record("job-1", grouped).await,
            Ok(Recorded::Held)
        );
        assert_eq!(writes(&rig).len(), 2);
    }

    #[tokio::test]
    async fn the_sweep_closes_expired_findings_and_spares_kept_ones_and_epics() {
        let mut fake = Fake::new();
        fake.issue(
            "Stale",
            "",
            &[FINDING_LABEL, "expires:14d"],
            true,
            NOW - 15 * 86_400,
        );
        fake.issue(
            "Fresh",
            "",
            &[FINDING_LABEL, "expires:14d"],
            true,
            NOW - 13 * 86_400,
        );
        fake.issue(
            "Kept",
            "",
            &[FINDING_LABEL, "expires:14d", KEEP_LABEL],
            true,
            NOW - 99 * 86_400,
        );
        fake.issue("No expiry", "", &[FINDING_LABEL], true, NOW - 99 * 86_400);
        fake.issue(
            "Long-lived",
            "",
            &[FINDING_LABEL, "expires:90d"],
            true,
            NOW - 60 * 86_400,
        );
        let rig = rig(fake);
        assert_eq!(rig.ledger.sweep().await, Ok(vec![1]));
        let writes = writes(&rig);
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].path, format!("/repos/{LEDGER}/issues/1/labels"));
        assert_eq!(writes[0].body, Some(json!({"labels": [EXPIRED_LABEL]})));
        assert_eq!(writes[1].method, "PATCH");
        assert_eq!(
            writes[1].body,
            Some(json!({"state": "closed", "state_reason": "not_planned"}))
        );
    }

    #[tokio::test]
    async fn every_kind_carries_its_expiry() {
        for (kind, label) in [
            (Kind::Failure, "expires:14d"),
            (Kind::ReviewFollowUp, "expires:30d"),
            (Kind::Finding, "expires:30d"),
            (Kind::Proposal, "expires:90d"),
        ] {
            let rig = rig(Fake::new());
            rig.ledger
                .record(
                    "j",
                    Finding {
                        kind,
                        ..finding("Some finding")
                    },
                )
                .await
                .unwrap();
            let body = writes(&rig)[0].body.clone().unwrap();
            assert_eq!(body["labels"][1], kind.label());
            assert_eq!(body["labels"][2], label);
            assert_eq!(expiry_days(label), Some(i64::from(kind.lifetime_days())));
        }
    }

    #[tokio::test]
    async fn a_secret_or_a_scan_that_cant_run_sends_nothing() {
        let rig = rig_with(settings(), Fake::new(), || {
            crate::scan::Verdict::Found(vec![])
        });
        assert_eq!(
            rig.ledger.record("j", finding("One")).await,
            Err(Refused::Secret)
        );
        assert!(rig.fake.lock().unwrap().calls.is_empty());
        let rig = rig_with(settings(), Fake::new(), || {
            crate::scan::Verdict::CouldNotRun("no gitleaks".into())
        });
        assert_eq!(
            rig.ledger.record("j", finding("One")).await,
            Err(Refused::ScanCouldNotRun("no gitleaks".into()))
        );
        assert!(rig.fake.lock().unwrap().calls.is_empty());
    }

    #[tokio::test]
    async fn a_claim_writes_only_in_the_ledger() {
        let mut fake = Fake::new();
        fake.issue("Work item", "Do it.", &[], true, NOW);
        let rig = rig(fake);
        for public in ["acme/oby", "ACME/oby", "acme/work-public"] {
            assert_eq!(
                rig.ledger.claim(public, 1, "job-1").await,
                Err(Refused::NotTheLedger(public.into()))
            );
        }
        assert!(rig.fake.lock().unwrap().calls.is_empty());
        rig.ledger.claim(LEDGER, 1, "job-1").await.unwrap();
        let writes = writes(&rig);
        assert_eq!(writes.len(), 2);
        assert_eq!(
            writes[0].body,
            Some(json!({"body": "Do it.\n\n<!-- oby-claim: job job-1 -->"}))
        );
        assert_eq!(writes[1].body, Some(json!({"labels": [CLAIM_LABEL]})));
        assert!(
            writes
                .iter()
                .all(|w| w.path.starts_with(&format!("/repos/{LEDGER}/")))
        );
    }

    #[test]
    fn only_the_ledger_may_be_written() {
        assert!(!may_write_issue(None, "acme/oby"));
        assert!(may_write_issue(Some(&settings()), "Acme/Work"));
        assert!(!may_write_issue(Some(&settings()), "acme/oby"));
        assert!(!may_write_issue(Some(&settings()), "acme/work/x"));
    }

    #[test]
    fn settings_are_checked() {
        assert_eq!(settings().problem(), None);
        let bad = [
            Settings {
                repo: "work".into(),
                ..settings()
            },
            Settings {
                app_id: "".into(),
                ..settings()
            },
            Settings {
                app_id: "1 2".into(),
                ..settings()
            },
            Settings {
                key: Some("relative.pem".into()),
                ..settings()
            },
            Settings {
                most_per_job: 0,
                ..settings()
            },
            Settings {
                most_per_job: 51,
                ..settings()
            },
            Settings {
                recently_closed_days: 0,
                ..settings()
            },
        ];
        for s in bad {
            assert!(s.problem().is_some(), "{s:?}");
        }
        let parsed: Settings =
            serde_json::from_value(json!({"repo": LEDGER, "appId": "123"})).unwrap();
        assert_eq!(parsed.most_per_job, MOST_PER_JOB);
        assert_eq!(parsed.recently_closed_days, 30);
        assert!(
            serde_json::from_value::<Settings>(json!({"repo": LEDGER, "appId": "1", "token": "x"}))
                .is_err()
        );
    }

    #[test]
    fn a_key_others_may_read_is_refused() {
        let folder =
            std::env::temp_dir().join(format!("obid-ledger-key-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("key.pem");
        std::fs::write(&path, "x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_key_file(&path).unwrap_err().contains("0600"));
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(read_key_file(&path).unwrap(), "x");
        assert!(parse_key("not a key").is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A real RSA key in a 0600 file opens the ledger, and its JWT signature verifies.
    #[test]
    fn a_key_file_opens_the_ledger_and_signs() {
        use aws_lc_rs::encoding::AsDer as _;
        let key =
            aws_lc_rs::signature::RsaKeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa2048).unwrap();
        let body = base64::engine::general_purpose::STANDARD.encode(key.as_der().unwrap().as_ref());
        // The PEM's label built here, so the secret scan doesn't take the test's own key for one.
        let label = ["PRIVATE", "KEY"].join(" ");
        let pem = format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n");
        let folder = std::env::temp_dir().join(format!(
            "obid-ledger-open-{}",
            uuid::Uuid::new_v4().simple()
        ));
        crate::store::make_private_dir(&folder).unwrap();
        let path = folder.join("ledger.pem");
        crate::store::write_private(&path, pem.as_bytes()).unwrap();
        let ledger = Ledger::open(
            &folder,
            Settings {
                key: Some(path),
                ..settings()
            },
        )
        .unwrap();
        let signature = (ledger.sign)(b"header.claims").unwrap();
        let public = aws_lc_rs::signature::UnparsedPublicKey::new(
            &aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256,
            aws_lc_rs::signature::KeyPair::public_key(&key)
                .as_ref()
                .to_vec(),
        );
        public.verify(b"header.claims", &signature).unwrap();
        assert!(
            Ledger::open(
                &folder,
                Settings {
                    repo: "nope".into(),
                    ..settings()
                }
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn titles_normalize_and_fingerprints_ignore_wording_noise() {
        assert_eq!(
            normalized("  Evict the CACHE!! (v2) "),
            "evict the cache v2"
        );
        assert_eq!(
            fingerprint(&finding("Evict the cache")),
            fingerprint(&finding("evict, the cache."))
        );
        assert_ne!(
            fingerprint(&finding("Evict the cache")),
            fingerprint(&Finding {
                kind: Kind::Failure,
                ..finding("Evict the cache")
            })
        );
    }
}
