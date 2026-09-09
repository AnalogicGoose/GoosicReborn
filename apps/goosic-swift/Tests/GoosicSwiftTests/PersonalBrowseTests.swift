import XCTest

@testable import GoosicSwift

final class PersonalBrowseIDTests: XCTestCase {
    /// A card from the anonymous catalog carries the bare list id.
    func testABarePlaylistIDGetsTheBrowsePrefix() {
        XCTAssertEqual(PersonalBrowseID.playlist("PLabc123"), "VLPLabc123")
    }

    /// A card from the account's own library carries the browse id it was given, already
    /// prefixed. Prefixing again produces `VLVLPL…`, which upstream answers with an empty page
    /// rather than an error — a playlist that opens looking like it has no tracks.
    func testAnAlreadyPrefixedIDIsNotPrefixedTwice() {
        XCTAssertEqual(PersonalBrowseID.playlist("VLPLabc123"), "VLPLabc123")
        XCTAssertEqual(
            PersonalBrowseID.playlist(PersonalBrowseID.playlist("PLabc123")),
            "VLPLabc123"
        )
    }

    /// The auto-generated mixes and radio lists go through the same rule.
    func testOtherPlaylistFamiliesUseTheSameRule() {
        XCTAssertEqual(PersonalBrowseID.playlist("RDCLAK5uy_x"), "VLRDCLAK5uy_x")
        XCTAssertEqual(PersonalBrowseID.playlist("LM"), "VLLM")
    }
}

/// A cursor is only meaningful to the reader that issued it: a token from the account's WebKit
/// profile means nothing to the anonymous Rust client, and the reverse is equally true. So the
/// source is a fact recorded about the page, not something re-derived from the key.
final class CatalogPageSourceTests: XCTestCase {
    func testAPersonalSourceCarriesWhatIsNeededToContinueIt() {
        let source = CatalogPageSource.personal(
            browseID: "VLPL1", title: "Playlist", shape: .tracks
        )
        guard case .personal(let browseID, let title, let shape) = source else {
            return XCTFail("expected a personal source")
        }
        XCTAssertEqual(browseID, "VLPL1")
        XCTAssertEqual(title, "Playlist")
        XCTAssertEqual(shape, .tracks)
    }

    func testSourcesFromDifferentReadersAreNotInterchangeable() {
        XCTAssertNotEqual(
            CatalogPageSource.service,
            .personal(browseID: "VLPL1", title: "Playlist", shape: .tracks)
        )
    }

    /// The shape crosses into JavaScript as this string, so the two sides have to agree on the
    /// spelling or the reader silently falls back to guessing from the browse id — an album that
    /// quietly renders as shelves instead of a track list, with nothing reported.
    ///
    /// Read from the source file rather than the bundle because that is the artefact a rename
    /// would touch, and nothing else in the build notices when these two drift apart.
    func testTheJavaScriptReadsTheSameShapeSpellingsSwiftWrites() throws {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()   // GoosicSwiftTests
            .deletingLastPathComponent()   // Tests
            .deletingLastPathComponent()   // goosic-swift
            .appendingPathComponent("Sources/GoosicSwift/Resources/PersonalCatalog.js")
        let program = try String(contentsOf: source, encoding: .utf8)

        XCTAssertTrue(
            program.contains("shape === \"\(CatalogPageShape.tracks.rawValue)\""),
            "the reader does not test for the shape Swift sends for a playlist or album"
        )
        XCTAssertTrue(
            program.contains("shape === \"\(CatalogPageShape.shelves.rawValue)\""),
            "the reader does not test for the shape Swift sends for a shelf page"
        )
        // The fourth argument has to exist, or every shape Swift sends is ignored.
        XCTAssertTrue(program.contains("function browse(browseId, title, continuation, shape)"))
    }
}
