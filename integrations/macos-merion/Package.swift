// swift-tools-version: 5.10
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
