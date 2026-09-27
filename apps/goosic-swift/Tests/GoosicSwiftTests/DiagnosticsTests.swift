import XCTest

@testable import GoosicSwift

/// The way a credential reaches a log is not that somebody logs a password. It is that somebody
/// logs a URL for context and does not think about its query string, which is where YouTube puts
/// identifiers and where a media URL puts its signature. So the safe form has to be the only form
/// on offer, and these are the assertions that keep it that way.
final class DiagnosticsTests: XCTestCase {
    func testAURLIsReducedToWhereItWentNotWhatItCarried() {
        let url = URL(string: "https://music.youtube.com/watch?v=abc123&pot=SECRET&sig=SIGNATURE")
        let origin = Diagnostics.origin(of: url)
        XCTAssertEqual(origin, "https://music.youtube.com")
        for secret in ["abc123", "SECRET", "SIGNATURE", "watch", "?"] {
            XCTAssertFalse(origin.contains(secret), "\(secret) survived into a diagnostic")
        }
    }

    /// A signed media URL is the single worst thing that could be logged, and it is exactly the
    /// kind of URL somebody reaches for when a playback failure needs explaining.
    func testASignedMediaURLKeepsOnlyItsHost() {
        let url = URL(string: "https://rr3---sn-abc.googlevideo.com/videoplayback?expire=1&signature=DEADBEEF")
        let origin = Diagnostics.origin(of: url)
        XCTAssertEqual(origin, "https://rr3---sn-abc.googlevideo.com")
        XCTAssertFalse(origin.contains("DEADBEEF"))
        XCTAssertFalse(origin.contains("videoplayback"))
    }

    func testCredentialsInAURLAreNotRendered() {
        let url = URL(string: "https://user:hunter2@accounts.google.com/signin")
        XCTAssertFalse(Diagnostics.origin(of: url).contains("hunter2"))
    }

    func testAMissingURLSaysSoRatherThanRenderingNothing() {
        XCTAssertEqual(Diagnostics.origin(of: nil), "none")
    }

    func testALineNamesItsSubsystemAndSortsItsFields() {
        let line = Diagnostics.line(.personalCatalog, "answered", ["elapsed": "12ms", "bytes": "900"])
        // Sorted, so two runs of the same sequence can be diffed against each other.
        XCTAssertEqual(line, "[personal-catalog] answered bytes=900 elapsed=12ms\n")
    }

    /// A field long enough to be a payload is not a field. An error whose message quotes an
    /// upstream response is the realistic way a whole body would otherwise land in a log.
    func testAnOversizedFieldIsTruncated() {
        let line = Diagnostics.line(.service, "failed", ["reason": String(repeating: "x", count: 500)])
        XCTAssertLessThan(line.count, 200)
        XCTAssertTrue(line.contains("…"))
    }

    /// Without this a message containing a newline forges a second diagnostic line, which is how
    /// a log stops being trustworthy as a record of what happened.
    func testANewlineCannotForgeASecondLine() {
        let line = Diagnostics.line(.service, "failed", ["reason": "first\n[service] fake event"])
        XCTAssertEqual(line.filter { $0 == "\n" }.count, 1)
    }
}
