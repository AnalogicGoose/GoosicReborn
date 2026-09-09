#if os(Linux)
import CGStreamer
import Foundation

/// The sole local-file audio renderer on Linux. Like the AVFoundation host it mirrors, it
/// accepts only the decoded cache path Rust produced; it never opens a WebM source, starts a
/// network request, or reads account state.
///
/// GStreamer does the decoding, so this is a pipeline rather than a player object, and two
/// consequences follow. Position has to be asked for rather than read, which is why samples come
/// from a timer on the GLib main loop; and the end of a file arrives as a bus message rather than
/// a delegate call. What Rust sees is identical either way: the same states, and a sample
/// sequence that only ever increases.
@MainActor
final class LocalPlaybackHost {
    private var pipeline: UnsafeMutablePointer<GstElement>?
    private var busWatch: guint = 0
    private var tick: guint = 0
    private var generation: UInt64?
    private var videoID: String?
    private var sequence: UInt64 = 0
    private var preferredVolume = 1.0
    private var muted = false
    /// Set when the pipeline reports end-of-stream, so the timer stops claiming it is playing.
    private var ended = false

    var onEvent: ((LocalPlaybackEvent) -> Void)?
    var onStatus: ((String) -> Void)?

    var loadedVideoID: String? { videoID }
    var isLoaded: Bool { pipeline != nil }

    /// GStreamer must be initialised once before anything else in it is touched.
    private static let initialized: Bool = {
        gst_init(nil, nil)
        return true
    }()

    /// Opens a decoded cache file. The caller must already hold the Rust local-file lease.
    func prepare(localFile: String, videoID: String, generation: UInt64) throws {
        _ = Self.initialized
        let url = URL(fileURLWithPath: localFile, isDirectory: false)
        guard url.isFileURL, FileManager.default.fileExists(atPath: url.path) else {
            throw NSError(domain: "GoosicPlayback", code: 2, userInfo: [
                NSLocalizedDescriptionKey: "Rust returned a local audio path that is not present on disk.",
            ])
        }
        let previousGeneration = self.generation
        let previousSequence = self.sequence
        stop()

        guard let element = gst_element_factory_make("playbin3", "goosic-local")
            ?? gst_element_factory_make("playbin", "goosic-local") else {
            throw NSError(domain: "GoosicPlayback", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "GStreamer has no playbin element; install gstreamer1-plugins-base.",
            ])
        }
        // Built rather than concatenated: a path with a space or a `#` in it is a different URI
        // once it has been escaped, and this is the one place that could get it wrong.
        var uriError: UnsafeMutablePointer<GError>?
        guard let uri = gst_filename_to_uri(url.path, &uriError) else {
            if let uriError { g_error_free(uriError) }
            gst_object_unref(UnsafeMutableRawPointer(element))
            throw NSError(domain: "GoosicPlayback", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "The decoded local audio path could not be expressed as a file URI.",
            ])
        }
        defer { g_free(uri) }
        Self.set(element, "uri", string: String(cString: uri))
        Self.set(element, "volume", double: muted ? 0 : preferredVolume)
        Self.set(element, "mute", boolean: muted)

        // Paused rather than playing: `prepare` opens the file, and `play` is a separate request
        // that the model makes only once Rust has confirmed the lease.
        guard gst_element_set_state(element, GST_STATE_PAUSED) != GST_STATE_CHANGE_FAILURE else {
            gst_object_unref(UnsafeMutableRawPointer(element))
            throw NSError(domain: "GoosicPlayback", code: 3, userInfo: [
                NSLocalizedDescriptionKey: "GStreamer could not prepare the decoded local audio file.",
            ])
        }

        pipeline = element
        ended = false
        self.videoID = videoID
        self.generation = generation
        // A same-owner track replacement keeps the same Rust generation, so its samples must
        // continue after the previous track. A newly claimed generation starts at sequence one.
        sequence = previousGeneration == generation ? previousSequence : 0
        watchBus(of: element)
    }

    @discardableResult
    func play() -> Bool {
        guard let pipeline, let generation, let videoID else {
            onStatus?("Prepare a decoded local file before pressing play.")
            return false
        }
        guard gst_element_set_state(pipeline, GST_STATE_PLAYING) != GST_STATE_CHANGE_FAILURE else {
            onStatus?("GStreamer rejected the local audio play request.")
            return false
        }
        ended = false
        scheduleTimer()
        emit(state: "playing", generation: generation, videoID: videoID)
        return true
    }

    func pause() {
        guard let pipeline, let generation, let videoID else { return }
        gst_element_set_state(pipeline, GST_STATE_PAUSED)
        stopTimer()
        emit(state: "paused", generation: generation, videoID: videoID)
    }

    /// Stops local audio synchronously before its Rust lease is released or changed.
    func stop() {
        teardownPipeline()
        generation = nil
        videoID = nil
        sequence = 0
    }

    /// Stops the current file but retains the lease identity and sample counter for an immediate
    /// same-owner track replacement.
    func stopForReplacement() {
        teardownPipeline()
    }

    private func teardownPipeline() {
        stopTimer()
        if busWatch != 0 {
            g_source_remove(busWatch)
            busWatch = 0
        }
        if let pipeline {
            // NULL first: a pipeline still holding the audio device would keep the sound card
            // open past the moment Rust believes this renderer let go.
            gst_element_set_state(pipeline, GST_STATE_NULL)
            gst_object_unref(UnsafeMutableRawPointer(pipeline))
        }
        pipeline = nil
        ended = false
    }

    func seek(to seconds: Double) {
        guard let pipeline, let generation, let videoID,
              seconds.isFinite, seconds >= 0 else { return }
        let total = duration()
        guard total > 0 else { return }
        let target = min(seconds, total)
        gst_element_seek_simple(
            pipeline,
            GST_FORMAT_TIME,
            GstSeekFlags(rawValue: GST_SEEK_FLAG_FLUSH.rawValue | GST_SEEK_FLAG_KEY_UNIT.rawValue),
            gint64(target * 1_000_000_000)
        )
        emit(state: isPlaying ? "playing" : "paused", generation: generation, videoID: videoID)
    }

    func setVolume(_ volume: Double) {
        guard volume.isFinite else { return }
        preferredVolume = min(max(volume, 0), 1)
        guard let pipeline, !muted else { return }
        Self.set(pipeline, "volume", double: preferredVolume)
    }

    func setMuted(_ muted: Bool) {
        self.muted = muted
        guard let pipeline else { return }
        Self.set(pipeline, "mute", boolean: muted)
        Self.set(pipeline, "volume", double: muted ? 0 : preferredVolume)
    }

    // MARK: - Pipeline state

    private var isPlaying: Bool {
        guard let pipeline, !ended else { return false }
        var state = GST_STATE_NULL
        var pending = GST_STATE_NULL
        gst_element_get_state(pipeline, &state, &pending, 0)
        return state == GST_STATE_PLAYING
    }

    private func position() -> Double {
        guard let pipeline else { return 0 }
        var nanoseconds: gint64 = 0
        guard gst_element_query_position(pipeline, GST_FORMAT_TIME, &nanoseconds) != 0,
              nanoseconds >= 0 else { return 0 }
        return Double(nanoseconds) / 1_000_000_000
    }

    private func duration() -> Double {
        guard let pipeline else { return 0 }
        var nanoseconds: gint64 = 0
        // A duration is not known the instant a file is opened, so a zero here means "not yet",
        // never "empty file". The shared projection already treats zero as unknown.
        guard gst_element_query_duration(pipeline, GST_FORMAT_TIME, &nanoseconds) != 0,
              nanoseconds >= 0 else { return 0 }
        return Double(nanoseconds) / 1_000_000_000
    }

    // MARK: - Property helpers

    /// `g_object_set` is variadic and unreachable from Swift, and `G_TYPE_STRING` and its
    /// siblings are cast macros the importer cannot see either. Naming the fundamental types is
    /// what is left, and it is at least explicit about what is being written.
    private static func set(_ element: UnsafeMutablePointer<GstElement>, _ name: String, string value: String) {
        var boxed = GValue()
        g_value_init(&boxed, g_type_from_name("gchararray"))
        g_value_set_string(&boxed, value)
        g_object_set_property(UnsafeMutableRawPointer(element).assumingMemoryBound(to: GObject.self), name, &boxed)
        g_value_unset(&boxed)
    }

    private static func set(_ element: UnsafeMutablePointer<GstElement>, _ name: String, double value: Double) {
        var boxed = GValue()
        g_value_init(&boxed, g_type_from_name("gdouble"))
        g_value_set_double(&boxed, value)
        g_object_set_property(UnsafeMutableRawPointer(element).assumingMemoryBound(to: GObject.self), name, &boxed)
        g_value_unset(&boxed)
    }

    private static func set(_ element: UnsafeMutablePointer<GstElement>, _ name: String, boolean value: Bool) {
        var boxed = GValue()
        g_value_init(&boxed, g_type_from_name("gboolean"))
        g_value_set_boolean(&boxed, gboolean(value ? 1 : 0))
        g_object_set_property(UnsafeMutableRawPointer(element).assumingMemoryBound(to: GObject.self), name, &boxed)
        g_value_unset(&boxed)
    }

    // MARK: - Samples

    /// Position is queried rather than delivered, so the sample rate is ours to choose. A quarter
    /// second is what the AVFoundation host uses, and matching it keeps the transport on both
    /// platforms moving at the same rate.
    private func scheduleTimer() {
        stopTimer()
        tick = g_timeout_add(250, Self.timerFired, Unmanaged.passUnretained(self).toOpaque())
    }

    private func stopTimer() {
        guard tick != 0 else { return }
        g_source_remove(tick)
        tick = 0
    }

    private static let timerFired: GSourceFunc = { userData in
        guard let userData else { return gboolean(0) }
        let host = Unmanaged<LocalPlaybackHost>.fromOpaque(userData).takeUnretainedValue()
        var keepGoing = false
        MainActor.assumeIsolated { keepGoing = host.sampleNow() }
        return gboolean(keepGoing ? 1 : 0)
    }

    /// Emits one sample and says whether the timer should fire again.
    private func sampleNow() -> Bool {
        guard let generation, let videoID, pipeline != nil else { return false }
        let playing = isPlaying
        emit(state: playing ? "playing" : "paused", generation: generation, videoID: videoID)
        if !playing { tick = 0 }
        return playing
    }

    private func emit(state: String, generation: UInt64, videoID: String) {
        sequence &+= 1
        onEvent?(LocalPlaybackEvent(
            generation: generation,
            videoID: videoID,
            sequence: sequence,
            state: state,
            currentTime: max(0, position()),
            duration: max(0, duration()),
            volume: muted ? 0 : preferredVolume,
            isMuted: muted
        ))
    }

    // MARK: - Bus

    /// End of stream and decoding failures arrive here rather than through a delegate.
    private func watchBus(of element: UnsafeMutablePointer<GstElement>) {
        guard let bus = gst_element_get_bus(element) else { return }
        defer { gst_object_unref(UnsafeMutableRawPointer(bus)) }
        busWatch = gst_bus_add_watch(bus, Self.busMessage, Unmanaged.passUnretained(self).toOpaque())
    }

    private static let busMessage: GstBusFunc = { _, message, userData in
        guard let message, let userData else { return gboolean(1) }
        let host = Unmanaged<LocalPlaybackHost>.fromOpaque(userData).takeUnretainedValue()
        let type = message.pointee.type
        if type == GST_MESSAGE_EOS {
            MainActor.assumeIsolated { host.finished() }
        } else if type == GST_MESSAGE_ERROR {
            var error: UnsafeMutablePointer<GError>?
            var debug: UnsafeMutablePointer<gchar>?
            gst_message_parse_error(message, &error, &debug)
            let text = error.map { String(cString: $0.pointee.message) } ?? "an unknown failure"
            if let error { g_error_free(error) }
            if let debug { g_free(debug) }
            MainActor.assumeIsolated { host.failed(text) }
        }
        return gboolean(1)
    }

    private func finished() {
        guard let generation, let videoID else { return }
        stopTimer()
        ended = true
        emit(state: "ended", generation: generation, videoID: videoID)
    }

    /// A decode failure is reported and the renderer is torn down. It must not keep the lease
    /// while producing nothing: Rust would believe a local file was still playing.
    private func failed(_ reason: String) {
        onStatus?("GStreamer could not play the decoded local file: \(reason).")
        guard let generation, let videoID else { return }
        stopTimer()
        ended = true
        emit(state: "ended", generation: generation, videoID: videoID)
    }
}
#endif
