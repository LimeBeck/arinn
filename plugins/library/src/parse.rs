// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! File-name and sidecar NFO parsing (ADR-0056).
//! NFO is read as text. External entities and DTDs are not resolved.

use arinn_protocol::validation;
use std::{
    fs,
    path::{Path, PathBuf},
};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "webm", "mpg", "mpeg",
];
const NFO_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Film,
    Episode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub kind: Kind,
    pub title: String,
    pub series_title: String,
    pub series_dir: String,
    pub series_key: String,
    pub has_year: bool,
    pub year: i32,
    pub has_season: bool,
    pub season: u32,
    pub has_episode: bool,
    pub episode: u32,
    pub series_has_year: bool,
    pub series_year: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nfo {
    pub title: Option<String>,
    pub year: Option<i32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

pub fn is_video(path: &Path) -> bool {
    extension(path).is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext.as_str()))
}

pub fn parse_video(root: &Path, path: &Path) -> Option<Parsed> {
    if !is_video(path) {
        return None;
    }
    let rel = path.strip_prefix(root).ok()?;
    let stem = path.file_stem()?.to_str()?;
    if stem.is_empty() || stem.starts_with('.') {
        return None;
    }
    let dir = series_dir(rel);
    let season_from_dir = dir.season;
    let cleaned = clean(stem);
    let (mut text, mut year) = take_year(&cleaned);
    let mut marker = find_marker(&text).map(|(s, e, start, end)| (s, Some(e), start, end));
    if marker.is_none() {
        // Anime names (ADR-0083) keep a `TVn` season or the episode number
        // in `[...]` groups, which the plain cleaning drops.
        let (alt, alt_year) = take_year(&clean(&unwrap_markers(stem)));
        marker = tv_marker(&alt);
        if marker.is_some() || season_from_dir.is_some() {
            text = alt;
            year = alt_year;
        }
    }
    let without_year = text;
    let episode = marker.is_some() || season_from_dir.is_some();
    if !episode {
        let title = fallback_title(&without_year, stem);
        return Some(film(title, year));
    }
    let (season, episode_no, before, after) = match marker {
        Some((season, episode_no, start, end)) => {
            let before = trim_seps(&without_year[..start]);
            let after = trim_seps(&without_year[end..]);
            (Some(season), episode_no, before, after)
        }
        // In a season folder the episode number may stand alone (ADR-0083).
        None => match loose_episode(&without_year) {
            Some((episode_no, start, end)) => (
                season_from_dir,
                Some(episode_no),
                trim_seps(&without_year[..start]),
                trim_seps(&without_year[end..]),
            ),
            None => (season_from_dir, None, without_year.clone(), String::new()),
        },
    };
    let season = season.or(season_from_dir);
    let series_dir = dir.path;
    let (series_title, series_year) = match dir.title {
        Some((title, year)) => (limit_text(&title), year),
        None => series_title(&series_dir, &before, stem),
    };
    let episode_title = if after.is_empty() {
        fallback_title(&before, stem)
    } else {
        fallback_title(&after, stem)
    };
    let series_key = if series_dir.is_empty() {
        format!("name:{}", normalize_key(&series_title))
    } else {
        format!("dir:{series_dir}")
    };
    Some(Parsed {
        kind: Kind::Episode,
        title: episode_title,
        series_title,
        series_dir,
        series_key,
        has_year: year.is_some(),
        year: year.unwrap_or(0),
        has_season: season.is_some(),
        season: season.unwrap_or(0),
        has_episode: episode_no.is_some(),
        episode: episode_no.unwrap_or(0),
        series_has_year: series_year.is_some(),
        series_year: series_year.unwrap_or(0),
    })
}

fn film(title: String, year: Option<i32>) -> Parsed {
    Parsed {
        kind: Kind::Film,
        title,
        series_title: String::new(),
        series_dir: String::new(),
        series_key: String::new(),
        has_year: year.is_some(),
        year: year.unwrap_or(0),
        has_season: false,
        season: 0,
        has_episode: false,
        episode: 0,
        series_has_year: false,
        series_year: 0,
    }
}

fn series_title(series_dir: &str, before: &str, stem: &str) -> (String, Option<i32>) {
    if let Some(name) = series_dir
        .split('/')
        .next_back()
        .filter(|part| !part.is_empty())
    {
        let (title, year) = take_year(&clean(name));
        return (fallback_title(&title, name), year);
    }
    let (title, year) = take_year(before);
    (fallback_title(&title, stem), year)
}

/// Where a video's series lives. `path` is the series directory relative to
/// the root, or empty; `title` is set by a `<Title> TVn` season folder, which
/// names the series without being its directory (ADR-0083).
#[derive(Default)]
struct SeriesDir {
    path: String,
    season: Option<u32>,
    title: Option<(String, Option<i32>)>,
}

fn series_dir(rel: &Path) -> SeriesDir {
    let mut parts = Vec::new();
    for component in rel.parent().unwrap_or(Path::new("")).components() {
        let Some(name) = component.as_os_str().to_str() else {
            return SeriesDir::default();
        };
        if name.starts_with('.') {
            return SeriesDir::default();
        }
        if let Some(season) = season_dir(name) {
            return SeriesDir {
                path: parts.join("/"),
                season: Some(season),
                title: None,
            };
        }
        if let Some((title, year, season)) = titled_season_dir(name) {
            return SeriesDir {
                path: String::new(),
                season: Some(season),
                title: Some((title, year)),
            };
        }
        parts.push(name.to_string());
    }
    SeriesDir {
        path: parts.join("/"),
        ..SeriesDir::default()
    }
}

/// `Season N`, or a folder that is only a `TVn` marker.
fn season_dir(name: &str) -> Option<u32> {
    let cleaned = clean(&unwrap_markers(name));
    if let Some((season, 0, end)) = find_tv(&cleaned)
        && end == cleaned.len()
    {
        return Some(season);
    }
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix("season")?.trim_start();
    let rest = rest
        .strip_prefix([' ', '.', '_', '-'])
        .unwrap_or(rest)
        .trim();
    index_token(rest)
}

/// `<Title> TVn` (an optional year anywhere): season `n` of `<Title>`.
fn titled_season_dir(name: &str) -> Option<(String, Option<i32>, u32)> {
    let (text, year) = take_year(&clean(&unwrap_markers(name)));
    let (season, start, end) = find_tv(&text)?;
    let title = trim_seps(&text[..start]);
    if title.is_empty() || !trim_seps(&text[end..]).is_empty() {
        return None;
    }
    Some((title, year, season))
}

pub fn read_nfo(path: &Path, root: &Path) -> Option<Nfo> {
    let root = root.canonicalize().ok()?;
    let canon = path.canonicalize().ok()?;
    if !canon.starts_with(&root) {
        return None;
    }
    let metadata = fs::metadata(&canon).ok()?;
    if !metadata.is_file() || metadata.len() > NFO_LIMIT {
        return None;
    }
    let text = fs::read_to_string(&canon).ok()?;
    parse_nfo(&text)
}

pub fn parse_nfo(text: &str) -> Option<Nfo> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return None;
    }
    let stripped = strip_comments_and_declarations(text);
    let title = element_text(&stripped, "title").filter(|value| !value.is_empty());
    let year = element_text(&stripped, "year")
        .and_then(|value| year_token(&value))
        .or_else(|| {
            element_text(&stripped, "premiered")
                .or_else(|| element_text(&stripped, "aired"))
                .and_then(|value| year_token(&value))
        });
    Some(Nfo {
        title,
        year,
        season: element_text(&stripped, "season").and_then(|value| index_token(value.trim())),
        episode: element_text(&stripped, "episode").and_then(|value| index_token(value.trim())),
    })
}

pub fn apply_nfo(parsed: &mut Parsed, nfo: &Nfo) {
    if let Some(title) = nfo.title.clone() {
        parsed.title = limit_text(&title);
    }
    if let Some(year) = nfo.year {
        parsed.has_year = true;
        parsed.year = year;
    }
    if let Some(season) = nfo.season {
        parsed.has_season = true;
        parsed.season = season;
    }
    if let Some(episode) = nfo.episode {
        parsed.has_episode = true;
        parsed.episode = episode;
    }
}

pub fn artwork(root: &Path, directory: &Path, stem: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    (
        find_art(
            root,
            directory,
            stem,
            &["-poster.jpg", "-poster.png"],
            &["poster.jpg", "poster.png", "folder.jpg", "cover.jpg"],
        ),
        find_art(
            root,
            directory,
            stem,
            &["-fanart.jpg", "-fanart.png"],
            &["fanart.jpg", "fanart.png", "backdrop.jpg", "backdrop.png"],
        ),
    )
}

fn find_art(
    root: &Path,
    directory: &Path,
    stem: &str,
    prefixed: &[&str],
    plain: &[&str],
) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        names.push((name.to_ascii_lowercase(), entry.path()));
    }
    for suffix in prefixed {
        let want = format!("{}{suffix}", stem.to_ascii_lowercase());
        if let Some((_, path)) = names.iter().find(|(name, _)| name == &want)
            && art_inside(root, path)
        {
            return Some(path.clone());
        }
    }
    for candidate in plain {
        if let Some((_, path)) = names.iter().find(|(name, _)| name == candidate)
            && art_inside(root, path)
        {
            return Some(path.clone());
        }
    }
    None
}

fn art_inside(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(canon) = path.canonicalize() else {
        return false;
    };
    if !canon.starts_with(root) {
        return false;
    }
    fs::metadata(canon).is_ok_and(|meta| meta.is_file())
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn clean(raw: &str) -> String {
    let spaced = raw.replace(['.', '_'], " ");
    collapse(&strip_square(&spaced))
}

fn strip_square(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in input.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn collapse(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn take_year(input: &str) -> (String, Option<i32>) {
    let chars: Vec<char> = input.chars().collect();
    let mut found = None;
    let mut index = 0;
    while index + 5 < chars.len() {
        if chars[index] == '('
            && chars[index + 5] == ')'
            && chars[index + 1..index + 5]
                .iter()
                .all(|ch| ch.is_ascii_digit())
        {
            let year: i32 = chars[index + 1..index + 5]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            if (1888..=2099).contains(&year) {
                found = Some((index, index + 6, year));
            }
        }
        index += 1;
    }
    if let Some((start, end, year)) = found {
        let mut title = String::new();
        title.extend(chars[..start].iter());
        title.extend(chars[end..].iter());
        return (collapse(&title), Some(year));
    }
    let collapsed = collapse(input);
    if let Some((rest, year)) = trailing_year(&collapsed) {
        return (rest, Some(year));
    }
    (collapsed, None)
}

fn trailing_year(input: &str) -> Option<(String, i32)> {
    let (rest, token) = input.rsplit_once(' ')?;
    let year = year_token(token)?;
    if rest.is_empty() {
        None
    } else {
        Some((rest.to_string(), year))
    }
}

fn year_token(token: &str) -> Option<i32> {
    let digits = token.get(..4)?;
    if token.len() > 4 && !token[4..].starts_with('-') {
        return None;
    }
    if digits.len() != 4 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let year = digits.parse().ok()?;
    (1888..=2099).contains(&year).then_some(year)
}

fn find_marker(input: &str) -> Option<(u32, u32, usize, usize)> {
    let lower = input.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b's'
            && boundary_before(&lower, index)
            && let Some((season, episode, len)) = parse_sxe(&lower[index + 1..])
        {
            let end = index + 1 + len;
            if boundary_after(&lower, end) {
                return Some((season, episode, index, end));
            }
        }
        index += 1;
    }
    index = 0;
    while index < bytes.len() {
        if let Some((season, episode, len)) = parse_x(&lower[index..])
            && boundary_before(&lower, index)
            && boundary_after(&lower, index + len)
        {
            return Some((season, episode, index, index + len));
        }
        index += 1;
    }
    None
}

/// A standalone `TVn` season marker (`TV1`, `TV-2`, `tv 3`; `.` and `_` are
/// spaces by now): season, start and end. `MTV2` and `TV 2000` are not.
fn find_tv(input: &str) -> Option<(u32, usize, usize)> {
    let lower = input.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] != b't' || !boundary_before(&lower, index) {
            continue;
        }
        let Some(rest) = lower[index..].strip_prefix("tv") else {
            continue;
        };
        let rest = rest.as_bytes();
        let gap = usize::from(matches!(rest.first(), Some(b' ' | b'-')));
        let Some((season, digits)) = take_digits(&rest[gap..], 1, 2) else {
            continue;
        };
        let end = index + 2 + gap + digits;
        if boundary_after(&lower, end)
            && let Some(season) = index_token(&season.to_string())
        {
            return Some((season, index, end));
        }
    }
    None
}

/// A `TVn` marker and the episode number right after it, if any.
fn tv_marker(input: &str) -> Option<(u32, Option<u32>, usize, usize)> {
    let (season, start, end) = find_tv(input)?;
    match lead_episode(&input[end..], false) {
        Some((episode, len)) => Some((season, Some(episode), start, end + len)),
        None => Some((season, None, start, end)),
    }
}

/// An episode number at the start of `text`, after spaces and dashes: `05`,
/// `E05`, `EP 05`, `Episode 5` or `#05`, with an optional `v2` revision.
/// `prefixed` requires the letter or `#` form. Returns the episode and the
/// bytes consumed, which end at a word boundary.
fn lead_episode(text: &str, prefixed: bool) -> Option<(u32, usize)> {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut index = 0;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'-') {
        index += 1;
    }
    let mut has_prefix = false;
    for prefix in ["episode", "ep", "e", "#"] {
        if !lower[index..].starts_with(prefix) {
            continue;
        }
        let mut next = index + prefix.len();
        if prefix != "#" && bytes.get(next) == Some(&b' ') {
            next += 1;
        }
        if bytes.get(next).is_some_and(u8::is_ascii_digit) {
            index = next;
            has_prefix = true;
            break;
        }
    }
    if prefixed && !has_prefix {
        return None;
    }
    let (episode, digits) = take_digits(&bytes[index..], 1, 4)?;
    let mut end = index + digits;
    if bytes.get(end) == Some(&b'v') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if !boundary_after(&lower, end) {
        return None;
    }
    Some((index_token(&episode.to_string())?, end))
}

/// The episode number of a name inside a season folder that carries no
/// marker: the whole name, then ` - 05`, then `E05`/`EP05`/`#05`, then a
/// trailing number of up to three digits (`Show 05`).
fn loose_episode(text: &str) -> Option<(u32, usize, usize)> {
    if let Some((episode, len)) = lead_episode(text, false)
        && trim_seps(&text[len..]).is_empty()
    {
        return Some((episode, 0, len));
    }
    for (index, _) in text.match_indices(" - ") {
        if let Some((episode, len)) = lead_episode(&text[index..], false) {
            return Some((episode, index, index + len));
        }
    }
    let lower = text.to_ascii_lowercase();
    for index in 0..text.len() {
        if text.is_char_boundary(index)
            && boundary_before(&lower, index)
            && !lower.as_bytes()[index].is_ascii_whitespace()
            && let Some((episode, len)) = lead_episode(&text[index..], true)
        {
            return Some((episode, index, index + len));
        }
    }
    let (_, last) = text.rsplit_once(' ')?;
    let start = text.len() - last.len();
    let digits = last.bytes().filter(u8::is_ascii_digit).count();
    match lead_episode(last, false) {
        Some((episode, len)) if len == last.len() && digits <= 3 => {
            Some((episode, start, text.len()))
        }
        _ => None,
    }
}

/// Opens the `[...]` groups that hold an anime marker so cleaning keeps it:
/// `[07]`, `[07v2]` become ` - 07`; `[TV-2]` becomes `TV-2`. Other groups
/// (release group, resolution, checksum) stay and are dropped later.
fn unwrap_markers(stem: &str) -> String {
    let mut out = String::new();
    let mut rest = stem;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find(']').map(|at| open + at) else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = clean(&rest[open + 1..close]);
        let digits = token.bytes().filter(u8::is_ascii_digit).count();
        if digits <= 3 && lead_episode(&token, false).is_some_and(|(_, len)| len == token.len()) {
            out.push_str(" - ");
            out.push_str(&token);
            out.push(' ');
        } else if find_tv(&token).is_some_and(|(_, start, end)| start == 0 && end == token.len()) {
            out.push(' ');
            out.push_str(&token);
            out.push(' ');
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

fn boundary_before(value: &str, index: usize) -> bool {
    index == 0 || !value.as_bytes()[index - 1].is_ascii_alphanumeric()
}

fn boundary_after(value: &str, index: usize) -> bool {
    index >= value.len() || !value.as_bytes()[index].is_ascii_alphanumeric()
}

fn parse_sxe(rest: &str) -> Option<(u32, u32, usize)> {
    let bytes = rest.as_bytes();
    let (season, n1) = take_digits(bytes, 1, 4)?;
    let mut index = n1;
    if index < bytes.len() && bytes[index] == b' ' {
        index += 1;
    }
    if index >= bytes.len() || bytes[index] != b'e' {
        return None;
    }
    index += 1;
    let (episode, n2) = take_digits(&bytes[index..], 1, 4)?;
    let season = index_token(&season.to_string())?;
    let episode = index_token(&episode.to_string())?;
    Some((season, episode, index + n2))
}

fn parse_x(rest: &str) -> Option<(u32, u32, usize)> {
    let bytes = rest.as_bytes();
    let (season, n1) = take_digits(bytes, 1, 2)?;
    if n1 >= bytes.len() || bytes[n1] != b'x' {
        return None;
    }
    let (episode, n2) = take_digits(&bytes[n1 + 1..], 1, 4)?;
    let season = index_token(&season.to_string())?;
    let episode = index_token(&episode.to_string())?;
    Some((season, episode, n1 + 1 + n2))
}

fn take_digits(bytes: &[u8], min: usize, max: usize) -> Option<(u32, usize)> {
    let mut len = 0;
    while len < bytes.len() && len < max && bytes[len].is_ascii_digit() {
        len += 1;
    }
    if len < min {
        return None;
    }
    let text = std::str::from_utf8(&bytes[..len]).ok()?;
    Some((text.parse().ok()?, len))
}

fn index_token(token: &str) -> Option<u32> {
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let value = token.parse().ok()?;
    (1..10_000).contains(&value).then_some(value)
}

fn trim_seps(value: &str) -> String {
    let mut text = value.trim();
    loop {
        let next = text
            .strip_prefix('-')
            .or_else(|| text.strip_prefix('|'))
            .unwrap_or(text)
            .trim();
        let next = next
            .strip_suffix('-')
            .or_else(|| next.strip_suffix('|'))
            .unwrap_or(next)
            .trim();
        if next == text {
            break;
        }
        text = next;
    }
    text.to_string()
}

fn fallback_title(parsed: &str, stem: &str) -> String {
    let title = if parsed.is_empty() {
        clean(stem)
    } else {
        parsed.to_string()
    };
    let title = limit_text(&title);
    if title.is_empty() {
        limit_text(stem)
    } else {
        title
    }
}

fn normalize_key(title: &str) -> String {
    collapse(&title.to_lowercase())
}

pub fn limit_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch == '\0' {
            continue;
        }
        if out.len() + ch.len_utf8() > validation::MAX_TEXT_BYTES {
            break;
        }
        out.push(ch);
    }
    out
}

fn strip_comments_and_declarations(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix("<!--") {
            let end = body.find("-->").map_or(body.len(), |index| index + 3);
            rest = &body[end..];
            continue;
        }
        if let Some(body) = rest.strip_prefix("<?") {
            let end = body.find("?>").map_or(body.len(), |index| index + 2);
            rest = &body[end..];
            continue;
        }
        let Some(next) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..next]);
        rest = &rest[next..];
        if rest.starts_with('<') && !rest.starts_with("<!--") && !rest.starts_with("<?") {
            let end = rest.find('>').map_or(rest.len(), |index| index + 1);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        }
    }
    out
}

fn element_text(xml: &str, name: &str) -> Option<String> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{name}");
    let mut search = lower.as_str();
    let mut offset = 0;
    while let Some(index) = search.find(&open) {
        let absolute = offset + index;
        let after = absolute + open.len();
        let boundary = xml.as_bytes().get(after).copied();
        if matches!(boundary, Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r')) {
            let tail = &xml[after..];
            let close_at = tail.find('>')?;
            if tail.as_bytes().get(close_at.saturating_sub(1)) == Some(&b'/') {
                return Some(String::new());
            }
            let body = &tail[close_at + 1..];
            let end_tag = format!("</{name}>");
            let end = body.to_ascii_lowercase().find(&end_tag)?;
            return Some(decode_entities(&strip_tags(&body[..end])));
        }
        offset = after;
        search = &lower[offset..];
    }
    None
}

fn strip_tags(input: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in input.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    collapse(&out)
}

fn decode_entities(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let token = &rest[..end];
        rest = &rest[end + 1..];
        match token {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {}
        }
    }
    out.push_str(rest);
    limit_text(out.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_film_year_and_an_episode() {
        let root = Path::new("/library");
        let film = parse_video(root, Path::new("/library/The Matrix (1999).mkv")).unwrap();
        assert_eq!(film.kind, Kind::Film);
        assert_eq!(film.title, "The Matrix");
        assert!(film.has_year);
        assert_eq!(film.year, 1999);

        let episode = parse_video(
            root,
            Path::new("/library/Show Name/Season 01/Show Name - S01E02 - Pilot.mkv"),
        )
        .unwrap();
        assert_eq!(episode.kind, Kind::Episode);
        assert_eq!(episode.series_dir, "Show Name");
        assert_eq!(episode.series_title, "Show Name");
        assert_eq!(episode.season, 1);
        assert_eq!(episode.episode, 2);
        assert_eq!(episode.title, "Pilot");
    }

    #[test]
    fn nfo_overrides_fields_and_refuses_external_entities() {
        let mut parsed = parse_video(
            Path::new("/library"),
            Path::new("/library/Wrong (1990).mkv"),
        )
        .unwrap();
        let nfo = parse_nfo("<movie><title>A &amp; B</title><year>2004</year></movie>").unwrap();
        apply_nfo(&mut parsed, &nfo);
        assert_eq!(parsed.title, "A & B");
        assert_eq!(parsed.year, 2004);

        let secret = "secret-from-external-entity";
        let hostile = format!(
            "<!DOCTYPE movie [<!ENTITY xxe SYSTEM \"file:///tmp/{secret}\">]><movie><title>&xxe;</title><year>1999</year></movie>"
        );
        assert!(parse_nfo(&hostile).is_none());
        let unresolved = parse_nfo("<movie><title>&xxe;</title></movie>").unwrap();
        assert_eq!(unresolved.title, None);
    }

    fn episode(path: &str) -> Parsed {
        let parsed = parse_video(Path::new("/library"), Path::new(path)).unwrap();
        assert_eq!(parsed.kind, Kind::Episode, "{path}");
        parsed
    }

    // ADR-0083: anime `TVn` seasons in the file name.
    #[test]
    fn tv_marker_in_the_file_name_is_a_season() {
        for (path, season, number) in [
            ("/library/Show/[Group] Show TV2 - 05 [1080p].mkv", 2, 5),
            ("/library/Show/Show.TV-2.06.mkv", 2, 6),
            ("/library/Show/Show TV2 [07].mkv", 2, 7),
            ("/library/Show/Show_tv_4_E08v2.mkv", 4, 8),
            ("/library/Show/Show [TV-3] - 09.mkv", 3, 9),
        ] {
            let parsed = episode(path);
            assert_eq!(parsed.series_title, "Show", "{path}");
            assert_eq!(parsed.series_key, "dir:Show", "{path}");
            assert_eq!((parsed.season, parsed.episode), (season, number), "{path}");
        }
        // Without a folder the series is the text before the marker.
        let root = episode("/library/Show TV1 - 03 - Arrival.mkv");
        assert_eq!(root.series_title, "Show");
        assert_eq!(root.series_key, "name:show");
        assert_eq!((root.season, root.episode), (1, 3));
        assert_eq!(root.title, "Arrival");
        // A season without an episode number keeps the number unset.
        let bare = episode("/library/Show/Show TV2 1080p.mkv");
        assert_eq!(bare.season, 2);
        assert!(!bare.has_episode);
    }

    // ADR-0083: `TVn` and `<Title> TVn` folders are season folders.
    #[test]
    fn tv_folders_are_seasons_of_one_series() {
        let first = episode("/library/Show TV1/01.mkv");
        let second = episode("/library/Anime/Show TV2/[Group] Show - 02 [1080p].mkv");
        for (parsed, season, number) in [(&first, 1, 1), (&second, 2, 2)] {
            assert_eq!(parsed.series_title, "Show");
            assert_eq!(parsed.series_key, "name:show");
            assert_eq!((parsed.season, parsed.episode), (season, number));
        }
        let nested = episode("/library/Show/TV2/Show - 01.mkv");
        assert_eq!(nested.series_key, "dir:Show");
        assert_eq!(nested.series_title, "Show");
        assert_eq!((nested.season, nested.episode), (2, 1));
        let numbered = episode("/library/Show/TV1/Show 05.mkv");
        assert_eq!((numbered.season, numbered.episode), (1, 5));
        // The same loose numbering applies in a `Season N` folder.
        let classic = episode("/library/Show/Season 1/Show - 03.mkv");
        assert_eq!((classic.season, classic.episode), (1, 3));
        let titled = episode("/library/Show/Season 1/Pilot.mkv");
        assert!(!titled.has_episode);
        assert_eq!(titled.title, "Pilot");
    }

    #[test]
    fn tv_lookalikes_stay_films() {
        for (path, title) in [
            ("/library/MTV2 Live.mkv", "MTV2 Live"),
            ("/library/The TV 2000 Special.mkv", "The TV 2000 Special"),
            ("/library/Movie [01].mkv", "Movie"),
            ("/library/Films/TVs Best.mkv", "TVs Best"),
        ] {
            let parsed = parse_video(Path::new("/library"), Path::new(path)).unwrap();
            assert_eq!(parsed.kind, Kind::Film, "{path}");
            assert_eq!(parsed.title, title, "{path}");
        }
    }

    #[test]
    fn a_resolution_token_is_not_a_year() {
        let film =
            parse_video(Path::new("/library"), Path::new("/library/Movie.1080p.mkv")).unwrap();
        assert!(!film.has_year);
        assert_eq!(film.title, "Movie 1080p");
    }
}
