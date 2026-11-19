//! What Oby knows about a code host without asking it: its names, its URLs, how a remote is
//! recognized as its, how it signs in, what it can do, and what its states mean.

use crate::remote::Repo;
use crate::words::Family;

/// The words a person sees for a host's things: "pull request" or "merge request".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nouns {
    pub change_request: &'static str,
    pub change_requests: &'static str,
    /// `PR`, `MR`.
    pub short: &'static str,
    pub issue: &'static str,
    pub issues: &'static str,
}

/// How a host signs in: its own CLI, keeping its own login, or obid as its OAuth client
/// (docs/contracts/connector-pattern.md's rule 5, proposed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignIn {
    /// The CLI (`gh`) and the command that signs it in (`gh auth login`).
    Cli {
        command: &'static str,
        login: &'static str,
    },
    /// obid as a public OAuth client (PKCE, a loopback redirect).
    OAuth,
}

/// What a host can do beyond reading and merging change requests. A host that can't is asked
/// nothing: its call answers [`crate::ConnectorError::Unsupported`]. No host's name is here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub merge_queue: bool,
    pub auto_merge: bool,
    pub drafts: bool,
    pub required_approvals: bool,
    pub codeowners: bool,
    /// A second account of the owner's may review (approve) what the first opened.
    pub reviewer_account: bool,
    pub check_logs: bool,
    pub rerun: bool,
}

impl Capabilities {
    pub fn has(&self, capability: Capability) -> bool {
        match capability {
            Capability::MergeQueue => self.merge_queue,
            Capability::AutoMerge => self.auto_merge,
            Capability::Drafts => self.drafts,
            Capability::RequiredApprovals => self.required_approvals,
            Capability::Codeowners => self.codeowners,
            Capability::ReviewerAccount => self.reviewer_account,
            Capability::CheckLogs => self.check_logs,
            Capability::Rerun => self.rerun,
        }
    }
}

/// One of [`Capabilities`], by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    MergeQueue,
    AutoMerge,
    Drafts,
    RequiredApprovals,
    Codeowners,
    ReviewerAccount,
    CheckLogs,
    Rerun,
}

impl Capability {
    pub const ALL: [Capability; 8] = [
        Capability::MergeQueue,
        Capability::AutoMerge,
        Capability::Drafts,
        Capability::RequiredApprovals,
        Capability::Codeowners,
        Capability::ReviewerAccount,
        Capability::CheckLogs,
        Capability::Rerun,
    ];

    /// What it is, after "doesn't offer".
    pub fn what(self) -> &'static str {
        match self {
            Capability::MergeQueue => "a merge queue",
            Capability::AutoMerge => "auto-merge",
            Capability::Drafts => "drafts",
            Capability::RequiredApprovals => "required approvals",
            Capability::Codeowners => "code owners",
            Capability::ReviewerAccount => "a second account to review with",
            Capability::CheckLogs => "check logs",
            Capability::Rerun => "re-running checks",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangeRequestLink {
    pub repo: Repo,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueLink {
    pub repo: Repo,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitLink {
    pub repo: Repo,
    pub sha: String,
}

/// One code host, as configured: plain data and pure functions, no I/O. One value per host the
/// owner set up (a public one, an Enterprise one), built from its [`crate::HostNames`].
///
/// Like T3 Code's `SourceControlDefinition`, but a trait over a configured value rather than a
/// record of functions per kind: an Enterprise hostname is the same code path as the public host,
/// never a special case.
pub trait CodeHostDefinition {
    /// A lower-case id: `github`, `gitlab`.
    fn kind(&self) -> &'static str;
    /// Its name, as a person reads it: `GitHub`.
    fn label(&self) -> &'static str;
    fn nouns(&self) -> Nouns;
    fn sign_in(&self) -> SignIn;
    fn capabilities(&self) -> Capabilities;
    /// Whether repositories may sit in nested groups (`group/sub/name`), as GitLab's do.
    fn nested_groups(&self) -> bool;

    /// `remote` recognized as one of this host's repositories ([`crate::HostNames::recognize`]
    /// does it for most); none for another host's or a malformed one, never a guess.
    fn recognize_remote(&self, remote: &str) -> Option<Repo>;

    fn change_request_url(&self, repo: &Repo, number: u64) -> String;
    /// None for a host without issues.
    fn issue_url(&self, repo: &Repo, number: u64) -> Option<String>;
    fn commit_url(&self, repo: &Repo, sha: &str) -> String;

    /// The change request a URL of this host names, with any trailing path, query or fragment
    /// (`…/files`, `?tab=checks`); none for any other URL.
    fn parse_change_request_url(&self, url: &str) -> Option<ChangeRequestLink>;
    fn parse_issue_url(&self, url: &str) -> Option<IssueLink>;
    fn parse_commit_url(&self, url: &str) -> Option<CommitLink>;

    /// The host's own words for a family's states, every one it may answer.
    fn native_words(&self, family: Family) -> &'static [&'static str];
    /// The obid word one of [`Self::native_words`] means, one of [`Family::api_words`]; none for a
    /// word the host isn't known to say, never a guess.
    fn map_native(&self, family: Family, native: &str) -> Option<&'static str>;
}
