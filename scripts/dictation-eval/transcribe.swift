// Transcribes one recording on this Mac with Apple's SpeechTranscriber, the recognizer the iPhone
// app uses (apps/ios/app/PhoneSpeech.swift), and prints one JSON line: the text, its mean
// confidence and the seconds it took. Nothing leaves the Mac.
//
//   transcribe <locale>[,<locale>…] <audio file> [terms file]
//
// With several locales it runs each on the same audio and keeps the most confident (DICTEVAL-3).
// The terms file (one per line) goes in as contextual strings, as Oby's own vocabulary would.
import AVFoundation
import Foundation
import Speech

struct Result: Encodable {
    let locale: String
    let text: String
    let confidence: Double
    let seconds: Double
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(1)
}

func transcribe(_ url: URL, locale wanted: String, terms: [String]) async throws -> Result {
    guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: wanted)) else {
        fail("SpeechTranscriber doesn't support \(wanted) on this Mac")
    }
    let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [],
                                        attributeOptions: [.transcriptionConfidence])
    // no request means the model is already there (the status can lag right after an install)
    if await AssetInventory.status(forModules: [transcriber]) != .installed,
       let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
        FileHandle.standardError.write(Data("installing the \(wanted) speech model…\n".utf8))
        try await request.downloadAndInstall()
    }
    let start = Date()
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    if !terms.isEmpty {
        let hints = AnalysisContext()
        hints.contextualStrings[.general] = terms
        try await analyzer.setContext(hints)
    }
    async let finals = transcriber.results.reduce(into: [AttributedString]()) { all, next in
        if next.isFinal { all.append(next.text) }
    }
    let file = try AVAudioFile(forReading: url)
    if let last = try await analyzer.analyzeSequence(from: file) {
        try await analyzer.finalizeAndFinish(through: last)
    } else {
        await analyzer.cancelAndFinishNow()
    }
    let parts = try await finals
    var text = ""
    var weighted = 0.0, chars = 0.0
    for part in parts {
        text += String(part.characters)
        for run in part.runs {
            guard let confidence = run.transcriptionConfidence else { continue }
            let n = Double(part[run.range].characters.count)
            weighted += confidence * n
            chars += n
        }
    }
    return Result(locale: wanted, text: text.trimmingCharacters(in: .whitespacesAndNewlines),
                  confidence: chars > 0 ? weighted / chars : 0, seconds: Date().timeIntervalSince(start))
}

let args = CommandLine.arguments
guard args.count >= 3 else { fail("usage: transcribe <locale>[,<locale>…] <audio file> [terms file]") }
let locales = args[1].split(separator: ",").map(String.init)
let url = URL(fileURLWithPath: args[2])
let terms = args.count > 3
    ? ((try? String(contentsOfFile: args[3], encoding: .utf8)) ?? "").split(separator: "\n").map(String.init).filter { !$0.isEmpty }
    : []

do {
    var results: [Result] = []
    for locale in locales { results.append(try await transcribe(url, locale: locale, terms: terms)) }
    // ponytail: the most confident whole transcript wins; mixing per-segment is phase 3's question
    let best = results.max { $0.confidence < $1.confidence }!
    let total = results.reduce(0) { $0 + $1.seconds }
    let out = Result(locale: best.locale, text: best.text, confidence: best.confidence, seconds: total)
    print(String(decoding: try JSONEncoder().encode(out), as: UTF8.self))
} catch {
    fail("\(error)")
}
