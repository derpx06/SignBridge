//! Agent <-> model-worker protocol over the worker's stdin/stdout.
//!
//! Frame: `u32 LE length | u8 kind | payload`, where length counts kind + payload.
//! Kind 0 is a JSON control message, kind 1 is mono f32 LE audio at the rate given in `Start`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{self, ErrorKind, Read, Write};

/// Large enough for several seconds of audio, small enough that a corrupt length can't OOM us.
pub const MAX_FRAME: usize = 4 << 20;
const CONTROL: u8 = 0;
const AUDIO: u8 = 1;

fn yes() -> bool {
    true
}

/// Agent -> worker.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToWorker {
    /// Begin a dictation; `cleanup` loads the cleanup LLM while the user speaks. Audio frames
    /// that follow are mono at `sample_rate` (the worker resamples to 16 kHz).
    Start {
        cleanup: bool,
        sample_rate: u32,
        #[serde(default)]
        session: u64,
        /// Suppress steady background noise before recognition (older agents omit it: on).
        #[serde(default = "yes")]
        denoise: bool,
        /// Cleanup style as `styling,structure,context` (empty: the model's default register).
        #[serde(default)]
        style: String,
        /// Decode partial text while speaking (only the live words in the pill use it; off saves most ASR CPU).
        #[serde(default = "yes")]
        partials: bool,
    },
    /// Finish the dictation: flush, decode, clean up, then `Done`.
    Stop,
    /// Drop the dictation without output.
    Cancel,
    /// Free optional models (memory pressure).
    Trim,
    /// Run `text` (a cloud transcript) through the same directive + cleanup path as recognised speech,
    /// then answer with `Done { text, session }`. `style` is encoded like `Start`'s.
    Clean {
        text: String,
        session: u64,
        cleanup: bool,
        #[serde(default)]
        style: String,
    },
    /// Exit now (the agent also treats closing stdin as shutdown).
    Shutdown,
}

/// Worker -> agent.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromWorker {
    Ready { model: String, rss_mb: f64 },
    Segment { id: String, text: String, start_ms: u64, end_ms: u64, is_final: bool },
    Done { text: String, #[serde(default)] session: u64 },
    Stats { rss_mb: f64, peak_mb: f64 },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Frame<T> {
    Control(T),
    Audio(Vec<f32>),
}

pub fn write_control<W: Write, T: Serialize>(w: &mut W, message: &T) -> io::Result<()> {
    let json = serde_json::to_vec(message).map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;
    write_frame(w, CONTROL, &json)
}

pub fn write_audio<W: Write>(w: &mut W, samples: &[f32]) -> io::Result<()> {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    write_frame(w, AUDIO, &bytes)
}

fn write_frame<W: Write>(w: &mut W, kind: u8, payload: &[u8]) -> io::Result<()> {
    let len = payload.len() + 1;
    if len > MAX_FRAME {
        return Err(io::Error::new(ErrorKind::InvalidInput, "frame too large"));
    }
    w.write_all(&(len as u32).to_le_bytes())?;
    w.write_all(&[kind])?;
    w.write_all(payload)?;
    w.flush()
}

/// Reads one frame. `Ok(None)` means the peer closed the stream (also mid-frame: it died).
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<Option<Frame<T>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(io::Error::new(ErrorKind::InvalidData, format!("bad frame length {len}")));
    }
    let mut buf = vec![0u8; len];
    match r.read_exact(&mut buf) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let payload = &buf[1..];
    match buf[0] {
        CONTROL => serde_json::from_slice(payload)
            .map(|m| Some(Frame::Control(m)))
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e)),
        AUDIO if payload.len().is_multiple_of(4) => Ok(Some(Frame::Audio(
            payload.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect(),
        ))),
        kind => Err(io::Error::new(ErrorKind::InvalidData, format!("bad frame kind {kind}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn control_and_audio_round_trip_in_order() {
        let mut wire = Vec::new();
        write_control(&mut wire, &ToWorker::Start { cleanup: true, sample_rate: 48_000, session: 7, denoise: true, style: String::new(), partials: true }).unwrap();
        write_audio(&mut wire, &[0.5, -1.0, 0.25]).unwrap();
        write_control(&mut wire, &ToWorker::Stop).unwrap();
        let mut r = Cursor::new(wire);
        assert_eq!(read_frame(&mut r).unwrap(), Some(Frame::Control(ToWorker::Start { cleanup: true, sample_rate: 48_000, session: 7, denoise: true, style: String::new(), partials: true })));
        assert_eq!(read_frame::<_, ToWorker>(&mut r).unwrap(), Some(Frame::Audio(vec![0.5, -1.0, 0.25])));
        assert_eq!(read_frame(&mut r).unwrap(), Some(Frame::Control(ToWorker::Stop)));
        assert_eq!(read_frame::<_, ToWorker>(&mut r).unwrap(), None);
    }

    #[test]
    fn truncated_stream_reads_as_closed() {
        let mut wire = Vec::new();
        write_control(&mut wire, &FromWorker::Done { text: "hello".into(), session: 7 }).unwrap();
        wire.truncate(wire.len() - 2);
        assert_eq!(read_frame::<_, FromWorker>(&mut Cursor::new(wire)).unwrap(), None);
    }

    #[test]
    fn corrupt_frames_are_rejected() {
        let huge = ((MAX_FRAME + 1) as u32).to_le_bytes().to_vec();
        assert!(read_frame::<_, ToWorker>(&mut Cursor::new(huge)).is_err());
        let bad_kind = [2u8, 0, 0, 0, 9, 0].to_vec();
        assert!(read_frame::<_, ToWorker>(&mut Cursor::new(bad_kind)).is_err());
        let ragged_audio = [3u8, 0, 0, 0, 1, 0, 0].to_vec();
        assert!(read_frame::<_, ToWorker>(&mut Cursor::new(ragged_audio)).is_err());
    }
}

/// glibc keeps freed memory around, so a long-lived process looks bigger than its live data
/// (about 15 MB leaked per two-minute dictation was seen in a comparable app). Both processes pin
/// large allocations to mmap, which is returned to the OS on free, and trim after each dictation.
pub mod heap {
    /// Call once at start-up, before other threads allocate.
    pub fn pin_mmap_threshold() {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: mallopt only sets an allocator tunable.
        unsafe {
            libc::mallopt(libc::M_MMAP_THRESHOLD, 128 * 1024);
        }
    }

    /// Returns freed heap pages to the OS; call when a dictation ends.
    pub fn trim() {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: malloc_trim is always safe to call.
        unsafe {
            libc::malloc_trim(0);
        }
    }
}
