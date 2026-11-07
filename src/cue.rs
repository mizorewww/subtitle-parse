//! Cues: what a subtitle file says, and when.

/// One piece of text on screen, in seconds.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A cue dropped because its time range cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedCue {
    /// The cue's header line, as written.
    pub header: String,
}

/// The result of [`parse`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub cues: Vec<Cue>,
    pub skipped: Vec<SkippedCue>,
}

/// Parses SRT or WebVTT text. Parsing never fails; anything that is not a cue is ignored.
///
/// - Timestamps may be `HH:MM:SS,mmm`, `HH:MM:SS.mmm` or `MM:SS.mmm`.
/// - Cue markup and HTML entities are removed; the lines of a cue are joined.
/// - Rolling captions (common in automatic subtitles) keep only the new text.
///   Each line of YouTube's two-line rolling window becomes one cue, from its
///   first word to the end of its freeze.
/// - A cue with an empty, backwards or non-finite time range lands in
///   [`Parsed::skipped`], so sorting and searching downstream can trust the times.
pub fn parse(content: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let out = &mut parsed.cues;
    let lines: Vec<&str> = content.lines().collect();
    // The lines of the previous cue, as they were on screen.
    let mut on_screen: Vec<String> = vec![];
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let Some((start, end)) = parse_cue_header(line) else {
            i += 1;
            continue;
        };
        i += 1;
        if !start.is_finite() || !end.is_finite() || end <= start {
            parsed.skipped.push(SkippedCue {
                header: line.to_owned(),
            });
            continue;
        }
        let mut text_parts: Vec<String> = vec![];
        while i < lines.len() && !ends_cue(&lines, i) {
            let cleaned = clean_cue_text(lines[i]);
            if !cleaned.is_empty() {
                text_parts.push(cleaned);
            }
            i += 1;
        }
        if text_parts.is_empty() {
            continue;
        }
        // Rolling captions repeat what is already on screen. Only a cue that
        // continues without a pause can be repeating the one before it.
        let previous = out.last_mut().filter(|previous| start <= previous.end);
        let repeated = previous
            .as_ref()
            .map_or(0, |_| repeated_lines(&on_screen, &text_parts));
        let fresh = text_parts[repeated..].join(" ");
        match previous {
            // Everything is already there: the same caption stays up longer.
            Some(previous) if fresh.is_empty() => previous.end = end.max(previous.end),
            // The caption grew by a few words, in place.
            Some(previous) if repeated == 0 && grows_from(&previous.text, &fresh) => {
                previous.text = fresh;
                previous.end = end.max(previous.end);
            }
            _ => out.push(Cue {
                start,
                end,
                text: fresh,
            }),
        }
        on_screen = text_parts;
    }
    parsed
}

/// Writes cues as SRT, numbered from 1. Blank lines in a cue's text are left
/// out, because a blank line ends a cue; other line breaks stay.
pub fn to_srt(cues: &[Cue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(index, cue)| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                index + 1,
                timestamp(cue.start, ','),
                timestamp(cue.end, ','),
                text_lines(&cue.text)
            )
        })
        .collect()
}

/// Writes cues as WebVTT, without cue identifiers. `&`, `<` and `>` in the text
/// are escaped, and blank lines are left out as in [`to_srt`]. Cues that came from
/// [`parse`] parse back to the same cues.
pub fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        let text = text_lines(&cue.text)
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!(
            "{} --> {}\n{text}\n\n",
            timestamp(cue.start, '.'),
            timestamp(cue.end, '.')
        ));
    }
    out
}

/// The lines of a cue's text without blank ones, which would end the cue.
fn text_lines(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `HH:MM:SS`, `separator` and milliseconds: `,` for SRT, `.` for WebVTT.
fn timestamp(seconds: f64, separator: char) -> String {
    let millis = (seconds.max(0.) * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000
    )
}

/// Lenient timestamp: `HH:MM:SS`, `MM:SS` or bare seconds, with `.` or `,` as the
/// decimal separator, surrounding whitespace, `[...]` and a trailing `s` tolerated.
/// Minutes or seconds of 60 and above, negative and non-finite values are `None`.
pub fn parse_timestamp(value: &str) -> Option<f64> {
    let value = value.trim().trim_matches(['[', ']']).trim();
    let value = value.strip_suffix('s').unwrap_or(value);
    let value = value.replace(',', ".");
    let parts: Vec<&str> = value.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut result = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let n: f64 = part.trim().parse().ok()?;
        if !n.is_finite() || n < 0.0 || (i > 0 && n >= 60.0) {
            return None;
        }
        result = result * 60.0 + n;
    }
    Some(result)
}

/// Whether the cue text stops at `lines[index]`. WebVTT ends a cue only at an
/// empty line, and YouTube's automatic captions use a line holding a single space
/// to keep their rolling window two lines tall, before the first line or after the
/// last. A blank line still ends the cue when the next cue, or nothing, follows it,
/// as in loosely written SRT.
fn ends_cue(lines: &[&str], index: usize) -> bool {
    let line = lines[index];
    if !line.trim().is_empty() {
        return false;
    }
    if line.is_empty() {
        return true;
    }
    let starts_cue = |offset: usize| {
        lines
            .get(index + offset)
            .is_some_and(|line| parse_cue_header(line).is_some())
    };
    match lines.get(index + 1).map(|next| next.trim()) {
        None | Some("") => true,
        Some(next) => starts_cue(1) || (next.bytes().all(|b| b.is_ascii_digit()) && starts_cue(2)),
    }
}

/// How many leading lines of `next` end `previous`: the longest overlap.
fn repeated_lines(previous: &[String], next: &[String]) -> usize {
    (1..=previous.len().min(next.len()))
        .rev()
        .find(|count| previous[previous.len() - count..] == next[..*count])
        .unwrap_or(0)
}

/// Whether `next` is `previous` with more words after it (not more letters in
/// the same word).
fn grows_from(previous: &str, next: &str) -> bool {
    next.strip_prefix(previous).is_some_and(|rest| {
        !rest.is_empty() && !rest.starts_with(|c: char| c.is_ascii_alphanumeric())
    })
}

fn parse_cue_header(line: &str) -> Option<(f64, f64)> {
    let (a, b) = line.split_once("-->")?;
    // A WebVTT header may carry settings such as `line:0` after the end time.
    let end = b.split_whitespace().next()?;
    Some((parse_timestamp(a)?, parse_timestamp(end)?))
}

/// Removes cue markup and decodes entities. Only the markup subtitle formats use
/// counts as a tag, so `x < 3 and y > 2` stays text.
fn clean_cue_text(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut rest = line.trim();
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let tag = &rest[open + 1..];
        match tag.find('>').filter(|close| is_cue_tag(&tag[..*close])) {
            Some(close) => rest = &tag[close + 1..],
            None => {
                text.push('<');
                rest = tag;
            }
        }
    }
    text.push_str(rest);
    decode_entities(&text).trim().to_string()
}

/// `inner` is what stands between `<` and `>`: a WebVTT word timestamp or a tag
/// from SRT and WebVTT. Tags that carry free text (`<v Speaker>`) may have it;
/// the rest allow only a class (`<c.yellow>`), so `a<b and c>d` is not a tag.
fn is_cue_tag(inner: &str) -> bool {
    if inner.contains('<') {
        return false;
    }
    if inner.starts_with(|c: char| c.is_ascii_digit()) {
        return inner
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b':' || b == b'.');
    }
    let inner = inner.strip_prefix('/').unwrap_or(inner);
    let end = inner
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(inner.len());
    let (name, rest) = inner.split_at(end);
    match name.to_ascii_lowercase().as_str() {
        "v" | "lang" | "font" | "span" => rest.is_empty() || rest.starts_with([' ', '\t', '.']),
        "b" | "i" | "u" | "s" | "c" | "em" | "strong" | "ruby" | "rt" | "br" => {
            rest.is_empty() || rest == "/" || rest.starts_with('.')
        }
        _ => false,
    }
}

/// One pass, so `&amp;lt;` becomes `&lt;` and goes no further. Anything that is
/// not a known entity stays as written.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp + 1..];
        let decoded = tail
            .find(';')
            .and_then(|end| Some((decode_entity(&tail[..end])?, end)));
        match decoded {
            Some((character, end)) => {
                out.push(character);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<char> {
    let numeric = name.strip_prefix('#').map(|number| {
        match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16),
            None => number.parse(),
        }
        .ok()
        .and_then(char::from_u32)
    });
    let character = match (name, numeric) {
        (_, Some(character)) => character?,
        ("nbsp", _) => ' ',
        ("lt", _) => '<',
        ("gt", _) => '>',
        ("quot", _) => '"',
        ("apos", _) => '\'',
        ("amp", _) => '&',
        _ => return None,
    };
    // A cue is one line of text: spacing entities are spaces, controls are not text.
    Some(if character == '\u{a0}' || character.is_control() {
        ' '
    } else {
        character
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cues(content: &str) -> Vec<Cue> {
        parse(content).cues
    }

    #[test]
    fn repeated_words_after_a_pause_remain_separate() {
        let events = cues(
            "1\n00:00:01,000 --> 00:00:02,000\nhello\n\n2\n00:00:10,000 --> 00:00:11,000\nhello\n",
        );
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].end, 2.0);
        assert!(parse_timestamp("00:00:01.bad").is_none());
        assert!(parse_timestamp("00:99:01.000").is_none());
    }

    #[test]
    fn backwards_cues_are_reported_as_skipped() {
        let header = "00:00:02,000 --> 00:00:01,000";
        let parsed = parse(&format!("{header}\nbackwards\n"));
        assert!(parsed.cues.is_empty());
        assert_eq!(parsed.skipped[0].header, header);
    }

    #[test]
    fn parses_srt_with_multiline_cues() {
        let srt = "\
1
00:00:01,000 --> 00:00:04,000
大家好，今天我们讲
编译原理

2
00:00:04,500 --> 00:00:07,250
这是第二段
";
        let ev = cues(srt);
        assert_eq!(ev.len(), 2);
        assert!((ev[0].start - 1.0).abs() < 1e-3);
        assert!((ev[0].end - 4.0).abs() < 1e-3);
        assert_eq!(ev[0].text, "大家好，今天我们讲 编译原理");
        assert!((ev[1].start - 4.5).abs() < 1e-3);
    }

    #[test]
    fn parses_vtt_and_strips_tags() {
        let vtt = "\
WEBVTT

NOTE a comment

00:01:01.000 --> 00:01:03.000 align:start
<c>hello</c> world&nbsp;!

00:03.500 --> 00:05.000
second cue
";
        let ev = cues(vtt);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].text, "hello world !");
        assert!((ev[0].start - 61.0).abs() < 1e-3);
        assert_eq!(ev[1].text, "second cue");
    }

    #[test]
    fn angle_brackets_outside_tags_are_text() {
        assert_eq!(clean_cue_text("x < 3 and y > 2"), "x < 3 and y > 2");
        assert_eq!(clean_cue_text("if a<b and c>d then"), "if a<b and c>d then");
        assert_eq!(clean_cue_text("1 <2"), "1 <2");
        assert_eq!(clean_cue_text("a &lt; b"), "a < b");
        // Real cue markup still goes, in every form subtitle files use it.
        for (cue, text) in [
            ("<i>word</i>", "word"),
            ("<font color=\"#fff\">word</font>", "word"),
            ("<v Speaker One>word</v>", "word"),
            ("<c.colorE5E5E5>word</c>", "word"),
            ("<lang en-GB>word</lang>", "word"),
            ("<B>word</B>", "word"),
            ("wo<00:00:01.500><c>rd</c>", "word"),
        ] {
            assert_eq!(clean_cue_text(cue), text, "{cue}");
        }
    }

    #[test]
    fn entities_decode_once_including_numeric_ones() {
        for (cue, text) in [
            ("it&#39;s", "it's"),
            ("it&#x27;s", "it's"),
            ("it&#X27;s &apos;ok&apos;", "it's 'ok'"),
            ("a&nbsp;b &#160; c", "a b   c"),
            ("&amp;lt;", "&lt;"),
            ("&#38;lt;", "&lt;"),
            (
                "R&D; fish & chips &unknown; &#xZZ; &#1114112;",
                "R&D; fish & chips &unknown; &#xZZ; &#1114112;",
            ),
            ("中&#25991;", "中文"),
        ] {
            assert_eq!(clean_cue_text(cue), text, "{cue}");
        }
    }

    #[test]
    fn rolling_lines_are_not_repeated() {
        // Automatic captions: each cue repeats the line before it.
        let vtt = "\
WEBVTT

00:00:01.000 --> 00:00:03.000 align:start position:0%
machine<00:00:01.500><c> learning</c>

00:00:03.000 --> 00:00:03.010 align:start position:0%
machine learning

00:00:03.010 --> 00:00:05.000 align:start position:0%
machine learning
is<00:00:03.500><c> fun</c>

00:00:05.000 --> 00:00:07.000 align:start position:0%
is fun
and<00:00:05.500><c> useful</c>
";
        let events = cues(vtt);
        let texts: Vec<_> = events.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["machine learning", "is fun", "and useful"]);
        assert_eq!((events[0].start, events[0].end), (1.0, 3.01));
        assert_eq!((events[1].start, events[1].end), (3.01, 5.0));
        assert_eq!(events[2].end, 7.0);

        // A cue that grows word by word is one sentence.
        let growing = cues(
            "00:00:01,000 --> 00:00:02,000\nhello\n\n\
             00:00:02,000 --> 00:00:03,000\nhello world\n\n\
             00:00:03,000 --> 00:00:04,000\nhello world again\n",
        );
        assert_eq!(growing.len(), 1);
        assert_eq!(growing[0].text, "hello world again");
        assert_eq!((growing[0].start, growing[0].end), (1.0, 4.0));
    }

    #[test]
    fn words_that_only_look_rolling_stay_separate() {
        // A pause between cues, and a shared prefix inside one word.
        let events = cues(
            "00:00:01,000 --> 00:00:02,000\nyes\nno\n\n\
             00:00:05,000 --> 00:00:06,000\nno\nmaybe\n\n\
             00:00:06,000 --> 00:00:07,000\nmaybe\n\n\
             00:00:07,000 --> 00:00:08,000\nmaybes\n",
        );
        let texts: Vec<_> = events.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["yes no", "no maybe", "maybes"]);
    }

    #[test]
    fn identical_neighbouring_cues_merge() {
        let srt = "\
1
00:00:01,000 --> 00:00:02,000
机器学习是

2
00:00:02,000 --> 00:00:03,000
机器学习是

3
00:00:03,000 --> 00:00:04,000
一门人工智能分支
";
        let ev = cues(srt);
        assert_eq!(ev.len(), 2, "identical neighbours merge");
        assert!((ev[0].end - 3.0).abs() < 1e-3, "the repeat extends the cue");
        assert_eq!(ev[1].text, "一门人工智能分支");
    }

    #[test]
    fn srt_output_parses_back() {
        let events = cues("1\n00:00:01,125 --> 01:00:02,375\nBonjour 世界\n");
        assert_eq!(
            to_srt(&events),
            "1\n00:00:01,125 --> 01:00:02,375\nBonjour 世界\n\n"
        );
        assert_eq!(events, cues(&to_srt(&events)));
    }

    #[test]
    fn vtt_output_parses_back_with_its_text_escaped() {
        let events = cues(
            "1\n00:00:01,125 --> 01:00:02,375\nBonjour 世界\n\n\
             2\n01:00:02,375 --> 01:00:04,000\nx < 3 &amp; y > 2 --> <b>done</b>\n",
        );
        assert_eq!(events[1].text, "x < 3 & y > 2 --> done");
        assert_eq!(
            to_vtt(&events),
            "WEBVTT\n\n\
             00:00:01.125 --> 01:00:02.375\nBonjour 世界\n\n\
             01:00:02.375 --> 01:00:04.000\nx &lt; 3 &amp; y &gt; 2 --&gt; done\n\n"
        );
        assert_eq!(events, cues(&to_vtt(&events)));
        assert_eq!(to_vtt(&[]), "WEBVTT\n\n");
    }

    #[test]
    fn writers_leave_out_blank_lines_and_keep_other_line_breaks() {
        let written = [Cue {
            start: 1.0,
            end: 2.0,
            text: "First line\n\n  \r\nsecond line\n".into(),
        }];
        assert_eq!(
            to_srt(&written),
            "1\n00:00:01,000 --> 00:00:02,000\nFirst line\nsecond line\n\n"
        );
        assert_eq!(
            to_vtt(&written),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nFirst line\nsecond line\n\n"
        );
        // The blank line no longer splits the cue: its whole text is read back.
        for text in [to_srt(&written), to_vtt(&written)] {
            assert_eq!(
                cues(&text),
                [Cue {
                    start: 1.0,
                    end: 2.0,
                    text: "First line second line".into(),
                }]
            );
        }
    }

    #[test]
    fn timestamps_accept_every_written_form() {
        for (text, seconds) in [
            ("01:02:03,500", 3723.5),
            ("02:03.5", 123.5),
            ("[01:00]", 60.0),
            ("120s", 120.0),
            (" 7.25 ", 7.25),
        ] {
            assert_eq!(parse_timestamp(text), Some(seconds), "{text}");
        }
        for text in ["", "a", "-1", "1:2:3:4", "00:60", "inf"] {
            assert_eq!(parse_timestamp(text), None, "{text}");
        }
    }
}
