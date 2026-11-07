//! `convert`: clean subtitles and write them as SRT, WebVTT or plain text.

use crate::io::{is_std, name, read_input, write_output};
use crate::{Failure, timestamp, warn};
use clap::ValueEnum;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use subtitle_parse::{Cue, parse, to_srt};

/// Clean a subtitle file and write it as SRT, WebVTT or plain text.
///
/// Reads SRT or WebVTT, removes markup and entities, drops cues with unusable
/// times and keeps each line of rolling automatic captions once.
#[derive(clap::Args)]
pub struct Args {
    /// Subtitle file to read, SRT or WebVTT; `-` reads standard input.
    #[arg(default_value = "-")]
    input: PathBuf,
    /// File to write; `-` writes standard output.
    #[arg(short, long, default_value = "-")]
    output: PathBuf,
    /// Output format [default: from the output file's extension, else srt]
    #[arg(short = 't', long = "to", value_enum)]
    format: Option<Format>,
    /// Do not warn about dropped cues.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum Format {
    /// SubRip, numbered from 1
    Srt,
    /// WebVTT
    Vtt,
    /// The text of each cue on its own line, without times
    Txt,
}

pub fn run(args: Args) -> Result<(), Failure> {
    let format = match args.format {
        Some(format) => format,
        None => format_of(&args.output)?,
    };
    let parsed = parse(&read_input(&args.input)?);
    if !args.quiet {
        for skipped in &parsed.skipped {
            warn(format_args!(
                "dropped a cue with an unusable time range: {}",
                skipped.header.trim()
            ));
        }
    }
    if parsed.cues.is_empty() {
        return Err(Failure::new(format!(
            "no cues found in {}",
            name(&args.input)
        )));
    }
    let text = match format {
        Format::Srt => to_srt(&parsed.cues),
        Format::Vtt => to_vtt(&parsed.cues),
        Format::Txt => to_txt(&parsed.cues),
    };
    write_output(&args.output, &text)
}

/// The format an output path asks for by its extension.
fn format_of(output: &Path) -> Result<Format, Failure> {
    if is_std(output) {
        return Ok(Format::Srt);
    }
    let extension = output
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("srt") => Ok(Format::Srt),
        Some("vtt") => Ok(Format::Vtt),
        Some("txt") => Ok(Format::Txt),
        _ => Err(Failure::new(format!(
            "cannot tell the format of {} from its extension; pass --to srt, vtt or txt",
            output.display()
        ))),
    }
}

/// WebVTT, with `&`, `<` and `>` escaped so that the text stays text.
fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        let text = cue
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let _ = write!(
            out,
            "{} --> {}\n{text}\n\n",
            timestamp(cue.start, '.'),
            timestamp(cue.end, '.')
        );
    }
    out
}

fn to_txt(cues: &[Cue]) -> String {
    cues.iter().map(|cue| format!("{}\n", cue.text)).collect()
}
