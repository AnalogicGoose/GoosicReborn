import Foundation

/// Installed at document start, before the official application can start a media element.
/// Native setters are retained so enforcing the preference cannot recursively trigger itself.
enum OfficialVolumeBootstrap {
    static func script(volume: Double, muted: Bool) -> String {
        let safeVolume = volume.isFinite ? min(max(volume, 0), 1) : 0
        return """
        (() => {
          let preferred = \(safeVolume), intendedMute = \(muted);
          const proto = HTMLMediaElement.prototype;
          const volume = Object.getOwnPropertyDescriptor(proto, 'volume');
          const muted = Object.getOwnPropertyDescriptor(proto, 'muted');
          const originalPlay = proto.play;
          const advertisement = () => !!document.querySelector('.ad-showing, .ad-interrupting');
          function apply(media) {
            if (advertisement()) return;
            if (volume.get.call(media) === preferred && muted.get.call(media) === intendedMute) return;
            muted.set.call(media, true);
            volume.set.call(media, preferred);
            muted.set.call(media, intendedMute);
          }
          Object.defineProperty(proto, 'volume', {
            ...volume,
            set(value) {
              if (advertisement()) { volume.set.call(this, value); return; }
              apply(this);
            }
          });
          Object.defineProperty(proto, 'muted', {
            ...muted,
            set(value) {
              if (advertisement()) { muted.set.call(this, value); return; }
              apply(this);
            }
          });
          proto.play = function(...args) { apply(this); return originalPlay.apply(this, args); };
          const applyAll = () => document.querySelectorAll('video, audio').forEach(apply);
          window.goosicSetVolumePreference = (value, mute) => {
            if (advertisement()) return;
            if (Number.isFinite(value)) preferred = Math.min(1, Math.max(0, value));
            if (typeof mute === 'boolean') intendedMute = mute;
            applyAll();
          };
          for (const event of ['loadstart', 'loadedmetadata', 'play', 'volumechange']) {
            document.addEventListener(event, event => {
              if (event.target instanceof HTMLMediaElement) apply(event.target);
            }, true);
          }
          new MutationObserver(applyAll).observe(document, {
            subtree: true, childList: true, attributes: true, attributeFilter: ['class', 'src']
          });
          applyAll();
        })();
        """
    }
}
