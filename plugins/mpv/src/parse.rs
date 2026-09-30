// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! mpv property values to the media contract. Nothing here is logged.

use arinn_protocol::{
    media::{self, BufferedRange, Chapter, MAX_CHAPTERS, MAX_RANGES, MAX_TRACKS, Track},
    validation::{self, ErrorCode},
};
use serde_json::{Map, Value, json};
use std::path::Path;

pub(crate) fn us_from_seconds(value: &Value) -> Option<i64> {
    let seconds = value
        .as_f64()
        .or_else(|| value.as_i64().map(|number| number as f64))?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let micros = seconds * 1_000_000.0;
    if micros >= i64::MAX as f64 {
        Some(i64::MAX)
    } else {
        Some(micros.round() as i64)
    }
}

pub(crate) fn format_seconds(position_us: i64) -> String {
    let seconds = position_us.max(0) as f64 / 1_000_000.0;
    format!("{seconds:.6}")
}

/// A title safe to put on the session. A value that contains a path separator
/// is replaced with the file name, then with a generic label.
pub(crate) fn session_title(path: &Path, media_title: Option<&str>) -> String {
    if let Some(title) = media_title.filter(|title| title_ok(title)) {
        return title.to_string();
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| title_ok(name))
        .unwrap_or("Playback")
        .to_string()
}

fn title_ok(value: &str) -> bool {
    !value.is_empty() && !value.contains(['/', '\\']) && validation::text(value).is_ok()
}

pub(crate) fn loadfile(path: &str, options: &Map<String, Value>, modern: bool) -> Value {
    if modern {
        json!(["loadfile", path, "replace", -1, options])
    } else {
        json!(["loadfile", path, "replace", options])
    }
}

pub(crate) fn options_for(start_us: i64, audio: &str, subtitle: &str) -> Map<String, Value> {
    let mut options = Map::new();
    options.insert("start".into(), json!(format_seconds(start_us)));
    if !audio.is_empty() {
        options.insert("alang".into(), json!(audio));
    }
    if !subtitle.is_empty() {
        options.insert("slang".into(), json!(subtitle));
    }
    options
}

/// Audio and subtitle tracks, plus whether a video track exists.
pub(crate) fn tracks(value: &Value) -> Result<(Vec<Track>, bool), ErrorCode> {
    let Some(items) = value.as_array() else {
        return Ok((Vec::new(), false));
    };
    let mut parsed = Vec::new();
    let mut has_video = false;
    for item in items {
        let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "video" {
            has_video = true;
            continue;
        }
        let (prefix, track_kind) = match kind {
            "audio" => ("a", media::TRACK_AUDIO),
            "sub" => ("s", media::TRACK_SUBTITLE),
            _ => continue,
        };
        let Some(mpv_id) = item
            .get("id")
            .and_then(|v| v.as_i64())
            .filter(|id| *id >= 0)
        else {
            continue;
        };
        let language = item
            .get("lang")
            .and_then(|v| v.as_str())
            .map(language_tag)
            .unwrap_or_default();
        let label = item
            .get("title")
            .and_then(|v| v.as_str())
            .filter(|title| validation::text(title).is_ok())
            .unwrap_or("")
            .to_string();
        let codec = item
            .get("codec")
            .and_then(|v| v.as_str())
            .map(codec_token)
            .unwrap_or_default();
        let channels = item
            .get("demux-channel-count")
            .and_then(|v| v.as_u64())
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or(0);
        parsed.push(Track {
            id: format!("{prefix}{mpv_id}"),
            kind: track_kind.to_string(),
            language,
            label,
            codec,
            channels,
            selected: item
                .get("selected")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            external: item
                .get("external")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
        if parsed.len() == MAX_TRACKS {
            break;
        }
    }
    keep_one_selected(&mut parsed, media::TRACK_AUDIO);
    keep_one_selected(&mut parsed, media::TRACK_SUBTITLE);
    media::validate_tracks(&parsed)?;
    Ok((parsed, has_video))
}

fn keep_one_selected(tracks: &mut [Track], kind: &str) {
    let mut seen = false;
    for track in tracks.iter_mut() {
        if track.kind != kind || !track.selected {
            continue;
        }
        if seen {
            track.selected = false;
        }
        seen = true;
    }
}

fn language_tag(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    if media::language_tag(&normalized).is_ok() {
        normalized
    } else {
        String::new()
    }
}

fn codec_token(value: &str) -> String {
    let token = value.trim().to_ascii_lowercase();
    if token.is_empty() {
        return String::new();
    }
    if token.len() <= 32
        && token.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        token
    } else {
        String::new()
    }
}

pub(crate) fn chapters(value: &Value) -> Vec<Chapter> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let start_us = us_from_seconds(item.get("time")?)?;
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .filter(|title| validation::text(title).is_ok())
                .unwrap_or("");
            Some(Chapter {
                id: format!("c{index}"),
                start_us,
                title: title.to_string(),
            })
        })
        .take(MAX_CHAPTERS)
        .collect()
}

pub(crate) fn ranges(value: &Value) -> Vec<BufferedRange> {
    let Some(items) = value.get("seekable-ranges").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let start_us = us_from_seconds(item.get("start")?)?;
            let end_us = us_from_seconds(item.get("end")?)?;
            let range = BufferedRange { start_us, end_us };
            range.validate().is_ok().then_some(range)
        })
        .take(MAX_RANGES)
        .collect()
}

/// `a12` is audio track 12, `s3` is subtitle track 3.
pub(crate) fn track_property(id: &str) -> Option<(&'static str, i64)> {
    let (property, rest) = if let Some(rest) = id.strip_prefix('a') {
        ("aid", rest)
    } else {
        let rest = id.strip_prefix('s')?;
        ("sid", rest)
    };
    let number: i64 = rest.parse().ok()?;
    (number >= 0).then_some((property, number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_drop_paths() {
        let path = Path::new("/library/films/clip.mkv");
        assert_eq!(session_title(path, Some("Secret Title")), "Secret Title");
        assert_eq!(
            session_title(path, Some("/library/films/clip.mkv")),
            "clip.mkv"
        );
        assert_eq!(session_title(Path::new("/"), None), "Playback");
    }

    #[test]
    fn tracks_keep_one_selection_and_notice_video() {
        let value = json!([
            {"id": 1, "type": "video", "selected": true, "codec": "h264"},
            {"id": 1, "type": "audio", "lang": "en_US", "title": "Stereo", "codec": "aac",
             "demux-channel-count": 2, "selected": true, "external": false},
            {"id": 2, "type": "audio", "lang": "jpn", "title": "Extra", "codec": "aac",
             "demux-channel-count": 2, "selected": true, "external": false},
            {"id": 1, "type": "sub", "lang": "eng", "title": "English", "codec": "subrip",
             "selected": false, "external": true}
        ]);
        let (tracks, has_video) = tracks(&value).unwrap();
        assert!(has_video);
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0].id, "a1");
        assert_eq!(tracks[0].language, "en-us");
        assert!(tracks[0].selected);
        assert!(!tracks[1].selected);
        assert!(tracks[2].external);
        assert_eq!(track_property("a1"), Some(("aid", 1)));
        assert_eq!(track_property("s1"), Some(("sid", 1)));
    }

    #[test]
    fn chapters_and_ranges_skip_bad_values() {
        let chapters = chapters(&json!([
            {"title": "Intro", "time": 0},
            {"title": "Next", "time": 12.5},
            {"title": "Bad", "time": -1}
        ]));
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[1].start_us, 12_500_000);
        let ranges = ranges(&json!({"seekable-ranges": [
            {"start": 0, "end": 15.25},
            {"start": 8, "end": 3}
        ]}));
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].end_us, 15_250_000);
    }
}
