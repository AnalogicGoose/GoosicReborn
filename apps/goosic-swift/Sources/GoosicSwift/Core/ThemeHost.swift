import Foundation
import SwiftUI

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

    /// The native SwiftUI scheme, or `nil` to follow the system.
    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}
