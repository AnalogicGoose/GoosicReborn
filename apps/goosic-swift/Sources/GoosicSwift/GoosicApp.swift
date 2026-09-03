import DefaultBackend
import SwiftCrossUI

@main
struct GoosicApp: App {
    var body: some Scene {
        WindowGroup("Goosic") {
            GoosicShell()
        }
        .defaultSize(width: 1_080, height: 720)
    }
}

struct GoosicShell: View {
    @State private var model = GoosicAppModel()

    var body: some View {
        NavigationSplitView {
            GoosicSidebar(model: model)
        } detail: {
            VStack(alignment: .leading, spacing: 0) {
                if let detail = model.detail {
                    EntityDetailScreen(entity: detail, model: model)
                } else {
                    RouteScreen(route: model.route, model: model)
                }
                Spacer()
                if model.queueVisible {
                    QueuePanel(model: model)
                }
                NowPlayingBar(model: model)
                // Kept mounted for the lifetime of the shell so there is one renderer and one
                // WebKit data store, even while navigating between native screens.
                OfficialPlaybackSurface(model: model)
                    .frame(width: 1, height: 1)
            }
        }
        // A music app should show music on launch. `connect()` is idempotent, and the sidebar
        // button remains the way back after a transport failure drops the child process.
        .onAppear { model.connect() }
    }
}
