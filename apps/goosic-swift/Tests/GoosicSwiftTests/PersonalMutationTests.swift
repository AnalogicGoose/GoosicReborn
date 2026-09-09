import XCTest

@testable import GoosicSwift

final class PersonalMutationTests: XCTestCase {
    private func string(_ value: Any?) -> String? { value as? String }

    func testEachMutationSendsTheArgumentsItsEndpointExpects() {
        XCTAssertEqual(
            string(PersonalMutation.rateTrack(videoID: "v1", rating: .liked).arguments["status"]),
            "LIKE"
        )
        XCTAssertEqual(
            string(PersonalMutation.rateTrack(videoID: "v1", rating: .none).arguments["status"]),
            "INDIFFERENT"
        )
        XCTAssertEqual(
            string(PersonalMutation.setPlaylistPrivacy(playlistID: "PL1", privacy: .unlisted)
                .arguments["privacy"]),
            "UNLISTED"
        )
    }

    /// Moving an entry to the front is "no predecessor", which is a different request from a
    /// predecessor that happens to be empty — sending the key with an empty value asks upstream
    /// to place the entry after an entry that does not exist.
    func testMovingToTheFrontOmitsThePredecessorEntirely() {
        let toFront = PersonalMutation.movePlaylistItem(
            playlistID: "PL1", entryID: "e1", afterEntryID: nil
        )
        XCTAssertNil(toFront.arguments["predecessorSetVideoId"])

        let afterOne = PersonalMutation.movePlaylistItem(
            playlistID: "PL1", entryID: "e1", afterEntryID: "e0"
        )
        XCTAssertEqual(string(afterOne.arguments["predecessorSetVideoId"]), "e0")
    }

    /// A playlist entry is removed by its own per-entry id, not by its video id: a playlist can
    /// hold the same track twice, and the video id alone does not say which one to remove.
    func testRemovingAnEntryCarriesBothIdentifiers() {
        let mutation = PersonalMutation.removeFromPlaylist(
            playlistID: "PL1", videoID: "v1", entryID: "entry-1"
        )
        XCTAssertEqual(string(mutation.arguments["videoId"]), "v1")
        XCTAssertEqual(string(mutation.arguments["setVideoId"]), "entry-1")
    }

    func testAnEmptyDescriptionOrTrackListIsOmittedFromACreate() {
        let bare = PersonalMutation.createPlaylist(
            title: "Mix", description: nil, privacy: .private, videoIDs: []
        )
        XCTAssertNil(bare.arguments["description"])
        XCTAssertNil(bare.arguments["videoIds"])
        XCTAssertEqual(string(bare.arguments["privacy"]), "PRIVATE")

        let seeded = PersonalMutation.createPlaylist(
            title: "Mix", description: "notes", privacy: .public, videoIDs: ["v1"]
        )
        XCTAssertEqual(string(seeded.arguments["description"]), "notes")
        XCTAssertEqual(seeded.arguments["videoIds"] as? [String], ["v1"])
    }

    // MARK: - Coalescing

    /// Pressing Like twice because the first press did not visibly do anything must not become
    /// two requests racing each other.
    func testTheSameChangeAskedForTwiceHasOneKey() {
        let first = PersonalMutation.rateTrack(videoID: "v1", rating: .liked)
        let second = PersonalMutation.rateTrack(videoID: "v1", rating: .liked)
        XCTAssertEqual(first.coalescingKey, second.coalescingKey)
    }

    /// Liking and then immediately unliking is a change of mind, not a repeat. Swallowing the
    /// second would leave the track in the state the user just rejected.
    func testAChangeOfMindIsNotCoalescedIntoTheChangeItReverses() {
        let liked = PersonalMutation.rateTrack(videoID: "v1", rating: .liked)
        let unliked = PersonalMutation.rateTrack(videoID: "v1", rating: .none)
        XCTAssertNotEqual(liked.coalescingKey, unliked.coalescingKey)
    }

    func testTheSameOperationOnDifferentSubjectsIsNotCoalesced() {
        XCTAssertNotEqual(
            PersonalMutation.addToPlaylist(playlistID: "PL1", videoID: "v1").coalescingKey,
            PersonalMutation.addToPlaylist(playlistID: "PL2", videoID: "v1").coalescingKey
        )
    }

    /// The key is built from a dictionary, whose order is not stable between runs. If it leaked
    /// into the key, two identical requests would sometimes fail to coalesce and sometimes not —
    /// the worst kind of bug to be handed.
    func testTheKeyDoesNotDependOnDictionaryOrder() {
        let mutation = PersonalMutation.createPlaylist(
            title: "Mix", description: "notes", privacy: .private, videoIDs: ["v1", "v2"]
        )
        let keys = (0..<50).map { _ in mutation.coalescingKey }
        XCTAssertEqual(Set(keys).count, 1)
    }

    /// Reading a list changes nothing, so it must not throw away what is cached about the
    /// library — opening the menu would otherwise refetch every personal page on screen.
    func testOnlyRealChangesInvalidateWhatIsCached() {
        XCTAssertFalse(PersonalMutation.listUserPlaylists.changesLibrary)
        XCTAssertTrue(PersonalMutation.rateTrack(videoID: "v", rating: .liked).changesLibrary)
        XCTAssertTrue(PersonalMutation.deletePlaylist(playlistID: "PL1").changesLibrary)
    }

    // MARK: - Decoding

    func testACreateAnswersWithTheIDOfWhatItMade() throws {
        let json = Data(#"{"playlistId":"PLnew"}"#.utf8)
        let result = try JSONDecoder().decode(PersonalMutationResult.self, from: json)
        XCTAssertEqual(result.playlistId, "PLnew")
        XCTAssertTrue(result.playlists.isEmpty)
    }

    func testAListingAnswersWithTheAccountsOwnPlaylists() throws {
        let json = Data("""
        {"value":[{"id":"PL1","title":"Focus","subtitle":"12 songs","thumbnail":null}]}
        """.utf8)
        let result = try JSONDecoder().decode(PersonalMutationResult.self, from: json)
        XCTAssertEqual(result.playlists.map(\.id), ["PL1"])
        XCTAssertEqual(result.playlists.first?.title, "Focus")
        XCTAssertNil(result.playlists.first?.thumbnail)
    }

    /// An operation with nothing to report still answers an object, so that "no value" and "no
    /// answer at all" cannot decode to the same thing.
    func testAnOperationWithNothingToReportStillDecodes() throws {
        let result = try JSONDecoder().decode(PersonalMutationResult.self, from: Data("{}".utf8))
        XCTAssertNil(result.playlistId)
        XCTAssertTrue(result.playlists.isEmpty)
    }
}

/// The operation names cross into JavaScript as strings. Nothing in the build notices when one
/// side is renamed and the other is not: the reader throws "Unknown mutation" at runtime, in a
/// WebKit callback, for a user who pressed a button that then did nothing.
final class PersonalMutationWireTests: XCTestCase {
    private func program() throws -> String {
        let source = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Sources/GoosicSwift/Resources/PersonalCatalog.js")
        return try String(contentsOf: source, encoding: .utf8)
    }

    func testEveryOperationSwiftCanAskForExistsInTheReader() throws {
        let program = try program()
        let everyMutation: [PersonalMutation] = [
            .rateTrack(videoID: "v", rating: .liked),
            .savePlaylist(playlistID: "PL1", saved: true),
            .followArtist(channelID: "UC1", followed: true),
            .addToPlaylist(playlistID: "PL1", videoID: "v"),
            .removeFromPlaylist(playlistID: "PL1", videoID: "v", entryID: "e"),
            .movePlaylistItem(playlistID: "PL1", entryID: "e", afterEntryID: nil),
            .createPlaylist(title: "t", description: nil, privacy: .private, videoIDs: []),
            .renamePlaylist(playlistID: "PL1", title: "t"),
            .setPlaylistDescription(playlistID: "PL1", description: ""),
            .setPlaylistPrivacy(playlistID: "PL1", privacy: .private),
            .deletePlaylist(playlistID: "PL1"),
            .listUserPlaylists,
        ]
        for mutation in everyMutation {
            XCTAssertTrue(
                program.contains("\n    \(mutation.operation):")
                    || program.contains("\n    \(mutation.operation),"),
                "the reader has no \(mutation.operation) operation"
            )
        }
    }

    /// Upstream answers HTTP 200 for an edit it refuses — not the owner, stale cookies — so a
    /// reader that does not check the envelope reports success for a change that never happened.
    func testTheReaderChecksTheEnvelopeStatusRatherThanTheHTTPStatus() throws {
        let program = try program()
        XCTAssertTrue(program.contains("STATUS_SUCCEEDED"))
        XCTAssertTrue(program.contains("function assertSucceeded"))
    }

    func testTheMutationEntryPointIsExposedToTheHost() throws {
        let program = try program()
        XCTAssertTrue(program.contains("async function mutate(operation, args)"))
        XCTAssertTrue(program.contains("return { browse, mutate };"))
    }
}
