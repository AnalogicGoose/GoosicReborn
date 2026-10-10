#if os(macOS)
import Foundation

/// WebKit's localized description hides the exception message. Inspect it only to classify
/// the failure; raw exception text and upstream response bodies must never enter diagnostics.
enum PersonalCatalogFailure: LocalizedError {
    case contextUnavailable
    case script(String)

    static func normalize(_ error: Error) -> Error {
        if PersonalSessionExpired.matches(error) { return PersonalSessionExpired() }
        let wrapped = error as NSError
        let message = wrapped.userInfo["WKJavaScriptExceptionMessage"] as? String ?? ""
        if message.contains(PersonalSessionExpired.marker) { return PersonalSessionExpired() }
        if message.contains("session context is unavailable") { return Self.contextUnavailable }
        if let underlying = wrapped.userInfo[NSUnderlyingErrorKey] as? NSError,
           underlying !== wrapped {
            let text = underlying.userInfo["WKJavaScriptExceptionMessage"] as? String ?? ""
            if text.contains(PersonalSessionExpired.marker) { return PersonalSessionExpired() }
        }
        let kind: String
        if message.contains("ReferenceError") { kind = "reference" }
        else if message.contains("TypeError") { kind = "type" }
        else if message.contains("SyntaxError") { kind = "syntax" }
        else if message.contains("HTTP 401") || message.contains("HTTP 403") { kind = "authorization" }
        else { kind = "request" }
        return Self.script(kind)
    }

    var errorDescription: String? {
        switch self {
        case .contextUnavailable:
            return "YouTube Music's account page is not ready. Try again, or renew your sign-in in Settings."
        case .script(let kind):
            return "YouTube Music could not refresh this page (\(kind)). Try again."
        }
    }
}
#endif
