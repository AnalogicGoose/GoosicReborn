// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "GoosicSwift",
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
            ]
        ),
        .testTarget(
            name: "GoosicSwiftTests",
            dependencies: ["GoosicSwift"]
        ),
    ]
)
