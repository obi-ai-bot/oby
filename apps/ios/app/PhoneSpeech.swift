import AVFoundation
import Speech
import UIKit

/// One on-device analyzer for a hold. Input and result streams live independently; an explicit
/// end finalizes them. Expiration, cancellation and a lost sender close them without planning.
@MainActor
final class PhoneSpeech {
    let request: String
    private let sourceFormat: AVAudioFormat
    private let analyzerFormat: AVAudioFormat
    private let converter: AVAudioConverter
    private let analyzer: SpeechAnalyzer
    private let input: AsyncStream<AnalyzerInput>.Continuation
    private var results: Task<String, Error>?
    private var expiry: Task<Void, Never>?
    private var ending = false
    private var onCancel: (() -> Void)?
    private(set) var cancelled = false

    private init(request: String, sourceFormat: AVAudioFormat, analyzerFormat: AVAudioFormat,
                 converter: AVAudioConverter, analyzer: SpeechAnalyzer,
                 input: AsyncStream<AnalyzerInput>.Continuation) {
        self.request = request
        self.sourceFormat = sourceFormat
        self.analyzerFormat = analyzerFormat
        self.converter = converter
        self.analyzer = analyzer
        self.input = input
    }

    /// English only (the owner's decision of 2026-11-01).
    static var locale: Locale { VoiceSettings.locale }

    static func context(repos: [String]) -> [String] {
        var names = ["Oby", "Claude", "Claude Code", "Codex", "Cursor", "Gemini", "GitHub", "PR", "pull request"]
        for repo in repos {
            let name = String(repo.split(separator: "/").last ?? Substring(repo))
            names += [repo, name, String(name.map { "-_.".contains($0) ? " " : $0 })]
        }
        var seen = Set<String>()
        return Array(names.filter { seen.insert($0.lowercased()).inserted }.prefix(100))
    }

    private static var downloading = false

    private static func transcriber() async throws -> SpeechTranscriber {
        guard let supported = await SpeechTranscriber.supportedLocale(equivalentTo: locale) else {
            throw RunnerError(message: "Your iPhone can't transcribe this language yet.")
        }
        let transcriber = SpeechTranscriber(locale: supported, transcriptionOptions: [],
                                           reportingOptions: [.volatileResults], attributeOptions: [])
        if await AssetInventory.status(forModules: [transcriber]) != .installed {
            if !downloading, let download = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
                downloading = true
                Task {
                    defer { downloading = false }
                    // Give a model download time to continue if the phone goes to the background.
                    nonisolated(unsafe) var background = UIBackgroundTaskIdentifier.invalid
                    background = UIApplication.shared.beginBackgroundTask(withName: "speech model") {
                        UIApplication.shared.endBackgroundTask(background)
                        background = .invalid
                    }
                    defer { if background != .invalid { UIApplication.shared.endBackgroundTask(background) } }
                    try? await download.downloadAndInstall()
                }
            }
            throw RunnerError(message: "Your iPhone is getting the speech model. Try again in a minute.")
        }
        return transcriber
    }

    static let phoneTapFrames: AVAudioFrameCount = 2048

    /// Tap size is ~43 ms at 48 kHz: phone buffers get a duration budget.
    static func phoneBufferCapacity(sampleRate: Double) -> Int {
        max(128, Int(ceil(sampleRate * 8 / Double(phoneTapFrames))))
    }

    static func make(request: String, sourceFormat source: AVAudioFormat, context: [String] = [], onCancel: (() -> Void)? = nil, partial: @escaping (String) -> Void) async throws -> PhoneSpeech {
        let transcriber = try await transcriber()
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]),
              let converter = AVAudioConverter(from: source, to: format) else {
            throw RunnerError(message: "Your iPhone can't use this audio format.")
        }
        converter.primeMethod = .none
        // Phone conversion can yield more than one analyzer buffer per tap; allow twice
        // the capture queue's budget.
        let capacity = phoneBufferCapacity(sampleRate: source.sampleRate) * 2
        let (stream, input) = AsyncStream.makeStream(of: AnalyzerInput.self, bufferingPolicy: .bufferingOldest(capacity))
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let hints = AnalysisContext()
        hints.contextualStrings[.general] = context
        try await analyzer.setContext(hints)
        let live = PhoneSpeech(request: request, sourceFormat: source, analyzerFormat: format,
                               converter: converter, analyzer: analyzer, input: input)
        live.onCancel = onCancel
        live.results = Task {
            var finalized = AttributedString()
            var volatile = AttributedString()
            for try await result in transcriber.results {
                try Task.checkCancellation()
                if result.isFinal { finalized += result.text; volatile = AttributedString() }
                else { volatile = result.text }
                partial(String((finalized + volatile).characters).trimmingCharacters(in: .whitespacesAndNewlines))
            }
            return String(finalized.characters).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        do { try await analyzer.start(inputSequence: stream) }
        catch { live.cancel(); throw error }
        live.armExpiry()
        return live
    }

    /// Closes the stream when no audio comes: 30 seconds with no new buffer, counted again on each
    /// one. Finalization gets its own 30 seconds.
    private func armExpiry() {
        expiry?.cancel()
        expiry = Task { [weak self] in
            try? await Task.sleep(for: .seconds(30))
            guard !Task.isCancelled else { return }
            self?.cancel()
        }
    }

    func append(_ buffer: AVAudioPCMBuffer) throws {
        guard !cancelled, !ending else { throw CancellationError() }
        try convert(buffer)
        armExpiry()
    }

    /// Reports interruptions as phone listening errors.
    func finish() async throws -> String {
        guard !cancelled, !ending else {
            throw RunnerError(message: "Listening was interrupted. Tap the mic to try again.")
        }
        do { return try await finalize() }
        catch is CancellationError {
            throw RunnerError(message: "Listening was interrupted. Tap the mic to try again.")
        }
    }

    /// A persistent converter preserves resampling across chunk boundaries; drain it at end.
    private func convert(_ buffer: AVAudioPCMBuffer?) throws {
        let capacity = buffer.map { Int(ceil(Double($0.frameLength) * analyzerFormat.sampleRate / sourceFormat.sampleRate)) + 32 } ?? 1024
        var supplied = false
        while true {
            guard let output = AVAudioPCMBuffer(pcmFormat: analyzerFormat, frameCapacity: AVAudioFrameCount(capacity)) else {
                throw RunnerError(message: "Your iPhone couldn't convert the audio.")
            }
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, status in
                guard let buffer else { status.pointee = .endOfStream; return nil }
                if supplied { status.pointee = .noDataNow; return nil }
                supplied = true
                status.pointee = .haveData
                return buffer
            }
            if let error { throw error }
            if status == .error { throw RunnerError(message: "Your iPhone couldn't convert the audio.") }
            if output.frameLength > 0 {
                if case .dropped = input.yield(AnalyzerInput(buffer: output)) {
                    throw RunnerError(message: "Your iPhone couldn't keep up with the audio.")
                }
            }
            if status != .haveData { return }
        }
    }

    private func finalize() async throws -> String {
        ending = true
        armExpiry()
        defer { cleanup() }
        do {
            try convert(nil)
            input.finish()
            try await analyzer.finalizeAndFinishThroughEndOfInput()
            let text = try await results?.value ?? ""
            guard !cancelled else { throw CancellationError() }
            return text
        } catch { cancel(); throw error }
    }

    func cancel() {
        guard !cancelled else { return }
        cancelled = true
        input.finish()
        results?.cancel()
        Task { await analyzer.cancelAndFinishNow() }
        cleanup()
        let notify = onCancel
        onCancel = nil
        notify?()
    }

    private func cleanup() {
        expiry?.cancel()
        expiry = nil
    }
}
