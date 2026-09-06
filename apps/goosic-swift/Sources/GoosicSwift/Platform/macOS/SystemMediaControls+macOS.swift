#if os(macOS)
import Foundation

import AppKit
@preconcurrency import MediaPlayer

/// Bridges confirmed Goosic playback to macOS Now Playing and Remote Command Center.
/// Commands call the model only; the adapter never reaches either playback host directly.
@MainActor
final class SystemMediaControls {
    private weak var model: GoosicAppModel?
    private let commandCenter: MPRemoteCommandCenter
    /// `nonisolated(unsafe)` so `deinit`, which is nonisolated, can unregister these. Every
    /// other access is from a main-actor method, and by the time `deinit` runs nothing else
    /// holds a reference, so there is no second accessor to race with.
    nonisolated(unsafe) private var commandTargets: [(MPRemoteCommand, Any)] = []
    private var artworkTask: Task<Void, Never>?
    private var artworkToken: UInt64 = 0
    private var projection: SystemMediaNowPlayingProjection?

    init(model: GoosicAppModel) {
        self.model = model
        commandCenter = .shared()
        installCommandTargets()
    }

    deinit {
        artworkTask?.cancel()
        for (command, target) in commandTargets {
            command.removeTarget(target)
        }
        commandTargets.removeAll()
    }

    func update(snapshot: SystemMediaPlaybackSnapshot) {
        let previousArtworkURL = projection?.artworkURL
        let next = SystemMediaNowPlayingProjection.make(from: snapshot)
        projection = next

        guard next.isActive else {
            artworkToken &+= 1
            artworkTask?.cancel()
            artworkTask = nil
            MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
            MPNowPlayingInfoCenter.default().playbackState = .stopped
            applyAvailability(.init(
                play: false, pause: false, togglePlayPause: false, next: false,
                previous: false, changePosition: false, stop: false, changeVolume: false
            ))
            return
        }

        var info: [String: Any] = [
            MPNowPlayingInfoPropertyElapsedPlaybackTime: next.elapsedTime,
            MPNowPlayingInfoPropertyPlaybackRate: next.playbackRate,
            MPMediaItemPropertyPlaybackDuration: next.duration,
        ]
        if let title = next.title { info[MPMediaItemPropertyTitle] = title }
        if let artist = next.artist { info[MPMediaItemPropertyArtist] = artist }
        if let album = next.album { info[MPMediaItemPropertyAlbumTitle] = album }
        // Keep an already fetched image while only the confirmed position/state changes.
        if previousArtworkURL == next.artworkURL,
           let artwork = MPNowPlayingInfoCenter.default().nowPlayingInfo?[MPMediaItemPropertyArtwork] {
            info[MPMediaItemPropertyArtwork] = artwork
        }
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
        MPNowPlayingInfoCenter.default().playbackState = next.playbackState.mediaPlayerValue
        applyAvailability(.make(from: snapshot))
        if previousArtworkURL != next.artworkURL {
            artworkToken &+= 1
            let token = artworkToken
            artworkTask?.cancel()
            artworkTask = nil
            loadArtworkIfNeeded(url: next.artworkURL, token: token)
        }
    }

    private func installCommandTargets() {
        let center = commandCenter
        add(center.playCommand) { [weak self] _ in self?.handlePlay() ?? .commandFailed }
        add(center.pauseCommand) { [weak self] _ in self?.handlePause() ?? .commandFailed }
        add(center.togglePlayPauseCommand) { [weak self] _ in self?.handleToggle() ?? .commandFailed }
        add(center.nextTrackCommand) { [weak self] _ in self?.handleNext() ?? .commandFailed }
        add(center.previousTrackCommand) { [weak self] _ in self?.handlePrevious() ?? .commandFailed }
        add(center.changePlaybackPositionCommand) { [weak self] event in self?.handlePosition(event) ?? .commandFailed }
        add(center.stopCommand) { [weak self] _ in self?.handleStop() ?? .commandFailed }
    }

    private func add(_ command: MPRemoteCommand, handler: @escaping (MPRemoteCommandEvent) -> MPRemoteCommandHandlerStatus) {
        command.isEnabled = false
        let target = command.addTarget(handler: handler)
        commandTargets.append((command, target))
    }

    private func applyAvailability(_ availability: SystemMediaCommandAvailability) {
        commandCenter.playCommand.isEnabled = availability.play
        commandCenter.pauseCommand.isEnabled = availability.pause
        commandCenter.togglePlayPauseCommand.isEnabled = availability.togglePlayPause
        commandCenter.nextTrackCommand.isEnabled = availability.next
        commandCenter.previousTrackCommand.isEnabled = availability.previous
        commandCenter.changePlaybackPositionCommand.isEnabled = availability.changePosition
        commandCenter.stopCommand.isEnabled = availability.stop
    }

    private func handlePlay() -> MPRemoteCommandHandlerStatus {
        guard let model, model.isPaused, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).play else {
            return .noSuchContent
        }
        model.togglePause()
        return .success
    }

    private func handlePause() -> MPRemoteCommandHandlerStatus {
        guard let model, !model.isPaused, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).pause else {
            return .noSuchContent
        }
        model.togglePause()
        return .success
    }

    private func handleToggle() -> MPRemoteCommandHandlerStatus {
        guard let model, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).togglePlayPause else {
            return .noSuchContent
        }
        model.togglePause()
        return .success
    }

    private func handleNext() -> MPRemoteCommandHandlerStatus {
        guard let model, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).next else { return .noSuchContent }
        model.next()
        return .success
    }

    private func handlePrevious() -> MPRemoteCommandHandlerStatus {
        guard let model, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).previous else { return .noSuchContent }
        model.previous()
        return .success
    }

    private func handlePosition(_ event: MPRemoteCommandEvent) -> MPRemoteCommandHandlerStatus {
        guard let change = event as? MPChangePlaybackPositionCommandEvent,
              let model,
              SystemMediaCommandAvailability.make(from: model.mediaSnapshot).changePosition else { return .noSuchContent }
        model.seek(to: change.positionTime)
        return .success
    }

    private func handleStop() -> MPRemoteCommandHandlerStatus {
        guard let model, SystemMediaCommandAvailability.make(from: model.mediaSnapshot).stop else { return .noSuchContent }
        model.releasePlayback()
        return .success
    }

    private func loadArtworkIfNeeded(url: URL?, token: UInt64) {
        guard let url, let scheme = url.scheme?.lowercased(), scheme == "https" || scheme == "http" else { return }
        artworkTask = Task { [weak self] in
            do {
                let (data, _) = try await URLSession.shared.data(from: url)
                guard !Task.isCancelled else { return }
                self?.applyArtwork(data: data, token: token)
            } catch {
                // Artwork is optional; metadata remains useful when a thumbnail cannot load.
            }
        }
    }

    private func applyArtwork(data: Data, token: UInt64) {
        guard token == artworkToken,
              let image = NSImage(data: data),
              var info = MPNowPlayingInfoCenter.default().nowPlayingInfo,
              projection?.isActive == true else { return }
        info[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: image.size) { _ in image }
        MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    }
}

private extension SystemMediaPlaybackState {
    var mediaPlayerValue: MPNowPlayingPlaybackState {
        switch self {
        case .stopped: return .stopped
        case .paused: return .paused
        case .playing: return .playing
        }
    }
}
#endif
