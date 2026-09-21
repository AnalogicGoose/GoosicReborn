#if os(macOS)
import Foundation
import SwiftCrossUI

import AppKit
// `NSViewRepresentable` and its `Context` are SwiftCrossUI's AppKit backend types, not AppKit's.
import AppKitBackend

final class MaterialSurfaceHostView: NSView {
    private(set) var selectedBackend: MaterialSurfaceBackend = .staticFallback
    private var effectView: NSView?
    private var appliedKind: MaterialSurfaceKind?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setAccessibilityElement(false)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        // This is a background leaf. Controls rendered above it must receive the event.
        nil
    }

    func apply(kind: MaterialSurfaceKind) {
        let backend = MaterialSurfacePlatformResolver.resolve(
            platform: .macOS,
            majorVersion: ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        )
        guard backend != selectedBackend || effectView == nil || appliedKind != kind else {
            effectView?.frame = bounds
            return
        }

        effectView?.removeFromSuperview()
        effectView = makeEffectView(backend: backend, kind: kind)
        selectedBackend = backend
        appliedKind = kind

        guard let effectView else { return }
        effectView.frame = bounds
        effectView.autoresizingMask = [.width, .height]
        effectView.setAccessibilityElement(false)
        addSubview(effectView, positioned: .below, relativeTo: nil)
    }

    private func makeEffectView(backend: MaterialSurfaceBackend, kind: MaterialSurfaceKind) -> NSView? {
        switch backend {
        case .macOSLiquidGlass:
            guard #available(macOS 26.0, *) else { return nil }
            let glass = NSGlassEffectView(frame: bounds)
            glass.style = kind == .nowPlaying ? .clear : .regular
            glass.cornerRadius = kind == .sidebar ? 0 : (kind == .queue ? 14 : 12)
            glass.tintColor = tint(for: kind)
            return glass
        case .macOSVisualEffect:
            let visualEffect = NSVisualEffectView(frame: bounds)
            switch kind {
            case .sidebar: visualEffect.material = .sidebar
            case .queue: visualEffect.material = .menu
            case .nowPlaying: visualEffect.material = .hudWindow
            }
            visualEffect.blendingMode = .behindWindow
            visualEffect.state = .active
            return visualEffect
        case .winUIBackdrop, .staticFallback:
            return nil
        }
    }

    private func tint(for kind: MaterialSurfaceKind) -> NSColor? {
        switch kind {
        case .sidebar: return nil
        case .queue: return NSColor.controlAccentColor.withAlphaComponent(0.08)
        case .nowPlaying: return NSColor.controlAccentColor.withAlphaComponent(0.12)
        }
    }
}

/// AppKit leaf used as a background only; it deliberately does not wrap the SwiftCrossUI content.
struct MaterialSurfaceAppKitBackend: NSViewRepresentable {
    let kind: MaterialSurfaceKind

    func makeNSView(context: Context) -> MaterialSurfaceHostView {
        let host = MaterialSurfaceHostView(frame: .zero)
        host.apply(kind: kind)
        return host
    }

    func updateNSView(_ nsView: MaterialSurfaceHostView, context: Context) {
        nsView.apply(kind: kind)
    }
}

struct MaterialSurface: View {
    let kind: MaterialSurfaceKind

    var body: some View {
        MaterialSurfaceAppKitBackend(kind: kind)
    }
}
#endif
