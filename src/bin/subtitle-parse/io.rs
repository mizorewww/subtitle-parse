//! Reading subtitles from a file or standard input, and writing results.

use crate::Failure;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use subtitle_parse::{MAX_FILE_BYTES, ReadError, decode_text, read_text};

/// `-` stands for standard input or standard output.
pub fn is_std(path: &Path) -> bool {
    path.as_os_str() == "-"
}

/// The input to read: the file given, or `-` for standard input.
pub fn input(given: &Option<PathBuf>) -> &Path {
    given.as_deref().unwrap_or(Path::new("-"))
}

/// Whether a command given `input` would wait for someone to type subtitles:
/// no input was named and standard input is a terminal. An explicit `-` reads it.
pub fn waits_on_terminal(given: &Option<PathBuf>, stdin_is_terminal: bool) -> bool {
    given.is_none() && stdin_is_terminal
}

/// How a path appears in messages.
pub fn name(path: &Path) -> String {
    if is_std(path) {
        "standard input".to_owned()
    } else {
        path.display().to_string()
    }
}

/// The text of a subtitle file, or of standard input for `-`, decoded as the
/// library decodes files: UTF-8 or UTF-16 by BOM, never lossily.
pub fn read_input(path: &Path) -> Result<String, Failure> {
    if !is_std(path) {
        return read_text(path).map_err(|error| match error {
            // The message already names the file.
            ReadError::Open { path, source } => {
                Failure::with_source(format!("cannot open {}", path.display()), source)
            }
            error => Failure::with_source(name(path), error),
        });
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Failure::with_source("cannot read standard input", error))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Failure::new(format!(
            "standard input is larger than {MAX_FILE_BYTES} bytes"
        )));
    }
    decode_text(&bytes).map_err(|error| Failure::with_source("standard input", error))
}

/// Writes `text` to the file at `path`, or to standard output for `-`.
///
/// A file is written whole or not at all: the text goes to a temporary file in
/// the same directory, which then replaces `path`. Converting a file in place
/// cannot leave it truncated.
pub fn write_output(path: &Path, text: &str) -> Result<(), Failure> {
    if is_std(path) {
        let mut stdout = std::io::stdout().lock();
        return stdout
            .write_all(text.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|error| Failure::with_source("cannot write standard output", error));
    }
    let failure = |error| Failure::with_source(format!("cannot write {}", path.display()), error);
    let Some(file_name) = path.file_name() else {
        return Err(failure(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a file name",
        )));
    };
    let mut temporary_name = std::ffi::OsString::from(".");
    temporary_name.push(file_name);
    temporary_name.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(temporary_name);
    let written =
        write_new(&temporary, text, path).and_then(|()| std::fs::rename(&temporary, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written.map_err(failure)
}

/// Creates `temporary` with `text`, and the permissions of `replacing` if it exists.
fn write_new(temporary: &Path, text: &str, replacing: &Path) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    if let Ok(metadata) = std::fs::metadata(replacing) {
        file.set_permissions(metadata.permissions())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_unnamed_input_on_a_terminal_waits() {
        assert!(waits_on_terminal(&None, true));
        assert!(!waits_on_terminal(&None, false), "piped input is read");
        let dash = Some(PathBuf::from("-"));
        assert!(!waits_on_terminal(&dash, true), "`-` asks for the terminal");
        let file = Some(PathBuf::from("a.srt"));
        assert!(!waits_on_terminal(&file, true));
        assert_eq!(input(&None), Path::new("-"));
        assert_eq!(input(&file), Path::new("a.srt"));
    }
}
