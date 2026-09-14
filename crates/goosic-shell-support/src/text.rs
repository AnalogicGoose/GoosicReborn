//! The few text rules the ported code needs, spelled the way Foundation spells them.
//!
//! The Swift shell trims and filters with `CharacterSet`, and Rust's nearest equivalents are close
//! but not identical. Where the difference could change an answer — which characters count as
//! control characters in a display name, for instance — it is written out here once rather than
//! approximated at each call site.

/// Foundation's `.whitespaces`: Unicode `Zs` plus tab, and deliberately not line breaks.
pub(crate) fn is_horizontal_space(c: char) -> bool {
    matches!(
        c,
        '\t' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}'
            | '\u{3000}'
    )
}

pub(crate) fn trim_spaces(text: &str) -> &str {
    text.trim_matches(is_horizontal_space)
}

/// Foundation's `.whitespacesAndNewlines`, which is exactly Unicode `White_Space` — the set
/// `str::trim` already uses.
pub(crate) fn trim_whitespace(text: &str) -> &str {
    text.trim()
}

/// Foundation's `.controlCharacters`: general categories `Cc` *and* `Cf`.
///
/// `char::is_control` covers only `Cc`, which would let format characters through — zero-width
/// spaces, and the bidirectional overrides that can make a display name read as something it is
/// not. The `Cf` table is written out because the standard library does not expose categories.
pub(crate) fn is_control_or_format(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{AD}'
                | '\u{600}'..='\u{605}'
                | '\u{61C}'
                | '\u{6DD}'
                | '\u{70F}'
                | '\u{890}'..='\u{891}'
                | '\u{8E2}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{110BD}'
                | '\u{110CD}'
                | '\u{13430}'..='\u{1343F}'
                | '\u{1BCA0}'..='\u{1BCA3}'
                | '\u{1D173}'..='\u{1D17A}'
                | '\u{E0001}'
                | '\u{E0020}'..='\u{E007F}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_characters_count_as_control_characters() {
        assert!(is_control_or_format('\u{200B}'), "zero-width space");
        assert!(is_control_or_format('\u{202E}'), "right-to-left override");
        assert!(is_control_or_format('\u{FEFF}'), "byte order mark");
        assert!(is_control_or_format('\n'));
        assert!(is_control_or_format('\t'));
        assert!(!is_control_or_format('a'));
        assert!(!is_control_or_format('é'));
    }

    #[test]
    fn spaces_are_trimmed_but_line_breaks_are_not() {
        assert_eq!(trim_spaces("\u{A0} Dark\t"), "Dark");
        assert_eq!(trim_spaces("dark\n"), "dark\n");
        assert_eq!(trim_whitespace("\n Ada \u{2028}"), "Ada");
    }
}
