#if !os(macOS)
import Foundation
import SwiftCrossUI

/// Non-macOS contract. Platform-specific backends can replace this harmless leaf later without
/// changing callers or importing WinUI into the Swift package.
struct MaterialSurfaceAppKitBackend: View {
    let kind: MaterialSurfaceKind

    var body: some View {
        EmptyView()
    }
}

struct MaterialSurface: View {
    let kind: MaterialSurfaceKind

    var body: some View {
        MaterialSurfaceAppKitBackend(kind: kind)
    }
}
#endif
