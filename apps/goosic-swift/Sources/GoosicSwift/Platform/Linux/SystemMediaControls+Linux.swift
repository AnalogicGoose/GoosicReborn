#if os(Linux)
import CGLib
import Foundation

/// What an MPRIS property is worth, in terms the compiler can move between isolation domains.
///
/// The bus calls back on the main loop, but a `GVariant` is a C handle and Swift cannot be told
/// that it is safe to carry across an actor boundary. Splitting the answer from its encoding
/// avoids the question entirely: the `@MainActor` half says what the value is, the C half turns
/// it into a variant, and nothing that is not `Sendable` ever crosses.
enum MPRISPropertyValue: Sendable, Equatable {
    case string(String)
    case objectPath(String)
    case boolean(Bool)
    case double(Double)
    case microseconds(Int64)
    case strings([String])
    case metadata(MPRISMetadata)
}

/// The subset of `xesam:` and `mpris:` fields Goosic can fill honestly.
struct MPRISMetadata: Sendable, Equatable {
    var trackPath: String
    var lengthMicroseconds: Int64?
    var title: String?
    var artist: String?
    var album: String?
    var artworkURL: String?
}

/// Publishes Goosic on MPRIS, the D-Bus interface Linux desktops use for media keys, panel
/// widgets and `playerctl`.
///
/// This is the Linux counterpart of the macOS `MPNowPlayingInfoCenter` adapter, and like it, it
/// decides nothing. What it displays comes from `SystemMediaNowPlayingProjection`, and every
/// command that arrives is checked against `SystemMediaCommandAvailability` recomputed from the
/// model's current snapshot before it is forwarded. A remote client cannot ask Goosic for a
/// transition the app itself would refuse: the bus is another caller, not another authority.
@MainActor
final class SystemMediaControls {
    private weak var model: GoosicAppModel?
    private var ownerID: guint = 0
    /// Held as an address rather than a pointer for the reason `MPRISPropertyValue` exists: a
    /// `GDBusConnection` is a C handle, and only this thread ever touches it.
    private var connectionAddress: UInt = 0
    private var rootRegistration: guint = 0
    private var playerRegistration: guint = 0
    /// The last projection published, so a change can be told from a repeat. The observer behind
    /// this reports several times a second, and a panel that redraws at that rate stutters.
    private var published: SystemMediaNowPlayingProjection?
    private var availability = SystemMediaCommandAvailability(
        play: false, pause: false, togglePlayPause: false, next: false,
        previous: false, changePosition: false, stop: false, changeVolume: false
    )
    private var volume: Double = 1

    private static let busName = "org.mpris.MediaPlayer2.goosic"
    private static let objectPath = "/org/mpris/MediaPlayer2"
    private static let rootInterface = "org.mpris.MediaPlayer2"
    private static let playerInterface = "org.mpris.MediaPlayer2.Player"
    nonisolated static let noTrackPath = "/org/mpris/MediaPlayer2/TrackList/NoTrack"

    /// Only the members Goosic can answer. Declaring a method here and refusing it at runtime
    /// would put a dead button in every panel on the desktop.
    private static let introspectionXML = """
    <node>
      <interface name='org.mpris.MediaPlayer2'>
        <property name='CanQuit' type='b' access='read'/>
        <property name='CanRaise' type='b' access='read'/>
        <property name='HasTrackList' type='b' access='read'/>
        <property name='Identity' type='s' access='read'/>
        <property name='DesktopEntry' type='s' access='read'/>
        <property name='SupportedUriSchemes' type='as' access='read'/>
        <property name='SupportedMimeTypes' type='as' access='read'/>
      </interface>
      <interface name='org.mpris.MediaPlayer2.Player'>
        <method name='Play'/>
        <method name='Pause'/>
        <method name='PlayPause'/>
        <method name='Stop'/>
        <method name='Next'/>
        <method name='Previous'/>
        <method name='Seek'>
          <arg type='x' name='Offset' direction='in'/>
        </method>
        <method name='SetPosition'>
          <arg type='o' name='TrackId' direction='in'/>
          <arg type='x' name='Position' direction='in'/>
        </method>
        <property name='PlaybackStatus' type='s' access='read'/>
        <property name='Metadata' type='a{sv}' access='read'/>
        <property name='Position' type='x' access='read'/>
        <property name='Volume' type='d' access='readwrite'/>
        <property name='Rate' type='d' access='read'/>
        <property name='MinimumRate' type='d' access='read'/>
        <property name='MaximumRate' type='d' access='read'/>
        <property name='CanGoNext' type='b' access='read'/>
        <property name='CanGoPrevious' type='b' access='read'/>
        <property name='CanPlay' type='b' access='read'/>
        <property name='CanPause' type='b' access='read'/>
        <property name='CanSeek' type='b' access='read'/>
        <property name='CanControl' type='b' access='read'/>
      </interface>
    </node>
    """

    init(model: GoosicAppModel) {
        self.model = model
        ownerID = g_bus_own_name(
            G_BUS_TYPE_SESSION,
            Self.busName,
            GBusNameOwnerFlags(rawValue: 0),
            Self.busAcquired,
            nil,
            Self.nameLost,
            Unmanaged.passUnretained(self).toOpaque(),
            nil
        )
    }

    // No `deinit`. These controls live as long as the shell, and the session bus releases a name
    // when its process exits, so nothing here outlives the program.

    /// Republishes what the desktop shows, and only when something a client can see has moved.
    func update(snapshot: SystemMediaPlaybackSnapshot) {
        let projection = SystemMediaNowPlayingProjection.make(from: snapshot)
        let availability = SystemMediaCommandAvailability.make(from: snapshot)
        let changed = projection != published || availability != self.availability
        published = projection
        self.availability = availability
        volume = snapshot.isMuted ? 0 : snapshot.volume
        guard changed else { return }
        emitPlayerPropertiesChanged()
    }

    // MARK: - Bus lifecycle

    private static let busAcquired: GBusAcquiredCallback = { connection, _, userData in
        guard let connection, let userData else { return }
        // A `@MainActor` class is `Sendable`, so resolving the object out here is what keeps the
        // pointer itself from crossing.
        let controls = Unmanaged<SystemMediaControls>.fromOpaque(userData).takeUnretainedValue()
        let address = UInt(bitPattern: connection)

        var error: UnsafeMutablePointer<GError>?
        guard let node = g_dbus_node_info_new_for_xml(SystemMediaControls.introspectionXML, &error) else {
            if let error { g_error_free(error) }
            return
        }
        defer { g_dbus_node_info_unref(node) }

        var vtable = GDBusInterfaceVTable(
            method_call: SystemMediaControls.methodCall,
            get_property: SystemMediaControls.getProperty,
            set_property: SystemMediaControls.setProperty,
            padding: (nil, nil, nil, nil, nil, nil, nil, nil)
        )
        var root: guint = 0
        var player: guint = 0
        if let info = g_dbus_node_info_lookup_interface(node, SystemMediaControls.rootInterface) {
            root = g_dbus_connection_register_object(
                connection, SystemMediaControls.objectPath, info, &vtable, userData, nil, &error
            )
        }
        if let info = g_dbus_node_info_lookup_interface(node, SystemMediaControls.playerInterface) {
            player = g_dbus_connection_register_object(
                connection, SystemMediaControls.objectPath, info, &vtable, userData, nil, &error
            )
        }
        if let error { g_error_free(error) }

        // GDBus dispatches on the thread whose main context owns the connection, and that is the
        // GTK main loop: this actor's thread.
        MainActor.assumeIsolated {
            controls.connectionAddress = address
            controls.rootRegistration = root
            controls.playerRegistration = player
        }
    }

    private static let nameLost: GBusNameLostCallback = { _, _, _ in
        // Another player already owns the name, or the session bus went away. Goosic keeps
        // playing either way; only the desktop integration is missing.
    }

    // MARK: - Encoding

    private static func withVariantType<T>(_ signature: String, _ body: (OpaquePointer) -> T) -> T? {
        guard let type = g_variant_type_new(signature) else { return nil }
        defer { g_variant_type_free(type) }
        return body(type)
    }

    private static func stringArray(_ values: [String]) -> OpaquePointer? {
        withVariantType("as") { type -> OpaquePointer? in
            var builder = GVariantBuilder()
            g_variant_builder_init(&builder, type)
            for value in values {
                g_variant_builder_add_value(&builder, g_variant_new_string(value))
            }
            return g_variant_builder_end(&builder)
        } ?? nil
    }

    private static func entry(_ key: String, _ value: OpaquePointer?) -> OpaquePointer? {
        guard let value else { return nil }
        return g_variant_new_dict_entry(g_variant_new_string(key), g_variant_new_variant(value))
    }

    private static func variant(for value: MPRISPropertyValue) -> OpaquePointer? {
        switch value {
        case .string(let text): return g_variant_new_string(text)
        case .objectPath(let path): return g_variant_new_object_path(path)
        case .boolean(let flag): return g_variant_new_boolean(gboolean(flag ? 1 : 0))
        case .double(let number): return g_variant_new_double(number)
        case .microseconds(let count): return g_variant_new_int64(gint64(count))
        case .strings(let values): return stringArray(values)
        case .metadata(let metadata): return variant(for: metadata)
        }
    }

    private static func variant(for metadata: MPRISMetadata) -> OpaquePointer? {
        withVariantType("a{sv}") { type -> OpaquePointer? in
            var builder = GVariantBuilder()
            g_variant_builder_init(&builder, type)
            var fields: [OpaquePointer?] = [
                entry("mpris:trackid", g_variant_new_object_path(metadata.trackPath))
            ]
            if let length = metadata.lengthMicroseconds {
                fields.append(entry("mpris:length", g_variant_new_int64(gint64(length))))
            }
            if let title = metadata.title {
                fields.append(entry("xesam:title", g_variant_new_string(title)))
            }
            if let artist = metadata.artist {
                fields.append(entry("xesam:artist", stringArray([artist])))
            }
            if let album = metadata.album {
                fields.append(entry("xesam:album", g_variant_new_string(album)))
            }
            if let art = metadata.artworkURL {
                fields.append(entry("mpris:artUrl", g_variant_new_string(art)))
            }
            for field in fields.compactMap({ $0 }) {
                g_variant_builder_add_value(&builder, field)
            }
            return g_variant_builder_end(&builder)
        } ?? nil
    }

    // MARK: - Property values

    /// A track id has to be a syntactically valid object path, and a video id is not one.
    ///
    /// Pure, and deliberately not isolated: it belongs to the encoding half, it is reachable
    /// from the C callbacks without a hop, and a test can call it on any thread.
    nonisolated static func trackPath(for videoID: String?) -> String {
        guard let videoID, !videoID.isEmpty else { return noTrackPath }
        return "/org/goosic/track/" + String(videoID.map { $0.isLetter || $0.isNumber ? $0 : "_" })
    }

    private static let getProperty: GDBusInterfaceGetPropertyFunc = {
        _, _, _, interfaceName, propertyName, _, userData in
        guard let interfaceName, let propertyName, let userData else { return nil }
        let controls = Unmanaged<SystemMediaControls>.fromOpaque(userData).takeUnretainedValue()
        let interface = String(cString: interfaceName)
        let name = String(cString: propertyName)
        var value: MPRISPropertyValue?
        MainActor.assumeIsolated { value = controls.propertyValue(interface, name) }
        guard let value else { return nil }
        return variant(for: value)
    }

    func propertyValue(_ interface: String, _ name: String) -> MPRISPropertyValue? {
        if interface == Self.rootInterface {
            switch name {
            case "CanQuit", "CanRaise", "HasTrackList": return .boolean(false)
            case "Identity": return .string("Goosic")
            case "DesktopEntry": return .string("goosic")
            // Goosic opens nothing handed to it from outside; the queue is its own.
            case "SupportedUriSchemes", "SupportedMimeTypes": return .strings([])
            default: return nil
            }
        }
        switch name {
        case "PlaybackStatus":
            switch published?.playbackState ?? .stopped {
            case .playing: return .string("Playing")
            case .paused: return .string("Paused")
            case .stopped: return .string("Stopped")
            }
        case "Metadata":
            return .metadata(metadata())
        case "Position":
            return .microseconds(Int64((published?.elapsedTime ?? 0) * 1_000_000))
        case "Volume":
            return .double(volume)
        // Goosic never varies the rate, and saying so keeps clients from offering a slider.
        case "Rate", "MinimumRate", "MaximumRate":
            return .double(1)
        case "CanGoNext": return .boolean(availability.next)
        case "CanGoPrevious": return .boolean(availability.previous)
        case "CanPlay": return .boolean(availability.play)
        case "CanPause": return .boolean(availability.pause)
        case "CanSeek": return .boolean(availability.changePosition)
        case "CanControl": return .boolean(true)
        default: return nil
        }
    }

    func metadata() -> MPRISMetadata {
        guard let published, published.isActive else {
            return MPRISMetadata(trackPath: Self.noTrackPath)
        }
        return MPRISMetadata(
            trackPath: Self.trackPath(for: model?.mediaSnapshot.track?.videoID),
            lengthMicroseconds: published.duration > 0
                ? Int64(published.duration * 1_000_000) : nil,
            title: published.title,
            artist: published.artist,
            album: published.album,
            artworkURL: published.artworkURL?.absoluteString
        )
    }

    private static let setProperty: GDBusInterfaceSetPropertyFunc = {
        _, _, _, _, propertyName, value, _, userData in
        guard let propertyName, let value, let userData else { return gboolean(0) }
        guard String(cString: propertyName) == "Volume" else { return gboolean(0) }
        let controls = Unmanaged<SystemMediaControls>.fromOpaque(userData).takeUnretainedValue()
        let requested = g_variant_get_double(value)
        var accepted = false
        MainActor.assumeIsolated { accepted = controls.setVolume(requested) }
        return gboolean(accepted ? 1 : 0)
    }

    func setVolume(_ requested: Double) -> Bool {
        guard let model, requested.isFinite,
              SystemMediaCommandAvailability.make(from: model.mediaSnapshot).changeVolume else {
            return false
        }
        model.setVolume(min(max(requested, 0), 1))
        return true
    }

    // MARK: - Methods

    private static let methodCall: GDBusInterfaceMethodCallFunc = {
        _, _, _, _, methodName, parameters, invocation, userData in
        guard let methodName, let invocation, let userData else { return }
        let controls = Unmanaged<SystemMediaControls>.fromOpaque(userData).takeUnretainedValue()
        let name = String(cString: methodName)
        // The argument is read here, where the variant lives, and crosses as a number.
        let argument: Int64?
        switch name {
        case "Seek": argument = parameters.flatMap { int64($0, at: 0) }
        case "SetPosition": argument = parameters.flatMap { int64($0, at: 1) }
        default: argument = nil
        }
        MainActor.assumeIsolated { controls.invoke(name, microseconds: argument) }
        // Every method here answers nothing, and answering at once is honest: these are
        // requests, and what actually happened arrives later as a confirmed sample.
        g_dbus_method_invocation_return_value(invocation, nil)
    }

    func invoke(_ method: String, microseconds: Int64?) {
        guard let model else { return }
        // Recomputed rather than read from the last update: the snapshot may have moved since
        // the panel drew the button that was pressed.
        let allowed = SystemMediaCommandAvailability.make(from: model.mediaSnapshot)
        switch method {
        case "Play" where allowed.play,
             "Pause" where allowed.pause,
             "PlayPause" where allowed.togglePlayPause:
            model.togglePause()
        case "Stop" where allowed.stop:
            model.releasePlayback()
        case "Next" where allowed.next:
            model.next()
        case "Previous" where allowed.previous:
            model.previous()
        case "Seek" where allowed.changePosition:
            guard let microseconds else { return }
            let target = (published?.elapsedTime ?? 0) + Double(microseconds) / 1_000_000
            model.seek(to: max(0, target))
        case "SetPosition" where allowed.changePosition:
            guard let microseconds else { return }
            model.seek(to: max(0, Double(microseconds) / 1_000_000))
        default:
            break
        }
    }

    /// `g_variant_get_child` is variadic and unreachable from Swift; the child-by-index form
    /// hands back a variant the typed getters can read.
    private static func int64(_ tuple: OpaquePointer, at index: Int) -> Int64? {
        guard let child = g_variant_get_child_value(tuple, gsize(index)) else { return nil }
        defer { g_variant_unref(child) }
        return Int64(g_variant_get_int64(child))
    }

    // MARK: - Change notification

    /// The properties a client redraws on. `Position` is deliberately absent: the specification
    /// keeps it out of this signal, because a player that announced every tick would wake every
    /// panel on the desktop several times a second.
    func changedProperties() -> [(String, MPRISPropertyValue)] {
        let status: String
        switch published?.playbackState ?? .stopped {
        case .playing: status = "Playing"
        case .paused: status = "Paused"
        case .stopped: status = "Stopped"
        }
        return [
            ("PlaybackStatus", .string(status)),
            ("Metadata", .metadata(metadata())),
            ("Volume", .double(volume)),
            ("CanGoNext", .boolean(availability.next)),
            ("CanGoPrevious", .boolean(availability.previous)),
            ("CanPlay", .boolean(availability.play)),
            ("CanPause", .boolean(availability.pause)),
            ("CanSeek", .boolean(availability.changePosition)),
        ]
    }

    private func emitPlayerPropertiesChanged() {
        guard let connection = OpaquePointer(bitPattern: connectionAddress) else { return }
        let properties = changedProperties()
        let changed = Self.withVariantType("a{sv}") { type -> OpaquePointer? in
            var builder = GVariantBuilder()
            g_variant_builder_init(&builder, type)
            for (name, value) in properties {
                guard let field = Self.entry(name, Self.variant(for: value)) else { continue }
                g_variant_builder_add_value(&builder, field)
            }
            return g_variant_builder_end(&builder)
        } ?? nil
        guard let changed, let invalidated = Self.stringArray([]) else { return }

        let payload = g_variant_new_tuple(
            [g_variant_new_string(Self.playerInterface), changed, invalidated], 3
        )
        var error: UnsafeMutablePointer<GError>?
        g_dbus_connection_emit_signal(
            connection, nil, Self.objectPath,
            "org.freedesktop.DBus.Properties", "PropertiesChanged",
            payload, &error
        )
        if let error { g_error_free(error) }
    }
}
#endif
