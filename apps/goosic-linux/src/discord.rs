//! Discord Rich Presence over the desktop app's local Unix IPC socket.
//!
//! Nothing leaves this machine from Goosic: the local Discord desktop app, already signed in,
//! publishes the activity when enabled in Settings. All socket I/O runs on a dedicated background
//! thread with >=4s coalescing so the GTK main loop never blocks.

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// The Goosic application on Discord's developer portal (same public client ID as macOS & Windows).
pub const APPLICATION_ID: &str = "1526113133570293800";

/// Discord ignores a client that updates presence faster than ~5 times per 20 seconds.
const MIN_INTERVAL: Duration = Duration::from_secs(4);
const IO_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordActivity {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork_url: Option<String>,
    pub started_unix_ms: i64,
    pub ends_unix_ms: Option<i64>,
}

impl DiscordActivity {
    pub fn from_playback(
        title: &str,
        artist: &str,
        album: &str,
        artwork_url: Option<&str>,
        position_seconds: f64,
        duration_seconds: f64,
    ) -> Option<Self> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;
        let pos_ms = if position_seconds.is_finite() && position_seconds >= 0.0 {
            (position_seconds * 1000.0).round() as i64
        } else {
            0
        };
        let started_unix_ms = now_ms.saturating_sub(pos_ms);
        let ends_unix_ms = if duration_seconds.is_finite() && duration_seconds > 0.0 {
            Some(started_unix_ms.saturating_add((duration_seconds * 1000.0).round() as i64))
        } else {
            None
        };
        Some(Self {
            title: title.to_owned(),
            artist: artist.trim().to_owned(),
            album: album.trim().to_owned(),
            artwork_url: artwork_url
                .filter(|url| url.starts_with("https://"))
                .map(str::to_owned),
            started_unix_ms,
            ends_unix_ms,
        })
    }

    fn same_track_and_timing(&self, other: &Self) -> bool {
        self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.artwork_url == other.artwork_url
            && (self.started_unix_ms - other.started_unix_ms).abs() < 3_500
    }
}

enum Command {
    Update(Option<DiscordActivity>),
    Shutdown,
}

pub struct DiscordPresence {
    enabled: Cell<bool>,
    last_sent: RefCell<Option<DiscordActivity>>,
    tx: Option<Sender<Command>>,
}

impl DiscordPresence {
    pub fn start() -> Self {
        let enabled = read_enabled_preference();
        let (tx, rx) = mpsc::channel::<Command>();
        let spawned = std::thread::Builder::new()
            .name("goosic-discord-ipc".to_owned())
            .spawn(move || worker_loop(rx))
            .is_ok();
        Self {
            enabled: Cell::new(enabled),
            last_sent: RefCell::new(None),
            tx: spawned.then_some(tx),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub fn set_enabled(&self, enabled: bool) {
        if self.enabled.replace(enabled) == enabled {
            return;
        }
        write_enabled_preference(enabled);
        if !enabled {
            self.clear();
        }
    }

    pub fn update(&self, activity: Option<DiscordActivity>) {
        let desired = if self.enabled.get() { activity } else { None };
        let unchanged = match (self.last_sent.borrow().as_ref(), desired.as_ref()) {
            (None, None) => true,
            (Some(a), Some(b)) => a.same_track_and_timing(b),
            _ => false,
        };
        if unchanged {
            return;
        }
        *self.last_sent.borrow_mut() = desired.clone();
        if let Some(tx) = &self.tx {
            let _ = tx.send(Command::Update(desired));
        }
    }

    pub fn clear(&self) {
        if self.last_sent.borrow_mut().take().is_some() {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Command::Update(None));
            }
        }
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Command::Update(None));
            let _ = tx.send(Command::Shutdown);
        }
    }
}

fn worker_loop(rx: mpsc::Receiver<Command>) {
    let mut socket: Option<UnixStream> = None;
    let mut pending: Option<Option<DiscordActivity>> = None;
    let mut last_send = Instant::now() - MIN_INTERVAL;
    let mut nonce: u64 = 0;

    loop {
        let wait = if pending.is_some() {
            let elapsed = last_send.elapsed();
            if elapsed >= MIN_INTERVAL {
                Duration::ZERO
            } else {
                MIN_INTERVAL - elapsed
            }
        } else {
            Duration::from_secs(60)
        };

        match rx.recv_timeout(wait) {
            Ok(Command::Update(activity)) => {
                pending = Some(activity);
                if last_send.elapsed() >= MIN_INTERVAL {
                    if let Some(next) = pending.take() {
                        nonce = nonce.wrapping_add(1);
                        send_activity(&mut socket, next.as_ref(), nonce);
                        last_send = Instant::now();
                    }
                }
            }
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(next) = pending.take() {
                    nonce = nonce.wrapping_add(1);
                    send_activity(&mut socket, next.as_ref(), nonce);
                }
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(next) = pending.take() {
                    nonce = nonce.wrapping_add(1);
                    send_activity(&mut socket, next.as_ref(), nonce);
                    last_send = Instant::now();
                }
            }
        }
    }
}

fn send_activity(socket: &mut Option<UnixStream>, activity: Option<&DiscordActivity>, nonce: u64) {
    if activity.is_none() && socket.is_none() {
        return;
    }
    if socket.is_none() {
        *socket = connect_and_handshake();
    }
    let Some(stream) = socket.as_mut() else {
        return;
    };
    let payload = set_activity_payload(activity, std::process::id(), nonce);
    if write_frame(stream, 1, &payload).is_err() || read_frame(stream).is_err() {
        // Discord closed or restarted; next update reconnects cleanly.
        *socket = None;
    }
}

fn connect_and_handshake() -> Option<UnixStream> {
    for path in candidate_socket_paths() {
        let Ok(mut stream) = UnixStream::connect(&path) else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let handshake = json!({
            "v": 1,
            "client_id": APPLICATION_ID,
        });
        if write_frame(&mut stream, 0, &handshake).is_ok() && read_frame(&mut stream).is_ok() {
            return Some(stream);
        }
    }
    None
}

pub fn candidate_socket_paths() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let base = PathBuf::from(runtime);
        dirs.push(base.clone());
        dirs.push(base.join("app/com.discordapp.Discord"));
        dirs.push(base.join("snap.discord"));
    }
    if let Ok(tmp) = std::env::var("TMPDIR") {
        dirs.push(PathBuf::from(tmp));
    }
    dirs.push(PathBuf::from("/tmp"));

    let mut paths = Vec::new();
    for dir in dirs {
        for index in 0..10 {
            paths.push(dir.join(format!("discord-ipc-{index}")));
        }
    }
    paths
}

pub fn clamp_field(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(128).collect();
    if out.chars().count() < 2 {
        out.push(' ');
    }
    Some(out)
}

pub fn set_activity_payload(activity: Option<&DiscordActivity>, pid: u32, nonce: u64) -> Value {
    let activity_json = match activity {
        Some(activity) => {
            let mut obj = serde_json::Map::new();
            // Type 2 = "Listening to"
            obj.insert("type".to_owned(), json!(2));
            if let Some(details) = clamp_field(&activity.title) {
                obj.insert("details".to_owned(), json!(details));
            }
            if let Some(state) = clamp_field(&activity.artist) {
                obj.insert("state".to_owned(), json!(state));
            }
            let mut timestamps = serde_json::Map::new();
            timestamps.insert("start".to_owned(), json!(activity.started_unix_ms));
            if let Some(end) = activity.ends_unix_ms {
                timestamps.insert("end".to_owned(), json!(end));
            }
            obj.insert("timestamps".to_owned(), Value::Object(timestamps));

            let mut assets = serde_json::Map::new();
            if let Some(url) = activity
                .artwork_url
                .as_deref()
                .filter(|u| u.starts_with("https://"))
            {
                assets.insert("large_image".to_owned(), json!(url));
            }
            if let Some(album) = clamp_field(&activity.album) {
                assets.insert("large_text".to_owned(), json!(album));
            }
            if !assets.is_empty() {
                obj.insert("assets".to_owned(), Value::Object(assets));
            }
            Value::Object(obj)
        }
        None => Value::Null,
    };

    json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": pid,
            "activity": activity_json,
        },
        "nonce": format!("goosic-{nonce}"),
    })
}

pub fn encode_frame(opcode: u32, payload: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(payload).unwrap_or_default();
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    frame
}

fn write_frame(stream: &mut UnixStream, opcode: u32, payload: &Value) -> std::io::Result<()> {
    let frame = encode_frame(opcode, payload);
    stream.write_all(&frame)
}

fn read_frame(stream: &mut UnixStream) -> std::io::Result<Vec<u8>> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 256 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Discord IPC frame too large",
        ));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn preference_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("goosic")
        .join("discord_presence")
}

fn read_enabled_preference() -> bool {
    std::fs::read_to_string(preference_path())
        .map(|text| text.trim() == "1")
        .unwrap_or(false)
}

fn write_enabled_preference(enabled: bool) {
    let path = preference_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, if enabled { "1\n" } else { "0\n" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_clamped_to_discords_length_bounds() {
        assert_eq!(clamp_field("   "), None);
        assert_eq!(clamp_field("X").as_deref(), Some("X "));
        let long = "a".repeat(200);
        assert_eq!(clamp_field(&long).unwrap().chars().count(), 128);
    }

    #[test]
    fn payload_includes_listening_type_and_https_artwork_only() {
        let activity = DiscordActivity {
            title: "Slide".into(),
            artist: "Calvin Harris".into(),
            album: "Funk Wav Bounces".into(),
            artwork_url: Some("https://lh3.googleusercontent.com/cover".into()),
            started_unix_ms: 1_700_000_000_000,
            ends_unix_ms: Some(1_700_000_200_000),
        };
        let payload = set_activity_payload(Some(&activity), 42, 7);
        assert_eq!(payload["cmd"], "SET_ACTIVITY");
        assert_eq!(payload["args"]["activity"]["type"], 2);
        assert_eq!(payload["args"]["activity"]["details"], "Slide");
        assert_eq!(payload["args"]["activity"]["state"], "Calvin Harris");
        assert_eq!(
            payload["args"]["activity"]["assets"]["large_image"],
            "https://lh3.googleusercontent.com/cover"
        );

        let cleared = set_activity_payload(None, 42, 8);
        assert!(cleared["args"]["activity"].is_null());
    }

    #[test]
    fn frame_encoding_writes_little_endian_header() {
        let payload = json!({"v": 1});
        let frame = encode_frame(0, &payload);
        assert_eq!(&frame[0..4], &0u32.to_le_bytes());
        let len = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        assert_eq!(len, frame.len() - 8);
    }
}
