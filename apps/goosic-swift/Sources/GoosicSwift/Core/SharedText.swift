import Foundation
import GoosicShellSupportFFI

/// Takes ownership of a string `goosic-shell-support-ffi` handed over, and frees it.
///
/// `nil` from the library reads as empty text; every export documents when it answers null.
func sharedText(_ raw: UnsafeMutablePointer<CChar>?) -> String {
    guard let raw else { return "" }
    defer { goosic_string_free(raw) }
    return String(cString: raw)
}
