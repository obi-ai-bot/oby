//! The reference host: a made-up forge, adapted as a real adapter would be, over a fake of its
//! service that answers in its own words and pages of 100. It passes the whole suite. Given one
//! [`Flaw`], it has one bug T3 Code's hosts kept fixing, and the suite must catch it.

use oby_connectors::conformance::{Fixture, RequestLog};
use oby_connectors::remote::{self, Location, MAX_URL_BYTES, Scheme};
use oby_connectors::{
    Capabilities, Capability, ChangeRequest, ChangeRequestLink, ChangeRequestState, Checks,
    ChecksState, CodeHost, CodeHostDefinition, CommitLink, ConnectorError, Cursor, Detail, Family,
    HostNames, IssueLink, ListQuery, MergeMethod, MergeState, Mergeable, Nouns, Page, Repo, Review,
    ReviewDecision, ReviewState, SignIn, Viewer, require,
};

/// One bug, by the case group that must catch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flaw {
    /// urls: a change request's number must be the URL's last segment, so `…/files` isn't read.
    DropsTrailingPath,
    /// remotes: scp's `git@host:path` split at the wrong place.
    MisparsesSsh,
    /// remotes: only the web host is known, not the SSH host (T3 Code's c6a120a958).
    IgnoresSshHost,
    /// remotes: a host that ends with the web host's name is taken as it.
    AcceptsLookalikeHost,
    /// states: `locked` mapped to a `LOCKED` obid doesn't have.
    InventsState,
    /// states: an unknown word read as open.
    GuessesUnknownState,
    /// states: merged read as closed (T3 Code's 60e54867fc).
    MergedAsClosed,
    /// pagination: the first page taken as the whole list (T3 Code's 9f815c94af).
    CutsAt100,
    /// pagination: the last page points back at the first.
    RepeatsPage,
    /// capabilities: the request is made, and the host's refusal passed on.
    AttemptsUnsupported,
    /// capabilities: an undeclared capability answers a plain failure, not Unsupported.
    FailsInsteadOfUnsupported,
    /// bounded parsing: no length limit, and a number parsed with `unwrap`.
    UnboundedParse,
    /// secrets: the host's error passed on without taking the token out.
    LeaksSecret,
}

const PAGE: usize = 100;

const STATES: &[&str] = &["opened", "locked", "closed", "merged"];
const REVIEW_DECISIONS: &[&str] = &["approved", "changes_requested", "review_required"];
const REVIEWS: &[&str] = &["approved", "requested_changes", "dismissed"];
const CHECKS: &[&str] = &["passed", "running", "pending", "failed", "canceled", "none"];
const MERGE_STATES: &[&str] = &[
    "can_be_merged",
    "need_rebase",
    "conflict",
    "draft_status",
    "ci_must_pass",
    "checking",
];
const MERGEABLES: &[&str] = &["can_be_merged", "cannot_be_merged", "unchecked"];

#[derive(Debug, Clone)]
pub struct ReferenceDefinition {
    names: HostNames,
    flaw: Option<Flaw>,
}

impl ReferenceDefinition {
    pub fn new(names: &HostNames, flaw: Option<Flaw>) -> Self {
        let mut names = names.clone();
        if flaw == Some(Flaw::IgnoresSshHost) {
            names.ssh.clear();
        }
        ReferenceDefinition { names, flaw }
    }

    /// A web URL of this host's, split at its `-` into the repository and the route after it.
    fn route<'a>(&self, url: &'a str) -> Option<(Repo, Vec<&'a str>)> {
        let location = remote::parse(url)?;
        if !self.names.serves_web(&location) {
            return None;
        }
        let Location { segments, .. } = location;
        let dash = segments.iter().position(|s| *s == "-")?;
        let (path, route) = (&segments[..dash], &segments[dash + 1..]);
        if path.len() < 2 || (!self.nested_groups() && path.len() != 2) {
            return None;
        }
        Some((
            Repo {
                host: self.names.web.clone(),
                path: path.join("/"),
            },
            route.to_vec(),
        ))
    }

    fn number(&self, route: &[&str], kind: &str) -> Option<u64> {
        if route.first() != Some(&kind) {
            return None;
        }
        if self.flaw == Some(Flaw::DropsTrailingPath) && route.len() != 2 {
            return None;
        }
        let text = route.get(1)?;
        if self.flaw == Some(Flaw::UnboundedParse) {
            return Some(text.parse().unwrap());
        }
        remote::parse_number(text)
    }

    /// Without a length limit: the whole path after the host, as written.
    fn unbounded(&self, remote: &str) -> Option<Repo> {
        let rest = remote.split_once("://").map_or(remote, |(_, rest)| rest);
        let (host, path) = rest.split_once('/')?;
        (host == self.names.web).then(|| Repo {
            host: host.into(),
            path: path.trim_end_matches(".git").into(),
        })
    }

    fn native(&self, family: Family, api: &str) -> &'static str {
        (self.native_words(family).iter())
            .find(|native| self.map_native(family, native) == Some(api))
            .expect("the reference host says every word the fixture uses")
    }
}

impl CodeHostDefinition for ReferenceDefinition {
    fn kind(&self) -> &'static str {
        "reference"
    }

    fn label(&self) -> &'static str {
        "Reference Forge"
    }

    fn nouns(&self) -> Nouns {
        Nouns {
            change_request: "change request",
            change_requests: "change requests",
            short: "CR",
            issue: "issue",
            issues: "issues",
        }
    }

    fn sign_in(&self) -> SignIn {
        SignIn::Cli {
            command: "forge",
            login: "forge auth login",
        }
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            merge_queue: false,
            auto_merge: true,
            drafts: true,
            required_approvals: true,
            codeowners: true,
            reviewer_account: false,
            check_logs: true,
            rerun: true,
        }
    }

    fn nested_groups(&self) -> bool {
        true
    }

    fn recognize_remote(&self, remote: &str) -> Option<Repo> {
        match self.flaw {
            Some(Flaw::UnboundedParse) if remote.len() > MAX_URL_BYTES => self.unbounded(remote),
            Some(Flaw::MisparsesSsh) if !remote.contains("://") => {
                let (host, path) = remote.trim().split_once(':')?;
                (host == self.names.web).then(|| Repo {
                    host: host.into(),
                    path: path.trim_end_matches(".git").into(),
                })
            }
            Some(Flaw::AcceptsLookalikeHost) => self.names.recognize(remote, true).or_else(|| {
                let location = remote::parse(remote)?;
                let mut segments = location.segments.clone();
                let name = segments.pop()?.trim_end_matches(".git");
                (location.host.ends_with(&self.names.web)
                    && location.scheme == Scheme::Https
                    && !segments.is_empty())
                .then(|| Repo {
                    host: self.names.web.clone(),
                    path: format!("{}/{name}", segments.join("/")),
                })
            }),
            _ => self.names.recognize(remote, self.nested_groups()),
        }
    }

    fn change_request_url(&self, repo: &Repo, number: u64) -> String {
        format!("https://{}/{}/-/pulls/{number}", repo.host, repo.path)
    }

    fn issue_url(&self, repo: &Repo, number: u64) -> Option<String> {
        Some(format!(
            "https://{}/{}/-/issues/{number}",
            repo.host, repo.path
        ))
    }

    fn commit_url(&self, repo: &Repo, sha: &str) -> String {
        format!("https://{}/{}/-/commit/{sha}", repo.host, repo.path)
    }

    fn parse_change_request_url(&self, url: &str) -> Option<ChangeRequestLink> {
        if self.flaw == Some(Flaw::UnboundedParse) && url.len() > MAX_URL_BYTES {
            let number = url.rsplit_once("/pulls/")?.1;
            return Some(ChangeRequestLink {
                repo: self.unbounded(url)?,
                number: number.parse().unwrap(),
            });
        }
        let (repo, route) = self.route(url)?;
        Some(ChangeRequestLink {
            repo,
            number: self.number(&route, "pulls")?,
        })
    }

    fn parse_issue_url(&self, url: &str) -> Option<IssueLink> {
        let (repo, route) = self.route(url)?;
        Some(IssueLink {
            repo,
            number: self.number(&route, "issues")?,
        })
    }

    fn parse_commit_url(&self, url: &str) -> Option<CommitLink> {
        let (repo, route) = self.route(url)?;
        match route.as_slice() {
            ["commit", sha, ..] if remote::is_commit_id(sha) => Some(CommitLink {
                repo,
                sha: (*sha).into(),
            }),
            _ => None,
        }
    }

    fn native_words(&self, family: Family) -> &'static [&'static str] {
        match family {
            Family::ChangeRequestState => STATES,
            Family::ReviewDecision => REVIEW_DECISIONS,
            Family::ReviewState => REVIEWS,
            Family::ChecksState => CHECKS,
            Family::MergeState => MERGE_STATES,
            Family::Mergeable => MERGEABLES,
        }
    }

    fn map_native(&self, family: Family, native: &str) -> Option<&'static str> {
        use Family as F;
        let word = match (family, native, self.flaw) {
            (F::ChangeRequestState, "locked", Some(Flaw::InventsState)) => "LOCKED",
            (F::ChangeRequestState, "merged", Some(Flaw::MergedAsClosed)) => "CLOSED",
            (F::ChangeRequestState, "opened" | "locked", _) => "OPEN",
            (F::ChangeRequestState, "closed", _) => "CLOSED",
            (F::ChangeRequestState, "merged", _) => "MERGED",
            (F::ChangeRequestState, _, Some(Flaw::GuessesUnknownState)) => "OPEN",
            (F::ReviewDecision, "approved", _) => "APPROVED",
            (F::ReviewDecision, "changes_requested", _) => "CHANGES_REQUESTED",
            (F::ReviewDecision, "review_required", _) => "REVIEW_REQUIRED",
            (F::ReviewState, "approved", _) => "APPROVED",
            (F::ReviewState, "requested_changes", _) => "CHANGES_REQUESTED",
            (F::ReviewState, "dismissed", _) => "DISMISSED",
            (F::ChecksState, "passed", _) => "SUCCESS",
            (F::ChecksState, "running" | "pending", _) => "PENDING",
            (F::ChecksState, "failed" | "canceled", _) => "FAILURE",
            (F::ChecksState, "none", _) => "NONE",
            (F::MergeState, "can_be_merged", _) => "CLEAN",
            (F::MergeState, "need_rebase", _) => "BEHIND",
            (F::MergeState, "conflict", _) => "DIRTY",
            (F::MergeState, "draft_status", _) => "DRAFT",
            (F::MergeState, "ci_must_pass", _) => "BLOCKED",
            (F::MergeState, "checking", _) => "UNKNOWN",
            (F::Mergeable, "can_be_merged", _) => "MERGEABLE",
            (F::Mergeable, "cannot_be_merged", _) => "CONFLICTING",
            (F::Mergeable, "unchecked", _) => "UNKNOWN",
            _ => return None,
        };
        Some(word)
    }
}

/// A change request as the service serves it: its own words.
#[derive(Debug, Clone)]
struct Native {
    number: u64,
    title: String,
    head: Option<String>,
    head_branch: String,
    base: String,
    author: Option<String>,
    state: &'static str,
    review: Option<&'static str>,
    merge_status: Option<&'static str>,
    mergeable: Option<&'static str>,
}

/// The fake service.
#[derive(Debug)]
struct Service {
    repo: Repo,
    viewer: String,
    secret: String,
    change_requests: Vec<Native>,
    requests: RequestLog,
}

impl Service {
    /// Its "not found", echoing its token the way a CLI echoes a remote.
    fn not_found(&self) -> String {
        format!(
            "remote: not found: https://x-access-token:{secret}@{}/{}.git (token {secret})",
            self.repo.host,
            self.repo.path,
            secret = self.secret
        )
    }
}

pub struct ReferenceHost {
    definition: ReferenceDefinition,
    service: Service,
}

impl ReferenceHost {
    pub fn new(fixture: &Fixture, flaw: Option<Flaw>) -> Self {
        let definition = ReferenceDefinition::new(&fixture.names, flaw);
        let change_requests = (fixture.change_requests.iter())
            .map(|cr| Native {
                number: cr.number,
                title: cr.title.clone(),
                head: cr.head.clone(),
                head_branch: cr.head_branch.clone(),
                base: cr.base.clone(),
                author: cr.author.clone(),
                state: definition.native(Family::ChangeRequestState, cr.state.as_api()),
                review: cr
                    .review_decision
                    .map(|d| definition.native(Family::ReviewDecision, d.as_api())),
                merge_status: cr
                    .merge_state
                    .map(|s| definition.native(Family::MergeState, s.as_api())),
                mergeable: cr
                    .mergeable
                    .map(|m| definition.native(Family::Mergeable, m.as_api())),
            })
            .collect();
        let service = Service {
            repo: fixture.repo.clone(),
            viewer: fixture.viewer.clone(),
            secret: fixture.secret.clone(),
            change_requests,
            requests: fixture.requests.clone(),
        };
        ReferenceHost {
            definition,
            service,
        }
    }

    fn flaw(&self) -> Option<Flaw> {
        self.definition.flaw
    }

    /// The service's error, made safe: its token taken out (but with [`Flaw::LeaksSecret`]).
    fn detail(&self, text: &str) -> Detail {
        let detail = Detail::new(text);
        if self.flaw() == Some(Flaw::LeaksSecret) {
            detail
        } else {
            detail.redact(&self.service.secret)
        }
    }

    fn ask(&self, request: &str, repo: &Repo) -> Result<(), ConnectorError> {
        self.service.requests.push(request);
        if *repo == self.service.repo {
            Ok(())
        } else {
            Err(ConnectorError::NotFound(
                self.detail(&self.service.not_found()),
            ))
        }
    }

    /// A capability-gated request: refused before it's made when undeclared.
    fn gated(
        &self,
        capability: Capability,
        request: &str,
        repo: &Repo,
    ) -> Result<(), ConnectorError> {
        match self.flaw() {
            Some(Flaw::AttemptsUnsupported) => {
                self.ask(request, repo)?;
                require(&self.definition, capability)
            }
            Some(Flaw::FailsInsteadOfUnsupported)
                if !self.definition.capabilities().has(capability) =>
            {
                Err(ConnectorError::failed("the service said 404"))
            }
            _ => {
                require(&self.definition, capability)?;
                self.ask(request, repo)
            }
        }
    }

    fn word<T>(
        &self,
        family: Family,
        native: &str,
        from_api: fn(&str) -> Option<T>,
    ) -> Result<T, ConnectorError> {
        (self
            .definition
            .map_native(family, native)
            .and_then(from_api))
        .ok_or_else(|| {
            ConnectorError::failed(&format!(
                "The service said {native:?}, which Oby doesn't know."
            ))
        })
    }

    fn decode(&self, native: &Native) -> Result<ChangeRequest, ConnectorError> {
        Ok(ChangeRequest {
            number: native.number,
            title: native.title.clone(),
            url: self
                .definition
                .change_request_url(&self.service.repo, native.number),
            base: native.base.clone(),
            head_branch: native.head_branch.clone(),
            head: native.head.clone(),
            state: self.word(
                Family::ChangeRequestState,
                native.state,
                ChangeRequestState::from_api,
            )?,
            review_decision: (native.review)
                .map(|r| self.word(Family::ReviewDecision, r, ReviewDecision::from_api))
                .transpose()?,
            merge_state: (native.merge_status)
                .map(|s| self.word(Family::MergeState, s, MergeState::from_api))
                .transpose()?,
            mergeable: (native.mergeable)
                .map(|m| self.word(Family::Mergeable, m, Mergeable::from_api))
                .transpose()?,
            author: native.author.clone(),
        })
    }
}

impl CodeHost for ReferenceHost {
    type Definition = ReferenceDefinition;

    fn definition(&self) -> &ReferenceDefinition {
        &self.definition
    }

    fn viewer(&self) -> Result<Viewer, ConnectorError> {
        self.service.requests.push("viewer");
        Ok(Viewer {
            login: self.service.viewer.clone(),
        })
    }

    fn can_push(&self, repo: &Repo) -> Result<bool, ConnectorError> {
        self.ask("permissions", repo)?;
        Ok(true)
    }

    fn list_change_requests(
        &self,
        repo: &Repo,
        query: &ListQuery,
        cursor: Option<&Cursor>,
    ) -> Result<Page<ChangeRequest>, ConnectorError> {
        self.ask("list", repo)?;
        let page: usize = match cursor {
            None => 0,
            Some(Cursor(text)) => (text.strip_prefix("page:").and_then(|n| n.parse().ok()))
                .ok_or_else(|| ConnectorError::failed("The service doesn't know that page."))?,
        };
        let mut matching = Vec::new();
        for native in &self.service.change_requests {
            let decoded = self.decode(native)?;
            if query.state.is_none_or(|state| decoded.state == state) {
                matching.push(decoded);
            }
        }
        let more = (page + 1) * PAGE < matching.len();
        let items = matching.into_iter().skip(page * PAGE).take(PAGE).collect();
        let next = match self.flaw() {
            Some(Flaw::CutsAt100) => None,
            Some(Flaw::RepeatsPage) if !more => Some(Cursor("page:0".into())),
            _ => more.then(|| Cursor(format!("page:{}", page + 1))),
        };
        Ok(Page { items, next })
    }

    fn change_request(&self, repo: &Repo, number: u64) -> Result<ChangeRequest, ConnectorError> {
        self.ask("get", repo)?;
        let native = (self
            .service
            .change_requests
            .iter()
            .find(|cr| cr.number == number))
        .ok_or_else(|| ConnectorError::NotFound(self.detail(&self.service.not_found())))?;
        self.decode(native)
    }

    fn checks(&self, repo: &Repo, _number: u64) -> Result<Checks, ConnectorError> {
        self.ask("checks", repo)?;
        Ok(Checks {
            summary: self.word(Family::ChecksState, "passed", ChecksState::from_api)?,
            failed: vec![],
        })
    }

    fn reviews(&self, repo: &Repo, _number: u64) -> Result<Vec<Review>, ConnectorError> {
        self.ask("reviews", repo)?;
        let state = self.word(Family::ReviewState, "approved", ReviewState::from_api)?;
        Ok(vec![Review {
            author: "reviewer".into(),
            state,
            commit: None,
        }])
    }

    fn merge(&self, repo: &Repo, _number: u64, _method: MergeMethod) -> Result<(), ConnectorError> {
        self.ask("merge", repo)
    }

    fn enqueue_merge(&self, repo: &Repo, _number: u64) -> Result<(), ConnectorError> {
        self.gated(Capability::MergeQueue, "enqueue", repo)
    }

    fn enable_auto_merge(
        &self,
        repo: &Repo,
        _number: u64,
        _method: MergeMethod,
    ) -> Result<(), ConnectorError> {
        self.gated(Capability::AutoMerge, "auto-merge", repo)
    }

    fn set_draft(&self, repo: &Repo, _number: u64, _draft: bool) -> Result<(), ConnectorError> {
        self.gated(Capability::Drafts, "draft", repo)
    }

    fn required_approvals(&self, repo: &Repo, _base: &str) -> Result<u32, ConnectorError> {
        self.gated(Capability::RequiredApprovals, "protection", repo)?;
        Ok(1)
    }

    fn code_owners(&self, repo: &Repo, _path: &str) -> Result<Vec<String>, ConnectorError> {
        self.gated(Capability::Codeowners, "codeowners", repo)?;
        Ok(vec!["acme/core".into()])
    }

    fn approve_as_reviewer(&self, repo: &Repo, _number: u64) -> Result<(), ConnectorError> {
        self.gated(Capability::ReviewerAccount, "approve", repo)
    }

    fn check_log(&self, repo: &Repo, _check: &str) -> Result<String, ConnectorError> {
        self.gated(Capability::CheckLogs, "log", repo)?;
        Ok("build passed".into())
    }

    fn rerun_checks(&self, repo: &Repo, _number: u64) -> Result<(), ConnectorError> {
        self.gated(Capability::Rerun, "rerun", repo)
    }
}
