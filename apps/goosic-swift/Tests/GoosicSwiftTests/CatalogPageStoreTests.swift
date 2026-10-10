import Foundation
import XCTest

@testable import GoosicSwift

final class CatalogPageStoreTests: XCTestCase {
    func testPartialCacheRetainsCompletenessWithoutPersistingCursor() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = CatalogPageStore(root: root)
        let key = CatalogKey.playlist("list")
        let page = GoosicCatalogPage(id: "list", title: "List", nextCursor: "upstream-secret-cursor")
        store.save(page, for: key, scope: "guest")
        let restored = try XCTUnwrap(store.load(key, scope: "guest"))
        XCTAssertTrue(restored.hasMore)
        XCTAssertTrue(restored.cachedPartial)
        XCTAssertNil(restored.nextCursor)
        let path = root.appendingPathComponent("guest/playlist%2Dlist.json")
        let file = try XCTUnwrap(FileManager.default.contentsOfDirectory(
            at: path.deletingLastPathComponent(), includingPropertiesForKeys: nil).first)
        XCTAssertFalse(try String(contentsOf: file, encoding: .utf8).contains("upstream-secret-cursor"))
        var complete = page
        complete.nextCursor = nil
        store.save(complete, for: key, scope: "guest")
        XCTAssertFalse(try XCTUnwrap(store.load(key, scope: "guest")).hasMore)
    }

    func testClampedPagesRemainPartialWithoutContinuation() {
        let page = GoosicCatalogPage(id: "list", title: "List", truncated: true)
        XCTAssertTrue(CatalogPageView(wire: page).hasMore)
    }
}
