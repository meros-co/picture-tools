// swift-tools-version:5.7
// The picture tools for Swift apps: a Metal pipeline that runs the
// generated shader over a texture, and a SwiftUI scope view, over the Rust
// crate's C ABI (feature `capi`). The app links the Rust library (its own,
// with meros-picture-tools in it, or the crate built as a static library);
// this package carries only the header.
import PackageDescription

let package = Package(
    name: "MerosPictureTools",
    platforms: [.iOS(.v15), .tvOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "MerosPictureTools", targets: ["MerosPictureTools"]),
    ],
    targets: [
        .target(
            name: "CMerosPictureTools",
            path: "Sources/CMerosPictureTools"
        ),
        .target(
            name: "MerosPictureTools",
            dependencies: ["CMerosPictureTools"],
            path: "Sources/MerosPictureTools",
            resources: [.process("Resources")]
        ),
        .testTarget(
            name: "MerosPictureToolsTests",
            dependencies: ["MerosPictureTools"],
            path: "Tests/MerosPictureToolsTests"
        ),
    ]
)
