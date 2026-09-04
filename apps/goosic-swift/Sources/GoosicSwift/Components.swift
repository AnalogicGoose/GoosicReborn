import SwiftCrossUI

/// Says plainly that the catalog is anonymous, so nobody reads a guest home shelf as "your" mix.
struct GuestCatalogNotice: View {
    var body: some View {
        HStack(spacing: 8) {
            Text("GUEST")
                .font(.caption)
            Text("Live YouTube Music catalog, browsed without an account")
                .font(.caption)
                .foregroundColor(.gray)
        }
        .padding(8)
        .background(Color.blue.opacity(0.12))
        .cornerRadius(6)
    }
}

struct GoosicSidebar: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("GOOSIC")
                .font(.title2)
                .padding(.bottom, 8)
            Text("Your music, in motion")
                .font(.caption)
                .foregroundColor(.gray)
                .padding(.bottom, 8)
            ForEach(GoosicRoute.allCases, id: \.self) { route in
                Button(action: { model.navigate(to: route) }) {
                    HStack(spacing: 8) {
                        Text(route.symbol)
                            .frame(width: 20)
                        Text(route.title)
                        Spacer()
                    }
                    .padding(7)
                    .background(model.route == route && model.detail == nil ? Color.blue.opacity(0.18) : Color.clear)
                    .cornerRadius(5)
                }
            }
            Spacer()
            Divider()
            HStack(spacing: 7) {
                Text(model.serviceConnected ? "●" : "○")
                    .foregroundColor(model.serviceConnected ? .green : .gray)
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.serviceConnected ? "Rust service connected" : "Service offline")
                        .font(.caption)
                    Text(model.activeAccountLabel)
                        .font(.caption2)
                        .foregroundColor(.gray)
                }
            }
            if let account = model.activeAccount {
                Text(account.email ?? account.channel ?? "Signed in")
                    .font(.caption2)
                    .foregroundColor(.gray)
                    .lineLimit(1)
            }
            Button(model.activeAccount == nil ? "Sign in" : "Manage accounts") {
                model.navigate(to: .settings)
            }
            .font(.caption)
            Button("Connect to Rust service") { model.connect() }
                .font(.caption)
                .padding(.top, 3)
                .disabled(model.serviceConnected)
            Text(model.status)
                .font(.caption2)
                .foregroundColor(.gray)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 3)
        }
        .padding(16)
        .frame(minWidth: 220)
        // Behind the controls, never wrapping them: the material is a background leaf, so
        // buttons and their accessibility stay native.
        .background(MaterialSurface(kind: .sidebar))
    }
}

struct ShelfView: View {
    let shelf: GoosicShelf
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(shelf.title)
                .font(.headline)
            if let tracks = shelf.trackList {
                ForEach(tracks) { track in
                    TrackRow(track: track, context: tracks, model: model)
                }
            } else {
                ScrollView(.horizontal) {
                    HStack(alignment: .top, spacing: 10) {
                        ForEach(shelf.cards) { card in
                            CatalogCardView(card: card, model: model)
                        }
                    }
                }
            }
        }
    }
}

/// Catalog artwork, with a glyph until the image is on disk.
///
/// `Image` reads its source while computing layout, so it is only ever handed a local file that
/// the cache has already written. `model.artworkVersion` is read here so a late download
/// re-renders this view.
struct ArtworkView: View {
    let remote: String?
    let placeholder: String
    let width: Double
    let height: Double
    let model: GoosicAppModel

    var body: some View {
        // Reading the version participates this view in artwork updates.
        let _ = model.artworkVersion
        if let file = model.artworkFile(for: remote) {
            // The file extension is a cache detail, so the format is sniffed from the bytes.
            Image(file, useFileExtension: false)
                .resizable()
                .frame(width: width, height: height)
                .cornerRadius(7)
        } else {
            Text(placeholder)
                .font(.title)
                .frame(width: width, height: height, alignment: .center)
                .background(Color.blue.opacity(0.16))
                .cornerRadius(7)
        }
    }
}

struct CatalogCardView: View {
    let card: GoosicCard
    let model: GoosicAppModel

    private var glyph: String {
        switch card.action {
        case .play: return "▶"
        case .show, .none: return "♪"
        }
    }

    var body: some View {
        Button(action: {
            switch card.action {
            case .show(let entity):
                model.show(entity)
            case .play(let track):
                model.play(track)
            case .none:
                break
            }
        }) {
            VStack(alignment: .leading, spacing: 5) {
                ArtworkView(
                    remote: card.thumbnail,
                    placeholder: glyph,
                    width: 148,
                    height: 82,
                    model: model
                )
                Text(card.title)
                    .font(.subheadline)
                Text(card.subtitle)
                    .font(.caption2)
                    .foregroundColor(.gray)
            }
            .frame(width: 148, alignment: .leading)
        }
        .disabled(card.action == nil || model.accountOperationInProgress)
    }
}

struct TrackRow: View {
    let track: GoosicTrack
    /// The list this row belongs to, so playing it queues its neighbours too.
    let context: [GoosicTrack]
    let model: GoosicAppModel

    init(track: GoosicTrack, context: [GoosicTrack] = [], model: GoosicAppModel) {
        self.track = track
        self.context = context
        self.model = model
    }

    private var isCurrent: Bool { model.currentTrack?.id == track.id }

    var body: some View {
        HStack(spacing: 10) {
            ArtworkView(
                remote: track.thumbnail,
                placeholder: isCurrent ? "▶" : "♪",
                width: 34,
                height: 34,
                model: model
            )
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(track.title)
                    if track.explicit {
                        Text("E")
                            .font(.caption2)
                            .foregroundColor(.gray)
                    }
                }
                Text(track.secondaryText)
                    .font(.caption)
                    .foregroundColor(.gray)
            }
            Spacer()
            Text(track.duration)
                .font(.caption)
                .foregroundColor(.gray)
            Button("Play") { model.play(track, in: context) }
                .font(.caption)
                .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.isAdvertisement)
        }
        .padding(.vertical, 5)
    }
}

struct NowPlayingBar: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(spacing: 5) {
            Divider()
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.currentTrack?.title ?? "Nothing playing")
                        .font(.subheadline)
                    Text(model.nowPlayingSubtitle)
                        .font(.caption)
                        .foregroundColor(.gray)
                }
                Spacer()
                Button("Previous") { model.previous() }
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.queue.tracks.isEmpty || model.isAdvertisement)
                Button(model.isPaused ? "Play" : "Pause") { model.togglePause() }
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle)
                Button("Next") { model.next() }
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.queue.tracks.isEmpty || model.isAdvertisement)
                Button("Radio") {
                    if let track = model.currentTrack { model.startRadio(from: track) }
                }
                .disabled(model.currentTrack == nil || model.accountOperationInProgress || model.playbackTransition != .idle)
                Button(model.queueVisible ? "Hide queue" : "Show queue") { model.toggleQueue() }
                    .disabled(model.accountOperationInProgress)
                Button("Release") { model.releasePlayback() }
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle)
            }
            .padding(.horizontal, 14)
            PlaybackTransportBar(model: model)
                .padding(.horizontal, 14)
                .padding(.bottom, 9)
            Text(model.status)
                .font(.caption2)
                .foregroundColor(.gray)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14)
                .padding(.bottom, 5)
        }
        .background(MaterialSurface(kind: .nowPlaying))
    }
}

/// Position, seek, and volume — all of it reflecting what the official player confirmed rather
/// than what was requested.
struct PlaybackTransportBar: View {
    let model: GoosicAppModel

    var body: some View {
        HStack(spacing: 10) {
            Text(model.elapsedText)
                .font(.caption2)
                .foregroundColor(.gray)
                .frame(minWidth: 42)
            if model.isSeekable {
                Slider(
                    value: Binding(
                        get: { model.displayedPosition },
                        set: { model.seek(to: $0) }
                    ),
                    in: 0...model.duration
                )
            } else {
                // A zero-length range is not a valid slider, and nothing is seekable before the
                // player reports a duration.
                Text("—")
                    .font(.caption2)
                    .foregroundColor(.gray)
                    .frame(maxWidth: .infinity)
            }
            Text(model.durationText)
                .font(.caption2)
                .foregroundColor(.gray)
                .frame(minWidth: 42)
            Button(model.isMuted ? "Unmute" : "Mute") { model.toggleMuted() }
                .font(.caption2)
                .disabled(model.accountOperationInProgress || model.isAdvertisement)
            Slider(
                value: Binding(
                    get: { model.isMuted ? 0 : model.volume },
                    set: { model.setVolume($0) }
                ),
                in: 0...1
            )
            .frame(width: 90)
            .disabled(model.accountOperationInProgress || model.isAdvertisement)
            Button(model.shuffle ? "Shuffle on" : "Shuffle off") { model.toggleShuffle() }
                .font(.caption2)
            Button(model.repeatMode.label) { model.cycleRepeatMode() }
                .font(.caption2)
            Button(model.autoplay ? "Autoplay on" : "Autoplay off") { model.setAutoplay(!model.autoplay) }
                .font(.caption2)
                .disabled(model.accountOperationInProgress)
        }
    }
}

struct QueuePanel: View {
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("Queue")
                    .font(.headline)
                Spacer()
                Text("\(model.queue.tracks.count) track(s)")
                    .font(.caption)
                    .foregroundColor(.gray)
            }
            if model.queue.tracks.isEmpty {
                Text("Empty. Play something from the catalog to build a queue.")
                    .font(.caption)
                    .foregroundColor(.gray)
            }
            ForEach(model.queue.tracks) { track in
                HStack {
                    Text(track.title)
                    Text("· \(track.artist)")
                        .font(.caption)
                        .foregroundColor(.gray)
                    Spacer()
                    if track.id == model.currentTrack?.id { Text("Current").font(.caption) }
                }
            }
        }
        .padding(12)
        .background(MaterialSurface(kind: .queue))
    }
}
