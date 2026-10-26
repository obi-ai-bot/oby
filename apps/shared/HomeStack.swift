import SwiftUI

/// Where Home's stack of decisions is (the Mac's HomeDecisions, the iPhone's AskStack), and how its
/// cards come and go.
enum HomeStack {
    /// The card on top and its place in the stack.
    struct Place: Equatable {
        var id = ""
        var index = 0
    }

    /// The top card's place: the same card while it waits; gone (answered, put off), the one that
    /// took its place, round to the first after the last.
    static func index(of place: Place, in ids: [String]) -> Int {
        guard !ids.isEmpty else { return 0 }
        return ids.firstIndex(of: place.id) ?? place.index % ids.count
    }

    /// One step round the stack, either way, wrapping at both ends.
    static func step(_ index: Int, by step: Int, count: Int) -> Int {
        count > 0 ? ((index + step) % count + count) % count : 0
    }

    /// How the top card leaves: answered, it lifts off; gone round, it sinks back, drifting the way
    /// you went (the demo's ask-out, ask-next, ask-prev).
    enum Exit {
        case lift, next, prev

        var transition: AnyTransition {
            switch self {
            case .lift: .offset(y: -16).combined(with: .scale(scale: 0.98)).combined(with: .opacity)
            case .next: .offset(x: -16, y: 10).combined(with: .scale(scale: 0.95, anchor: .bottom)).combined(with: .opacity)
            case .prev: .offset(x: 16, y: 10).combined(with: .scale(scale: 0.95, anchor: .bottom)).combined(with: .opacity)
            }
        }

        /// The next card comes up from where it peeked.
        static let rise: AnyTransition = .offset(y: 8).combined(with: .scale(scale: 0.95, anchor: .bottom)).combined(with: .opacity)
    }
}
