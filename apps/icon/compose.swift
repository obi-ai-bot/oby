// Builds Oby's icons from Sangam's 2D Oby head (oby-head.png: white, its eyes cut out, its ears
// up), rising into black from the icon's bottom edge, as in his design (design/app/build).
// Usage: swift apps/icon/compose.swift apps/icon/oby-head.png <icon folder>

import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

let arguments = CommandLine.arguments
guard arguments.count == 3 else { fatalError("usage: compose.swift <oby-head.png> <Icon folder>") }
let icon = URL(filePath: arguments[2])
guard let source = CGImageSourceCreateWithURL(URL(filePath: arguments[1]) as CFURL, nil),
      let oby = CGImageSourceCreateImageAtIndex(source, 0, nil) else { fatalError("can't read the head") }

/// The share of the head's height, from its top, where its sides stop being straight (its bottom
/// corners start curving in): below it the sides carry straight on down past the icon's edge.
let straight: CGFloat = {
    let width = oby.width, height = oby.height
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    let context = CGContext(data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.draw(oby, in: CGRect(x: 0, y: 0, width: width, height: height))
    // rows in the buffer run top to bottom; the left column, from halfway down
    let row = (height / 2..<height).first { pixels[($0 * width) * 4 + 3] < 250 } ?? height
    return CGFloat(row - 8) / CGFloat(height)
}()

let p3 = CGColorSpace(name: CGColorSpace.displayP3)!

/// Oby rising into `box` from its bottom edge: 89% of its width wide, its ears' tips 29% down.
func rise(_ context: CGContext, in box: CGRect) {
    let width = box.width * 0.893
    let height = width * CGFloat(oby.height) / CGFloat(oby.width)
    let head = CGRect(x: box.midX - width / 2, y: box.maxY - box.height * 0.289 - height, width: width, height: height)
    context.interpolationQuality = .high
    context.draw(oby, in: head)
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: head.minX, y: 0, width: width, height: head.maxY - height * straight))
}

/// Oby's dark: #585858 at the top to black at the bottom (AppIcon.icon's fill, the same).
let night = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB)!,
                       colors: [CGColor(srgbRed: 0x58 / 255, green: 0x58 / 255, blue: 0x58 / 255, alpha: 1), CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 1)] as CFArray,
                       locations: [0, 1])!

/// A square canvas: its background (Oby's dark, unless `background` is false), then Oby rising
/// into `box` (the whole canvas if nil), all inside `corner`'s rounded square when there is one.
func canvas(_ size: Int, oby show: Bool = true, box: CGRect? = nil, corner: CGFloat = 0, background: Bool = true) -> CGImage {
    let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: p3,
                            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let side = CGFloat(size), all = CGRect(x: 0, y: 0, width: side, height: side)
    if corner > 0 {
        context.addPath(CGPath(roundedRect: all, cornerWidth: corner, cornerHeight: corner, transform: nil))
        context.clip()
    }
    if background { context.drawLinearGradient(night, start: CGPoint(x: 0, y: side), end: .zero, options: []) }
    if show { rise(context, in: box ?? all) }
    return context.makeImage()!
}

func write(_ image: CGImage, _ path: String, size: Int? = nil) {
    let url = icon.appending(path: path)
    try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    var out = image
    if let size, size != image.width {
        let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: p3,
                                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: size, height: size))
        out = context.makeImage()!
    }
    let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, out, nil)
    CGImageDestinationFinalize(destination)
}

// Apple: one Icon Composer document for iPhone, iPad, Mac and Apple Watch. Oby rises on its own
// layer; the document's fill is Oby's dark, and the system draws the shape, Liquid Glass, and the
// dark, tinted and clear looks from them.
write(canvas(1024, background: false), "AppIcon.icon/Assets/oby.png")

// Everywhere else, from one 1024 master: full-bleed square, rounded square, and Android's layers.
let square = canvas(1024)
let rounded = canvas(1024, corner: 1024 * 0.225)
// the Electron app's Dock icon while developing, which macOS draws as it is: AppIcon.icon as macOS
// renders it (actool, its default look, with the system's edge and shading), at the 256 px its
// .icns goes up to, the most the Dock ever draws (128 pt, magnified); without Xcode, the
// flat rounded square on macOS's icon grid (840 of 1024, 92 in from each edge, 189 round)
func compiled() -> CGImage? {
    let temp = FileManager.default.temporaryDirectory.appending(path: "oby-icon-\(getpid())")
    defer { try? FileManager.default.removeItem(at: temp) }
    try? FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
    func run(_ arguments: [String]) -> Bool {
        let process = Process()
        process.executableURL = URL(filePath: "/usr/bin/xcrun")
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return false }
        process.waitUntilExit()
        return process.terminationStatus == 0
    }
    let iconset = temp.appending(path: "AppIcon.iconset")
    guard run(["actool", icon.appending(path: "AppIcon.icon").path, "--compile", temp.path, "--platform", "macosx",
               "--minimum-deployment-target", "14.0", "--app-icon", "AppIcon",
               "--output-partial-info-plist", temp.appending(path: "info.plist").path]),
          run(["iconutil", "-c", "iconset", temp.appending(path: "AppIcon.icns").path, "-o", iconset.path]),
          let data = try? Data(contentsOf: iconset.appending(path: "icon_128x128@2x.png")),  // read now: the folder goes
          let source = CGImageSourceCreateWithData(data as CFData, nil)
    else { return nil }
    return CGImageSourceCreateImageAtIndex(source, 0, nil)
}
if let dock = compiled() {
    write(dock, "exports/oby-1024.png", size: 1024)
} else {
    let dock = CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8, bytesPerRow: 0, space: p3,
                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    dock.interpolationQuality = .high
    dock.draw(canvas(840, corner: 189), in: CGRect(x: 92, y: 92, width: 840, height: 840))
    write(dock.makeImage()!, "exports/oby-1024.png")
    print("no actool (Xcode): the Dock icon while developing is the flat rounded square")
}
write(canvas(1024, background: false), "exports/oby-foreground-1024.png")
// Android and Wear OS adaptive icons: 108 dp layers at xxxhdpi (432 px), of which a 72 dp square
// shows; Oby rises from the bottom of that, its sides on down to the layer's edge.
write(canvas(432, box: CGRect(x: 72, y: 72, width: 288, height: 288), background: false), "exports/android/ic_launcher_foreground.png")
write(canvas(432, oby: false), "exports/android/ic_launcher_background.png")
write(square, "exports/android/play-store-512.png", size: 512)
// Windows and Linux: the rounded square, at the sizes each one asks for.
for size in [16, 24, 32, 48, 64, 128, 256, 512] { write(rounded, "exports/desktop/oby-\(size).png", size: size) }

// .ico: a header, one directory entry per size, then each size as a PNG (Windows reads PNG entries).
func png(_ image: CGImage, size: Int) -> Data {
    let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.interpolationQuality = .high
    context.draw(image, in: CGRect(x: 0, y: 0, width: size, height: size))
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, context.makeImage()!, nil)
    CGImageDestinationFinalize(destination)
    return data as Data
}
let sizes = [16, 24, 32, 48, 64, 128, 256]
let images = sizes.map { png(rounded, size: $0) }
var ico = Data([0, 0, 1, 0, UInt8(sizes.count), 0])
var offset = 6 + 16 * sizes.count
for (size, image) in zip(sizes, images) {
    let side = UInt8(size == 256 ? 0 : size)  // 0 means 256
    ico += [side, side, 0, 0, 1, 0, 32, 0]
    ico += withUnsafeBytes(of: UInt32(image.count).littleEndian) { Data($0) }
    ico += withUnsafeBytes(of: UInt32(offset).littleEndian) { Data($0) }
    offset += image.count
}
images.forEach { ico += $0 }
try! ico.write(to: icon.appending(path: "exports/desktop/oby.ico"))

// The web: favicons and the home-screen icon (iOS rounds that one itself).
write(rounded, "exports/web/favicon-32.png", size: 32)
write(rounded, "exports/web/icon-192.png", size: 192)
write(rounded, "exports/web/icon-512.png", size: 512)
write(square, "exports/web/apple-touch-icon.png", size: 180)
print("icons written to \(icon.path)")
