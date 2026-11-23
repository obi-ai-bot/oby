// oby-picture-text: the text macOS reads in one image, for the secret scan of an Obysphere picture
// before it leaves (main/pictureText.ts, docs/contracts/sphere-app.md SPHEREAPP-18). Vision's own
// text recognition, on this Mac, nothing sent anywhere. Usage: oby-picture-text <image>. Prints
// each line of text it reads, up to three readings of each, one a line; exit 0 when it read the
// image (no text prints nothing), anything else when it couldn't, which sends nothing.
import CoreGraphics
import Foundation
import ImageIO
import Vision

func fail(_ code: Int32, _ why: String) -> Never {
  FileHandle.standardError.write(Data("oby-picture-text: \(why)\n".utf8))
  exit(code)
}

let args = CommandLine.arguments
guard args.count == 2 else { fail(64, "usage: oby-picture-text <image>") }
guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil),
  let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
else { fail(65, "can't read the image") }

// A picture is small (256 px a side): drawn 4 times larger, Vision reads small text in it too
let scale = 4
guard let space = CGColorSpace(name: CGColorSpace.sRGB),
  let canvas = CGContext(
    data: nil, width: image.width * scale, height: image.height * scale, bitsPerComponent: 8,
    bytesPerRow: 0, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
else { fail(70, "can't draw the image") }
// on white, so text on a clear background still reads
canvas.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
canvas.fill(CGRect(x: 0, y: 0, width: canvas.width, height: canvas.height))
canvas.interpolationQuality = .high
canvas.draw(image, in: CGRect(x: 0, y: 0, width: canvas.width, height: canvas.height))
guard let larger = canvas.makeImage() else { fail(70, "can't draw the image") }

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
// a token isn't a word: no correcting it into one
request.usesLanguageCorrection = false
do {
  try VNImageRequestHandler(cgImage: larger, options: [:]).perform([request])
} catch {
  fail(70, "Vision couldn't read the image: \(error.localizedDescription)")
}
var out = ""
for observation in request.results ?? [] {
  for candidate in observation.topCandidates(3) { out += candidate.string + "\n" }
}
FileHandle.standardOutput.write(Data(out.utf8))
