#if os(Linux)
import CGLib
import XCTest

@testable import GoosicSwift

/// Encoding a value is only half of it; D-Bus then walks the result to serialise it, and that
/// walk is what reads the variant's type. An earlier version freed the `GVariantType` as soon as
/// the builder had finished, which left every container pointing at released memory — a
/// segmentation fault that arrived later, in GDBus, with nothing in the backtrace pointing here.
///
/// `g_variant_print` performs the same walk, so these tests fail on that bug the way the app did.
final class MPRISEncodingTests: XCTestCase {
    private func printed(_ variant: OpaquePointer?) throws -> String {
        let variant = try XCTUnwrap(variant)
        // A freshly built variant is floating; sinking it makes this test own it.
        g_variant_ref_sink(variant)
        defer { g_variant_unref(variant) }
        let text = try XCTUnwrap(g_variant_print(variant, gboolean(1)))
        defer { g_free(text) }
        return String(cString: text)
    }

    func testEveryPropertyShapeSurvivesBeingSerialised() throws {
        let values: [MPRISPropertyValue] = [
            .string("Playing"),
            .objectPath(SystemMediaControls.noTrackPath),
            .boolean(true),
            .double(0.8),
            .microseconds(1_500_000),
            .strings([]),
            .strings(["audio/mpeg", "audio/ogg"]),
        ]
        for value in values {
            let text = try printed(SystemMediaControls.variant(for: value))
            XCTAssertFalse(text.isEmpty, "\(value) encoded to nothing")
        }
    }

    func testFullMetadataSurvivesBeingSerialised() throws {
        let metadata = MPRISMetadata(
            trackPath: SystemMediaControls.trackPath(for: "dQw4w9WgXcQ"),
            lengthMicroseconds: 214_000_000,
            title: "A title",
            artist: "An artist",
            album: "An album",
            artworkURL: "https://example.invalid/art.jpg"
        )
        let text = try printed(SystemMediaControls.variant(for: metadata))
        for expected in ["mpris:trackid", "mpris:length", "xesam:title", "xesam:artist",
                         "xesam:album", "mpris:artUrl"] {
            XCTAssertTrue(text.contains(expected), "\(expected) is missing from \(text)")
        }
    }

    /// Nothing playing still has to encode: a panel asks for `Metadata` before a first track.
    func testEmptyMetadataStillCarriesATrackID() throws {
        let text = try printed(SystemMediaControls.variant(for: MPRISMetadata(
            trackPath: SystemMediaControls.noTrackPath
        )))
        XCTAssertTrue(text.contains("mpris:trackid"))
        XCTAssertFalse(text.contains("xesam:title"))
    }

    /// The dictionary the change signal carries is built the same way and walked the same way.
    func testAnEmptyStringArrayIsStillAValidVariant() throws {
        XCTAssertEqual(try printed(SystemMediaControls.stringArray([])), "@as []")
    }
}
#endif
