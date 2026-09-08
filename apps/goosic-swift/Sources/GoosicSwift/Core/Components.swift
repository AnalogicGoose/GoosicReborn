import SwiftCrossUI

/// Says plainly that the catalog is anonymous, so nobody reads a guest home shelf as "your" mix.
struct GuestCatalogNotice: View {
    var body: some View {
        HStack(spacing: 9) {
            Text("GUEST")
                .font(.system(size: 11, weight: .bold))
                .foregroundColor(Palette.accent)
            Text("Live YouTube Music catalog, browsed without an account")
                .font(.system(size: 12))
                .foregroundColor(Palette.secondaryText)
        }
        .padding(.horizontal, 11)
        .padding(.vertical, 8)
        .background(Palette.accentSoft)
        .cornerRadius(8)
    }
}

/// One navigation row. Selection is the accent at full strength with white on top, because a
/// tinted wash reads as "hovered" rather than "you are here" once the rows are this large.
struct SidebarRow: View {
    let route: GoosicRoute
    let isSelected: Bool
    let model: GoosicAppModel

    var body: some View {
        Button(action: { model.navigate(to: route) }) {
            HStack(spacing: 11) {
                Text(route.symbol)
                    .font(.system(size: 14))
                    .frame(width: 18)
                Text(route.title)
                    .font(.system(size: 14, weight: isSelected ? .semibold : .regular))
                Spacer()
            }
            // Only the selected row states a colour: the accent is dark enough that white on top
            // is right in either scheme, while an unselected row has to keep the toolkit's own
            // label colour so light mode does not get white text on a light sidebar.
            .if(isSelected) { $0.foregroundColor(.white) }
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(isSelected ? Palette.accent : Color.clear)
            .cornerRadius(8)
        }
        // The row draws its own selection, so the backend's button chrome would sit a second
        // bordered rectangle behind every entry in the list.
        .buttonStyle(.plain)
    }
}

struct SidebarSection: View {
    let section: GoosicRoute.Section
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let title = section.title {
                Text(title)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundColor(Palette.secondaryText)
                    .padding(.horizontal, 12)
                    .padding(.top, 14)
                    .padding(.bottom, 4)
            }
            ForEach(GoosicRoute.routes(in: section), id: \.self) { route in
                SidebarRow(
                    route: route,
                    isSelected: model.route == route && model.detail == nil,
                    model: model
                )
            }
        }
    }
}

struct GoosicSidebar: View {
    let model: GoosicAppModel

    /// The avatar letter. Taken from the label the footer already shows, so the circle and the
    /// name can never disagree about who is signed in.
    private var accountInitial: String {
        String(model.activeAccountLabel.prefix(1)).uppercased()
    }

    private var accountDetail: String {
        guard let account = model.activeAccount else { return "Browsing without an account" }
        return account.email ?? account.channel ?? "Signed in"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("GOOSIC")
                .font(.system(size: 19, weight: .bold))
                .foregroundColor(Palette.accent)
                .padding(.horizontal, 12)
                .padding(.top, 4)
                .padding(.bottom, 12)
            SidebarSection(section: .primary, model: model)
            SidebarSection(section: .discover, model: model)
            SidebarSection(section: .collection, model: model)
            Spacer()
            SidebarSection(section: .utility, model: model)
            // While the service is answering it is an implementation detail, so the footer says
            // who you are browsing as instead of naming it. Losing it changes what you can do,
            // and only then is it worth the reader's attention -- along with the way back.
            Button(action: { model.navigate(to: .settings) }) {
                HStack(spacing: 10) {
                    Text(accountInitial)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundColor(.white)
                        .frame(width: 30, height: 30, alignment: .center)
                        .background(Palette.accent)
                        .cornerRadius(15)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(model.activeAccountLabel)
                            .font(.system(size: 13, weight: .semibold))
                            .lineLimit(1)
                        Text(accountDetail)
                            .font(.system(size: 11))
                            .foregroundColor(Palette.secondaryText)
                            .lineLimit(1)
                    }
                    Spacer()
                }
                .padding(8)
                .background(Palette.raised)
                .cornerRadius(10)
            }
            .buttonStyle(.plain)
            .padding(.top, 10)
            if !model.serviceConnected {
                HStack(spacing: 7) {
                    Text("○")
                        .foregroundColor(.orange)
                    Text("Disconnected")
                        .font(.system(size: 12))
                    Spacer()
                    Button("Reconnect") { model.connect() }
                        .font(.system(size: 12))
                }
                .padding(.top, 7)
            }
        }
        .padding(14)
        .frame(minWidth: 248)
        // Behind the controls, never wrapping them: the material is a background leaf, so
        // buttons and their accessibility stay native.
        .background(MaterialSurface(kind: .sidebar))
    }
}

struct ShelfView: View {
    let shelf: GoosicShelf
    let model: GoosicAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(shelf.title)
                .font(.system(size: 19, weight: .bold))
            if let tracks = shelf.trackList {
                ForEach(tracks) { track in
                    TrackRow(track: track, context: tracks, model: model)
                }
            } else {
                ScrollView(.horizontal) {
                    HStack(alignment: .top, spacing: 16) {
                        ForEach(shelf.cards) { card in
                            CatalogCardView(card: card, model: model)
                        }
                    }
                }
            }
        }
        .padding(.bottom, 10)
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

    /// Rounded in proportion to the artwork, so a 34pt row thumbnail and a 168pt card do not
    /// share one radius and read as different shapes.
    private var cornerRadius: Int {
        max(4, Int((min(width, height) * 0.055).rounded()))
    }

    var body: some View {
        // Reading the version participates this view in artwork updates.
        let _ = model.artworkVersion
        if let file = model.artworkFile(for: remote) {
            // The file extension is a cache detail, so the format is sniffed from the bytes.
            Image(file, useFileExtension: false)
                .resizable()
                .frame(width: width, height: height)
                .cornerRadius(cornerRadius)
        } else {
            Text(placeholder)
                .font(.system(size: min(width, height) * 0.32))
                .foregroundColor(Palette.secondaryText)
                .frame(width: width, height: height, alignment: .center)
                .background(Palette.raised)
                .cornerRadius(cornerRadius)
        }
    }
}

struct CatalogCardView: View {
    let card: GoosicCard
    let model: GoosicAppModel

    /// Wide enough for a cover to read as artwork rather than an icon, and narrow enough that a
    /// shelf still shows there is more to scroll to.
    static let artworkSize: Double = 168

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
            VStack(alignment: .leading, spacing: 7) {
                // Square, like the covers themselves. The old 16:9 well letterboxed every piece
                // of album art it was given.
                ArtworkView(
                    remote: card.thumbnail,
                    placeholder: glyph,
                    width: Self.artworkSize,
                    height: Self.artworkSize,
                    model: model
                )
                Text(card.title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                Text(card.subtitle)
                    .font(.system(size: 12))
                    .foregroundColor(Palette.secondaryText)
                    .lineLimit(2)
            }
            .frame(width: Self.artworkSize, alignment: .leading)
        }
        // The artwork is the affordance. Chrome around it would box every cover in a shelf and
        // turn a wall of album art into a wall of buttons.
        .buttonStyle(.plain)
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
        HStack(spacing: 11) {
            ArtworkView(
                remote: track.thumbnail,
                placeholder: isCurrent ? "▶" : "♪",
                width: 40,
                height: 40,
                model: model
            )
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    // The playing row is named in the accent, so it is findable in a long list
                    // without the row itself becoming a coloured band.
                    Text(track.title)
                        .font(.system(size: 14, weight: isCurrent ? .semibold : .regular))
                        .if(isCurrent) { $0.foregroundColor(Palette.accent) }
                        .lineLimit(1)
                    if track.explicit {
                        Text("E")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundColor(Palette.secondaryText)
                    }
                }
                Text(track.secondaryText)
                    .font(.system(size: 12))
                    .foregroundColor(Palette.secondaryText)
                    .lineLimit(1)
            }
            Spacer()
            Text(track.duration)
                .font(.system(size: 12))
                .foregroundColor(Palette.secondaryText)
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
            HStack(spacing: 11) {
                // The cover travels with the track, so the bar says what is playing without
                // being read.
                ArtworkView(
                    remote: model.currentTrack?.thumbnail,
                    placeholder: "♪",
                    width: 42,
                    height: 42,
                    model: model
                )
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.currentTrack?.title ?? "Nothing playing")
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(1)
                    Text(model.nowPlayingSubtitle)
                        .font(.system(size: 12))
                        .foregroundColor(Palette.secondaryText)
                        .lineLimit(1)
                }
                Spacer()
                Button("⏮") { model.previous() }
                    .font(.system(size: 15))
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.queue.tracks.isEmpty || model.isAdvertisement)
                Button(model.isPaused ? "⏵" : "⏸") { model.togglePause() }
                    .font(.system(size: 17))
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle)
                Button("⏭") { model.next() }
                    .font(.system(size: 15))
                    .disabled(model.accountOperationInProgress || model.playbackTransition != .idle || model.queue.tracks.isEmpty || model.isAdvertisement)
                // Secondary to the transport itself, so they are set smaller rather than
                // competing with play at the same weight.
                Button("Radio") {
                    if let track = model.currentTrack { model.startRadio(from: track) }
                }
                .font(.system(size: 12))
                .disabled(model.currentTrack == nil || model.accountOperationInProgress || model.playbackTransition != .idle)
                Button(model.lyricsVisible ? "Hide lyrics" : "Lyrics") { model.toggleLyrics() }
                    .font(.system(size: 12))
                    .disabled(model.accountOperationInProgress)
                Button(model.queueVisible ? "Hide queue" : "Show queue") { model.toggleQueue() }
                    .font(.system(size: 12))
                    .disabled(model.accountOperationInProgress)
                Button("Stop") { model.releasePlayback() }
                    .font(.system(size: 12))
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

/// The lyrics for whatever is playing, with the current line marked when they are synced.
struct LyricsPanel: View {
    let model: GoosicAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("Lyrics")
                        .font(.headline)
                    Spacer()
                    Text(model.lyricsStatus)
                        .font(.caption)
                        .foregroundColor(.gray)
                }
                if let lyrics = model.lyrics {
                    let active = model.activeLyricIndex
                    ForEach(Array(lyrics.lines.enumerated()), id: \.element.id) { index, line in
                        Text(line.text.isEmpty ? "♪" : line.text)
                            .font(index == active ? .headline : .subheadline)
                            .foregroundColor(index == active ? .white : .gray)
                    }
                    if lyrics.truncated == true {
                        Text("These lyrics were long, so only the first part is shown.")
                            .font(.caption2)
                            .foregroundColor(.gray)
                            .padding(.top, 4)
                    }
                }
            }
            .padding(12)
        }
        .frame(height: 220)
        .background(MaterialSurface(kind: .queue))
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
