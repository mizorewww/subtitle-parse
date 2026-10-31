# subtitle-parse

Tolerant SRT and WebVTT parsing for real-world subtitle files, as a small library.
No required dependencies.

- **Parsing**: SRT and WebVTT timestamps, cue markup, numeric and named entities.
  `x < 3 and y > 2` stays text. Rolling automatic captions lose their repeats.
- **Reading**: UTF-8 and UTF-16 by BOM, never with lossy replacement.
- **Finding**: sidecar files beside a video, or every subtitle file in a directory.
- **Choosing**: order tracks by preference, interface language and original language.

Failures are typed. The crate has no user-facing text of its own, so an application
words them in its own language.

```toml
[dependencies]
subtitle-parse = { git = "https://github.com/mizorewww/subtitle-parse" }
```

```rust
let parsed = subtitle_parse::parse("WEBVTT\n\n00:01.000 --> 00:02.500\n<c.yellow>Hello</c> &amp; welcome\n");
assert_eq!(parsed.cues[0].text, "Hello & welcome");
assert!(parsed.skipped.is_empty());

let srt = subtitle_parse::to_srt(&parsed.cues);
```

```rust,no_run
use std::path::Path;
use subtitle_parse::{best_file, read_text, sidecars};

let tracks = sidecars(Path::new("lecture.mp4"));
if let Some(path) = best_file(tracks, &[], "en", None) {
    let text = read_text(&path)?;
    println!("{} cues", subtitle_parse::parse(&text).cues.len());
}
# Ok::<(), subtitle_parse::ReadError>(())
```

The `serde` feature serializes `Cue` and the track types.

Not on crates.io yet; depend on it by Git revision.

## License

[MIT](LICENSE)
