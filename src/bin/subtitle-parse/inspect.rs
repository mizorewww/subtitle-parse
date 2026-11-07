//! `inspect`: what a subtitle file holds once parsed.

use crate::io::{read_input, write_output};
use crate::{Failure, timestamp};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use subtitle_parse::parse;

/// Show how many cues a subtitle file holds and the time they span.
///
/// Counts are after cleaning: rolling automatic captions count each line once.
/// Cues dropped for unusable times are listed with their header line.
#[derive(clap::Args)]
pub struct Args {
    /// Subtitle file to read, SRT or WebVTT; `-` reads standard input.
    #[arg(default_value = "-")]
    input: PathBuf,
}

pub fn run(args: Args) -> Result<(), Failure> {
    let parsed = parse(&read_input(&args.input)?);
    let mut out = String::new();
    let _ = writeln!(out, "cues:     {}", parsed.cues.len());
    let first = parsed.cues.iter().map(|cue| cue.start).reduce(f64::min);
    let last = parsed.cues.iter().map(|cue| cue.end).reduce(f64::max);
    if let (Some(first), Some(last)) = (first, last) {
        let _ = writeln!(out, "start:    {}", timestamp(first, '.'));
        let _ = writeln!(out, "end:      {}", timestamp(last, '.'));
        let _ = writeln!(out, "duration: {}", timestamp(last - first, '.'));
    }
    let _ = writeln!(out, "dropped:  {}", parsed.skipped.len());
    for skipped in &parsed.skipped {
        let _ = writeln!(out, "  {}", skipped.header.trim());
    }
    write_output(Path::new("-"), &out)
}
