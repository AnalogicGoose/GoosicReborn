#if os(Linux)
import CWebKitGTK
import Foundation
import Gtk
import GtkBackend
import SwiftCrossUI

/// The WebKitGTK renderer, wrapped as a `Gtk.Widget` so SwiftCrossUI can lay it out.
///
/// This is the Linux counterpart of the macOS `WKWebView`, and it deliberately loads nothing on
/// its own. The document is chosen by the official playback host once Rust has granted the lease,
/// so a renderer cannot reach a playable page without going through the authority first.
final class WebKitWebViewWidget: Gtk.Widget {
    init() {
        super.init(webkit_web_view_new())
        expandHorizontally = true
        useExpandHorizontally = true
        expandVertically = true
        useExpandVertically = true
    }

    /// The same object, typed for the WebKit calls. GObject inheritance makes this cast safe:
    /// `WebKitWebView` is a `GtkWidget`, so the pointer is already the right one.
    var webViewPointer: UnsafeMutablePointer<WebKitWebView> {
        UnsafeMutableRawPointer(widgetPointer).assumingMemoryBound(to: WebKitWebView.self)
    }

    /// Loads markup with no network access, for mounting the renderer before a lease exists.
    func load(html: String) {
        webkit_web_view_load_html(webViewPointer, html, nil)
    }
}

/// Mounts the renderer inside the SwiftCrossUI hierarchy. The macOS side does the same job with
/// `NSViewRepresentable` and `OfficialPlaybackContainer`; this is the GTK mirror of it.
///
/// The widget is created once and handed back through `onCreate`, so the host can hold the single
/// renderer it owns rather than letting layout churn produce a second one.
struct WebKitSurface: GtkWidgetRepresentable {
    let onCreate: (WebKitWebViewWidget) -> Void

    func makeGtkWidget(context: Context) -> WebKitWebViewWidget {
        let widget = WebKitWebViewWidget()
        onCreate(widget)
        return widget
    }

    func updateGtkWidget(_ gtkWidget: WebKitWebViewWidget, context: Context) {}
}
#endif