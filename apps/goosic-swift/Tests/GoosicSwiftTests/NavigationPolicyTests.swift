import XCTest

#if os(macOS)
import ObjectiveC.runtime
import WebKit
#endif

@testable import GoosicSwift

/// `WKNavigationDelegate` is an Objective-C protocol whose members are all optional, so a
/// navigation-policy method with the wrong signature is not a compile error and not a runtime
/// error: WebKit simply never finds it and allows every navigation. That is the failure mode
/// these tests exist for. The policy methods are the only thing keeping the official player on
/// music.youtube.com and the sign-in window on Google's own hosts, and the compiler reports the
/// mismatch as a warning ("nearly matches optional requirement") that a build log buries.
///
/// The SDK declares the handler as `WK_SWIFT_UI_ACTOR void (^)(WKNavigationActionPolicy)`, which
/// imports as `@MainActor @Sendable`; a declaration missing those attributes is a different type
/// and therefore a different method.
#if os(macOS)
final class NavigationPolicyAdoptionTests: XCTestCase {
    private static let decidePolicy = NSSelectorFromString(
        "webView:decidePolicyForNavigationAction:decisionHandler:"
    )

    /// Asserts against the class, not an instance: an unimplemented optional requirement is
    /// invisible from the outside, and building a host would need a window and a profile.
    private func assertImplements(_ type: AnyClass, _ selector: Selector, _ file: StaticString = #filePath, _ line: UInt = #line) {
        XCTAssertNotNil(
            class_getInstanceMethod(type, selector),
            "\(type) does not implement \(selector) — WebKit will allow every navigation",
            file: file, line: line
        )
    }

    func testTheOfficialPlaybackHostImplementsTheNavigationPolicyRequirement() {
        assertImplements(OfficialPlaybackHost.self, Self.decidePolicy)
    }

    func testTheAccountLoginHostImplementsTheNavigationPolicyRequirement() {
        assertImplements(AccountLoginHost.self, Self.decidePolicy)
    }
}
#endif

/// The rules themselves, reachable without WebKit. `NavigationPolicyAdoptionTests` proves WebKit
/// asks; these prove the answers are right.
final class NavigationPolicyRuleTests: XCTestCase {
    private func url(_ string: String) -> URL? { URL(string: string) }

    // MARK: - Official playback

    func testThePlayerStaysOnItsOwnHost() {
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("https://music.youtube.com/watch?v=a"), frame: .main), .allow)
        // The player is parked on a blank document before it is given a track.
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("about:blank"), frame: .main), .allow)
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("https://www.youtube.com/watch?v=a"), frame: .main), .cancel)
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("https://evil.example/"), frame: .main), .cancel)
        // Plain http on the right host is still the wrong document.
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("http://music.youtube.com/"), frame: .main), .cancel)
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: nil, frame: .main), .cancel)
    }

    /// Advertisements arrive in subframes, and this application reports them rather than
    /// bypassing them; a policy that cancelled them would be blocking ads by accident.
    func testThePlayerLetsItsOwnPageEmbedSubframes() {
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("https://googleads.g.doubleclick.net/x"), frame: .subframe), .allow)
    }

    func testThePlayerRefusesPopupsThatWouldBecomeASecondMediaOwner() {
        XCTAssertEqual(OfficialNavigationPolicy.decide(url: url("https://music.youtube.com/watch?v=a"), frame: .newWindow), .cancel)
    }

    // MARK: - Sign-in

    func testSignInFollowsGoogleAndYouTubeInTheMainFrame() {
        for allowed in [
            "https://accounts.google.com/signin",
            "https://accounts.google.com.ni/signin",
            "https://consent.google.co.uk/x",
            "https://music.youtube.com/",
            "https://accounts.youtube.com/",
        ] {
            XCTAssertEqual(LoginNavigationPolicy.decide(url: url(allowed), frame: .main), .allow, allowed)
        }
    }

    func testSignInRefusesAnywhereAPasswordShouldNotBeTyped() {
        for refused in [
            "https://accounts.google.evil.com/signin",
            "https://accounts.google.com.evil.example/signin",
            "https://evil.google.com/signin",
            "http://accounts.google.com/signin",
            "https://user:pass@accounts.google.com/signin",
            "https://accounts.google.com:8443/signin",
        ] {
            XCTAssertEqual(LoginNavigationPolicy.decide(url: url(refused), frame: .main), .cancel, refused)
        }
    }

    /// Google serves its challenges — reCAPTCHA above all — from hosts the main-frame list does
    /// not name. Policing those is how a sign-in window that enforces its rules for the first
    /// time stops being able to sign anybody in.
    func testSignInLetsItsChallengesLoadInSubframes() {
        XCTAssertEqual(LoginNavigationPolicy.decide(url: url("https://www.google.com/recaptcha/api2/anchor"), frame: .subframe), .allow)
    }

    func testSignInRefusesPopups() {
        XCTAssertEqual(LoginNavigationPolicy.decide(url: url("https://accounts.google.com/signin"), frame: .newWindow), .cancel)
    }
}
