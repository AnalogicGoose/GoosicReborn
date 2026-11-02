import XCTest
@testable import GoosicSwift

/// The same cases as `the_volume_slider_follows_an_audio_taper` in `goosic-shell-support` and the
/// Windows `SliderFollowsAnAudioTaper`, so a saved volume opens at one slider position everywhere.
final class VolumeTaperTests: XCTestCase {
    func testSliderFollowsAnAudioTaper() {
        for (position, gain) in [(0.0, 0.0), (0.5, 0.125), (1.0, 1.0), (1.5, 1.0)] {
            XCTAssertEqual(VolumeTaper.gain(atPosition: position), gain, accuracy: 1e-10)
            XCTAssertEqual(VolumeTaper.position(forGain: gain), min(max(position, 0), 1), accuracy: 1e-10)
        }
        XCTAssertEqual(VolumeTaper.gain(atPosition: .nan), 0)
        XCTAssertEqual(VolumeTaper.position(forGain: -0.5), 0)
    }
}
