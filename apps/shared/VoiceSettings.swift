import Foundation

/// Speech is English only (the owner's decision of 2026-11-01): Oby writes down what you say in US
/// English on the iPhone, the Apple Watch and the Mac, whatever the device's own language.
nonisolated enum VoiceSettings {
    /// What dictation listens for.
    static let locale = Locale(identifier: "en-US")

    /// "English": the language's name in itself, for a sentence like "This Mac can't write down
    /// English yet".
    static func name(_ locale: Locale) -> String {
        let code = locale.language.languageCode?.identifier ?? locale.identifier
        return locale.localizedString(forLanguageCode: code)?.capitalized(with: locale) ?? code
    }
}
