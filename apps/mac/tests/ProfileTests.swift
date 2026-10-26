import AppKit
import Foundation
import Testing
@testable import Oby

/// Your name and picture (`Profile`): kept on this Mac, a blank name back to your macOS name, and a
/// picture cut to 256 pixels a side and under 200 KB.
@MainActor
struct ProfileTests {
    let file = URL(filePath: NSTemporaryDirectory()).appending(path: "profile-\(UUID().uuidString)")

    /// `width` × `height` of random pixels: a photo as busy as one gets.
    static func noise(width: Int, height: Int) -> NSImage {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 3,
                                   hasAlpha: false, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        for index in 0..<(rep.bytesPerRow * height) { rep.bitmapData![index] = .random(in: 0...255) }
        let image = NSImage(size: NSSize(width: width, height: height))
        image.addRepresentation(rep)
        return image
    }

    @Test func aNameIsKeptAndABlankOneIsYourMacName() throws {
        let defaults = UserDefaults.scratch
        let profile = Profile(defaults: defaults, file: file)
        #expect(profile.name == NSFullUserName())

        try profile.save(name: "  Dmytro Kovalenko ", picture: nil)
        #expect(profile.name == "Dmytro Kovalenko")
        #expect(Profile(defaults: defaults, file: file).name == "Dmytro Kovalenko")
        // Home's greeting asks by it.
        #expect(HomeLine.placeholder(fullName: profile.name) == "What do you want me to work on, Dmytro?")

        try profile.save(name: " ", picture: nil)
        #expect(Profile(defaults: defaults, file: file).name == NSFullUserName())
    }

    @Test func aPictureIsKeptSmallAndTakenOut() throws {
        defer { try? FileManager.default.removeItem(at: file) }
        let profile = Profile(defaults: .scratch, file: file)
        #expect(profile.picture == nil)

        try profile.save(name: "", picture: Self.noise(width: 1200, height: 800))
        let kept = try Data(contentsOf: file)
        #expect(kept.count <= 200_000)
        #expect(NSBitmapImageRep(data: kept)?.pixelsWide == 256)
        #expect(NSBitmapImageRep(data: kept)?.pixelsHigh == 256)
        #expect(Profile(defaults: .scratch, file: file).picture != nil)

        // Saved again unchanged, it isn't cut again.
        let shown = try #require(profile.picture)
        try profile.save(name: "Dmytro", picture: shown)
        #expect(try Data(contentsOf: file) == kept)

        try profile.save(name: "Dmytro", picture: nil)
        #expect(profile.picture == nil)
        #expect(!FileManager.default.fileExists(atPath: file.path))
    }

    /// A plain picture stays a PNG; only a busy one becomes a JPEG to stay under 200 KB.
    @Test func aPlainPictureStaysAPNG() throws {
        let plain = NSImage(size: NSSize(width: 400, height: 400), flipped: false) { rect in
            NSColor.systemTeal.setFill()
            rect.fill()
            return true
        }
        let png = try #require(Profile.small(plain))
        #expect(png.starts(with: [0x89, 0x50, 0x4E, 0x47]))
        let busy = try #require(Profile.small(Self.noise(width: 600, height: 600)))
        #expect(busy.starts(with: [0xFF, 0xD8]))
        #expect(busy.count <= 200_000)
    }
}
