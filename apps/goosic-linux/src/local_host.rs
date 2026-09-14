//! The one GStreamer pipeline that plays downloaded files.
//!
//! It accepts only the decoded cache path Rust produced under the `localDownloadedFile` lease; it
//! never opens a source file, starts a network request or reads account state. GStreamer is a
//! pipeline rather than a player object, so position is asked for on a quarter-second timer rather
//! than delivered, and the end of a file arrives on the bus. What Rust sees is what the official
//! host gives it: states, and a sample sequence that only increases.
//!
//! Events reach the shell from an idle callback rather than from inside the call that produced
//! them. A shell that answers an event by stopping or replacing the file therefore never tears the
//! pipeline down underneath a GStreamer callback that is still running.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;
use gtk::glib;

/// How often position is sampled while playing — the rate the official host reports at too, so
/// both transports move alike.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

/// One report from the local renderer, in the shape the official bridge's reports take.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalEvent {
    pub generation: u64,
    pub video_id: String,
    pub sequence: u64,
    pub state: &'static str,
    pub current_time: f64,
    pub duration: f64,
}

/// Where the host tells the shell what happened.
pub struct LocalHandlers {
    pub on_event: Box<dyn Fn(LocalEvent)>,
    pub on_status: Box<dyn Fn(String)>,
}

struct Loaded {
    pipeline: gst::Element,
    /// Dropping the guard removes the bus watch, so it lives exactly as long as the pipeline.
    _bus: gst::bus::BusWatchGuard,
}

pub struct LocalHost {
    loaded: RefCell<Option<Loaded>>,
    tick: RefCell<Option<glib::SourceId>>,
    /// The generation and video of the file loaded under Rust's lease.
    identity: RefCell<Option<(u64, String)>>,
    sequence: Cell<u64>,
    volume: Cell<f64>,
    muted: Cell<bool>,
    /// Set by the end of the stream, so the timer stops calling a finished file playing.
    ended: Cell<bool>,
    handlers: LocalHandlers,
}

impl LocalHost {
    pub fn new(handlers: LocalHandlers) -> Rc<LocalHost> {
        Rc::new(LocalHost {
            loaded: RefCell::new(None),
            tick: RefCell::new(None),
            identity: RefCell::new(None),
            sequence: Cell::new(0),
            volume: Cell::new(1.0),
            muted: Cell::new(false),
            ended: Cell::new(false),
            handlers,
        })
    }

    pub fn loaded_video_id(&self) -> Option<String> {
        self.identity
            .borrow()
            .as_ref()
            .map(|(_, video_id)| video_id.clone())
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded.borrow().is_some()
    }

    /// Opens a decoded file, paused. The caller must already hold Rust's local lease; playing is a
    /// separate request.
    pub fn prepare(
        self: &Rc<Self>,
        file: &Path,
        video_id: &str,
        generation: u64,
    ) -> Result<(), String> {
        gst::init().map_err(|error| format!("GStreamer could not start: {error}"))?;
        if !file.is_file() {
            return Err("Rust returned a local audio path that is not present on disk.".to_owned());
        }
        let previous_generation = self
            .identity
            .borrow()
            .as_ref()
            .map(|(generation, _)| *generation);
        let previous_sequence = self.sequence.get();
        self.teardown();

        let pipeline = gst::ElementFactory::make("playbin3")
            .name("goosic-local")
            .build()
            .or_else(|_| {
                gst::ElementFactory::make("playbin")
                    .name("goosic-local")
                    .build()
            })
            .map_err(|_| {
                "GStreamer has no playbin element; install the GStreamer base plugins.".to_owned()
            })?;
        // Built rather than concatenated: a path with a space or a `#` in it is a different URI
        // once escaped, and this is the one place that could get it wrong.
        let uri = glib::filename_to_uri(file, None).map_err(|_| {
            "The decoded local audio path could not be expressed as a file URI.".to_owned()
        })?;
        pipeline.set_property("uri", uri.as_str());
        pipeline.set_property(
            "volume",
            if self.muted.get() {
                0.0
            } else {
                self.volume.get()
            },
        );
        pipeline.set_property("mute", self.muted.get());

        if pipeline.set_state(gst::State::Paused).is_err() {
            let _ = pipeline.set_state(gst::State::Null);
            return Err("GStreamer could not prepare the decoded local audio file.".to_owned());
        }
        let watch = pipeline.bus().and_then(|bus| {
            let weak = Rc::downgrade(self);
            bus.add_watch_local(move |_, message| {
                if let Some(host) = weak.upgrade() {
                    match message.view() {
                        gst::MessageView::Eos(_) => host.finished(),
                        gst::MessageView::Error(error) => host.failed(&error.error().to_string()),
                        _ => {}
                    }
                }
                glib::ControlFlow::Continue
            })
            .ok()
        });
        let Some(watch) = watch else {
            let _ = pipeline.set_state(gst::State::Null);
            return Err("GStreamer could not watch the local pipeline.".to_owned());
        };

        *self.loaded.borrow_mut() = Some(Loaded {
            pipeline,
            _bus: watch,
        });
        self.ended.set(false);
        *self.identity.borrow_mut() = Some((generation, video_id.to_owned()));
        // Replacing one file with another keeps Rust's generation, so its samples continue after
        // the previous file's. A newly claimed generation starts counting again.
        self.sequence
            .set(if previous_generation == Some(generation) {
                previous_sequence
            } else {
                0
            });
        Ok(())
    }

    /// Starts the prepared file. Returns false, and says why, when there is nothing to start.
    pub fn play(self: &Rc<Self>) -> bool {
        let Some(pipeline) = self.pipeline() else {
            self.status("Prepare a decoded local file before pressing play.");
            return false;
        };
        if pipeline.set_state(gst::State::Playing).is_err() {
            self.status("GStreamer refused to play the local file.");
            return false;
        }
        self.ended.set(false);
        self.schedule_timer();
        self.emit("playing");
        true
    }

    pub fn pause(self: &Rc<Self>) {
        let Some(pipeline) = self.pipeline() else {
            return;
        };
        let _ = pipeline.set_state(gst::State::Paused);
        self.stop_timer();
        self.emit("paused");
    }

    /// Stops the file and forgets its identity. Call before Rust's lease is released or changed.
    pub fn stop(&self) {
        self.teardown();
        *self.identity.borrow_mut() = None;
        self.sequence.set(0);
    }

    /// Stops the file but keeps the lease identity and sample count, for replacing it with another
    /// under the same generation.
    pub fn stop_for_replacement(&self) {
        self.teardown();
    }

    pub fn seek(self: &Rc<Self>, seconds: f64) {
        let Some(pipeline) = self.pipeline() else {
            return;
        };
        let total = self.duration();
        if !seconds.is_finite() || seconds < 0.0 || total <= 0.0 {
            return;
        }
        let target = gst::ClockTime::from_nseconds((seconds.min(total) * 1e9) as u64);
        let _ = pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, target);
        self.emit(if self.is_playing() {
            "playing"
        } else {
            "paused"
        });
    }

    pub fn set_volume(&self, volume: f64) {
        if !volume.is_finite() {
            return;
        }
        self.volume.set(volume.clamp(0.0, 1.0));
        if let (Some(pipeline), false) = (self.pipeline(), self.muted.get()) {
            pipeline.set_property("volume", self.volume.get());
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.set(muted);
        if let Some(pipeline) = self.pipeline() {
            pipeline.set_property("mute", muted);
            pipeline.set_property("volume", if muted { 0.0 } else { self.volume.get() });
        }
    }

    fn pipeline(&self) -> Option<gst::Element> {
        self.loaded
            .borrow()
            .as_ref()
            .map(|loaded| loaded.pipeline.clone())
    }

    fn teardown(&self) {
        self.stop_timer();
        let loaded = self.loaded.borrow_mut().take();
        if let Some(loaded) = loaded {
            // Null first: a pipeline still holding the audio device would keep the sound card open
            // past the moment Rust believes this renderer let go.
            let _ = loaded.pipeline.set_state(gst::State::Null);
        }
        self.ended.set(false);
    }

    fn is_playing(&self) -> bool {
        !self.ended.get()
            && self
                .pipeline()
                .is_some_and(|pipeline| pipeline.current_state() == gst::State::Playing)
    }

    fn position(&self) -> f64 {
        self.pipeline()
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .map_or(0.0, gst::ClockTime::seconds_f64)
    }

    /// Zero means not known yet, never an empty file: a duration is not known the instant a file
    /// is opened, and the shared rules already read zero as unknown.
    fn duration(&self) -> f64 {
        self.pipeline()
            .and_then(|pipeline| pipeline.query_duration::<gst::ClockTime>())
            .map_or(0.0, gst::ClockTime::seconds_f64)
    }

    fn schedule_timer(self: &Rc<Self>) {
        self.stop_timer();
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(SAMPLE_INTERVAL, move || match weak.upgrade() {
            Some(host) => host.sample_now(),
            None => glib::ControlFlow::Break,
        });
        *self.tick.borrow_mut() = Some(source);
    }

    fn stop_timer(&self) {
        let source = self.tick.borrow_mut().take();
        if let Some(source) = source {
            source.remove();
        }
    }

    /// One sample; the timer runs again only while the file plays.
    fn sample_now(self: &Rc<Self>) -> glib::ControlFlow {
        if self.identity.borrow().is_none() || !self.is_loaded() {
            // Returning Break removes the source, so its id must not be removed a second time.
            self.tick.borrow_mut().take();
            return glib::ControlFlow::Break;
        }
        let playing = self.is_playing();
        if !playing {
            self.tick.borrow_mut().take();
        }
        self.emit(if playing { "playing" } else { "paused" });
        if playing {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    }

    fn finished(self: &Rc<Self>) {
        self.stop_timer();
        self.ended.set(true);
        self.emit("ended");
    }

    /// A decode failure ends the track. The renderer must not keep the lease while producing
    /// nothing: Rust would believe a local file was still playing.
    fn failed(self: &Rc<Self>, reason: &str) {
        self.status(&format!(
            "GStreamer could not play the decoded local file: {reason}."
        ));
        self.stop_timer();
        self.ended.set(true);
        self.emit("ended");
    }

    fn emit(self: &Rc<Self>, state: &'static str) {
        let Some((generation, video_id)) = self.identity.borrow().clone() else {
            return;
        };
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        let event = LocalEvent {
            generation,
            video_id,
            sequence,
            state,
            current_time: self.position().max(0.0),
            duration: self.duration().max(0.0),
        };
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(host) = weak.upgrade() {
                (host.handlers.on_event)(event);
            }
        });
    }

    fn status(&self, message: &str) {
        (self.handlers.on_status)(message.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn silent_host() -> Rc<LocalHost> {
        LocalHost::new(LocalHandlers {
            on_event: Box::new(|_| {}),
            on_status: Box::new(|_| {}),
        })
    }

    /// A one-second silent WAV: PCM needs no codec, so the test needs nothing beyond base plugins.
    fn silent_wav(path: &Path) {
        let rate: u32 = 8_000;
        let data = vec![0_u8; rate as usize * 2];
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16_u32.to_le_bytes()).unwrap();
        file.write_all(&1_u16.to_le_bytes()).unwrap(); // PCM
        file.write_all(&1_u16.to_le_bytes()).unwrap(); // mono
        file.write_all(&rate.to_le_bytes()).unwrap();
        file.write_all(&(rate * 2).to_le_bytes()).unwrap(); // bytes per second
        file.write_all(&2_u16.to_le_bytes()).unwrap(); // block align
        file.write_all(&16_u16.to_le_bytes()).unwrap(); // bits per sample
        file.write_all(b"data").unwrap();
        file.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
        file.write_all(&data).unwrap();
    }

    /// Prepare leaves the pipeline paused, which decodes without playing anything, so this runs
    /// silently and without a display.
    #[test]
    fn a_decoded_file_opens_paused_and_reports_its_length() {
        let directory =
            std::env::temp_dir().join(format!("goosic-local-host-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join("one second.wav");
        silent_wav(&file);

        let host = silent_host();
        host.prepare(&file, "dQw4w9WgXcQ", 3)
            .expect("the file opens");
        let pipeline = host.pipeline().expect("a pipeline");
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));
        assert!(
            (host.duration() - 1.0).abs() < 0.05,
            "duration {}",
            host.duration()
        );
        assert_eq!(host.loaded_video_id().as_deref(), Some("dQw4w9WgXcQ"));
        assert!(!host.is_playing(), "prepared, not playing");

        host.stop();
        assert!(!host.is_loaded());
        assert!(host.loaded_video_id().is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_path_that_is_not_on_disk_is_refused_before_anything_opens() {
        let host = silent_host();
        let refused = host.prepare(Path::new("/nonexistent/goosic.wav"), "dQw4w9WgXcQ", 1);
        assert!(refused.is_err());
        assert!(!host.is_loaded());
    }
}
