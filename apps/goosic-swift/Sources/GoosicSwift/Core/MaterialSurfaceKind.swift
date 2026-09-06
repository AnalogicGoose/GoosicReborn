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
