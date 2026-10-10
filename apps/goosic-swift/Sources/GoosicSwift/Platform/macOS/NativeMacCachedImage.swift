#if os(macOS)
import SwiftUI
import ImageIO

private struct NativeMacArtworkModelKey: EnvironmentKey {
    static let defaultValue: GoosicAppModel? = nil
}

extension EnvironmentValues {
    var goosicArtworkModel: GoosicAppModel? {
        get { self[NativeMacArtworkModelKey.self] }
        set { self[NativeMacArtworkModelKey.self] = newValue }
    }
}

/// Every native surface uses the same bounded, cookie-free artwork cache. File decoding is
/// also off the main actor; a cancelled task cannot publish the previous song's cover.
struct NativeMacCachedImage<Content: View>: View {
    let url: String?
    var maxPixels: Int = 400
    @ViewBuilder let content: (Image?) -> Content
    @Environment(\.goosicArtworkModel) private var model
    var body: some View {
        if let model {
            NativeMacObservedImage(cache: model.artwork, url: url, maxPixels: maxPixels, content: content)
        } else {
            content(nil)
        }
    }
}

/// Observe artwork completion directly. An unchanged model reference in Environment does
/// not tell SwiftUI that a remote URL now has a local file.
private struct NativeMacObservedImage<Content: View>: View {
    @ObservedObject var cache: ArtworkCache
    let url: String?
    let maxPixels: Int
    @ViewBuilder let content: (Image?) -> Content
    @State private var rendered: (key: NativeMacImageKey, image: CGImage)?

    var body: some View {
        let key = cache.localFile(for: url).map { NativeMacImageKey(file: $0, pixels: maxPixels) }
        content(rendered.flatMap { value in
            value.key == key ? Image(decorative: value.image, scale: 1) : nil
        })
        .onChange(of: url, initial: true) { _, value in cache.prioritize(value) }
        .task(id: key, priority: .userInitiated) {
            rendered = nil
            guard let key else { return }
            let image = await NativeMacDecodedImages.image(for: key)
            guard !Task.isCancelled, let image else { return }
            rendered = (key, image)
        }
    }
}

private struct NativeMacImageKey: Hashable, Sendable {
    let file: URL
    let pixels: Int
    var cacheKey: NSString { "\(file.path):\(pixels)" as NSString }
}

/// Bounded decoded thumbnails survive scrolling and page navigation. Concurrent appearances
/// of the same cover share one decode, including the player bar and queue.
@MainActor
private enum NativeMacDecodedImages {
    private static let cache: NSCache<NSString, CGImage> = {
        let value = NSCache<NSString, CGImage>()
        value.totalCostLimit = 32 * 1024 * 1024
        value.countLimit = 128
        return value
    }()
    private static var pending: [NativeMacImageKey: Task<CGImage?, Never>] = [:]

    static func image(for key: NativeMacImageKey) async -> CGImage? {
        if let image = cache.object(forKey: key.cacheKey) { return image }
        if let task = pending[key] { return await task.value }
        let task = Task.detached(priority: .userInitiated) {
            guard let source = CGImageSourceCreateWithURL(key.file as CFURL, nil) else { return nil as CGImage? }
            return CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: key.pixels,
                kCGImageSourceShouldCacheImmediately: true,
            ] as CFDictionary)
        }
        pending[key] = task
        let image = await task.value
        pending.removeValue(forKey: key)
        if let image { cache.setObject(image, forKey: key.cacheKey, cost: image.bytesPerRow * image.height) }
        return image
    }
}
#endif
