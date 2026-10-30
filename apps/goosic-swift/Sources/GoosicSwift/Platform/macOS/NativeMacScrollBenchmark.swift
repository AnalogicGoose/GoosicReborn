#if os(macOS)
import AppKit
import QuartzCore

/// A development measurement, never reached in normal use. With `GOOSIC_SCROLL_BENCH` set to
/// `page` or `queue`, and normally `GOOSIC_UI_FIXTURE=perf`, it scrolls that region with
/// synthetic continuous wheel events once per display frame, prints how long each frame took
/// to stderr, and quits. A frame that arrives late is a frame the main thread was too busy to
/// draw on time, which is what a listener sees as a stutter.
///
/// `GOOSIC_SCROLL_BENCH_POINTER=away` measures with the pointer off the window; the default puts
/// it over the scrolled region, as it is when someone scrolls. The result line also says how
/// much of the run Goosic was the active app, since macOS tracks hover only then: a run made
/// while another app kept the focus measured a different case.
@MainActor
enum NativeMacScrollBenchmark {
    static func startIfRequested() {
        let environment = ProcessInfo.processInfo.environment
        guard let target = environment["GOOSIC_SCROLL_BENCH"], !target.isEmpty else { return }
        let seconds = Double(environment["GOOSIC_SCROLL_BENCH_SECONDS"] ?? "") ?? 8
        DispatchQueue.main.asyncAfter(deadline: .now() + 4) {
            guard let window = NSApplication.shared.windows.first(where: { $0.isVisible && $0.contentView != nil }),
                  let scrollView = scrollView(in: window, target: target) else {
                FileHandle.standardError.write(Data("scroll-bench: no \(target) scroll view\n".utf8))
                exit(2)
            }
            // Where the pointer rests decides which row is hovered as rows slide under it, and
            // hover is real work. Move the window, never the pointer: `over` puts the pointer on
            // the scrolled region, as it is when someone scrolls; `away` keeps it off the window.
            let pointer = NSEvent.mouseLocation
            let region = window.convertToScreen(scrollView.convert(scrollView.bounds, to: nil))
            var origin = window.frame.origin
            if environment["GOOSIC_SCROLL_BENCH_POINTER"] == "away" {
                origin.x = pointer.x + 40
            } else {
                origin.x += pointer.x - region.midX
                origin.y += pointer.y - region.midY
            }
            window.setFrameOrigin(origin)
            NativeMacScrollDriver.start(scrollView: scrollView, window: window, seconds: seconds,
                                        label: "\(target) pointer=\(environment["GOOSIC_SCROLL_BENCH_POINTER"] ?? "over")")
        }
    }

    /// The tallest vertical scroll view in the page column, or in the trailing panel for the queue.
    private static func scrollView(in window: NSWindow, target: String) -> NSScrollView? {
        var found: [NSScrollView] = []
        func visit(_ view: NSView) {
            if let scroll = view as? NSScrollView { found.append(scroll) }
            view.subviews.forEach(visit)
        }
        if let root = window.contentView?.superview ?? window.contentView { visit(root) }
        let width = window.frame.width
        return found
            .filter { scroll in
                guard let document = scroll.documentView, !scroll.isHidden, scroll.window != nil else { return false }
                let frame = scroll.convert(scroll.bounds, to: nil)
                let vertical = document.frame.height > scroll.bounds.height + 1
                let inPanel = frame.minX > width - 340
                return vertical && (target == "queue" ? inPanel : !inPanel) && frame.width > 200
            }
            .max { $0.bounds.height * $0.bounds.width < $1.bounds.height * $1.bounds.width }
    }
}

@MainActor
private final class NativeMacScrollDriver: NSObject {
    private static var current: NativeMacScrollDriver?
    private let scrollView: NSScrollView
    private let seconds: Double
    private let label: String
    private var link: CADisplayLink?
    private var start: CFTimeInterval = 0
    private var last: CFTimeInterval = 0
    private var intervals: [Double] = []
    private var period: Double = 1.0 / 60
    private var direction: Double = -1
    private var stuckFrames = 0
    private var lastOrigin: CGFloat = .nan
    private var distance: CGFloat = 0
    /// Frames on which the pointer was over the scrolled region, which makes rows hover.
    private var pointerFrames = 0
    /// The gesture has begun, so later events continue it, as a trackpad's do.
    private var began = false
    /// Frames on which Goosic was the active app. Hover is tracked only then.
    private var activeFrames = 0

    private init(scrollView: NSScrollView, seconds: Double, label: String) {
        self.scrollView = scrollView
        self.seconds = seconds
        self.label = label
    }

    static func start(scrollView: NSScrollView, window: NSWindow, seconds: Double, label: String) {
        let driver = NativeMacScrollDriver(scrollView: scrollView, seconds: seconds, label: label)
        current = driver
        let link = scrollView.displayLink(target: driver, selector: #selector(frame(_:)))
        link.add(to: .main, forMode: .common)
        driver.link = link
    }

    @objc private func frame(_ link: CADisplayLink) {
        let now = CACurrentMediaTime()
        if start == 0 {
            start = now
            period = max(link.targetTimestamp - link.timestamp, 1.0 / 240)
        } else {
            intervals.append(now - last)
        }
        last = now
        if let window = scrollView.window,
           window.convertToScreen(scrollView.convert(scrollView.bounds, to: nil)).contains(NSEvent.mouseLocation) {
            pointerFrames += 1
        }
        if NSApplication.shared.isActive { activeFrames += 1 }
        if now - start >= seconds { finish(); return }

        // About 2,400 points a second at 60 Hz: a brisk trackpad flick, held.
        let step = 40 * (period * 60)
        let origin = scrollView.contentView.bounds.origin.y
        if origin == lastOrigin { stuckFrames += 1 } else { stuckFrames = 0 }
        distance += abs(origin - (lastOrigin.isNaN ? origin : lastOrigin))
        lastOrigin = origin
        if stuckFrames >= 3 { direction = -direction; stuckFrames = 0 }
        guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1,
                                  wheel1: Int32(direction * step), wheel2: 0, wheel3: 0) else { return }
        event.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        // A trackpad's own phases, so the scroll view sees one held gesture: began, then changed.
        event.setIntegerValueField(.scrollWheelEventScrollPhase, value: began ? 2 : 1)
        began = true
        if let wheel = NSEvent(cgEvent: event) { scrollView.scrollWheel(with: wheel) }
    }

    private func finish() {
        link?.invalidate()
        let region = scrollView.window.map { $0.convertToScreen(scrollView.convert(scrollView.bounds, to: nil)) } ?? .zero
        let pointer = CGPoint(x: NSEvent.mouseLocation.x - region.minX, y: region.maxY - NSEvent.mouseLocation.y)
        let sorted = intervals.sorted()
        func percentile(_ p: Double) -> Double { sorted[min(sorted.count - 1, Int(Double(sorted.count) * p))] * 1000 }
        let budget = period * 1.5
        let hitches = intervals.filter { $0 > budget }
        let hitchTime = intervals.reduce(0) { $0 + max(0, $1 - period) }
        let elapsed = intervals.reduce(0, +)
        let line = String(
            format: "scroll-bench %@: frames=%d refresh=%.0fHz median=%.2fms p95=%.2fms p99=%.2fms max=%.1fms late=%d (%.1f%%) hitch=%.1fms/s scrolled=%.0fpt pointerOver=%.0f%% at=(%.0f,%.0f) of %.0fx%.0f active=%.0f%%\n",
            label, intervals.count, 1 / period, percentile(0.5), percentile(0.95), percentile(0.99),
            (sorted.last ?? 0) * 1000, hitches.count, 100 * Double(hitches.count) / Double(max(intervals.count, 1)),
            1000 * hitchTime / max(elapsed, 0.001), distance,
            100 * Double(pointerFrames) / Double(max(intervals.count, 1)),
            pointer.x, pointer.y, region.width, region.height,
            100 * Double(activeFrames) / Double(max(intervals.count, 1))
        )
        FileHandle.standardError.write(Data(line.utf8))
        exit(0)
    }
}
#endif
