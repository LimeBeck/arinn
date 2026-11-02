// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_music_common::{Track, text};
use serde_json::Value;

/// Parse an audio probe without trusting the filename extension. An attached
/// album cover is not a moving-video stream and does not disqualify the file.
pub fn parse(value: &Value, fallback: &str) -> Option<Track> {
    let streams = value.get("streams")?.as_array()?;
    let audio = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"))?;
    if streams.iter().any(|s| {
        s.get("codec_type").and_then(Value::as_str) == Some("video")
            && s.pointer("/disposition/attached_pic")
                .and_then(Value::as_u64)
                != Some(1)
    }) {
        return None;
    }
    let format = value.get("format").unwrap_or(&Value::Null);
    let tag = |names: &[&str]| -> String {
        for container in [format.get("tags"), audio.get("tags")]
            .into_iter()
            .flatten()
        {
            if let Some(tags) = container.as_object() {
                for name in names {
                    if let Some(v) = tags
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case(name))
                        .and_then(|(_, v)| v.as_str())
                    {
                        let cleaned = text(v);
                        if !cleaned.is_empty() {
                            return cleaned;
                        }
                    }
                }
            }
        }
        String::new()
    };
    let title = tag(&["title"]);
    let artist = tag(&["artist"]);
    let album_artist = tag(&["album_artist", "albumartist"]);
    let duration_us = [format.get("duration"), audio.get("duration")]
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .and_then(|v| v.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        })
        .find(|v| v.is_finite() && *v >= 0.0 && *v <= 31_536_000.0)
        .map(|seconds| (seconds * 1_000_000.0).round() as u64)
        .unwrap_or(0);
    Some(Track {
        title: if title.is_empty() {
            text(fallback)
        } else {
            title
        },
        album: tag(&["album"]),
        // Only the explicit tag: a compilation without one must not split
        // into one album per track artist. Display falls back to `artist`.
        album_artist,
        artist,
        disc: number(&tag(&["disc", "discnumber"])),
        number: number(&tag(&["track", "tracknumber"])),
        duration_us,
        available: true,
        embedded_artwork: streams.iter().any(|s| {
            s.pointer("/disposition/attached_pic")
                .and_then(Value::as_u64)
                == Some(1)
        }),
        ..Track::default()
    })
}

fn number(value: &str) -> u32 {
    value
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|v| *v <= 100_000)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tags_disc_compilations_and_attached_cover_are_not_video() {
        let value = json!({"streams":[{"codec_type":"audio"},{"codec_type":"video","disposition":{"attached_pic":1}}],
            "format":{"duration":"3.125","tags":{"TITLE":"A\n title", "ARTIST":"Solo", "ALBUMARTIST":"Various", "TRACK":"2/12", "DISC":"1/2"}}});
        let track = parse(&value, "fallback").unwrap();
        assert_eq!(track.title, "A title");
        assert_eq!(track.album_artist, "Various");
        assert_eq!(
            (track.number, track.disc, track.duration_us),
            (2, 1, 3_125_000)
        );
        assert!(track.embedded_artwork);
    }
    #[test]
    fn missing_tags_are_unknown_and_bad_durations_never_become_huge_positions() {
        let value = json!({"streams":[{"codec_type":"audio"}],"format":{"duration":"NaN"}});
        let track = parse(&value, "untagged").unwrap();
        assert_eq!(track.title, "untagged");
        assert!(track.artist.is_empty());
        assert_eq!(track.duration_us, 0);
        assert!(parse(&json!({"streams":[{"codec_type":"video"}]}), "bad").is_none());
        assert!(
            parse(
                &json!({"streams":[{"codec_type":"audio"},{"codec_type":"video"}]}),
                "bad"
            )
            .is_none()
        );
    }
}
