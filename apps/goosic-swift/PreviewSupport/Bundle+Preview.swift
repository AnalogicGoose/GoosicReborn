#if !SWIFT_PACKAGE
import Foundation
#if GOOSIC_PREVIEW_NO_WEBKIT
import AppKit
import SwiftCrossUI
#endif

// SwiftPM synthesizes `Bundle.module`; the preview app target compiles the same sources
// directly, so its copied resources live in the application bundle instead.
extension Bundle {
    static let module = Bundle.main
}

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

/// Keeps the portable screen graph type-checkable without bringing AppKitBackend (and its
/// WebKit overlay) into the Canvas process. Native previews render `NativeMacCatalogPage`.
struct NativeMacCatalogRouteSurface: SwiftCrossUI.View {
    let route: GoosicRoute
    let title: String
    let subtitle: String
    let state: CatalogLoadState
    let model: GoosicAppModel

    var body: some SwiftCrossUI.View {
        SwiftCrossUI.EmptyView()
    }
}
#endif
#endif
