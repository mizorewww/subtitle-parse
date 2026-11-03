//! Tracks: which subtitles exist for a video, and which to offer first.

use std::path::PathBuf;

/// A discovered subtitle track. Discovery is not proof that its text can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Track {
    pub id: String,
    pub language: Option<String>,
    pub name: Option<String>,
    pub kind: TrackKind,
    /// A machine translation of another track, such as YouTube's automatic
    /// translations (yt-dlp gives their URL a `tlang` parameter). Only the source
    /// can say so; a language code does not.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "core::ops::Not::not")
    )]
    pub translated: bool,
    pub origin: TrackOrigin,
    /// Position in discovery order; ties in [`sort_tracks`] keep to it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub source_order: usize,
}

/// Whether a person or a machine wrote the track, when the source says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrackKind {
    Manual,
    Automatic,
    Unknown,
}

/// Where a track's text can be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum TrackOrigin {
    Online {
        /// The exact key the source uses. This is not a display label.
        language_key: String,
        automatic: bool,
        #[cfg_attr(feature = "serde", serde(default))]
        inline_text: Option<String>,
    },
    Embedded {
        stream_index: u32,
        codec: String,
    },
    File {
        path: PathBuf,
    },
}

impl Track {
    /// A track for a subtitle file, with no claim about who wrote it.
    pub fn file(path: PathBuf, language: Option<String>) -> Self {
        Self {
            id: format!("file:{}", path.display()),
            language,
            name: None,
            kind: TrackKind::Unknown,
            translated: false,
            origin: TrackOrigin::File { path },
            source_order: usize::MAX,
        }
    }
}

/// Lower-case, `-`-separated, with ISO 639-2 codes (as ffprobe reports them)
/// reduced to their ISO 639-1 form.
pub fn normalize_language(language: &str) -> String {
    let value = language.trim().replace('_', "-").to_ascii_lowercase();
    match value.as_str() {
        "zho" | "chi" => "zh".into(),
        "eng" => "en".into(),
        "fra" | "fre" => "fr".into(),
        "deu" | "ger" => "de".into(),
        "jpn" => "ja".into(),
        "kor" => "ko".into(),
        "spa" => "es".into(),
        "por" => "pt".into(),
        "rus" => "ru".into(),
        "ara" => "ar".into(),
        "ita" => "it".into(),
        _ => value,
    }
}

/// Preference list → interface language → known original language → discovery
/// order. A missed preference always falls through, never removes other languages.
/// A generic preference such as `zh` covers its language family equally; a
/// script/region preference such as `zh-Hans` prefers that exact variant.
/// Within an identical language, known human captions precede known automatic ones.
///
/// A machine translation follows every track that is not one, whatever its
/// language: it is another track's words, translated word by word, and sources
/// limit it harder (YouTube answers 429 for translations it serves at once for the
/// original). Human captions in the interface language still come first.
pub fn sort_tracks(
    tracks: &mut [Track],
    preferences: &[String],
    interface_language: &str,
    original_language: Option<&str>,
) {
    let mut languages = Vec::new();
    for language in preferences
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(interface_language))
        .chain(original_language)
    {
        let value = normalize_language(language);
        if !value.is_empty() && !languages.contains(&value) {
            languages.push(value);
        }
    }
    // Precompute one rank for each exact language group. This preserves the
    // extractor's order between remaining languages, including non-Latin tracks.
    let mut groups: Vec<Option<String>> = Vec::new();
    let mut discovery: Vec<_> = tracks.iter().collect();
    discovery.sort_by_key(|track| track.source_order);
    for track in discovery {
        let language = track.language.as_deref().map(normalize_language);
        if !groups.contains(&language) {
            groups.push(language);
        }
    }
    let rank = |track: &Track| {
        let language = track.language.as_deref().map(normalize_language);
        let preferred = language
            .as_ref()
            .and_then(|language| {
                languages
                    .iter()
                    .enumerate()
                    .find_map(|(index, preference)| {
                        let same = language == preference;
                        let family = language.split('-').next() == preference.split('-').next();
                        (same || family).then_some((
                            index,
                            if same || !preference.contains('-') {
                                0
                            } else {
                                1
                            },
                        ))
                    })
            })
            .unwrap_or((languages.len(), 0));
        let group = groups
            .iter()
            .position(|group| group == &language)
            .unwrap_or(usize::MAX);
        (track.translated, preferred, group)
    };
    tracks.sort_by_key(rank);
    // Unknown labels retain their positions. Only reorder slots whose type the
    // source explicitly distinguishes, avoiding invented claims about a track.
    let mut start = 0;
    while start < tracks.len() {
        let language = tracks[start].language.as_deref().map(normalize_language);
        let translated = tracks[start].translated;
        let end = tracks[start..]
            .iter()
            .position(|track| {
                track.language.as_deref().map(normalize_language) != language
                    || track.translated != translated
            })
            .map(|offset| start + offset)
            .unwrap_or(tracks.len());
        let positions: Vec<_> = (start..end)
            .filter(|index| tracks[*index].kind != TrackKind::Unknown)
            .collect();
        let mut known: Vec<_> = positions
            .iter()
            .map(|index| tracks[*index].clone())
            .collect();
        known.sort_by_key(|track| track.kind == TrackKind::Automatic);
        for (position, track) in positions.into_iter().zip(known) {
            tracks[position] = track;
        }
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(language: &str, kind: TrackKind) -> Track {
        Track {
            id: format!("{language}:{kind:?}"),
            language: Some(language.into()),
            name: None,
            kind,
            translated: false,
            source_order: 0,
            origin: TrackOrigin::Online {
                language_key: language.into(),
                automatic: kind == TrackKind::Automatic,
                inline_text: None,
            },
        }
    }

    fn languages(tracks: &[Track]) -> Vec<&str> {
        tracks
            .iter()
            .map(|track| track.language.as_deref().unwrap())
            .collect()
    }

    #[test]
    fn missed_preferences_fall_through_without_hiding_languages() {
        let mut tracks = vec![
            track("fr", TrackKind::Automatic),
            track("ja", TrackKind::Manual),
            track("fr", TrackKind::Manual),
            track("de", TrackKind::Manual),
        ];
        sort_tracks(&mut tracks, &["zh".into()], "en", Some("ja"));
        assert_eq!(languages(&tracks), ["ja", "fr", "fr", "de"]);
        assert_eq!(tracks[1].kind, TrackKind::Manual);
        assert_eq!(tracks[2].kind, TrackKind::Automatic);
        sort_tracks(&mut tracks, &["xx".into()], "yy", None);
        assert_eq!(tracks.len(), 4);
    }

    #[test]
    fn explicit_language_then_interface_then_original_then_discovery_order() {
        let mut tracks = vec![
            track("ru", TrackKind::Manual),
            track("fr", TrackKind::Manual),
            track("de", TrackKind::Manual),
            track("en", TrackKind::Manual),
            track("ja", TrackKind::Manual),
        ];
        sort_tracks(&mut tracks, &["ja".into()], "en", Some("de"));
        assert_eq!(languages(&tracks), ["ja", "en", "de", "ru", "fr"]);
    }

    #[test]
    fn a_script_preference_prefers_the_variant_without_inventing_a_kind() {
        let mut tracks = vec![
            track("zh", TrackKind::Automatic),
            track("zh-Hans", TrackKind::Unknown),
            track("ja", TrackKind::Manual),
        ];
        for (index, track) in tracks.iter_mut().enumerate() {
            track.source_order = index;
        }
        sort_tracks(&mut tracks, &[], "zh-Hans", Some("ja"));
        assert_eq!(tracks[0].language.as_deref(), Some("zh-Hans"));
        assert_eq!(tracks[0].kind, TrackKind::Unknown);
        assert_eq!(tracks[1].kind, TrackKind::Automatic);
    }

    #[test]
    fn generic_language_preference_does_not_promote_unspecified_script() {
        let mut tracks = vec![
            track("zh-Hans", TrackKind::Unknown),
            track("zh", TrackKind::Automatic),
            track("ja", TrackKind::Manual),
        ];
        for (index, track) in tracks.iter_mut().enumerate() {
            track.source_order = index;
        }
        sort_tracks(&mut tracks, &["zh".into()], "en", None);
        assert_eq!(tracks[0].language.as_deref(), Some("zh-Hans"));
        assert_eq!(tracks[1].language.as_deref(), Some("zh"));
    }

    #[test]
    fn translations_follow_the_original_whatever_the_interface_language() {
        // YouTube for an English video with automatic captions only: yt-dlp lists
        // `en-orig`, `en`, translations of the captions (`zh-Hans`), and with manual
        // subtitles their translations too (`zh-Hans-en`).
        let mut tracks = Vec::new();
        for (index, (language, kind, translated)) in [
            ("zh-Hans-en", TrackKind::Automatic, true),
            ("en-orig", TrackKind::Automatic, false),
            ("ja", TrackKind::Automatic, true),
            ("en", TrackKind::Automatic, false),
            ("zh-Hans", TrackKind::Automatic, true),
        ]
        .into_iter()
        .enumerate()
        {
            let mut track = track(language, kind);
            track.translated = translated;
            track.source_order = index;
            tracks.push(track);
        }
        for interface in ["zh", "zh-Hans", "ja", "en", "fr"] {
            sort_tracks(&mut tracks, &[], interface, Some("en"));
            assert!(
                !tracks[0].translated && !tracks[1].translated,
                "{interface}: {:?}",
                languages(&tracks)
            );
            assert!(tracks[2..].iter().all(|track| track.translated));
        }
        // An explicit preference does not promote a translation either.
        sort_tracks(&mut tracks, &["zh-Hans".into()], "zh", None);
        assert_eq!(languages(&tracks)[..2], ["en-orig", "en"]);
        // Among translations, the usual order holds.
        assert_eq!(languages(&tracks)[2..], ["zh-Hans", "zh-Hans-en", "ja"]);

        // Human captions in the interface language still come first, then the
        // original language's automatic captions, then translations.
        let mut human = track("zh-Hans", TrackKind::Manual);
        human.source_order = 9;
        tracks.push(human);
        sort_tracks(&mut tracks, &[], "zh", Some("en"));
        assert_eq!(tracks[0].kind, TrackKind::Manual);
        assert_eq!(languages(&tracks)[..3], ["zh-Hans", "en-orig", "en"]);
    }

    #[test]
    fn languages_normalize_to_one_spelling() {
        for (written, normal) in [("ZHO", "zh"), (" zh_Hans ", "zh-hans"), ("fre", "fr")] {
            assert_eq!(normalize_language(written), normal);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_wire_format_is_stable() {
        let mut online = track("en", TrackKind::Automatic);
        online.source_order = 2;
        let value = serde_json::to_value(&online).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "id": "en:Automatic",
                "language": "en",
                "name": null,
                "kind": "automatic",
                "origin": {
                    "kind": "online",
                    "language_key": "en",
                    "automatic": true,
                    "inline_text": null,
                },
                "source_order": 2,
            })
        );
        assert_eq!(serde_json::from_value::<Track>(value).unwrap(), online);
        // Only a translation says so; older readers ignore the field.
        let mut translated = online.clone();
        translated.translated = true;
        let value = serde_json::to_value(&translated).unwrap();
        assert_eq!(value["translated"], true);
        assert_eq!(serde_json::from_value::<Track>(value).unwrap(), translated);
        let older: Track = serde_json::from_value(serde_json::json!({
            "id": "a",
            "language": null,
            "name": null,
            "kind": "unknown",
            "origin": {"kind": "embedded", "stream_index": 3, "codec": "mov_text"},
        }))
        .unwrap();
        assert_eq!(older.source_order, 0);
    }
}
