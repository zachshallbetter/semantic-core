// swift-tools-version: 6.0
import PackageDescription

// The static library comes from `scripts/build-ffi.sh` at the repository
// root, which runs cargo and copies `libsc_ffi.a` into `Libraries/`
// (gitignored). The linker search path below is relative to this package.
let package = Package(
    name: "SemanticCore",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SemanticCore", targets: ["SemanticCore"])
    ],
    targets: [
        .systemLibrary(name: "CSemanticCore", path: "Sources/CSemanticCore"),
        .target(
            name: "SemanticCore",
            dependencies: ["CSemanticCore"],
            swiftSettings: [.swiftLanguageMode(.v6)],
            linkerSettings: [
                .unsafeFlags(["-L", "Libraries"]),
                .linkedLibrary("sc_ffi"),
            ]
        ),
        .testTarget(
            name: "SemanticCoreTests",
            dependencies: ["SemanticCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
