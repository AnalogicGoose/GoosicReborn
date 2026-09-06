// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "goosic-swift",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "goosic-swift", targets: ["GoosicSwift"]),
    ],
    dependencies: [
        .package(url: "https://github.com/moreSwift/swift-cross-ui.git", exact: "0.9.0"),
    ],
    targets: [
        .executableTarget(
            name: "GoosicSwift",
            dependencies: [
                .product(name: "SwiftCrossUI", package: "swift-cross-ui"),
                .product(name: "DefaultBackend", package: "swift-cross-ui"),
                .product(name: "AppKitBackend", package: "swift-cross-ui", condition: .when(platforms: [.macOS])),
                .product(name: "GtkBackend", package: "swift-cross-ui", condition: .when(platforms: [.linux])),
                .product(name: "Gtk", package: "swift-cross-ui", condition: .when(platforms: [.linux])),
                .target(name: "CWebKitGTK", condition: .when(platforms: [.linux])),
                .target(name: "CGLib", condition: .when(platforms: [.linux])),
                .target(name: "CGStreamer", condition: .when(platforms: [.linux])),
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
            dependencies: ["GoosicSwift"]
        ),
    ]
)
