// swift-tools-version: 6.0

import Foundation
import PackageDescription

/// The macOS SDK the app is linked against, as the Makefile reads it from `xcrun`. AppKit picks
/// its design — menus with icons, the Liquid Glass toolbar — from the SDK stamped into the
/// binary, so a fixed older number would make a newer Mac draw the app as if it were older.
let macOSSDK = ProcessInfo.processInfo.environment["GOOSIC_MACOS_SDK"]
    .flatMap { $0.isEmpty ? nil : $0 } ?? "26.0"

/// The shared shell rules as a static archive, from `goosic-shell-support-ffi`. The Makefile builds
/// it before every Swift build; packaging points `GOOSIC_FFI_LIBRARY` at a universal release
/// archive. The archive is named by its full path because the same directory also holds the
/// crate's dylib, which `-l` would prefer, and the app must not depend on a library it does not
/// ship.
let repositoryRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
let ffiLibrary = ProcessInfo.processInfo.environment["GOOSIC_FFI_LIBRARY"]
    .flatMap { $0.isEmpty ? nil : $0 }
    ?? repositoryRoot.appendingPathComponent("target/debug/libgoosic_shell_support_ffi.a").path

let package = Package(
    name: "goosic-swift",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "goosic-swift", targets: ["GoosicSwift"]),
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle.git", exact: "2.10.0"),
    ],
    targets: [
        .systemLibrary(name: "GoosicShellSupportFFI"),
        .executableTarget(
            name: "GoosicSwift",
            dependencies: [
                "GoosicShellSupportFFI",
                .product(name: "Sparkle", package: "Sparkle", condition: .when(platforms: [.macOS])),
            ],
            resources: [
                .copy("Resources/PersonalCatalog.js"),
                .copy("Resources/AppIcons"),
            ],
            // SwiftPM stamps the executable's build version with the deployment target as its
            // SDK (`sdk 14.0`), and AppKit chooses its design from that stamp: the app ran with
            // the pre-26 look everywhere, Liquid Glass included, despite being built against the
            // current SDK. This states the SDK actually used (see `macOSSDK`) while keeping
            // macOS 14 as the minimum it runs on.
            linkerSettings: [
                .unsafeFlags(
                    ["-Xlinker", "-platform_version", "-Xlinker", "macos",
                     "-Xlinker", "14.0", "-Xlinker", macOSSDK,
                     "-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks",
                     "-Xlinker", "-rpath", "-Xlinker", "@executable_path",
                     "-Xlinker", ffiLibrary],
                    .when(platforms: [.macOS])
                ),
            ]
        ),
        .testTarget(
            name: "GoosicSwiftTests",
            dependencies: ["GoosicSwift"],
            // SwiftPM puts Sparkle in Debug beside the test bundle but does not add that
            // directory to the bundle's runpaths. Three levels up from Contents/MacOS is
            // Debug, independent of the configured scratch path.
            linkerSettings: [
                .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@loader_path/../../..",
                              "-Xlinker", ffiLibrary],
                             .when(platforms: [.macOS])),
            ]
        ),
    ]
)
