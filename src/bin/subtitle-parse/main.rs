//! `subtitle-parse`: clean, inspect and find subtitle files from the command line.

mod convert;
mod inspect;
mod io;
mod tracks;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use std::error::Error;
use std::fmt;
use std::io::{IsTerminal, Write};
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

impl Command {
    /// The name and input of a command that reads subtitles.
    fn input(&self) -> Option<(&'static str, &Option<std::path::PathBuf>)> {
        match self {
            Self::Convert(args) => Some(("convert", &args.input)),
            Self::Inspect(args) => Some(("inspect", &args.input)),
            Self::Tracks(_) => None,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let waiting = cli
        .command
        .input()
        .filter(|(_, input)| io::waits_on_terminal(input, std::io::stdin().is_terminal()));
    if let Some((name, _)) = waiting {
        // Rather than wait for subtitles typed at the terminal, say what is missing.
        let mut command = Cli::command();
        command.build();
        let subcommand = command.find_subcommand_mut(name).expect("a known command");
        subcommand
            .error(
                ErrorKind::MissingRequiredArgument,
                "no input file, and standard input is a terminal\n\n\
                 tip: name a subtitle file, pipe one in, or pass `-` to type it",
            )
            .exit();
    }
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

/// A time in seconds as `HH:MM:SS.mmm`, as WebVTT writes it.
pub fn timestamp(seconds: f64) -> String {
    let millis = (seconds.max(0.) * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}
