import SwiftCrossUI

/// The shell's colours, in one place so a surface cannot drift from the rest.
///
/// These are literal rather than system colours because the brand accent has to be the same red
/// on every backend; a system accent would be whatever the user set their desktop to, which is
/// not what "Goosic red" means. Neutrals are deliberately not literal: they come from the
/// toolkit so light and dark both work without a second palette here.
enum Palette {
    /// The brand accent: selection, the playing row, and the primary transport control.
    static let accent = Color(red: 0.96, green: 0.19, blue: 0.31)

    /// The accent behind something large, where full strength would dominate the page.
    static let accentSoft = Color(red: 0.96, green: 0.19, blue: 0.31, opacity: 0.16)

    /// A raised surface: cards, and the artwork placeholder.
    static let raised = Color(white: 1, opacity: 0.06)

    /// A raised surface under the pointer or holding a selection that is not the accent.
    static let raisedStrong = Color(white: 1, opacity: 0.11)

    /// Secondary text: subtitles, section headers, and anything supporting.
    static let secondaryText = Color(white: 0.62)

    /// A hairline, for separating regions without drawing a hard line.
    static let hairline = Color(white: 1, opacity: 0.09)
}
