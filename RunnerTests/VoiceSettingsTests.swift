import Foundation
import Testing
@testable import ObyShared

/// What dictation listens for, the same on the iPhone, the Watch and the Mac: English only (the
/// owner's decision of 2026-11-01).
struct VoiceSettingsTests {
    @Test func dictationListensInUSEnglish() {
        #expect(VoiceSettings.locale.identifier(.bcp47) == "en-US")
        #expect(VoiceSettings.name(VoiceSettings.locale) == "English")
    }
}
