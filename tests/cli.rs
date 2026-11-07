//! The `subtitle-parse` command, run as a user runs it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use subtitle_parse::{parse, to_srt};

const ROLLING_VTT: &str = include_str!("fixtures/youtube-rolling.en.vtt");

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    run_in(None, args, None)
}

fn run_with_stdin(args: &[&str], stdin: &[u8]) -> Output {
    run_in(None, args, Some(stdin))
}

fn run_in(dir: Option<&Path>, args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_subtitle-parse"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn().expect("the binary runs");
    let mut pipe = child.stdin.take().unwrap();
    let stdin = stdin.unwrap_or_default().to_vec();
    // A command may exit before reading all of its input.
    let writer = std::thread::spawn(move || pipe.write_all(&stdin));
    let output = child.wait_with_output().unwrap();
    let _ = writer.join().unwrap();
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Fails with status 1, a message on standard error and nothing on standard output.
fn assert_fails(output: &Output, message: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let stderr = stderr(output);
    assert!(stderr.starts_with("subtitle-parse: error: "), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn convert_writes_rolling_captions_as_clean_srt() {
    let output = run(&["convert", path(&fixture("youtube-rolling.en.vtt"))]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(stdout(&output), to_srt(&parse(ROLLING_VTT).cues));
    assert!(output.stderr.is_empty());
}

#[test]
fn convert_reads_standard_input() {
    let output = run_with_stdin(&["convert", "-", "--to", "txt"], ROLLING_VTT.as_bytes());
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 26, "every spoken line once");
    assert!(text.starts_with("In this video, I'll show you exactly how\n"));
    assert!(!text.contains("<c>"));
}

#[test]
fn convert_writes_webvtt_with_text_escaped() {
    let output = run(&[
        "convert",
        path(&fixture("cli/dropped-cue.srt")),
        "--to",
        "vtt",
        "--quiet",
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout(&output),
        "WEBVTT\n\n\
         00:00:01.000 --> 00:00:02.500\nHello &amp; welcome\n\n\
         00:00:02.500 --> 00:00:04.000\nx &lt; 3 and y &gt; 2\n\n"
    );
    assert!(output.stderr.is_empty(), "--quiet hides the warning");
}

#[test]
fn convert_warns_about_dropped_cues() {
    let output = run(&["convert", path(&fixture("cli/dropped-cue.srt"))]);
    assert!(output.status.success(), "{output:?}");
    assert!(stdout(&output).starts_with("1\n00:00:01,000 --> 00:00:02,500\nHello & welcome\n"));
    assert_eq!(
        stderr(&output),
        "subtitle-parse: warning: dropped a cue with an unusable time range: \
         00:00:05,000 --> 00:00:01,000\n"
    );
}

#[test]
fn convert_takes_the_format_from_the_output_extension() {
    let dir = tempfile::tempdir().unwrap();
    for (name, start) in [
        ("clean.vtt", "WEBVTT\n"),
        ("clean.SRT", "1\n00:00:00,080 --> "),
        ("clean.txt", "In this video"),
    ] {
        let target = dir.path().join(name);
        let output = run(&[
            "convert",
            path(&fixture("youtube-rolling.en.vtt")),
            "-o",
            path(&target),
        ]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        let written = std::fs::read_to_string(&target).unwrap();
        assert!(written.starts_with(start), "{name}: {written}");
    }
}

#[test]
fn convert_refuses_an_output_of_unknown_format() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("clean.ass");
    let output = run(&[
        "convert",
        path(&fixture("youtube-rolling.en.vtt")),
        "-o",
        path(&target),
    ]);
    assert_fails(&output, "pass --to srt, vtt or txt");
    assert!(!target.exists());
}

#[test]
fn convert_fails_on_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.srt");
    assert_fails(&run(&["convert", path(&missing)]), "cannot open");
}

#[test]
fn convert_fails_on_text_that_is_not_utf8() {
    assert_fails(
        &run(&["convert", path(&fixture("cli/latin1.srt"))]),
        "invalid UTF-8 text",
    );
    assert_fails(
        &run_with_stdin(&["convert"], &[0xff, 0xfe, 0x41]),
        "standard input: incomplete UTF-16 text",
    );
}

#[test]
fn convert_fails_when_there_are_no_cues() {
    assert_fails(
        &run(&["convert", path(&fixture("cli/no-cues.vtt"))]),
        "no cues found",
    );
    assert_fails(
        &run_with_stdin(&["convert"], b""),
        "no cues found in standard input",
    );
}

#[test]
fn inspect_counts_cues_after_cleaning() {
    let output = run(&["inspect", path(&fixture("youtube-rolling.en.srt"))]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout(&output),
        "cues:     26\n\
         start:    00:00:00.080\n\
         end:      00:00:55.120\n\
         duration: 00:00:55.040\n\
         dropped:  0\n"
    );
}

#[test]
fn inspect_lists_dropped_cues() {
    let output = run_with_stdin(
        &["inspect"],
        &std::fs::read(fixture("cli/dropped-cue.srt")).unwrap(),
    );
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert!(text.starts_with("cues:     2\n"), "{text}");
    assert!(
        text.ends_with("dropped:  1\n  00:00:05,000 --> 00:00:01,000\n"),
        "{text}"
    );
}

#[test]
fn inspect_reports_a_file_without_cues() {
    let output = run(&["inspect", path(&fixture("cli/no-cues.vtt"))]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(stdout(&output), "cues:     0\ndropped:  0\n");
}

/// The file names in a `tracks` listing, with their languages.
fn listed(output: &Output) -> Vec<(String, String)> {
    stdout(output)
        .lines()
        .map(|line| {
            let (language, path) = line.split_once('\t').expect("language, tab, path");
            let name = Path::new(path).file_name().unwrap().to_str().unwrap();
            (language.to_owned(), name.to_owned())
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(language, name)| (language.to_string(), name.to_string()))
        .collect()
}

#[test]
fn tracks_lists_sidecars_in_preferred_order() {
    let video = fixture("cli/videos/lecture.mp4");
    let output = run(&["tracks", path(&video), "--prefer", "zh,fr"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        listed(&output),
        pairs(&[
            ("zh-Hans", "lecture.zh-Hans.srt"),
            ("fr", "lecture.fr.srt"),
            ("en", "lecture.en.vtt"),
            ("-", "lecture.srt"),
        ]),
        "other.srt belongs to another video"
    );
    let output = run(&["tracks", path(&video), "--interface", "en", "-p", "ja"]);
    assert_eq!(listed(&output)[0], pairs(&[("en", "lecture.en.vtt")])[0]);
}

#[test]
fn tracks_best_prints_one_path() {
    let output = run(&[
        "tracks",
        path(&fixture("cli/videos/lecture.mp4")),
        "--best",
        "--original",
        "FRA",
    ]);
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 1);
    assert!(text.trim_end().ends_with("lecture.fr.srt"), "{text}");
}

#[test]
fn tracks_finds_sidecars_of_a_bare_file_name() {
    let output = run_in(
        Some(&fixture("cli/videos")),
        &["tracks", "lecture.mp4", "--best", "-p", "en"],
        None,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        stdout(&output),
        "lecture.en.vtt\n",
        "as the video was given"
    );
}

#[test]
fn tracks_lists_every_subtitle_file_in_a_directory() {
    let output = run(&["tracks", path(&fixture("cli/videos"))]);
    assert!(output.status.success(), "{output:?}");
    let names: Vec<_> = listed(&output).into_iter().map(|(_, name)| name).collect();
    assert_eq!(names.len(), 5);
    assert!(names.contains(&"other.srt".to_owned()));
}

#[test]
fn tracks_fails_when_nothing_is_found() {
    let dir = tempfile::tempdir().unwrap();
    assert_fails(
        &run(&["tracks", path(&dir.path().join("lecture.mp4"))]),
        "no subtitle files found",
    );
}

#[test]
fn usage_errors_exit_with_2_and_help_with_0() {
    let output = run(&["convert", "--to", "ass"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("invalid value 'ass'"));
    assert_eq!(run(&[]).status.code(), Some(2));
    assert_eq!(run(&["frobnicate"]).status.code(), Some(2));

    let help = run(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    for command in ["convert", "inspect", "tracks"] {
        assert!(stdout(&help).contains(command));
    }
    assert!(run(&["--version"]).status.success());
}
