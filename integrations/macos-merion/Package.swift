// swift-tools-version: 5.10
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import PackageDescription

let package = Package(
    name: "MerionKit",
    platforms: [.macOS("26.0"), .iOS("26.0")],
    products: [.library(name: "MerionKit", targets: ["MerionKit"])],
    targets: [
        .target(name: "MerionKit", path: "Sources/MerionKit"),
        .testTarget(
            name: "MerionKitTests",
            dependencies: ["MerionKit"],
            path: "Tests/MerionKitTests",
            resources: [.copy("Fixtures")]
        ),
    ]
)
