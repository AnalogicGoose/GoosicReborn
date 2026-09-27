import SwiftUI

@main
struct GoosicDebugHostApp: App {
    private var scheme: ColorScheme? {
        switch ProcessInfo.processInfo.environment["GOOSIC_UI_APPEARANCE"] {
        case "dark": .dark
        case "light": .light
        default: nil
        }
    }

    var body: some Scene {
        WindowGroup("Goosic UI Test Host") {
            GoosicMacUITestHost()
                .preferredColorScheme(scheme)
        }
        .defaultSize(width: 1_020, height: 680)
    }
}
