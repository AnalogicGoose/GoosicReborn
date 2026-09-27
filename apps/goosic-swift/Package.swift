// swift-tools-version: 6.0

import Foundation
import PackageDescription

/// The macOS SDK the app is linked against, as the Makefile reads it from `xcrun`. AppKit picks
/// its design — menus with icons, the Liquid Glass toolbar — from the SDK stamped into the
/// binary, so a fixed older number would make a newer Mac draw the app as if it were older.
let macOSSDK = ProcessInfo.processInfo.environment["GOOSIC_MACOS_SDK"]
    .flatMap { $0.isEmpty ? nil : $0 } ?? "26.0"

let package = Package(
    name: "goosic-swift",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "goosic-swift", targets: ["GoosicSwift"]),
    ],
    dependencies: [
        .package(url: "https://github.com/moreSwift/swift-cross-ui.git", exact: "0.9.0"),
        .package(url: "https://github.com/sparkle-project/Sparkle.git", exact: "2.10.0"),
    ],
    targets: [
        .executableTarget(
            name: "GoosicSwift",
            dependencies: [
                .product(name: "SwiftCrossUI", package: "swift-cross-ui"),
                .product(name: "DefaultBackend", package: "swift-cross-ui"),
                .product(name: "AppKitBackend", package: "swift-cross-ui", condition: .when(platforms: [.macOS])),
                .product(name: "Sparkle", package: "Sparkle", condition: .when(platforms: [.macOS])),
                .product(name: "GtkBackend", package: "swift-cross-ui", condition: .when(platforms: [.linux])),
                .product(name: "Gtk", package: "swift-cross-ui", condition: .when(platforms: [.linux])),
                .target(name: "CWebKitGTK", condition: .when(platforms: [.linux])),
                .target(name: "CGLib", condition: .when(platforms: [.linux])),
                .target(name: "CGStreamer", condition: .when(platforms: [.linux])),
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
                     "-Xlinker", "14.0", "-Xlinker", macOSSDK],
                    .when(platforms: [.macOS])
                ),
            ]
        ),
        // WebKitGTK's GTK 4 binding. Only Linux depends on it; macOS keeps using WebKit.framework.
        .systemLibrary(
            name: "CWebKitGTK",
            pkgConfig: "webkitgtk-6.0",
            providers: [
                .apt(["libwebkitgtk-6.0-dev"]),
                .yum(["webkitgtk6.0-devel"]),
            ]
        ),
        // GIO, for the D-Bus connection MPRIS needs. WebKitGTK's headers already drag GLib in,
        // but a media-controls file importing the WebKit module would read as a mistake, and
        // GTK is already proof that two system modules may cover the same GLib headers here.
        .systemLibrary(
            name: "CGLib",
            pkgConfig: "gio-2.0",
            providers: [
                .apt(["libglib2.0-dev"]),
                .yum(["glib2-devel"]),
            ]
        ),
        // GStreamer, which is how Linux plays a decoded file. macOS reaches AVFoundation for the
        // same job; both are only ever pointed at the WAV cache Rust produced.
        .systemLibrary(
            name: "CGStreamer",
            pkgConfig: "gstreamer-1.0",
            providers: [
                .apt(["libgstreamer1.0-dev"]),
                .yum(["gstreamer1-devel"]),
            ]
        ),
        .testTarget(
            name: "GoosicSwiftTests",
            dependencies: ["GoosicSwift"],
            // SwiftPM puts Sparkle in Debug beside the test bundle but does not add that
            // directory to the bundle's runpaths. Three levels up from Contents/MacOS is
            // Debug, independent of the configured scratch path.
            linkerSettings: [
                .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@loader_path/../../.."],
                             .when(platforms: [.macOS])),
            ]
        ),
    ]
)
