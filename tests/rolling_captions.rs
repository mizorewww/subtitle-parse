//! YouTube's automatic captions, in the shape yt-dlp writes them: a two-line
//! rolling window whose new line is karaoke-timed, a 10 ms freeze between windows,
//! and a line holding one space where a window has a single line. The `.srt` is
//! what `yt-dlp --convert-subs srt` (ffmpeg) makes of the `.vtt`. The words were
//! written for these tests.

use subtitle_parse::{Cue, parse};

const VTT: &str = include_str!("fixtures/youtube-rolling.en.vtt");
const SRT: &str = include_str!("fixtures/youtube-rolling.en.srt");

/// The spoken lines, once each, in order.
fn spoken() -> Vec<String> {
    VTT.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let header = lines.next()?;
            let lines: Vec<_> = lines.collect();
            // A freeze cue: the finished line, then the blank line.
            (header.contains("-->") && lines.len() == 2 && lines[1] == " ")
                .then(|| lines[0].to_owned())
        })
        .collect()
}

fn check(cues: &[Cue]) {
    let spoken = spoken();
    assert_eq!(spoken.len(), 26);
    let texts: Vec<_> = cues.iter().map(|cue| cue.text.as_str()).collect();
    assert_eq!(texts, spoken, "every line once, nothing repeated");
    // Each line lasts from its first word to the end of its freeze, so that
    // screenshots taken at a cue's time show what it says.
    assert_eq!((cues[0].start, cues[0].end), (0.08, 2.55));
    assert_eq!((cues[1].start, cues[1].end), (2.55, 4.75));
    let fresh = texts
        .iter()
        .position(|text| text.starts_with("Now let's"))
        .unwrap();
    assert_eq!(
        (cues[fresh].start, cues[fresh].end),
        (cues[fresh - 1].end, 36.4)
    );
    for pair in cues.windows(2) {
        assert!(pair[0].end - pair[0].start > 1.0, "{pair:?}");
        assert_eq!(pair[0].end, pair[1].start);
    }
}

#[test]
fn youtube_rolling_captions_say_each_line_once_at_its_time() {
    let vtt = parse(VTT);
    assert!(vtt.skipped.is_empty());
    check(&vtt.cues);
    check(&parse(SRT).cues);
    // As a Windows editor saves it.
    check(&parse(&SRT.replace('\n', "\r\n")).cues);
}

#[test]
fn a_blank_line_before_the_next_cue_still_ends_a_cue() {
    // Loosely written SRT: separators that hold spaces, with and without numbers.
    let cues = parse(
        "1\n00:00:01,000 --> 00:00:02,000\nfirst\n \n2\n00:00:02,000 --> 00:00:03,000\nsecond\n\t\n\
         00:00:03,000 --> 00:00:04,000\nthird\n ",
    )
    .cues;
    let texts: Vec<_> = cues.iter().map(|cue| cue.text.as_str()).collect();
    assert_eq!(texts, ["first", "second", "third"]);
}

#[test]
fn ordinary_two_line_cues_stay_whole() {
    // Neither line repeats the cue before, so nothing is taken away.
    let cues = parse(
        "WEBVTT\n\n00:01.000 --> 00:03.000\nThe first line\nand its second line\n\n\
         00:03.000 --> 00:05.000\nA new sentence\nthat also has two lines\n",
    )
    .cues;
    let texts: Vec<_> = cues.iter().map(|cue| cue.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "The first line and its second line",
            "A new sentence that also has two lines"
        ]
    );
}
