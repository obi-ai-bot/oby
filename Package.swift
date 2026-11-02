// swift-tools-version: 6.2
import PackageDescription

/// What the Xcode project compiles Oby's Swift with, so oby-mcp compiles the same way here.
let features: [SwiftSetting] = [
    "MemberImportVisibility", "BareSlashRegexLiterals",
    // SWIFT_APPROACHABLE_CONCURRENCY, spelled out.
    "DisableOutwardActorInference", "GlobalActorIsolatedTypesUsability", "InferIsolatedConformances",
    "InferSendableFromCaptures", "NonisolatedNonsendingByDefault",
].map { .enableUpcomingFeature($0) }

let package = Package(
    name: "Oby",
    platforms: [.macOS("26.0")],
    targets: [
        // Oby's runner as an MCP server over stdio, for Claude Code and other assistants (docs/mcp.md).
        .executableTarget(name: "oby-mcp", path: "mcp/server", swiftSettings: features),
        .testTarget(name: "MCPTests", dependencies: ["oby-mcp"], path: "mcp/tests", swiftSettings: features),
    ],
    swiftLanguageModes: [.v5]
)
