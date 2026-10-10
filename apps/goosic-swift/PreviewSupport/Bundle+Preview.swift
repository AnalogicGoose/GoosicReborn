#if !SWIFT_PACKAGE
import Foundation
#if GOOSIC_PREVIEW_NO_WEBKIT
import AppKit
#endif

#if GOOSIC_PREVIEW_NO_WEBKIT
/// Canvas-only mount point. Preview rendering never creates the authenticated WebKit player.
@MainActor
final class OfficialPlaybackContainer: NSView {}

@MainActor
extension OfficialPlaybackHost {
    func makeContainer() -> OfficialPlaybackContainer {
        OfficialPlaybackContainer()
    }
}

#endif
#endif
