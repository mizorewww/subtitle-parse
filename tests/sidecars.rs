//! Sidecar discovery for a video given by bare file name, as in the README.
//! This test changes the working directory, so it has a process of its own.

use std::path::{Path, PathBuf};
use subtitle_parse::{TrackOrigin, best_file, sidecars};

#[test]
fn a_bare_file_name_finds_sidecars_in_the_current_directory() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["lecture.srt", "lecture.en.vtt", "other.srt"] {
        std::fs::write(
            dir.path().join(name),
            "1\n00:00:00,000 --> 00:00:01,000\nHello\n",
        )
        .unwrap();
    }
    std::fs::create_dir(dir.path().join("media")).unwrap();
    std::fs::write(
        dir.path().join("media/lecture.fr.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nBonjour\n",
    )
    .unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let paths: Vec<PathBuf> = sidecars(Path::new("lecture.mp4"))
        .into_iter()
        .map(|track| match track.origin {
            TrackOrigin::File { path } => path,
            origin => panic!("{origin:?}"),
        })
        .collect();
    // Paths keep the form the video was given in.
    assert_eq!(
        paths,
        [
            PathBuf::from("lecture.en.vtt"),
            PathBuf::from("lecture.srt")
        ]
    );
    assert_eq!(
        best_file(sidecars(Path::new("lecture.mp4")), &[], "en", None),
        Some(PathBuf::from("lecture.en.vtt"))
    );
    assert_eq!(
        best_file(sidecars(Path::new("media/lecture.mp4")), &[], "en", None),
        Some(Path::new("media").join("lecture.fr.srt"))
    );
    // `media/` is a directory: `media.srt` beside it is not its sidecar.
    std::fs::write("media.srt", "1\n00:00:00,000 --> 00:00:01,000\nHello\n").unwrap();
    assert!(sidecars(Path::new("media/")).is_empty());
    // Leave the directory before it is removed.
    std::env::set_current_dir(original).unwrap();
}
