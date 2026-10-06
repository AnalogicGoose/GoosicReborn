#if os(macOS)
import AppKit
import UniformTypeIdentifiers

@MainActor
enum NativeMacLyricsImport {
    static func open(model: GoosicAppModel) {
        guard let track = model.currentTrack else { return }
        let panel = NSOpenPanel()
        panel.title = "Import lyrics for \(track.title)"
        panel.message = "Choose a timed lyrics JSON file for this recording. Imported lyrics apply to the current listening session."
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.begin { result in
            guard result == .OK, let url = panel.url else { return }
            do {
                let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                guard size > 0, size <= 2 * 1024 * 1024 else {
                    model.announce("Choose a lyrics file smaller than 2 MB.", isError: true)
                    return
                }
                let data = try Data(contentsOf: url)
                let document = try JSONDecoder().decode(GoosicLyrics.self, from: data)
                guard model.currentTrack?.videoID == track.videoID else {
                    model.announce("The song changed. Import lyrics for the current song.", isError: true)
                    return
                }
                model.installImportedLyrics(document)
            } catch {
                model.announce("Could not read that lyrics file. Check its JSON format and try again.", isError: true)
            }
        }
    }
}
#endif
