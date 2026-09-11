import Foundation

/// What to do with a volume the renderer reports.
enum VolumeReconciliation: Equatable {
    /// Leave the stored volume alone. The renderer is still reporting the value it had before
    /// the preference was pushed into it.
    case ignore
    /// The renderer has caught up with what was asked for. Nothing to store, but stop waiting.
    case settled
    /// Somebody changed the volume in the player itself. Follow it, and remember it.
    case adopt(Double)
}

/// How the hidden official renderer relates to Goosic's stored volume preference.
///
/// Unlike a local audio player, the official page is not exposed for interaction: its WebView is
/// mounted off-screen and does not accept pointer events. An unexpected renderer value therefore
/// signals that YouTube Music replaced or reset a media element, not that the user selected a
/// new volume there.
enum OfficialRendererVolumeDecision: Equatable {
    case waitingForEcho
    case settled
    case reapply(Double)
}

/// Deciding whether a renderer's reported volume is news.
///
/// A fresh renderer starts with its own volume and may report that value before it has received
/// the user's preference. Until it echoes the requested value, its reports describe the past.
///
/// With no request in flight, `reconcile` reports a value change for callers that can genuinely
/// accept a renderer as a source of truth. The off-screen official WebView is not one of those:
/// `reconcileOfficialRenderer` turns that same result into a reapplication of Goosic's stored
/// preference instead.
enum VolumeSync {
    /// Volumes are floats that survive a round trip through JavaScript; exact equality would
    /// leave a push outstanding forever over a difference nobody can hear.
    static let tolerance = 0.01

    static func reconcile(
        reported: Double,
        current: Double,
        requested: Double?
    ) -> VolumeReconciliation {
        if let requested {
            return abs(reported - requested) <= tolerance ? .settled : .ignore
        }
        return abs(reported - current) > tolerance ? .adopt(reported) : .settled
    }

    /// The official player confirms the native preference; it never becomes the preference's
    /// author. This prevents a fresh media element's 100% default from being saved over the
    /// value the person chose in Goosic.
    static func reconcileOfficialRenderer(
        reported: Double,
        preferred: Double,
        requested: Double?
    ) -> OfficialRendererVolumeDecision {
        switch reconcile(reported: reported, current: preferred, requested: requested) {
        case .ignore:
            return .waitingForEcho
        case .settled:
            return .settled
        case .adopt:
            return .reapply(preferred)
        }
    }

    /// A pre-roll can be followed by a replacement media element. Its first report is the
    /// renderer's default, not a volume choice the user made inside the player.
    static func shouldReapplyPreference(
        wasAdvertisement: Bool,
        isAdvertisement: Bool
    ) -> Bool {
        wasAdvertisement && !isAdvertisement
    }
}
