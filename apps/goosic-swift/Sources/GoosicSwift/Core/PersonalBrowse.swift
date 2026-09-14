import Foundation

/// How a page should be rendered, which the response alone does not settle.
///
/// An album and an artist page are both shelves of responsive rows to a parser; to a person one is
/// a track list and the other is a set of sections. The caller knows which it asked for, so it
/// says, rather than leaving the reader to guess from the shape of the answer.
enum CatalogPageShape: String {
    /// A flat, ordered track list: a playlist or an album.
    case tracks
    /// Sections of cards: an artist, Home, a library section.
    case shelves
    /// Let the reader decide from the browse id. Correct for library sections, where the caller
    /// genuinely does not know what a section will come back as.
    case auto
}

/// Which reader produced a page, so its continuation goes back to the same one.
///
/// Continuations used to be routed by asking what the key *was* — Home, or a library section — and
/// whether an account happened to be active at the moment the user scrolled. That answers a
/// different question than the one being asked. A cursor is only meaningful to the reader that
/// issued it: a token from the account's WebKit profile means nothing to the anonymous Rust
/// client, and one from Rust means nothing to the page. What matters is where the page came from,
/// which is a fact about the page, not about the key or about the present.
enum CatalogPageSource: Equatable {
    case service
    case personal(browseID: String, title: String, shape: CatalogPageShape)
}

/// Turning an entity identifier into the browse id YouTube Music expects.
enum PersonalBrowseID {
    /// A playlist is browsed under its list id with a `VL` prefix, while watch endpoints and the
    /// anonymous catalog route use the bare id.
    ///
    /// Both spellings genuinely arrive. A card from the account's own library carries the browse
    /// id it was given — already `VL`-prefixed — and a card from the anonymous catalog carries the
    /// bare list id, so whichever one the user last tapped decides which spelling the shell is
    /// holding. Prefixing unconditionally would produce `VLVLPL…`, which upstream answers with an
    /// empty page rather than an error: a playlist that opens blank and looks like it has no
    /// tracks. Rust normalises the same way, deliberately — the two must not disagree about what a
    /// playlist is called.
    static func playlist(_ id: String) -> String {
        id.hasPrefix("VL") ? id : "VL\(id)"
    }
}
