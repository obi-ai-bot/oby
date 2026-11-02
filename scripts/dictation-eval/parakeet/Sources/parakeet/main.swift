// Transcribes recordings with Parakeet TDT 0.6B v3 and writes one JSON file per clip, in the shape
// transcribe.swift prints: the text, its confidence and the seconds it took. The model loads once
// for every clip; it downloads from Hugging Face the first time. Nothing else leaves the Mac.
//
//   parakeet <output dir> <audio file>…
import FluidAudio
import Foundation

struct Result: Encodable {
    let text: String
    let confidence: Double
    let seconds: Double
}

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write(Data("usage: parakeet <output dir> <audio file>…\n".utf8))
    exit(1)
}
let out = URL(fileURLWithPath: args[1])
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

let models = try await AsrModels.downloadAndLoad(version: .v3)
let asr = AsrManager(config: ASRConfig(tdtConfig: TdtConfig(blankId: AsrModelVersion.v3.blankId),
                                       encoderHiddenSize: AsrModelVersion.v3.encoderHiddenSize))
try await asr.loadModels(models)

// ponytail: no hints yet; FluidAudio's custom vocabulary needs a second (CTC) model, add it if terms are missed
for path in args.dropFirst(2) {
    let clip = URL(fileURLWithPath: path)
    let name = clip.deletingPathExtension().lastPathComponent
    do {
        var state = TdtDecoderState.make(decoderLayers: await asr.decoderLayerCount)
        let start = Date()
        let result = try await asr.transcribe(clip, decoderState: &state)
        let row = Result(text: result.text.trimmingCharacters(in: .whitespacesAndNewlines),
                         confidence: Double(result.confidence), seconds: Date().timeIntervalSince(start))
        try JSONEncoder().encode(row).write(to: out.appending(path: "\(name).json"))
    } catch {
        FileHandle.standardError.write(Data("parakeet failed on \(name): \(error)\n".utf8))
    }
}
