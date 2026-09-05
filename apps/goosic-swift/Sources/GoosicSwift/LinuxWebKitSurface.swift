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
    /// The bridge lives in a world of its own, which is what keeps the page out of it.
    static let scriptWorld = "goosic"

    private var onMessage: ((String) -> Void)?
    private var handlerID: UInt = 0
    private var bridgeIsOpen = false

    init() {
        super.init(webkit_web_view_new())
        configure()
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

    /// The renderer's own content manager, which is where the bridge is registered.
    var userContentManager: OpaquePointer {
        webkit_web_view_get_user_content_manager(webViewPointer)
    }

    /// Applies the settings the official player needs, mirroring the macOS configuration.
    private func configure() {
        guard let settings = webkit_web_view_get_settings(webViewPointer) else { return }
        // YouTube Music refuses to run its player under a bare engine agent, so the agent names
        // a Safari version, exactly as the macOS host does.
        let agent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 "
            + "(KHTML, like Gecko) " + OfficialBridge.safariUserAgentSuffix
        webkit_settings_set_user_agent(settings, agent)
        // The user pressed play in Goosic, and that gesture does not cross into the web view.
        webkit_settings_set_media_playback_requires_user_gesture(settings, gboolean(0))
    }

    /// Opens the bridge in an isolated script world and starts delivering what it receives.
    ///
    /// This is where Linux is stricter than macOS rather than weaker. `WKScriptMessage` carries a
    /// `securityOrigin` the host checks after the fact, so on macOS the page may post and be
    /// rejected. WebKitGTK delivers no origin, so instead the handler is registered in a world of
    /// its own: the page shares the DOM but not the JavaScript scope, and cannot see
    /// `window.webkit.messageHandlers.goosicBridge` to post to it at all.
    func openBridge(onMessage: @escaping (String) -> Void) {
        self.onMessage = onMessage
        guard !bridgeIsOpen else { return }
        guard webkit_user_content_manager_register_script_message_handler(
            userContentManager, OfficialBridge.name, Self.scriptWorld
        ) != 0 else { return }

        let context = Unmanaged.passUnretained(self).toOpaque()
        handlerID = g_signal_connect_data(
            UnsafeMutableRawPointer(userContentManager),
            "script-message-received::" + OfficialBridge.name,
            unsafeBitCast(Self.receive, to: GCallback.self),
            context,
            nil,
            GConnectFlags(rawValue: 0)
        )
        bridgeIsOpen = true
    }

    /// The C callback. It carries no state of its own: the widget arrives as the user data the
    /// signal was connected with, and the payload is turned into the JSON the shared bridge
    /// already knows how to decode.
    private static let receive: @convention(c) (
        UnsafeMutableRawPointer?, OpaquePointer?, UnsafeMutableRawPointer?
    ) -> Void = { _, value, userData in
        guard let value, let userData else { return }
        guard let json = jsc_value_to_json(value, 0) else { return }
        defer { g_free(json) }
        let widget = Unmanaged<WebKitWebViewWidget>.fromOpaque(userData).takeUnretainedValue()
        widget.onMessage?(String(cString: json))
    }

    /// Installs a script into the bridge's world, restricted to the one origin it may run on.
    func install(script source: String) {
        var allowed = ["https://" + OfficialBridge.allowedHost + "/*"]
        allowed.withUnsafeMutableBufferPointer { buffer in
            var list: [UnsafePointer<CChar>?] = buffer.map { UnsafePointer(strdup($0)) }
            list.append(nil)
            defer { list.forEach { $0.map { free(UnsafeMutableRawPointer(mutating: $0)) } } }
            list.withUnsafeMutableBufferPointer { allowList in
                guard let script = webkit_user_script_new_for_world(
                    source,
                    WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
                    WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_END,
                    Self.scriptWorld,
                    allowList.baseAddress,
                    nil
                ) else { return }
                webkit_user_content_manager_add_script(userContentManager, script)
                webkit_user_script_unref(script)
            }
        }
    }

    /// Drops every script from a previous load, so a superseded document cannot keep reporting.
    func removeInstalledScripts() {
        webkit_user_content_manager_remove_all_scripts(userContentManager)
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