// swift-tools-version: 6.0
// NVIDIA's Parakeet TDT 0.6B v3 on this Mac through FluidAudio's Core ML build (#1749).
import PackageDescription

let package = Package(
    name: "parakeet",
    platforms: [.macOS(.v14)],
    dependencies: [
        // a tag at least two weeks old; v0.18.x is newer
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7"),
    ],
    targets: [
        .executableTarget(name: "parakeet", dependencies: [.product(name: "FluidAudio", package: "FluidAudio")]),
    ]
)
