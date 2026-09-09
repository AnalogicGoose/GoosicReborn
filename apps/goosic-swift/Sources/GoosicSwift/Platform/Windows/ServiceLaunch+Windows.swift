#if os(Windows)
import Foundation

/// How a configured service path becomes something `Process` can launch on Windows.
///
/// Two things differ from the POSIX rule and both are silent failures rather than errors.
/// Windows separates path components with `\`, so the POSIX test for `/` reads an ordinary
/// absolute path as a bare command name; and there is no `env` to defer the `PATH` search to,
/// so that name would be launched as `/usr/bin/env`, which does not exist. The service then
/// never starts and the shell reports only that it is unavailable.
enum ServiceLaunch {
    static func plan(for configured: String) -> (url: URL, arguments: [String]) {
        if configured.contains("/") || configured.contains("\\") {
            return (URL(fileURLWithPath: configured), [])
        }
        // A bare name: search `PATH` here, because `Process` resolves a relative
        // `executableURL` against the working directory rather than searching for it.
        return (resolve(name: configured) ?? URL(fileURLWithPath: configured), [])
    }

    /// The first entry on `PATH` holding an executable of that name, trying the extensions
    /// Windows treats as executable when the name carries none.
    private static func resolve(name: String) -> URL? {
        let environment = ProcessInfo.processInfo.environment
        let searched = environment["PATH"] ?? ""
        let extensions = URL(fileURLWithPath: name).pathExtension.isEmpty
            ? (environment["PATHEXT"] ?? ".COM;.EXE;.BAT;.CMD")
                .split(separator: ";")
                .map(String.init)
            : [""]

        for directory in searched.split(separator: ";") {
            let base = URL(fileURLWithPath: String(directory)).appendingPathComponent(name)
            for suffix in extensions {
                let candidate = suffix.isEmpty
                    ? base
                    : URL(fileURLWithPath: base.path + suffix.lowercased())
                if FileManager.default.isExecutableFile(atPath: candidate.path) {
                    return candidate
                }
            }
        }
        return nil
    }
}
#endif
