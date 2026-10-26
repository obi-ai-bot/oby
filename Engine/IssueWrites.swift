import Foundation

/// Where this engine may write GitHub issues: nowhere, unless a test turns `Desk.writesIssues` on.
/// Agents' findings, review follow-ups and failures go to obid's private findings ledger
/// (core/crates/obid/src/ledger.rs, docs/runner-api.md, "obid only: the ledger"), through an App
/// that can write that repository's issues and no other. The Swift runner is frozen
/// (core/crates/obid/tests/PORTING.md) and doesn't write the ledger, so it files, labels, claims
/// and closes no issue: nothing it does lands in a repository's issues, public ones above all.
/// What it found stays in the job's report and Oby's inbox.
nonisolated enum IssueWrites {
    /// Why an issue wasn't written, for the record or the API's answer.
    static let off = "Oby writes issues only in its private findings ledger, and this runner doesn't write it."
}
