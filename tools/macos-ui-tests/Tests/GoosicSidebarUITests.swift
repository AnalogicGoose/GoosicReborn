import XCTest

final class GoosicSidebarUITests: XCTestCase {
    private let fixturePlaylist = "Fixture playlist 40"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testPinnedAccountRemainsUsableAfterScrollingToFinalPlaylist() throws {
        let app = launchFixture(appearance: "dark")

        let account = app.buttons["sidebar.account"]
        XCTAssertTrue(account.waitForExistence(timeout: 5))

        let finalPlaylist = app.buttons["sidebar.playlist.ui-test-playlist-40"]
        for _ in 0..<12 where !finalPlaylist.exists {
            app.windows.firstMatch.swipeUp()
        }
        XCTAssertTrue(finalPlaylist.waitForExistence(timeout: 2))

        account.click()
        XCTAssertTrue(app.staticTexts["Settings"].waitForExistence(timeout: 3))
    }

    func testFixtureRendersInLightAppearance() throws {
        let app = launchFixture(appearance: "light")
        XCTAssertTrue(app.buttons["sidebar.account"].waitForExistence(timeout: 5))
    }

    private func launchFixture(appearance: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment = ["GOOSIC_UI_FIXTURE": "sidebar", "GOOSIC_DIAGNOSTICS": "0", "GOOSIC_UI_APPEARANCE": appearance]
        app.launch()
        return app
    }

    override func tearDown() {
        guard (testRun?.failureCount ?? 0) > 0 else { return }
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = "Sidebar failure"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
