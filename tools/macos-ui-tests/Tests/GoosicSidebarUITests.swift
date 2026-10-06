import XCTest

final class GoosicSidebarUITests: XCTestCase {
    private let fixturePlaylist = "Fixture playlist 40"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testPinnedAccountRemainsUsableAfterScrollingToFinalPlaylist() throws {
        let app = launchFixture(appearance: "dark")

        let account = app.descendants(matching: .any).matching(identifier: "sidebar.account").firstMatch
        XCTAssertTrue(account.waitForExistence(timeout: 5))

        let finalPlaylist = app.buttons["sidebar.playlist.ui-test-playlist-40"]
        for _ in 0..<12 where !finalPlaylist.exists {
            app.windows.firstMatch.swipeUp()
        }
        XCTAssertTrue(finalPlaylist.waitForExistence(timeout: 2))

        account.click()
        XCTAssertTrue(app.menuItems["Account Settings…"].firstMatch.waitForExistence(timeout: 3))
    }

    func testFixtureRendersInLightAppearance() throws {
        let app = launchFixture(appearance: "light")
        XCTAssertTrue(app.descendants(matching: .any).matching(identifier: "sidebar.account").firstMatch.waitForExistence(timeout: 5))
    }

    func testCollectionWithInspectorKeepsPlaybackAndQueueEditingReachable() throws {
        let app = XCUIApplication()
        app.launchEnvironment = ["GOOSIC_UI_FIXTURE": "quality", "GOOSIC_DIAGNOSTICS": "0",
                                 "GOOSIC_UI_APPEARANCE": "light"]
        app.launch()
        let play = app.buttons["Play"].firstMatch
        XCTAssertTrue(play.waitForExistence(timeout: 5))
        XCTAssertTrue(play.isHittable)
        let clear = app.buttons["Clear Up Next"]
        XCTAssertTrue(clear.isHittable)
        clear.click()
        let undo = app.buttons["Undo clearing Up Next"]
        XCTAssertTrue(undo.waitForExistence(timeout: 2))
        undo.click()
        XCTAssertFalse(undo.exists)
        let attachment = XCTAttachment(screenshot: app.windows.firstMatch.screenshot())
        attachment.name = "Collection with Up Next"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testSettingsExposesAllThemeChoices() throws {
        let app = launchFixture(appearance: "light")
        app.buttons["sidebar.route.settings"].click()
        let theme = app.popUpButtons["Theme"]
        for _ in 0..<4 where !theme.isHittable { app.windows.firstMatch.swipeUp() }
        XCTAssertTrue(theme.exists)
        theme.click()
        XCTAssertTrue(app.menuItems["System"].exists)
        XCTAssertTrue(app.menuItems["Light"].exists)
        XCTAssertTrue(app.menuItems["Dark"].exists)
        app.typeKey(.escape, modifierFlags: [])
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
