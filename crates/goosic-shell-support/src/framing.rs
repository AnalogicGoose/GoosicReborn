//! Finding one NDJSON frame in a stream that arrives in whatever sizes the pipe felt like.
//!
//! This is the part of a transport that looks trivial and is not. A read returns bytes, not
//! messages: one read may carry half a frame, or three frames, or a frame and the first byte of
//! the next. Every shell has to reassemble that identically, because the alternative is a client
//! that works until a response happens to straddle a buffer boundary.

use crate::{TransportError, MAX_FRAME_BYTES};

/// Accumulates bytes from a pipe and hands back whole frames.
#[derive(Debug)]
pub struct FrameReader {
    buffer: Vec<u8>,
    max_frame_bytes: usize,
}

impl Default for FrameReader {
    fn default() -> Self {
        Self::new(MAX_FRAME_BYTES)
    }
}

impl FrameReader {
    pub fn new(max_frame_bytes: usize) -> Self {
        Self { buffer: Vec::new(), max_frame_bytes }
    }

    /// Adds whatever a read produced. Chunk boundaries carry no meaning.
    pub fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// How much is held without a terminator yet, which is only useful for diagnostics.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Takes the next complete frame, or `None` when one has not arrived yet.
    ///
    /// The size limit is checked against a completed frame *and* against a buffer still waiting
    /// for its newline. Only checking the first would let a stream with no terminators grow
    /// without bound, which is the shape a desynchronised pipe actually has.
    ///
    /// The terminator counts towards the limit, so the largest accepted frame is one byte shorter
    /// than `max_frame_bytes`. That is an arbitrary boundary and therefore exactly the kind a
    /// second implementation gets wrong by one; it is written down here and pinned by a test
    /// because the macOS shell already made this choice and the shells have to agree.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, TransportError> {
        match self.buffer.iter().position(|byte| *byte == b'\n') {
            Some(newline) => {
                if newline >= self.max_frame_bytes {
                    return Err(TransportError::ResponseTooLarge);
                }
                let frame: Vec<u8> = self.buffer.drain(..=newline).take(newline).collect();
                Ok(Some(frame))
            }
            None => {
                if self.buffer.len() > self.max_frame_bytes {
                    return Err(TransportError::ResponseTooLarge);
                }
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(reader: &mut FrameReader) -> Option<String> {
        reader
            .next_frame()
            .expect("frame was refused")
            .map(|bytes| String::from_utf8(bytes).expect("frame was not utf-8"))
    }

    #[test]
    fn a_frame_split_across_two_reads_is_reassembled() {
        let mut reader = FrameReader::default();
        reader.push(br#"{"ok":"#);
        assert_eq!(frame(&mut reader), None, "half a frame is not a frame");
        reader.push(b"true}\n");
        assert_eq!(frame(&mut reader).as_deref(), Some(r#"{"ok":true}"#));
        assert_eq!(frame(&mut reader), None);
    }

    #[test]
    fn several_frames_in_one_read_come_back_in_order() {
        let mut reader = FrameReader::default();
        reader.push(b"first\nsecond\nthird\n");
        assert_eq!(frame(&mut reader).as_deref(), Some("first"));
        assert_eq!(frame(&mut reader).as_deref(), Some("second"));
        assert_eq!(frame(&mut reader).as_deref(), Some("third"));
        assert_eq!(frame(&mut reader), None);
    }

    #[test]
    fn a_frame_followed_by_the_start_of_the_next_leaves_the_remainder_buffered() {
        let mut reader = FrameReader::default();
        reader.push(b"done\n{\"partial\"");
        assert_eq!(frame(&mut reader).as_deref(), Some("done"));
        assert_eq!(frame(&mut reader), None);
        assert_eq!(reader.buffered(), 10);
    }

    #[test]
    fn a_byte_at_a_time_still_produces_the_frame() {
        let mut reader = FrameReader::default();
        for byte in b"hello\n" {
            reader.push(&[*byte]);
        }
        assert_eq!(frame(&mut reader).as_deref(), Some("hello"));
    }

    #[test]
    fn an_empty_line_is_an_empty_frame_rather_than_nothing() {
        // The caller decides what to do with it; swallowing it here would hide a service that
        // has started emitting blank lines.
        let mut reader = FrameReader::default();
        reader.push(b"\n");
        assert_eq!(frame(&mut reader).as_deref(), Some(""));
    }

    #[test]
    fn a_completed_frame_over_the_limit_is_refused() {
        let mut reader = FrameReader::new(8);
        reader.push(b"123456789\n");
        assert_eq!(reader.next_frame(), Err(TransportError::ResponseTooLarge));
    }

    #[test]
    fn a_stream_with_no_terminator_is_refused_before_it_grows_without_bound() {
        let mut reader = FrameReader::new(8);
        reader.push(b"123456789");
        assert_eq!(reader.next_frame(), Err(TransportError::ResponseTooLarge));
    }

    #[test]
    fn the_largest_frame_that_fits_with_its_terminator_is_accepted() {
        // Seven bytes and a newline are eight, which is the limit.
        let mut reader = FrameReader::new(8);
        reader.push(b"1234567\n");
        assert_eq!(frame(&mut reader).as_deref(), Some("1234567"));
    }

    #[test]
    fn a_frame_whose_terminator_pushes_it_over_the_limit_is_refused() {
        // Eight bytes of content plus a newline is nine. The delimiter counts, and this is the
        // off-by-one another shell would land on if it measured only the content.
        let mut reader = FrameReader::new(8);
        reader.push(b"12345678\n");
        assert_eq!(reader.next_frame(), Err(TransportError::ResponseTooLarge));
    }
}
