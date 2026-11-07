//! Reading subtitles from a file or standard input, and writing results.

use crate::Failure;
use std::io::{Read, Write};
use std::path::Path;
use subtitle_parse::{MAX_FILE_BYTES, ReadError, decode_text, read_text};

/// `-` stands for standard input or standard output.
pub fn is_std(path: &Path) -> bool {
    path.as_os_str() == "-"
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
pub fn write_output(path: &Path, text: &str) -> Result<(), Failure> {
    if is_std(path) {
        let mut stdout = std::io::stdout().lock();
        return stdout
            .write_all(text.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|error| Failure::with_source("cannot write standard output", error));
    }
    std::fs::write(path, text)
        .map_err(|error| Failure::with_source(format!("cannot write {}", path.display()), error))
}
