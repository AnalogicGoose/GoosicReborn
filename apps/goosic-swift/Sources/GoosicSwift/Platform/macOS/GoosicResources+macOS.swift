#if os(macOS) && !GOOSIC_PORTABLE
import Foundation

/// SwiftPM runs with its resource bundle beside the executable. A signed .app keeps the same
/// bundle in Contents/Resources, where code signing permits it.
enum GoosicMacResources {
    static let bundle: Bundle = {
        if let resources = Bundle.main.resourceURL,
           let packaged = Bundle(url: resources.appendingPathComponent("goosic-swift_GoosicSwift.bundle")) {
            return packaged
        }
        #if SWIFT_PACKAGE
        return Bundle.module
        #else
        return Bundle.main
        #endif
    }()
}
#endif
