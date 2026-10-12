// swift-tools-version: 6.0
import PackageDescription

// Source checkouts generate this artifact with scripts/build-ffi.sh.
// Distribution packages include it and need neither Rust nor unsafe flags.
let package = Package(
    name: "SemanticCore",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SemanticCore", targets: ["SemanticCore"]),
        .library(name: "SemanticCoreSpotlight", targets: ["SemanticCoreSpotlight"]),
    ],
    targets: [
        .binaryTarget(name: "CSemanticCore", path: "Libraries/CSemanticCore.xcframework"),
        .target(
            name: "SemanticCore",
            dependencies: ["CSemanticCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "SemanticCoreTests",
            dependencies: ["SemanticCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .target(
            name: "SemanticCoreSpotlight",
            dependencies: ["SemanticCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "SemanticCoreSpotlightTests",
            dependencies: ["SemanticCoreSpotlight"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
