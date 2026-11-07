//! `subtitle-parse`: clean, inspect and find subtitle files from the command line.

mod convert;
mod inspect;
mod io;
mod tracks;

use clap::{Parser, Subcommand};
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::process::ExitCode;

/// Clean, inspect and find SRT and WebVTT subtitles.
///
/// Parsing is tolerant: markup and entities are removed, cues with unusable
/// times are dropped, and rolling automatic captions (such as YouTube's) keep
/// each line once.
#[derive(Parser)]
#[command(version, propagate_version = true, max_term_width = 100)]
#[command(after_help = "Exit status: 0 on success, 1 on failure, 2 on a usage error.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Convert(convert::Args),
    Inspect(inspect::Args),
    Tracks(tracks::Args),
}

/// Why a command failed, for standard error.
#[derive(Debug)]
pub struct Failure {
    message: String,
    source: Option<Box<dyn Error>>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(message: impl Into<String>, source: impl Error + 'static) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Standard output closed early, as by `| head`: nothing is wrong.
    fn is_broken_pipe(&self) -> bool {
        self.source
            .as_ref()
            .and_then(|source| source.downcast_ref::<std::io::Error>())
            .is_some_and(|error| error.kind() == std::io::ErrorKind::BrokenPipe)
    }
}

impl fmt::Display for Failure {
    /// The message, then each cause after a colon.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        let mut cause = self.source.as_deref();
        while let Some(error) = cause {
            write!(f, ": {error}")?;
            cause = error.source();
        }
        Ok(())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Convert(args) => convert::run(args),
        Command::Inspect(args) => inspect::run(args),
        Command::Tracks(args) => tracks::run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) if failure.is_broken_pipe() => ExitCode::SUCCESS,
        Err(failure) => {
            let _ = writeln!(std::io::stderr(), "subtitle-parse: error: {failure}");
            ExitCode::FAILURE
        }
    }
}

/// A warning on standard error.
pub fn warn(message: impl fmt::Display) {
    // Standard error may be closed; a warning is not worth a panic.
    let _ = writeln!(std::io::stderr(), "subtitle-parse: warning: {message}");
}

/// `HH:MM:SS` followed by `separator` and milliseconds, as SRT (`,`) and WebVTT
/// (`.`) write times.
pub fn timestamp(seconds: f64, separator: char) -> String {
    let millis = (seconds.max(0.) * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}
