//! What Oby asks a code host, as plain data: the shape of T3 Code's `PullRequestProvider`, but
//! synchronous and with no I/O of its own. obid's adapters do the I/O (a CLI, HTTP) behind it.

use std::collections::HashSet;

use crate::definition::{Capability, CodeHostDefinition};
use crate::error::ConnectorError;
use crate::remote::Repo;
use crate::words::{
    ChangeRequestState, ChecksState, MergeState, Mergeable, ReviewDecision, ReviewState,
};

/// Who the host says Oby is signed in as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub login: String,
}

/// A change request, typed status only: never anything a person wrote but its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRequest {
    pub number: u64,
    pub title: String,
    /// Built by the definition, never taken from the host's answer.
    pub url: String,
    pub base: String,
    pub head_branch: String,
    /// Its head commit as the host has it.
    pub head: Option<String>,
    pub state: ChangeRequestState,
    pub review_decision: Option<ReviewDecision>,
    pub merge_state: Option<MergeState>,
    pub mergeable: Option<Mergeable>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checks {
    pub summary: ChecksState,
    /// The names of the checks that failed.
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub author: String,
    pub state: ReviewState,
    /// The commit it reviewed.
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListQuery {
    /// Only these; all of them when none.
    pub state: Option<ChangeRequestState>,
}

/// Where the next page starts, as the host said: opaque to everyone but its adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// None on the last page.
    pub next: Option<Cursor>,
}

/// A code host's calls. Every call names its repository; the capability-gated ones answer
/// [`ConnectorError::Unsupported`] without reaching the host when its definition doesn't declare
/// the capability: start each with [`require`].
pub trait CodeHost {
    type Definition: CodeHostDefinition;

    fn definition(&self) -> &Self::Definition;

    fn viewer(&self) -> Result<Viewer, ConnectorError>;
    fn can_push(&self, repo: &Repo) -> Result<bool, ConnectorError>;
    /// One page, as the host serves it; [`list_all`] follows them.
    fn list_change_requests(
        &self,
        repo: &Repo,
        query: &ListQuery,
        cursor: Option<&Cursor>,
    ) -> Result<Page<ChangeRequest>, ConnectorError>;
    fn change_request(&self, repo: &Repo, number: u64) -> Result<ChangeRequest, ConnectorError>;
    fn checks(&self, repo: &Repo, number: u64) -> Result<Checks, ConnectorError>;
    /// Each reviewer's latest review that counts.
    fn reviews(&self, repo: &Repo, number: u64) -> Result<Vec<Review>, ConnectorError>;
    fn merge(&self, repo: &Repo, number: u64, method: MergeMethod) -> Result<(), ConnectorError>;

    /// [`Capability::MergeQueue`].
    fn enqueue_merge(&self, repo: &Repo, number: u64) -> Result<(), ConnectorError>;
    /// [`Capability::AutoMerge`].
    fn enable_auto_merge(
        &self,
        repo: &Repo,
        number: u64,
        method: MergeMethod,
    ) -> Result<(), ConnectorError>;
    /// [`Capability::Drafts`].
    fn set_draft(&self, repo: &Repo, number: u64, draft: bool) -> Result<(), ConnectorError>;
    /// [`Capability::RequiredApprovals`]: the approving reviews a merge into `base` needs.
    fn required_approvals(&self, repo: &Repo, base: &str) -> Result<u32, ConnectorError>;
    /// [`Capability::Codeowners`]: who owns `path`.
    fn code_owners(&self, repo: &Repo, path: &str) -> Result<Vec<String>, ConnectorError>;
    /// [`Capability::ReviewerAccount`]: approve as the owner's reviewer account.
    fn approve_as_reviewer(&self, repo: &Repo, number: u64) -> Result<(), ConnectorError>;
    /// [`Capability::CheckLogs`]: one check's log, someone's words, only ever quoted to an agent.
    fn check_log(&self, repo: &Repo, check: &str) -> Result<String, ConnectorError>;
    /// [`Capability::Rerun`]: re-run the head's failed checks.
    fn rerun_checks(&self, repo: &Repo, number: u64) -> Result<(), ConnectorError>;
}

/// Ok when `definition` declares `capability`; else the polite refusal, before anything is asked.
pub fn require<D: CodeHostDefinition + ?Sized>(
    definition: &D,
    capability: Capability,
) -> Result<(), ConnectorError> {
    if definition.capabilities().has(capability) {
        Ok(())
    } else {
        Err(ConnectorError::Unsupported {
            host: definition.label(),
            capability,
        })
    }
}

/// The most pages [`list_all`] follows: past it, the list is said to be partial, never cut.
pub const MAX_PAGES: usize = 1000;

/// Every change request `query` matches, every page followed (T3 Code's 9f815c94af: a list cut
/// at 100). A host that repeats a page, or has more than [`MAX_PAGES`], fails rather than loop or
/// answer part of the list as the whole.
pub fn list_all<H: CodeHost + ?Sized>(
    host: &H,
    repo: &Repo,
    query: &ListQuery,
) -> Result<Vec<ChangeRequest>, ConnectorError> {
    let mut all = Vec::new();
    let mut cursor: Option<Cursor> = None;
    let mut seen = HashSet::new();
    for _ in 0..MAX_PAGES {
        let page = host.list_change_requests(repo, query, cursor.as_ref())?;
        all.extend(page.items);
        let Some(next) = page.next else {
            return Ok(all);
        };
        if !seen.insert(next.clone()) {
            return Err(ConnectorError::failed(
                "The host gave the same page twice; the list is partial.",
            ));
        }
        cursor = Some(next);
    }
    Err(ConnectorError::failed(&format!(
        "The list goes past {MAX_PAGES} pages; it is partial."
    )))
}
