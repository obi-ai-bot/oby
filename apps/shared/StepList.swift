import SwiftUI

extension TurnStep.State {
    /// What VoiceOver says for the step's mark.
    var spoken: String {
        switch self {
        case .running: "Working"
        case .done: "Done"
        case .failed: "Failed"
        }
    }
}

