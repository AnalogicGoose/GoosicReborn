import Foundation

/// Where the target's bundled resources are, in a build and in a packaged app.
///
/// SwiftPM's generated `Bundle.module` looks for its resource bundle beside the executable and in
/// the build directory. A macOS `.app` cannot keep it beside the executable: anything loose in
/// `Contents/MacOS` or at the bundle root makes code signing refuse the app. A packaged app keeps
/// it in `Contents/Resources` instead, which is looked at first; everything else falls through to
/// `Bundle.module`, so `swift run` and the tests behave exactly as before.
enum GoosicResources {
    static let bundleName = "goosic-swift_GoosicSwift.bundle"

    static let bundle: Bundle = {
        if let resources = Bundle.main.resourceURL,
           let packaged = Bundle(url: resources.appendingPathComponent(bundleName)) {
            return packaged
        }
        return Bundle.module
    }()
}
