//! `subtitle-parse`: clean, inspect and find subtitle files from the command line.

use clap::Parser;

/// Clean, inspect and find SRT and WebVTT subtitles.
///
/// Parsing is tolerant: markup and entities are removed, cues with unusable
/// times are dropped, and rolling automatic captions (such as YouTube's) keep
/// each line once.
#[derive(Parser)]
#[command(version, propagate_version = true, max_term_width = 100)]
#[command(after_help = "Exit status: 0 on success, 1 on failure, 2 on a usage error.")]
struct Cli {}

fn main() {
    Cli::parse();
}
