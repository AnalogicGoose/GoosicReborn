use std::io::{self, BufRead, Write};

use goosic_catalog::Catalog;
use goosic_core::PlaybackAuthority;
use goosic_protocol::{ErrorObject, RequestEnvelope, ResponseEnvelope};

const MAX_FRAME_BYTES: usize = 64 * 1024;

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

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut authority = PlaybackAuthority::new();
    // One catalog client for the process lifetime so its anonymous visitor identity is reused.
    let catalog = Catalog::new();
    // Preferences are opened once; a failure is reported per request rather than at startup,
    // because a missing settings file must not stop playback from working.
    let mut settings = goosic_service::settings::Settings::new();
    let mut downloads = goosic_service::downloads::Downloads::new();
    // One lyrics client for the process lifetime, so its one-document cache survives scrubbing.
    let lyrics = goosic_lyrics::LyricsClient::new();
    let mut accounts = goosic_service::accounts::Accounts::new();
    if let Err(error) = accounts.synchronize_authority(&mut authority) {
        eprintln!("goosic-service: could not restore active account: {error}");
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
                    Ok(request) => goosic_service::handle_request(
                        &mut authority,
                        &catalog,
                        &mut settings,
                        &mut downloads,
                        &mut accounts,
                        &lyrics,
                        request,
                    ),
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

        // stdout is exclusively the NDJSON protocol; diagnostics must remain off this stream.
        if let Err(error) = serde_json::to_writer(&mut stdout, &response) {
            eprintln!("goosic-service: could not encode response: {error}");
            break;
        }
        if let Err(error) = stdout.write_all(b"\n").and_then(|_| stdout.flush()) {
            eprintln!("goosic-service: could not write response: {error}");
            break;
        }
    }
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
}
