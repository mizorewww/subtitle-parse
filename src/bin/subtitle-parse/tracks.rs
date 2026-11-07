//! `tracks`: the subtitle files for a video, best first.

use crate::Failure;
use crate::io::write_output;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use subtitle_parse::{TrackOrigin, files_in, sidecars, sort_tracks};

/// List the subtitle files for a video, best first.
///
/// For a video, these are the files beside it named after it: `lecture.mp4` has
/// `lecture.srt`, `lecture.en.vtt` and `lecture.zh-Hans.srt`. For a directory,
/// every SRT and WebVTT file in it, with the language after the first dot.
/// Tracks are ordered by --prefer, then --interface, then --original, then by
/// name; a language that matches none of them still appears. Each line is the
/// language (`-` when the name has none), a tab and the path.
#[derive(clap::Args)]
pub struct Args {
    /// A video file, or a directory of subtitle files.
    path: PathBuf,
    /// A preferred language, most preferred first; repeat or separate with commas.
    #[arg(short, long, value_name = "LANG", value_delimiter = ',')]
    prefer: Vec<String>,
    /// The language of the user interface, tried after every --prefer.
    #[arg(long, value_name = "LANG")]
    interface: Option<String>,
    /// The video's original language, tried after --interface.
    #[arg(long, value_name = "LANG")]
    original: Option<String>,
    /// Print only the path of the best track.
    #[arg(long)]
    best: bool,
}

pub fn run(args: Args) -> Result<(), Failure> {
    let mut tracks = if args.path.is_dir() {
        files_in(&args.path)
    } else if args.path.parent() == Some(Path::new("")) {
        // `sidecars` reads the video's parent, which a bare file name lacks.
        sidecars(&Path::new(".").join(&args.path))
    } else {
        sidecars(&args.path)
    };
    sort_tracks(
        &mut tracks,
        &args.prefer,
        args.interface.as_deref().unwrap_or_default(),
        args.original.as_deref(),
    );
    let mut out = String::new();
    for track in &tracks {
        let TrackOrigin::File { path } = &track.origin else {
            continue;
        };
        if args.best {
            let _ = writeln!(out, "{}", path.display());
            break;
        }
        let language = track.language.as_deref().unwrap_or("-");
        let _ = writeln!(out, "{language}\t{}", path.display());
    }
    if out.is_empty() {
        return Err(Failure::new(format!(
            "no subtitle files found for {}",
            args.path.display()
        )));
    }
    write_output(Path::new("-"), &out)
}
