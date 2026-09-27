use std::io::{self, BufRead, Write};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

use goosic_catalog::Catalog;
use goosic_core::PlaybackAuthority;
use goosic_lyrics::LyricsClient;
use goosic_protocol::{ErrorObject, RequestEnvelope, ResponseEnvelope};

const MAX_FRAME_BYTES: usize = 64 * 1024;

/// How many upstream reads may be in flight at once.
///
/// The number is a compromise between the two ways this can be wrong. Too few and a Home screen
/// that fans out into several shelves at once serialises anyway; too many and a burst of scrolling
/// opens more sockets to one third-party service than it deserves, which is a good way to be rate
/// limited. Requests past the limit wait for a worker, and — this is the point of the whole
/// arrangement — waiting there does not delay a play, pause, or seek, because those are never
/// dispatched here.
const UPSTREAM_WORKERS: usize = 4;

enum Frame {
    End,
    Line(Vec<u8>),
    TooLarge,
}

/// Reads one NDJSON frame while bounding memory. Oversized frames are consumed through their
/// newline so a subsequent request can still be processed safely.
fn read_frame<R: BufRead>(reader: &mut R, frame: &mut Vec<u8>) -> io::Result<Frame> {
    frame.clear();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if frame.is_empty() && !oversized {
                return Ok(Frame::End);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !oversized {
            if frame.len().saturating_add(consumed) > MAX_FRAME_BYTES {
                oversized = true;
                frame.clear();
            } else {
                frame.extend_from_slice(&available[..consumed]);
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if oversized {
        Ok(Frame::TooLarge)
    } else {
        Ok(Frame::Line(frame.clone()))
    }
}

/// Whether a command is a read-only upstream lookup that can be answered off the main loop.
///
/// The test for membership is not "is it slow" but "what can it touch". These two families take
/// their client by shared reference, hold their own interior state behind a mutex, and are
/// dispatched before the playback authority precisely because they cannot alter ownership,
/// generation, or sequence. Everything else — settings, downloads, accounts, and the authority
/// itself — needs `&mut` to state that only the main loop owns, and stays there.
fn is_upstream_read(command: &str) -> bool {
    command.starts_with("catalog.") || command.starts_with("lyrics.")
}

/// Owns stdout. Every response goes through here, from the main loop and from the workers alike,
/// so that concurrency can never interleave two responses inside one line: the protocol's promise
/// is one response per line, and it says nothing about the order those lines arrive in. Ordering
/// was never something a client could rely on anyway — it matches responses by request id — and
/// now that a slow catalog read no longer delays the play command queued behind it, out-of-order
/// completion is the whole point rather than an accident.
fn write_responses(responses: Receiver<ResponseEnvelope>) {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for response in responses {
        // stdout is exclusively the NDJSON protocol; diagnostics must remain off this stream.
        if let Err(error) = serde_json::to_writer(&mut stdout, &response) {
            eprintln!("goosic-service: could not encode response: {error}");
            return;
        }
        if let Err(error) = stdout.write_all(b"\n").and_then(|_| stdout.flush()) {
            eprintln!("goosic-service: could not write response: {error}");
            return;
        }
    }
}

/// A read dispatched to the pool, carrying everything the worker needs to answer it alone.
struct UpstreamRead {
    command: String,
    request_id: String,
    payload: goosic_protocol::RequestPayload,
}

fn run_upstream_read(
    catalog: &Catalog,
    lyrics: &LyricsClient,
    read: UpstreamRead,
) -> ResponseEnvelope {
    let UpstreamRead {
        command,
        request_id,
        payload,
    } = read;
    goosic_service::catalog::handle(catalog, &command, &request_id, &payload)
        .or_else(|| goosic_service::lyrics::handle(lyrics, &command, &request_id, &payload))
        .unwrap_or_else(|| {
            // Unreachable unless `is_upstream_read` and the handlers disagree about which
            // commands exist, which is a bug worth an answer rather than a dropped request.
            ResponseEnvelope::failure(
                request_id,
                ErrorObject {
                    code: "unsupportedCommand".into(),
                    message: format!("unsupported command {command}"),
                },
            )
        })
}

fn main() {
    let stdin = io::stdin();
    let mut authority = PlaybackAuthority::new();
    // One catalog client for the process lifetime so its anonymous visitor identity is reused.
    let catalog = Arc::new(Catalog::new());
    // Preferences are opened once; a failure is reported per request rather than at startup,
    // because a missing settings file must not stop playback from working.
    let mut settings = goosic_service::settings::Settings::new();
    let mut downloads = goosic_service::downloads::Downloads::new();
    // One lyrics client for the process lifetime, so its one-document cache survives scrubbing.
    let lyrics = Arc::new(goosic_lyrics::LyricsClient::new());
    let mut accounts = goosic_service::accounts::Accounts::new();
    if let Err(error) = accounts.synchronize_authority(&mut authority) {
        eprintln!("goosic-service: could not restore active account: {error}");
    }

    let (responses, response_sink) = mpsc::channel::<ResponseEnvelope>();
    let writer = thread::spawn(move || write_responses(response_sink));

    // One queue, several workers: whichever is idle takes the next read.
    let (reads, read_source) = mpsc::channel::<UpstreamRead>();
    let read_source = Arc::new(std::sync::Mutex::new(read_source));
    let mut workers = Vec::with_capacity(UPSTREAM_WORKERS);
    for _ in 0..UPSTREAM_WORKERS {
        let catalog = Arc::clone(&catalog);
        let lyrics = Arc::clone(&lyrics);
        let read_source = Arc::clone(&read_source);
        let responses = responses.clone();
        workers.push(thread::spawn(move || loop {
            // The lock is held only long enough to take one read, never across the upstream
            // request itself, so the workers do not serialise against each other.
            let read = match read_source.lock() {
                Ok(source) => source.recv(),
                Err(_) => return,
            };
            let Ok(read) = read else { return };
            if responses
                .send(run_upstream_read(&catalog, &lyrics, read))
                .is_err()
            {
                return;
            }
        }));
    }

    let mut input = stdin.lock();
    let mut frame = Vec::with_capacity(MAX_FRAME_BYTES.min(8 * 1024));
    loop {
        let response = match read_frame(&mut input, &mut frame) {
            Ok(Frame::End) => break,
            Ok(Frame::TooLarge) => ResponseEnvelope::failure(
                "",
                ErrorObject {
                    code: "frameTooLarge".into(),
                    message: format!("request frame exceeds {MAX_FRAME_BYTES} bytes"),
                },
            ),
            Ok(Frame::Line(line)) if line.iter().all(u8::is_ascii_whitespace) => {
                ResponseEnvelope::failure(
                    "",
                    ErrorObject {
                        code: "invalidRequest".into(),
                        message: "request line is empty".into(),
                    },
                )
            }
            Ok(Frame::Line(mut line)) => {
                if line.last() == Some(&b'\n') {
                    line.pop();
                }
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                match serde_json::from_slice::<RequestEnvelope>(&line) {
                    Ok(request) => {
                        // Version is checked here as well as in `handle_request`, because a read
                        // sent to a worker never reaches that check.
                        if request.protocol_version != goosic_protocol::PROTOCOL_VERSION {
                            ResponseEnvelope::failure(
                                request.request_id,
                                ErrorObject {
                                    code: "unsupportedProtocolVersion".into(),
                                    message: format!(
                                        "expected protocol version {}",
                                        goosic_protocol::PROTOCOL_VERSION
                                    ),
                                },
                            )
                        } else if is_upstream_read(&request.command) {
                            if reads
                                .send(UpstreamRead {
                                    command: request.command,
                                    request_id: request.request_id,
                                    payload: request.payload,
                                })
                                .is_err()
                            {
                                break;
                            }
                            // The worker answers this one; take the next frame now rather than
                            // waiting for it, which is the entire reason this exists.
                            continue;
                        } else {
                            goosic_service::handle_request(
                                &mut authority,
                                &catalog,
                                &mut settings,
                                &mut downloads,
                                &mut accounts,
                                &lyrics,
                                request,
                            )
                        }
                    }
                    Err(error) => ResponseEnvelope::failure(
                        "",
                        ErrorObject {
                            code: "invalidRequest".into(),
                            message: format!("invalid JSON request: {error}"),
                        },
                    ),
                }
            }
            Err(error) => {
                eprintln!("goosic-service: could not read request: {error}");
                break;
            }
        };

        if responses.send(response).is_err() {
            break;
        }
    }

    // Close the queue so the workers finish what they hold and stop, then close the last response
    // sender so the writer drains and stops. Dropping these in the other order would leave the
    // writer waiting on senders the workers still hold.
    drop(reads);
    drop(responses);
    for worker in workers {
        let _ = worker.join();
    }
    let _ = writer.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn oversized_frame_is_discarded_and_next_frame_is_read() {
        let oversized = format!("{}\n", "x".repeat(MAX_FRAME_BYTES + 1));
        let input = format!("{oversized}{{}}\n");
        let mut reader = BufReader::new(input.as_bytes());
        let mut frame = Vec::new();
        assert!(matches!(
            read_frame(&mut reader, &mut frame),
            Ok(Frame::TooLarge)
        ));
        assert!(matches!(
            read_frame(&mut reader, &mut frame),
            Ok(Frame::Line(_))
        ));
    }

    #[test]
    fn only_read_only_upstream_lookups_leave_the_main_loop() {
        assert!(is_upstream_read("catalog.search"));
        assert!(is_upstream_read("catalog.browse"));
        assert!(is_upstream_read("lyrics.get"));
        // Everything that touches authority-owned state stays serial.
        for command in [
            "playback.claim",
            "playback.release",
            "playback.sample",
            "state.get",
            "hello",
            "account.change",
            "settings.get",
            "downloads.prepare",
        ] {
            assert!(!is_upstream_read(command), "{command} must stay serial");
        }
    }

    /// The pool shares both clients across threads; if either stops being `Sync` the sharing is
    /// no longer sound, and this is the line that should fail rather than a data race.
    #[test]
    fn shared_upstream_clients_are_thread_safe() {
        fn assert_shareable<T: Send + Sync>() {}
        assert_shareable::<Catalog>();
        assert_shareable::<LyricsClient>();
    }
}
