import Foundation
import SwiftCrossUI

/// The appearance the shell renders in.
enum GoosicTheme: String, CaseIterable, Equatable {
    /// Follow whatever the operating system is set to.
    case system
    case light
    case dark

    var label: String {
        switch self {
        case .system: return "System"
        case .light: return "Light"
        case .dark: return "Dark"
        }
    }

    /// Decodes a stored value, falling back to following the system.
    ///
    /// The preference can come from a hand-edited file or from a previous Goosic install, so an
    /// unrecognized value is corrected rather than trusted.
    static func named(_ raw: String) -> GoosicTheme {
        GoosicTheme(rawValue: raw.trimmingCharacters(in: .whitespaces).lowercased()) ?? .system
    }

    /// The scheme to hand SwiftCrossUI, or `nil` to follow the system.
    ///
    /// Appearance goes through the toolkit rather than through AppKit directly: the backend sets
    /// `window.appearance` (and each control's) from its own environment, so anything set on
    /// `NSApplication` is overwritten on the next layout. Routing it here also means the choice
    /// works on every backend rather than only on macOS.
    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}
