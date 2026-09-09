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

/// Deciding whether a renderer's reported volume is news.
///
/// The official player is a web page with a volume of its own, and it reports one on every event.
/// Two true things about that are in tension: a fresh page starts at its own volume and has to be
/// told the user's preference, and a user who moves the volume slider inside the page means it.
/// Following every report satisfies the second and breaks the first — the page announces its
/// default, the shell adopts it, and the preference the user set is gone. That is the whole of the
/// "volume keeps going back to 100" bug: the push was sent, the next event still carried the old
/// value because the push had not taken effect yet, and the shell believed it.
///
/// The fix is to stop treating a report as news while a push is outstanding. Until the renderer
/// echoes the value it was given, its reports describe the past.
///
/// The other half of the bug was quieter. When the shell did adopt a volume, it changed the value
/// in memory and never wrote it down, so the preference on disk and the volume in the app
/// disagreed until something else happened to save. `adopt` carries the value precisely so the
/// caller has to decide what to do with it, rather than assigning a property and moving on.
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
}
