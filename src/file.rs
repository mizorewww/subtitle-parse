//! Subtitle files on disk: reading them without damaging the words, and finding them.

use crate::track::{Track, TrackOrigin, sort_tracks};
use std::error::Error;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

/// [`read_text`] refuses files larger than this.
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// Why bytes could not become text.
#[derive(Debug)]
pub enum DecodeError {
    /// A UTF-16 byte order mark followed by an odd number of bytes.
    IncompleteUtf16,
    InvalidUtf16(std::string::FromUtf16Error),
    InvalidUtf8(std::string::FromUtf8Error),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompleteUtf16 => f.write_str("incomplete UTF-16 text"),
            Self::InvalidUtf16(_) => f.write_str("invalid UTF-16 text"),
            Self::InvalidUtf8(_) => f.write_str("invalid UTF-8 text"),
        }
    }
}

impl Error for DecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::IncompleteUtf16 => None,
            Self::InvalidUtf16(error) => Some(error),
            Self::InvalidUtf8(error) => Some(error),
        }
    }
}

/// Why [`read_text`] failed.
#[derive(Debug)]
pub enum ReadError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Read(std::io::Error),
    /// Larger than [`MAX_FILE_BYTES`].
    TooLarge,
    Decode(DecodeError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, .. } => write!(f, "cannot open {}", path.display()),
            Self::Read(_) => f.write_str("cannot read the subtitle file"),
            Self::TooLarge => write!(f, "larger than {MAX_FILE_BYTES} bytes"),
            Self::Decode(_) => f.write_str("cannot decode the subtitle file"),
        }
    }
}

impl Error for ReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Read(source) => Some(source),
            Self::TooLarge => None,
            Self::Decode(error) => Some(error),
        }
    }
}

/// Reads and decodes a subtitle file with [`decode_text`], up to [`MAX_FILE_BYTES`].
pub fn read_text(path: &Path) -> Result<String, ReadError> {
    let file = std::fs::File::open(path).map_err(|source| ReadError::Open {
        path: path.to_owned(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(ReadError::Read)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ReadError::TooLarge);
    }
    decode_text(&bytes).map_err(ReadError::Decode)
}

/// Decodes UTF-8 or UTF-16 (byte order from the BOM), removing the BOM. Without
/// one the text is UTF-8. Nothing is replaced: invalid input is an error.
pub fn decode_text(bytes: &[u8]) -> Result<String, DecodeError> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if !(bytes.len() - 2).is_multiple_of(2) {
            return Err(DecodeError::IncompleteUtf16);
        }
        let little = bytes[0] == 0xff;
        let words: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        return String::from_utf16(&words).map_err(DecodeError::InvalidUtf16);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(DecodeError::InvalidUtf8)
}

/// Whether the extension is `srt` or `vtt`, in any case.
pub fn is_subtitle_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "srt" | "vtt"))
}

/// Every file beside `video` with its name, or its name and a language
/// (`lecture.srt`, `lecture.zh-Hans.vtt`), keeping both formats, sorted by path.
/// An unreadable directory has none.
pub fn sidecars(video: &Path) -> Vec<Track> {
    let Some(parent) = video.parent() else {
        return Vec::new();
    };
    let Some(stem) = video.file_stem().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            if !path.is_file() || !is_subtitle_file(path) {
                return false;
            }
            path.file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|candidate| {
                    candidate == stem || candidate.starts_with(&format!("{stem}."))
                })
        })
        .collect();
    paths.sort();
    numbered(paths, |path| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .and_then(|candidate| candidate.strip_prefix(&format!("{stem}.")))
            .filter(|candidate| {
                candidate
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic() || byte == b'-' || byte == b'_')
            })
            .map(str::to_owned)
    })
}

/// Every subtitle file in `dir`, with the language after the first dot of its
/// stem (`name.en.srt`), sorted by path. An unreadable directory has none.
pub fn files_in(dir: &Path) -> Vec<Track> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_subtitle_file(path))
        .collect();
    paths.sort();
    numbered(paths, |path| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .and_then(|stem| stem.split_once('.'))
            .map(|(_, language)| language.to_owned())
    })
}

fn numbered(paths: Vec<PathBuf>, language: impl Fn(&Path) -> Option<String>) -> Vec<Track> {
    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let language = language(&path);
            let mut track = Track::file(path, language);
            track.source_order = index;
            track
        })
        .collect()
}

/// The path of the first file track once [`sort_tracks`] has ordered `tracks`.
pub fn best_file(
    mut tracks: Vec<Track>,
    preferences: &[String],
    interface_language: &str,
    original_language: Option<&str>,
) -> Option<PathBuf> {
    sort_tracks(
        &mut tracks,
        preferences,
        interface_language,
        original_language,
    );
    tracks.into_iter().find_map(|track| match track.origin {
        TrackOrigin::File { path } => Some(path),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cue::{parse, to_srt};

    #[test]
    fn sidecars_include_every_language_and_both_formats_without_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("Lecture.mp4");
        for name in [
            "Lecture.fr.srt",
            "Lecture.JA.VTT",
            "Lecture.srt",
            "Other.srt",
        ] {
            std::fs::write(
                dir.path().join(name),
                "1\n00:00:00,000 --> 00:00:01,000\nBonjour\n",
            )
            .unwrap();
        }
        let tracks = sidecars(&video);
        assert_eq!(tracks.len(), 3);
        for language in ["fr", "JA"] {
            assert!(
                tracks
                    .iter()
                    .any(|track| track.language.as_deref() == Some(language))
            );
        }
    }

    #[test]
    fn utf16_files_preserve_words_and_timing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("arbitrary-name.srt");
        let text = "1\n00:00:01,125 --> 00:00:02,375\nBonjour 世界\n";
        let bytes: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        std::fs::write(&file, bytes).unwrap();
        let cues = parse(&read_text(&file).unwrap()).cues;
        assert_eq!(cues.len(), 1);
        assert_eq!(cues, parse(&to_srt(&cues)).cues);
        std::fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            read_text(&file),
            Err(ReadError::Decode(DecodeError::IncompleteUtf16))
        ));
        std::fs::write(&file, [0xff, 0xff, 0xfe]).unwrap();
        assert!(matches!(
            read_text(&file),
            Err(ReadError::Decode(DecodeError::InvalidUtf8(_)))
        ));
    }

    #[test]
    fn decoding_never_replaces_characters() {
        assert_eq!(decode_text(b"\xef\xbb\xbfabc").unwrap(), "abc");
        assert_eq!(decode_text(b"\xfe\xff\x00a").unwrap(), "a");
        assert!(matches!(
            decode_text(b"a\xff"),
            Err(DecodeError::InvalidUtf8(_))
        ));
        // An unpaired surrogate.
        assert!(matches!(
            decode_text(b"\xff\xfe\x00\xd8"),
            Err(DecodeError::InvalidUtf16(_))
        ));
    }

    #[test]
    fn oversized_and_missing_files_are_distinct_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.srt");
        assert!(matches!(read_text(&path), Err(ReadError::Open { .. })));
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES as u64 + 1).unwrap();
        assert!(matches!(read_text(&path), Err(ReadError::TooLarge)));
    }

    #[test]
    fn best_file_follows_the_preference_order() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        for name in ["sub.en.srt", "sub.zh-Hans.srt", "sub.ja.srt"] {
            std::fs::write(d.join(name), b"1\n00:00:01,000 --> 00:00:02,000\nx\n").unwrap();
        }
        let picked = best_file(files_in(d), &[], "zh", Some("en")).unwrap();
        assert_eq!(picked.file_name().unwrap(), "sub.zh-Hans.srt");
        assert_eq!(best_file(Vec::new(), &[], "zh", None), None);
    }

    #[test]
    fn sidecar_lookup_prefers_a_matching_language() {
        let temp = tempfile::tempdir().unwrap();
        let d = temp.path();
        let video = d.join("lecture.mp4");
        std::fs::write(&video, b"v").unwrap();
        let pick = || best_file(sidecars(&video), &[], "zh", Some("en"));
        assert!(pick().is_none());
        std::fs::write(
            d.join("lecture.srt"),
            b"1\n00:00:01,000 --> 00:00:02,000\nx\n",
        )
        .unwrap();
        assert_eq!(pick().unwrap(), d.join("lecture.srt"));
        std::fs::write(
            d.join("lecture.zh.VTT"),
            b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nx\n",
        )
        .unwrap();
        assert_eq!(pick().unwrap(), d.join("lecture.zh.VTT"));
    }
}
