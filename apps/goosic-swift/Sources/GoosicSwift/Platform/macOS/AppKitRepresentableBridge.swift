#if os(macOS) && !GOOSIC_PORTABLE
import AppKitBackend

/// Keeps AppKitBackend's representable protocol distinct from SwiftUI's protocol in files that
/// intentionally host one framework inside the other.
protocol GoosicAppKitRepresentable: NSViewRepresentable {}
#endif
