import Foundation

/// What only you can answer, from the runner's jobs, the longest-waiting first (docs/pipeline-cases.md,
/// I.1): Home's decisions, the conversation's cards and the menu bar's count read it.
enum IslandInbox {
    /// An item of the runner's inbox.
    struct Entry: Identifiable {
        var item: InboxItem
        var id: String { "runner/" + item.id }
    }

    /// Whether anything waits now.
    static func waits(feed: RunnerFeed, now: Date = .now) -> Bool {
        !entries(feed: feed, now: now).isEmpty
    }

    static func entries(feed: RunnerFeed, now: Date) -> [Entry] {
        InboxItem.waiting(feed.inbox, now: now).map { Entry(item: $0) }.sorted { $0.item.created < $1.item.created }
    }
}
