import SwiftUI

/// Facts about a job that its card, its page and its conversation all show.
enum JobSummary {
    /// The account's name as the runner knows it, else its id.
    static func account(of job: Job, feed: RunnerFeed) -> String {
        feed.accounts.first { $0.account.id == job.account }?.account.name ?? job.account
    }

    /// How far along a job is, 0 to 1, from its pipeline's stage and, within Work, its round; nil
    /// when the pipeline can't say (no stages).
    nonisolated static func progress(_ job: Job) -> Double? {
        let pipeline = job.pipeline
        guard pipeline.total > 0 else { return nil }
        var round = 0.0
        if pipeline.stages.first(where: { $0.state == .current })?.id == .work, job.maxRounds > 1 {
            round = Double(job.currentRound - 1) / Double(job.maxRounds)
        }
        return min(1, max(0, (Double(pipeline.number - 1) + round) / Double(pipeline.total)))
    }
}
