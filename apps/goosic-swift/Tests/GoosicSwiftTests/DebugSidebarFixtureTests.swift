import XCTest

@testable import GoosicSwift

final class DebugSidebarFixtureTests: XCTestCase {
    func testFixtureIsLocalAndDenseEnoughToExerciseScrolling() async {
        await MainActor.run {
            let model = GoosicAppModel(debugSidebarFixture: true)
            XCTAssertTrue(model.usesDebugSidebarFixture)
            XCTAssertTrue(model.serviceConnected)
            XCTAssertEqual(model.activeAccountLabel, "UI Test Account")
            XCTAssertEqual(model.userPlaylists.count, 40)
            XCTAssertEqual(model.userPlaylists.last?.title, "Fixture playlist 40")
        }
    }
}
