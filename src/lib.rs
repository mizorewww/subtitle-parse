//! Tolerant SRT and WebVTT parsing, sidecar discovery and subtitle track ordering.
//!
//! ```
//! let parsed = subtitle_parse::parse("1\n00:00:01,000 --> 00:00:02,500\n<i>Hello</i> &amp; welcome\n");
//! assert_eq!(parsed.cues[0].text, "Hello & welcome");
//! assert_eq!(parsed.cues[0].end, 2.5);
//! ```
//!
//! The crate has no user-facing text of its own: [`ReadError`] and [`DecodeError`]
//! are typed so that an application can word them in its own language.

mod cue;
mod file;
mod track;

pub use cue::{Cue, Parsed, SkippedCue, parse, parse_timestamp, to_srt};
pub use file::{
    DecodeError, MAX_FILE_BYTES, ReadError, best_file, decode_text, files_in, is_subtitle_file,
    read_text, sidecars,
};
pub use track::{Track, TrackKind, TrackOrigin, normalize_language, sort_tracks};
