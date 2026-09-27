#if os(macOS) && !GOOSIC_PREVIEW_NO_WEBKIT
import WebKit

extension NavigationFrame {
    /// `WKNavigationAction.targetFrame` is `nil` when the navigation has no frame to land in
    /// yet, which is WebKit's way of describing a popup rather than a missing value.
    init(_ frame: WKFrameInfo?) {
        guard let frame else {
            self = .newWindow
            return
        }
        self = frame.isMainFrame ? .main : .subframe
    }
}
#endif
