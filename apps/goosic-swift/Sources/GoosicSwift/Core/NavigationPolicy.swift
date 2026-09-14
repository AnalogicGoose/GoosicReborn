import Foundation

/// Which frame a navigation would land in. `WKNavigationAction.targetFrame` reports this as an
/// optional object whose `isMainFrame` flag has to be read through two layers of optionality, and
/// the `nil` case means something quite different from the other two — it is a frame that does not
/// exist yet, which is to say a popup. Naming the three cases keeps the distinction from being
/// re-derived, differently, in each host.
enum NavigationFrame: Equatable {
    case main
    /// A frame the page already owns: an advertisement, a consent widget, a reCAPTCHA challenge.
    case subframe
    /// A frame that does not exist yet — `target="_blank"`, `window.open`.
    case newWindow
}

enum NavigationDecision: Equatable {
    case allow
    case cancel
}

/// Where the two WebKit surfaces may navigate.
///
/// These are pure because the alternative is a rule that only exists inside a delegate callback,
/// where nothing can reach it: for the whole life of this code both policies were written with
/// signatures WebKit never matched, so neither one ran and every navigation was allowed. A rule
/// that decides where a password may be typed has to be reachable by a test that does not need
/// WebKit, a window, or a network.
///
/// Both policies judge the main frame and concede the subframes. That split is deliberate. The
/// main frame is the document the user is looking at and typing into, so it is the one that must
/// stay on a known host. A subframe is content the allowed page itself chose to embed, and it is
/// how sign-in serves its challenges and how YouTube Music serves its advertisements — refusing
/// those does not make the surface safer, it makes it broken, and advertisements in particular are
/// reported here, never bypassed. Popups are refused outright by both: neither surface has any use
/// for a second window, and for the player a second window would be a second media owner.
enum LoginNavigationPolicy {
    static func decide(url: URL?, frame: NavigationFrame) -> NavigationDecision {
        switch frame {
        case .newWindow: return .cancel
        case .subframe: return .allow
        case .main: return AccountLoginValidation.isAllowedLoginURL(url) ? .allow : .cancel
        }
    }
}

enum OfficialNavigationPolicy {
    /// The player is pointed at `about:blank` before it is given a track, so the blank document
    /// is part of the normal sequence rather than a navigation away from the host.
    static func decide(url: URL?, frame: NavigationFrame) -> NavigationDecision {
        switch frame {
        case .newWindow: return .cancel
        case .subframe: return .allow
        case .main: return isAllowedDocument(url) ? .allow : .cancel
        }
    }

    static func isAllowedDocument(_ url: URL?) -> Bool {
        guard let url else { return false }
        if url.absoluteString == "about:blank" { return true }
        return url.scheme == "https" && url.host == OfficialBridge.allowedHost
    }
}
