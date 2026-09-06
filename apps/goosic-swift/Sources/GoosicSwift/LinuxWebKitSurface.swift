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

    /// Runs `body` with a NULL-terminated allow list naming the one origin these scripts may run
    /// on. The strings are owned by this call and freed before it returns.
    private static func withAllowList(_ body: (UnsafePointer<UnsafePointer<CChar>?>) -> Void) {
        let pattern = strdup("https://" + OfficialBridge.allowedHost + "/*")
        defer { free(pattern) }
        var list: [UnsafePointer<CChar>?] = [UnsafePointer(pattern), nil]
        list.withUnsafeMutableBufferPointer { buffer in
            body(UnsafePointer(buffer.baseAddress!))
        }
    }

    /// Installs a script into the bridge's world, restricted to the one origin it may run on.
    func install(script source: String) {
        Self.withAllowList { allowList in
            guard let script = webkit_user_script_new_for_world(
                source,
                WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
                WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_END,
                Self.scriptWorld,
                allowList,
                nil
            ) else { return }
            webkit_user_content_manager_add_script(userContentManager, script)
            webkit_user_script_unref(script)
        }
    }

    /// Installs a script into the page's own world.
    ///
    /// The media-session guard has to run here rather than in the bridge's world. It overrides
    /// `navigator.mediaSession` for the page, and an isolated world gets its own wrappers of the
    /// same objects, so a guard installed there would leave the page's session untouched.
    func install(pageScript source: String) {
        Self.withAllowList { allowList in
            guard let script = webkit_user_script_new(
                source,
                WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
                WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
                allowList,
                nil
            ) else { return }
            webkit_user_content_manager_add_script(userContentManager, script)
            webkit_user_script_unref(script)
        }
    }

    /// Drops every script from a previous load, so a superseded document cannot keep reporting.
    func removeInstalledScripts() {
        webkit_user_content_manager_remove_all_scripts(userContentManager)
    }

    private var onLoadFinished: (() -> Void)?

    /// Runs JavaScript in the bridge's isolated world, which shares the page's DOM but not its
    /// scope: the player can be driven without the page being able to observe the driver.
    ///
    /// `completion` receives the result encoded as JSON, or a message describing the failure.
    /// Passing none runs the script and ignores what it returns, which is what the transport
    /// commands want — they are requests, and the answer arrives through the bridge instead.
    func evaluate(_ script: String, completion: ((String?, String?) -> Void)? = nil) {
        guard let completion else {
            webkit_web_view_evaluate_javascript(
                webViewPointer, script, -1, Self.scriptWorld, nil, nil, nil, nil
            )
            return
        }
        let box = Unmanaged.passRetained(Evaluation(completion)).toOpaque()
        webkit_web_view_evaluate_javascript(
            webViewPointer, script, -1, Self.scriptWorld, nil, nil, Self.evaluated, box
        )
    }

    /// Carries a Swift closure across the C callback boundary, retained until it fires once.
    private final class Evaluation {
        let completion: (String?, String?) -> Void
        init(_ completion: @escaping (String?, String?) -> Void) { self.completion = completion }
    }

    private static let evaluated: GAsyncReadyCallback = { source, result, userData in
        guard let userData else { return }
        let evaluation = Unmanaged<Evaluation>.fromOpaque(userData).takeRetainedValue()
        guard let source else {
            evaluation.completion(nil, "the renderer went away before the script returned")
            return
        }
        let view = UnsafeMutableRawPointer(source).assumingMemoryBound(to: WebKitWebView.self)
        var error: UnsafeMutablePointer<GError>?
        guard let value = webkit_web_view_evaluate_javascript_finish(view, result, &error) else {
            let message = error.map { String(cString: $0.pointee.message) } ?? "an unknown failure"
            if let error { g_error_free(error) }
            evaluation.completion(nil, message)
            return
        }
        defer { g_object_unref(UnsafeMutableRawPointer(value)) }
        guard let json = jsc_value_to_json(value, 0) else {
            evaluation.completion(nil, nil)
            return
        }
        defer { g_free(json) }
        evaluation.completion(String(cString: json), nil)
    }

    /// Navigates to a document. Only the host calls this, and only after Rust granted the lease.
    func loadPage(url: String) {
        webkit_web_view_load_uri(webViewPointer, url)
    }

    func stopLoading() {
        webkit_web_view_stop_loading(webViewPointer)
    }

    /// Loads markup with no network access, for mounting the renderer before a lease exists and
    /// for returning it to a document that can play nothing once the lease is gone.
    func load(html: String) {
        webkit_web_view_load_html(webViewPointer, html, nil)
    }

    /// Refuses every navigation that does not stay on the official host.
    ///
    /// macOS decides this in `decidePolicyFor navigationAction`. Without it a redirect could
    /// carry the renderer to a document the observer was never bound to, and the host would be
    /// driving a page it cannot vouch for while still holding Rust's lease.
    func guardNavigation() {
        g_signal_connect_data(
            UnsafeMutableRawPointer(webViewPointer),
            "decide-policy",
            unsafeBitCast(Self.decidePolicy, to: GCallback.self),
            nil, nil, GConnectFlags(rawValue: 0)
        )
    }

    private static let decidePolicy: @convention(c) (
        UnsafeMutableRawPointer?, OpaquePointer?, UInt32, UnsafeMutableRawPointer?
    ) -> gboolean = { _, decision, type, _ in
        guard let decision else { return gboolean(0) }
        // WebKitPolicyDecision is a derivable type and Swift gives it a typed pointer; the
        // navigation subclass is declared final and stays opaque. One object, two spellings.
        let policy = UnsafeMutableRawPointer(decision)
            .assumingMemoryBound(to: WebKitPolicyDecision.self)
        guard type == WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION.rawValue else {
            return gboolean(0)
        }
        guard let action = webkit_navigation_policy_decision_get_navigation_action(decision),
                let request = webkit_navigation_action_get_request(action),
                let uri = webkit_uri_request_get_uri(request) else {
            webkit_policy_decision_ignore(policy)
            return gboolean(1)
        }
        let text = String(cString: uri)
        if text == "about:blank" || text.hasPrefix("https://" + OfficialBridge.allowedHost) {
            webkit_policy_decision_use(policy)
        } else {
            webkit_policy_decision_ignore(policy)
        }
        return gboolean(1)
    }

    /// Reports when a document has finished loading, which is when the page is worth probing.
    func observeLoad(onFinished: @escaping () -> Void) {
        onLoadFinished = onFinished
        g_signal_connect_data(
            UnsafeMutableRawPointer(webViewPointer),
            "load-changed",
            unsafeBitCast(Self.loadChanged, to: GCallback.self),
            Unmanaged.passUnretained(self).toOpaque(),
            nil, GConnectFlags(rawValue: 0)
        )
    }

    private static let loadChanged: @convention(c) (
        UnsafeMutableRawPointer?, UInt32, UnsafeMutableRawPointer?
    ) -> Void = { _, event, userData in
        guard let userData, event == WEBKIT_LOAD_FINISHED.rawValue else { return }
        Unmanaged<WebKitWebViewWidget>.fromOpaque(userData)
            .takeUnretainedValue()
            .onLoadFinished?()
    }
}

/// Mounts the renderer inside the SwiftCrossUI hierarchy. The macOS side does the same job with
/// `NSViewRepresentable` and `OfficialPlaybackContainer`; this is the GTK mirror of it.
///
/// The renderer is handed in rather than built here, and that is the whole point. SwiftCrossUI
/// may rebuild a representable during layout; constructing a widget on each call would leave the
/// shell with a second renderer while the first is still loaded and still playing — a media owner
/// Rust never granted a lease to. The host owns the one widget, for the same reason the macOS side
/// reparents its existing `WKWebView` instead of making another.
struct WebKitSurface: GtkWidgetRepresentable {
    let widget: WebKitWebViewWidget

    func makeGtkWidget(context: Context) -> WebKitWebViewWidget { widget }

    func updateGtkWidget(_ gtkWidget: WebKitWebViewWidget, context: Context) {}
}
#endif