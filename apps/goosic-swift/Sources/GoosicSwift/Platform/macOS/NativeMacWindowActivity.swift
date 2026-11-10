#if os(macOS)
import AppKit
import SwiftUI

/// Tells the views of one window whether anyone can see it and whether the Mac is saving power.
///
/// A `TimelineView` keeps producing frames for a window that is hidden, minimised or entirely
/// covered: with the full player open, the backdrop held the app near 17% of a core whether or
/// not the window was on screen. Closing the window keeps the music playing, so that was the
/// cost of listening with Goosic out of sight. Views that animate on a clock read these values
/// and stop while the frames would go nowhere.
struct NativeMacWindowActivity: ViewModifier {
    @State private var windowHidden = false
    @State private var lowPower = ProcessInfo.processInfo.isLowPowerModeEnabled

    func body(content: Content) -> some View {
        content
            .environment(\.goosicWindowHidden, windowHidden)
            .environment(\.goosicLowPower, lowPower)
            .background(NativeMacWindowOcclusionReader { visible in
                if windowHidden == visible { windowHidden = !visible }
            })
            .onReceive(
                NotificationCenter.default.publisher(for: .NSProcessInfoPowerStateDidChange)
                    .receive(on: RunLoop.main)
            ) { _ in
                lowPower = ProcessInfo.processInfo.isLowPowerModeEnabled
            }
    }
}

private struct GoosicWindowHiddenKey: EnvironmentKey {
    static let defaultValue = false
}

private struct GoosicLowPowerKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    /// Whether no part of the hosting window is on screen.
    var goosicWindowHidden: Bool {
        get { self[GoosicWindowHiddenKey.self] }
        set { self[GoosicWindowHiddenKey.self] = newValue }
    }

    /// Whether the Mac is in Low Power Mode, where decorative motion is held still.
    var goosicLowPower: Bool {
        get { self[GoosicLowPowerKey.self] }
        set { self[GoosicLowPowerKey.self] = newValue }
    }
}

/// Reports the hosting window's occlusion state, which covers hiding the app, minimising the
/// window, closing it, another Space, and being fully covered by other windows.
private struct NativeMacWindowOcclusionReader: NSViewRepresentable {
    let onChange: (Bool) -> Void

    func makeNSView(context: Context) -> OcclusionView {
        let view = OcclusionView()
        view.onChange = onChange
        return view
    }

    func updateNSView(_ view: OcclusionView, context: Context) {
        view.onChange = onChange
    }

    final class OcclusionView: NSView {
        var onChange: ((Bool) -> Void)?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            // Selector observation needs no token and ends with the view.
            NotificationCenter.default.removeObserver(
                self, name: NSWindow.didChangeOcclusionStateNotification, object: nil)
            guard let window else { return }
            NotificationCenter.default.addObserver(
                self, selector: #selector(occlusionChanged),
                name: NSWindow.didChangeOcclusionStateNotification, object: window)
            report()
        }

        @objc private func occlusionChanged(_ notification: Notification) {
            report()
        }

        private func report() {
            guard let window else { return }
            let visible = window.occlusionState.contains(.visible)
            // Never during a view update: the first report arrives while SwiftUI installs the view.
            DispatchQueue.main.async { [weak self] in self?.onChange?(visible) }
        }
    }
}
#endif
