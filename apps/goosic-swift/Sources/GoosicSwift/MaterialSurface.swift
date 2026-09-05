import Foundation
import SwiftCrossUI

/// The native surfaces that share Goosic's material treatment.
enum MaterialSurfaceKind: String, CaseIterable, Equatable {
    case sidebar
    case queue
    case nowPlaying
}

/// Inputs to the material backend resolver. Keeping this value independent of Foundation's
/// process APIs makes backend selection deterministic and straightforward to test.
enum MaterialSurfacePlatform: Equatable {
    case macOS
    case windows
    case other
}

enum MaterialSurfaceBackend: Equatable {
    case macOSLiquidGlass
    case macOSVisualEffect
    case winUIBackdrop
    case staticFallback
}

enum MaterialSurfacePlatformResolver {
    static func resolve(platform: MaterialSurfacePlatform, majorVersion: Int?) -> MaterialSurfaceBackend {
        switch platform {
        case .macOS:
            guard let majorVersion else { return .staticFallback }
            if majorVersion >= 26 { return .macOSLiquidGlass }
            if (14...25).contains(majorVersion) { return .macOSVisualEffect }
            return .staticFallback
        case .windows:
            return .winUIBackdrop
        case .other:
            return .staticFallback
        }
    }

    static func backend(for platform: MaterialSurfacePlatform, majorVersion: Int?) -> MaterialSurfaceBackend {
        resolve(platform: platform, majorVersion: majorVersion)
    }

    static func resolve(platform: MaterialSurfacePlatform, version: Int?) -> MaterialSurfaceBackend {
        resolve(platform: platform, majorVersion: version)
    }

    static func backend(for platform: MaterialSurfacePlatform, version: Int?) -> MaterialSurfaceBackend {
        resolve(platform: platform, majorVersion: version)
    }
}

/// Short compatibility name for callers that only need the pure resolver.
typealias MaterialSurfaceResolver = MaterialSurfacePlatformResolver
typealias MaterialSurfaceBackendResolver = MaterialSurfacePlatformResolver

#if os(macOS)
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
#else

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
