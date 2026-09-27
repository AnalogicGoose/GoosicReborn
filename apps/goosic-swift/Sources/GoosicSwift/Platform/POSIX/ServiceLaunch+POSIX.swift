#if !os(Windows)
import Foundation

/// How a configured service path becomes something `Process` can launch, on the platforms
/// whose separator is `/`.
///
/// Foundation's `Process` will not search `PATH` for a bare name, so a name without a
/// separator is handed to `env`, which will. Anything carrying a separator is already a path
/// and is launched directly.
enum ServiceLaunch {
    static func plan(for configured: String) -> (url: URL, arguments: [String]) {
        if configured.contains("/") {
            return (URL(fileURLWithPath: configured), [])
        }
        return (URL(fileURLWithPath: "/usr/bin/env"), [configured])
    }
}
#endif
