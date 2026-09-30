// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! SQLite catalogue, resume and import state. The database is the provider's.
//! Paths and titles stay in this file's rows and are not written to logs.

use crate::{
    parse::{self, Kind, Parsed},
    progress::{self, Progress},
    uri,
};
use arinn_protocol::{
    library::{self, Collection, ImportCandidate, Item, Resume},
    media::{self, PlaybackSource},
    validation::ErrorCode,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const MAX_ROOTS: usize = 64;
const MAX_FILES: usize = 50_000;
const MAX_DEPTH: usize = 64;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct Settings {
    pub autoscan: bool,
    pub roots: Vec<PathBuf>,
    pub probe: bool,
    pub ffprobe: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autoscan: true,
            roots: Vec::new(),
            probe: true,
            ffprobe: PathBuf::from("ffprobe"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default = "yes")]
    autoscan: bool,
    #[serde(default = "yes")]
    probe: bool,
    #[serde(default)]
    ffprobe: Option<String>,
    #[serde(default)]
    roots: Vec<RootFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootFile {
    path: String,
}

fn yes() -> bool {
    true
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, ErrorCode> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(ErrorCode::Unavailable),
        };
        if text.len() > 256 * 1024 {
            return Err(ErrorCode::InvalidArgument);
        }
        let file: ConfigFile = toml::from_str(&text).map_err(|_| ErrorCode::InvalidArgument)?;
        if file.roots.len() > MAX_ROOTS {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut roots = Vec::new();
        for root in file.roots {
            let path = normalize_root(Path::new(&root.path))?;
            if !roots.contains(&path) {
                roots.push(path);
            }
        }
        let ffprobe = match file.ffprobe {
            None => PathBuf::from("ffprobe"),
            Some(value) if value == "ffprobe" || Path::new(&value).is_absolute() => {
                PathBuf::from(value)
            }
            Some(_) => return Err(ErrorCode::InvalidArgument),
        };
        Ok(Self {
            autoscan: file.autoscan,
            roots,
            probe: file.probe,
            ffprobe,
        })
    }
}

pub struct Store {
    conn: Connection,
}

#[derive(Clone)]
struct Row {
    root_id: String,
    id: String,
    rel_path: String,
    kind: String,
    series_key: String,
    parent_id: String,
    title: String,
    has_year: bool,
    year: i32,
    has_season: bool,
    season: u32,
    has_episode: bool,
    episode: u32,
    duration_us: i64,
    audio: String,
    subtitle: String,
    poster_id: String,
    fanart_id: String,
    available: bool,
    inode: Option<i64>,
    size: Option<i64>,
    mtime_sec: Option<i64>,
    mtime_nsec: Option<i64>,
    probed: bool,
}

struct DiskFile {
    rel: String,
    absolute: PathBuf,
    parsed: Parsed,
    inode: Option<i64>,
    size: Option<i64>,
    mtime_sec: i64,
    mtime_nsec: i64,
    has_poster: bool,
    has_fanart: bool,
    fresh_probe: Option<(i64, String, String)>,
}

struct SeriesMeta {
    title: String,
    has_year: bool,
    year: i32,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, ErrorCode> {
        if let Some(parent) = path.parent() {
            crate::paths::private_dir(parent)?;
        }
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(ErrorCode::PermissionDenied);
        }
        let conn = Connection::open(path).map_err(|_| ErrorCode::Internal)?;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS collections (
               kind TEXT PRIMARY KEY,
               id TEXT NOT NULL,
               title TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS roots (
               id TEXT PRIMARY KEY,
               path TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS items (
               id TEXT PRIMARY KEY,
               root_id TEXT NOT NULL,
               rel_path TEXT NOT NULL,
               kind TEXT NOT NULL,
               series_key TEXT NOT NULL DEFAULT '',
               parent_id TEXT NOT NULL DEFAULT '',
               title TEXT NOT NULL,
               has_year INTEGER NOT NULL DEFAULT 0,
               year INTEGER NOT NULL DEFAULT 0,
               has_season INTEGER NOT NULL DEFAULT 0,
               season INTEGER NOT NULL DEFAULT 0,
               has_episode INTEGER NOT NULL DEFAULT 0,
               episode INTEGER NOT NULL DEFAULT 0,
               duration_us INTEGER NOT NULL DEFAULT -1,
               audio_language TEXT NOT NULL DEFAULT '',
               subtitle_language TEXT NOT NULL DEFAULT '',
               poster_id TEXT NOT NULL DEFAULT '',
               fanart_id TEXT NOT NULL DEFAULT '',
               available INTEGER NOT NULL DEFAULT 0,
               inode INTEGER,
               size INTEGER,
               mtime_sec INTEGER,
               mtime_nsec INTEGER,
               probed INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS resumes (
               content_id TEXT PRIMARY KEY,
               position_us INTEGER NOT NULL,
               duration_us INTEGER NOT NULL,
               completed INTEGER NOT NULL,
               observed_at_us INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS imports (
               completion_id TEXT PRIMARY KEY,
               item_id TEXT NOT NULL
             );",
        )
        .map_err(|_| ErrorCode::Internal)?;
        seed_collections(&conn)?;
        Ok(Self { conn })
    }

    pub fn scan(&mut self, settings: &Settings) -> Result<(), ErrorCode> {
        let mut known = HashMap::new();
        let configured = settings.roots.clone();
        for path in &configured {
            let id = self.root_id(path)?;
            known.insert(id, path.clone());
        }
        let stored = self
            .conn
            .prepare("SELECT id, path FROM roots")
            .map_err(|_| ErrorCode::Internal)?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)?;
        for (id, path) in &stored {
            if !configured.iter().any(|root| root == Path::new(path)) {
                self.mark_root_unavailable(id)?;
            }
        }
        for (id, path) in known {
            if root_readable(&path) {
                self.scan_root(&id, &path, settings, &configured)?;
            } else {
                self.mark_root_unavailable(&id)?;
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Result<(Vec<Collection>, Vec<Item>), ErrorCode> {
        let collections = self.collections()?;
        let mut rows = self.load_all()?;
        rows.sort_by(|a, b| {
            kind_order(&a.kind)
                .cmp(&kind_order(&b.kind))
                .then(a.title.cmp(&b.title))
                .then(a.id.cmp(&b.id))
        });
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(row_item(&row, &collections)?);
        }
        library::validate_entries(&collections, &items)?;
        Ok((collections, items))
    }

    pub fn resumes(&self) -> Result<Vec<Resume>, ErrorCode> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT content_id, position_us, duration_us, completed, observed_at_us
                 FROM resumes ORDER BY content_id",
            )
            .map_err(|_| ErrorCode::Internal)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Resume {
                    content_id: row.get(0)?,
                    position_us: row.get(1)?,
                    duration_us: row.get(2)?,
                    completed: row.get::<_, i64>(3)? != 0,
                    observed_at_us: row.get::<_, i64>(4)? as u64,
                })
            })
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)?;
        library::validate_resumes(&rows)?;
        Ok(rows)
    }

    pub fn launch(&self, id: &str) -> Result<PlaybackSource, ErrorCode> {
        let row = self.item_row(id)?.ok_or(ErrorCode::NotFound)?;
        if !row.available || row.kind == "series" {
            return Err(ErrorCode::Unavailable);
        }
        let root = self.root_path_of(&row)?;
        let path = root.join(&row.rel_path);
        if !path.is_file() || !contained(&root, &path) {
            return Err(ErrorCode::Unavailable);
        }
        let resume = self.resume_row(id)?;
        let mut start = 0;
        if let Some(resume) = resume
            && !resume.completed
            && resume.position_us > 0
        {
            start = resume.position_us;
        }
        if row.duration_us >= 0 && start > row.duration_us {
            start = row.duration_us;
        }
        Ok(PlaybackSource {
            local_id: id.to_string(),
            uri: uri::to_file_uri(&path)?,
            start_position_us: start,
            audio_language: row.audio,
            subtitle_language: row.subtitle,
        })
    }

    pub fn set_progress(
        &mut self,
        id: &str,
        position_us: i64,
        duration_us: i64,
        ended: bool,
    ) -> Result<(), ErrorCode> {
        if self.item_row(id)?.is_none() {
            return Err(ErrorCode::NotFound);
        }
        let previous = self.progress_of(id)?;
        let next = progress::apply_progress(&previous, position_us, duration_us, ended)?;
        self.write_progress(id, &next)
    }

    pub fn restart(&mut self, id: &str) -> Result<(), ErrorCode> {
        if self.item_row(id)?.is_none() {
            return Err(ErrorCode::NotFound);
        }
        let next = progress::restart(&self.progress_of(id)?);
        self.write_progress(id, &next)
    }

    pub fn import(&mut self, candidates: &[ImportCandidate]) -> Result<Vec<String>, ErrorCode> {
        let mut ids = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(existing) = self.completion(&candidate.completion_id)? {
                ids.push(existing);
                continue;
            }
            let path = uri::path_from_uri(&candidate.file_uri)?;
            if !path.is_file() {
                return Err(ErrorCode::Unavailable);
            }
            let root = self.available_root_for(&path)?;
            if !contained(&root, &path) {
                return Err(ErrorCode::InvalidArgument);
            }
            let root_id = self.root_id(&root)?;
            let id = self.upsert_file(
                &root_id,
                &root,
                &path,
                &Settings {
                    probe: false,
                    ..Settings::default()
                },
            )?;
            self.conn
                .execute(
                    "INSERT INTO imports (completion_id, item_id) VALUES (?1, ?2)",
                    params![candidate.completion_id, id],
                )
                .map_err(|_| ErrorCode::Internal)?;
            ids.push(id);
        }
        if ids.len() != ids.iter().collect::<HashSet<_>>().len() {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(ids)
    }

    fn root_id(&mut self, path: &Path) -> Result<String, ErrorCode> {
        let text = path.to_str().ok_or(ErrorCode::InvalidArgument)?;
        if let Some(id) = self
            .conn
            .query_row("SELECT id FROM roots WHERE path = ?1", [text], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|_| ErrorCode::Internal)?
        {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        self.conn
            .execute(
                "INSERT INTO roots (id, path) VALUES (?1, ?2)",
                params![id, text],
            )
            .map_err(|_| ErrorCode::Internal)?;
        Ok(id)
    }

    fn mark_root_unavailable(&self, root_id: &str) -> Result<(), ErrorCode> {
        self.conn
            .execute(
                "UPDATE items SET available = 0 WHERE root_id = ?1",
                [root_id],
            )
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }

    fn scan_root(
        &mut self,
        root_id: &str,
        root: &Path,
        settings: &Settings,
        configured: &[PathBuf],
    ) -> Result<(), ErrorCode> {
        let mut truncated = false;
        let mut paths = Vec::new();
        walk(root, root, 0, configured, &mut paths, &mut truncated)?;
        let mut files = Vec::new();
        for path in paths {
            if let Some(file) = disk_file(root, &path)? {
                files.push(file);
            }
        }
        let existing = self.load_root(root_id)?;
        let mut claimed = HashSet::new();
        let mut paired: Vec<(DiskFile, Option<Row>)> = Vec::new();
        for file in files {
            let by_path = existing.iter().find(|row| {
                row.kind != "series" && row.rel_path == file.rel && !claimed.contains(&row.id)
            });
            let by_fp = by_path.cloned().or_else(|| {
                existing
                    .iter()
                    .find(|row| {
                        row.kind != "series"
                            && !claimed.contains(&row.id)
                            && same_fingerprint(row, &file)
                    })
                    .cloned()
            });
            if by_path.is_none()
                && file.inode.is_some()
                && claimed_inode(&existing, &claimed, file.inode)
            {
                continue;
            }
            if let Some(row) = by_fp.clone() {
                claimed.insert(row.id.clone());
            }
            paired.push((file, by_fp));
        }
        if settings.probe {
            let need: Vec<usize> = paired
                .iter()
                .enumerate()
                .filter(|(_, (file, old))| needs_probe(file, old.as_ref()))
                .map(|(index, _)| index)
                .collect();
            let probed = probe_many(
                &settings.ffprobe,
                &need
                    .iter()
                    .map(|index| paired[*index].0.absolute.clone())
                    .collect::<Vec<_>>(),
            );
            for (index, result) in need.into_iter().zip(probed) {
                paired[index].0.fresh_probe = result;
            }
        }
        let mut rows = Vec::new();
        let mut metas = HashMap::new();
        for (file, old) in paired {
            let meta = (file.parsed.kind == Kind::Episode).then(|| SeriesMeta {
                title: file.parsed.series_title.clone(),
                has_year: file.parsed.series_has_year,
                year: file.parsed.series_year,
            });
            let row = file_row(root_id, file, old);
            if let Some(meta) = meta {
                metas.insert(row.id.clone(), meta);
            }
            rows.push(row);
        }
        if !truncated {
            for row in existing.iter().filter(|row| row.kind != "series") {
                if !claimed.contains(&row.id) {
                    let mut missing = row.clone();
                    missing.available = false;
                    rows.push(missing);
                }
            }
        } else {
            for row in existing.iter().filter(|row| row.kind != "series") {
                if !claimed.contains(&row.id) {
                    rows.push(row.clone());
                }
            }
        }
        let series_existing: Vec<Row> = existing
            .into_iter()
            .filter(|row| row.kind == "series")
            .collect();
        link_series(root, &mut rows, series_existing, &metas);
        self.replace_root(root_id, &rows)
    }

    fn upsert_file(
        &mut self,
        root_id: &str,
        root: &Path,
        path: &Path,
        settings: &Settings,
    ) -> Result<String, ErrorCode> {
        let file = disk_file(root, path)?.ok_or(ErrorCode::InvalidArgument)?;
        let existing = self.load_root(root_id)?;
        let old = existing
            .iter()
            .find(|row| row.kind != "series" && row.rel_path == file.rel)
            .cloned()
            .or_else(|| {
                existing
                    .iter()
                    .find(|row| row.kind != "series" && same_fingerprint(row, &file))
                    .cloned()
            });
        let mut pair = (file, old);
        if settings.probe && needs_probe(&pair.0, pair.1.as_ref()) {
            pair.0.fresh_probe = probe_one(&settings.ffprobe, &pair.0.absolute);
        }
        let meta = (pair.0.parsed.kind == Kind::Episode).then(|| SeriesMeta {
            title: pair.0.parsed.series_title.clone(),
            has_year: pair.0.parsed.series_has_year,
            year: pair.0.parsed.series_year,
        });
        let row = file_row(root_id, pair.0, pair.1);
        let id = row.id.clone();
        let mut metas = HashMap::new();
        if let Some(meta) = meta {
            metas.insert(id.clone(), meta);
        }
        let mut rows = existing
            .into_iter()
            .filter(|item| item.id != id && item.kind != "series")
            .collect::<Vec<_>>();
        let series_existing = self
            .load_root(root_id)?
            .into_iter()
            .filter(|item| item.kind == "series")
            .collect();
        rows.push(row);
        link_series(root, &mut rows, series_existing, &metas);
        self.replace_root(root_id, &rows)?;
        Ok(id)
    }

    fn replace_root(&mut self, root_id: &str, rows: &[Row]) -> Result<(), ErrorCode> {
        let tx = self.conn.transaction().map_err(|_| ErrorCode::Internal)?;
        tx.execute("DELETE FROM items WHERE root_id = ?1", [root_id])
            .map_err(|_| ErrorCode::Internal)?;
        for row in rows {
            tx.execute(
                "INSERT INTO items (
                    id, root_id, rel_path, kind, series_key, parent_id, title,
                    has_year, year, has_season, season, has_episode, episode,
                    duration_us, audio_language, subtitle_language, poster_id, fanart_id,
                    available, inode, size, mtime_sec, mtime_nsec, probed
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
                params![
                    row.id,
                    root_id,
                    row.rel_path,
                    row.kind,
                    row.series_key,
                    row.parent_id,
                    row.title,
                    flag(row.has_year),
                    row.year,
                    flag(row.has_season),
                    row.season,
                    flag(row.has_episode),
                    row.episode,
                    row.duration_us,
                    row.audio,
                    row.subtitle,
                    row.poster_id,
                    row.fanart_id,
                    flag(row.available),
                    row.inode,
                    row.size,
                    row.mtime_sec,
                    row.mtime_nsec,
                    flag(row.probed),
                ],
            )
            .map_err(|_| ErrorCode::Internal)?;
        }
        tx.commit().map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }

    fn available_root_for(&self, path: &Path) -> Result<PathBuf, ErrorCode> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM roots")
            .map_err(|_| ErrorCode::Internal)?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)?;
        let mut best: Option<PathBuf> = None;
        let mut saw_unavailable = false;
        for text in paths {
            let root = PathBuf::from(text);
            if !path.starts_with(&root) {
                continue;
            }
            if !root_readable(&root) {
                saw_unavailable = true;
                continue;
            }
            if contained(&root, path)
                && best
                    .as_ref()
                    .is_none_or(|current| root.as_os_str().len() > current.as_os_str().len())
            {
                best = Some(root);
            }
        }
        best.ok_or(if saw_unavailable {
            ErrorCode::Unavailable
        } else {
            ErrorCode::InvalidArgument
        })
    }

    fn completion(&self, id: &str) -> Result<Option<String>, ErrorCode> {
        self.conn
            .query_row(
                "SELECT item_id FROM imports WHERE completion_id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ErrorCode::Internal)
    }

    fn collections(&self) -> Result<Vec<Collection>, ErrorCode> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, kind FROM collections ORDER BY kind")
            .map_err(|_| ErrorCode::Internal)?;
        let mut rows = stmt
            .query_map([], |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    kind: row.get(2)?,
                })
            })
            .map_err(|_| ErrorCode::Internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ErrorCode::Internal)?;
        rows.sort_by_key(|collection| match collection.kind.as_str() {
            "film" => 0,
            "series" => 1,
            _ => 2,
        });
        Ok(rows)
    }

    fn load_all(&self) -> Result<Vec<Row>, ErrorCode> {
        self.load_filtered("")
    }

    fn load_root(&self, root_id: &str) -> Result<Vec<Row>, ErrorCode> {
        self.load_filtered(root_id)
    }

    fn load_filtered(&self, root_id: &str) -> Result<Vec<Row>, ErrorCode> {
        let (sql, one) = if root_id.is_empty() {
            (
                "SELECT id, rel_path, kind, series_key, parent_id, title, has_year, year,
                        has_season, season, has_episode, episode, duration_us, audio_language,
                        subtitle_language, poster_id, fanart_id, available, inode, size,
                        mtime_sec, mtime_nsec, probed, root_id
                 FROM items",
                false,
            )
        } else {
            (
                "SELECT id, rel_path, kind, series_key, parent_id, title, has_year, year,
                        has_season, season, has_episode, episode, duration_us, audio_language,
                        subtitle_language, poster_id, fanart_id, available, inode, size,
                        mtime_sec, mtime_nsec, probed, root_id
                 FROM items WHERE root_id = ?1",
                true,
            )
        };
        let mut stmt = self.conn.prepare(sql).map_err(|_| ErrorCode::Internal)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(Row {
                root_id: row.get(23)?,
                id: row.get(0)?,
                rel_path: row.get(1)?,
                kind: row.get(2)?,
                series_key: row.get(3)?,
                parent_id: row.get(4)?,
                title: row.get(5)?,
                has_year: row.get::<_, i64>(6)? != 0,
                year: row.get(7)?,
                has_season: row.get::<_, i64>(8)? != 0,
                season: row.get(9)?,
                has_episode: row.get::<_, i64>(10)? != 0,
                episode: row.get(11)?,
                duration_us: row.get(12)?,
                audio: row.get(13)?,
                subtitle: row.get(14)?,
                poster_id: row.get(15)?,
                fanart_id: row.get(16)?,
                available: row.get::<_, i64>(17)? != 0,
                inode: row.get(18)?,
                size: row.get(19)?,
                mtime_sec: row.get(20)?,
                mtime_nsec: row.get(21)?,
                probed: row.get::<_, i64>(22)? != 0,
            })
        };
        let collected = if one {
            stmt.query_map([root_id], map)
                .map_err(|_| ErrorCode::Internal)?
                .collect::<Result<Vec<_>, _>>()
        } else {
            stmt.query_map([], map)
                .map_err(|_| ErrorCode::Internal)?
                .collect::<Result<Vec<_>, _>>()
        };
        collected.map_err(|_| ErrorCode::Internal)
    }

    fn item_row(&self, id: &str) -> Result<Option<Row>, ErrorCode> {
        Ok(self.load_all()?.into_iter().find(|row| row.id == id))
    }

    fn root_path_of(&self, row: &Row) -> Result<PathBuf, ErrorCode> {
        let path: String = self
            .conn
            .query_row(
                "SELECT path FROM roots WHERE id = ?1",
                [&row.root_id],
                |row| row.get(0),
            )
            .map_err(|_| ErrorCode::NotFound)?;
        Ok(PathBuf::from(path))
    }

    fn resume_row(&self, id: &str) -> Result<Option<Resume>, ErrorCode> {
        Ok(self
            .resumes()?
            .into_iter()
            .find(|resume| resume.content_id == id))
    }

    fn progress_of(&self, id: &str) -> Result<Progress, ErrorCode> {
        Ok(self
            .resume_row(id)?
            .map(|resume| Progress {
                position_us: resume.position_us,
                duration_us: resume.duration_us,
                completed: resume.completed,
            })
            .unwrap_or(Progress {
                position_us: 0,
                duration_us: -1,
                completed: false,
            }))
    }

    fn write_progress(&self, id: &str, progress: &Progress) -> Result<(), ErrorCode> {
        let observed = now_us();
        self.conn
            .execute(
                "INSERT INTO resumes (content_id, position_us, duration_us, completed, observed_at_us)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(content_id) DO UPDATE SET
                   position_us = excluded.position_us,
                   duration_us = excluded.duration_us,
                   completed = excluded.completed,
                   observed_at_us = excluded.observed_at_us",
                params![
                    id,
                    progress.position_us,
                    progress.duration_us,
                    flag(progress.completed),
                    i64::try_from(observed).unwrap_or(i64::MAX),
                ],
            )
            .map_err(|_| ErrorCode::Internal)?;
        Ok(())
    }
}

fn seed_collections(conn: &Connection) -> Result<(), ErrorCode> {
    for (kind, title) in [("film", "Films"), ("series", "Series"), ("other", "Other")] {
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM collections WHERE kind = ?1",
                [kind],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ErrorCode::Internal)?;
        if exists.is_none() {
            conn.execute(
                "INSERT INTO collections (kind, id, title) VALUES (?1, ?2, ?3)",
                params![kind, Uuid::new_v4().to_string(), title],
            )
            .map_err(|_| ErrorCode::Internal)?;
        }
    }
    Ok(())
}

fn row_item(row: &Row, collections: &[Collection]) -> Result<Item, ErrorCode> {
    let collection_kind = match row.kind.as_str() {
        "film" => "film",
        "series" | "episode" => "series",
        _ => "other",
    };
    let collection_id = collections
        .iter()
        .find(|collection| collection.kind == collection_kind)
        .map(|collection| collection.id.clone())
        .ok_or(ErrorCode::Internal)?;
    Ok(Item {
        id: row.id.clone(),
        collection_id,
        parent_id: row.parent_id.clone(),
        title: row.title.clone(),
        kind: row.kind.clone(),
        has_year: row.has_year,
        year: if row.has_year { row.year } else { 0 },
        has_season: row.has_season,
        season: if row.has_season { row.season } else { 0 },
        has_episode: row.has_episode,
        episode: if row.has_episode { row.episode } else { 0 },
        duration_us: row.duration_us,
        poster: row.poster_id.clone(),
        fanart: row.fanart_id.clone(),
        available: row.available,
    })
}

fn kind_order(kind: &str) -> u8 {
    match kind {
        "series" => 0,
        "episode" => 1,
        "film" => 2,
        _ => 3,
    }
}

fn flag(value: bool) -> i64 {
    i64::from(value)
}

fn now_us() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
    )
    .unwrap_or(u64::MAX)
}

fn normalize_root(path: &Path) -> Result<PathBuf, ErrorCode> {
    if !path.is_absolute() {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(ErrorCode::InvalidArgument);
                }
            }
            Component::RootDir => out.push("/"),
            Component::Normal(part) => out.push(part),
            Component::Prefix(_) => return Err(ErrorCode::InvalidArgument),
        }
    }
    if out == Path::new("/") || out.as_os_str().is_empty() {
        return Err(ErrorCode::InvalidArgument);
    }
    Ok(out)
}

fn root_readable(path: &Path) -> bool {
    fs::read_dir(path).is_ok()
}

fn contained(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    path.starts_with(root)
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    configured: &[PathBuf],
    out: &mut Vec<PathBuf>,
    truncated: &mut bool,
) -> Result<(), ErrorCode> {
    if *truncated || depth > MAX_DEPTH {
        *truncated = true;
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|_| ErrorCode::Unavailable)?;
    for entry in entries.flatten() {
        if out.len() >= MAX_FILES {
            *truncated = true;
            return Ok(());
        }
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            if parse::is_video(&path) && contained(root, &path) {
                out.push(path);
            }
            continue;
        }
        if meta.is_dir() {
            if configured.iter().any(|other| other == &path) && path != root {
                continue;
            }
            walk(root, &path, depth + 1, configured, out, truncated)?;
        } else if meta.is_file() && parse::is_video(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn disk_file(root: &Path, path: &Path) -> Result<Option<DiskFile>, ErrorCode> {
    let Some(rel) = relative(root, path) else {
        return Ok(None);
    };
    let Some(mut parsed) = parse::parse_video(root, path) else {
        return Ok(None);
    };
    let nfo_path = path.with_extension("nfo");
    let nfo = parse::read_nfo(&nfo_path).unwrap_or_default();
    parse::apply_nfo(&mut parsed, &nfo);
    let meta = fs::metadata(path).map_err(|_| ErrorCode::Unavailable)?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let directory = path.parent().unwrap_or(root);
    let (poster, fanart) = parse::artwork(directory, stem);
    Ok(Some(DiskFile {
        rel,
        absolute: path.to_path_buf(),
        parsed,
        inode: i64::try_from(meta.ino()).ok(),
        size: i64::try_from(meta.len()).ok(),
        mtime_sec: meta.mtime(),
        mtime_nsec: meta.mtime_nsec(),
        has_poster: poster.is_some(),
        has_fanart: fanart.is_some(),
        fresh_probe: None,
    }))
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_str()?;
                if text.is_empty() || text == "." || text == ".." || text.contains('\0') {
                    return None;
                }
                parts.push(text.to_string());
            }
            _ => return None,
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn same_fingerprint(row: &Row, file: &DiskFile) -> bool {
    row.inode.is_some()
        && row.inode == file.inode
        && row.size == file.size
        && row.mtime_sec == Some(file.mtime_sec)
        && row.mtime_nsec == Some(file.mtime_nsec)
}

fn claimed_inode(existing: &[Row], claimed: &HashSet<String>, inode: Option<i64>) -> bool {
    existing
        .iter()
        .any(|row| claimed.contains(&row.id) && row.inode.is_some() && row.inode == inode)
}

fn needs_probe(file: &DiskFile, old: Option<&Row>) -> bool {
    !matches!(old, Some(old) if old.probed && same_fingerprint(old, file))
}

fn file_row(root_id: &str, file: DiskFile, old: Option<Row>) -> Row {
    let id = old
        .as_ref()
        .map(|row| row.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (duration, audio, subtitle, probed) =
        if let Some((duration, audio, subtitle)) = file.fresh_probe.clone() {
            (duration, audio, subtitle, true)
        } else if let Some(old) = old.as_ref() {
            if old.probed && same_fingerprint(old, &file) {
                (
                    old.duration_us,
                    old.audio.clone(),
                    old.subtitle.clone(),
                    true,
                )
            } else {
                (
                    old.duration_us,
                    old.audio.clone(),
                    old.subtitle.clone(),
                    false,
                )
            }
        } else {
            (-1, String::new(), String::new(), false)
        };
    let poster_id = if file.has_poster {
        old.as_ref()
            .map(|row| row.poster_id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    } else {
        String::new()
    };
    let fanart_id = if file.has_fanart {
        old.as_ref()
            .map(|row| row.fanart_id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    } else {
        String::new()
    };
    let kind = match file.parsed.kind {
        Kind::Film => "film",
        Kind::Episode => "episode",
    };
    Row {
        root_id: root_id.to_string(),
        id,
        rel_path: file.rel,
        kind: kind.to_string(),
        series_key: file.parsed.series_key,
        parent_id: String::new(),
        title: file.parsed.title,
        has_year: file.parsed.has_year,
        year: file.parsed.year,
        has_season: file.parsed.has_season,
        season: file.parsed.season,
        has_episode: file.parsed.has_episode,
        episode: file.parsed.episode,
        duration_us: duration,
        audio,
        subtitle,
        poster_id,
        fanart_id,
        available: true,
        inode: file.inode,
        size: file.size,
        mtime_sec: Some(file.mtime_sec),
        mtime_nsec: Some(file.mtime_nsec),
        probed,
    }
}

fn link_series(
    root: &Path,
    rows: &mut Vec<Row>,
    existing: Vec<Row>,
    metas: &HashMap<String, SeriesMeta>,
) {
    let mut pool = existing;
    let mut created: HashMap<String, String> = HashMap::new();
    let episodes: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind == "episode")
        .map(|(index, _)| index)
        .collect();
    for index in episodes {
        let key = rows[index].series_key.clone();
        if key.is_empty() {
            continue;
        }
        if let Some(id) = created.get(&key) {
            rows[index].parent_id = id.clone();
            if rows[index].available
                && let Some(series) = rows.iter_mut().find(|row| row.id == *id)
            {
                series.available = true;
            }
            continue;
        }
        let dir = key.strip_prefix("dir:").unwrap_or("").to_string();
        let inode = series_inode(root, &dir);
        let position = pool.iter().position(|row| {
            row.series_key == key || (inode.is_some() && !dir.is_empty() && row.inode == inode)
        });
        let mut series = if let Some(position) = position {
            pool.remove(position)
        } else {
            blank_series(&rows[index].root_id, &key, inode)
        };
        series.series_key = key.clone();
        series.rel_path = key.clone();
        series.inode = inode;
        series.parent_id.clear();
        series.available = rows[index].available;
        if let Some(meta) = metas.get(&rows[index].id) {
            if !meta.title.is_empty() {
                series.title = meta.title.clone();
            }
            if meta.has_year {
                series.has_year = true;
                series.year = meta.year;
            }
        }
        if !dir.is_empty() {
            if let Some(nfo) = parse::read_nfo(&root.join(&dir).join("tvshow.nfo")) {
                if let Some(title) = nfo.title {
                    series.title = title;
                }
                if let Some(year) = nfo.year {
                    series.has_year = true;
                    series.year = year;
                }
            }
            let (poster, fanart) = parse::artwork(&root.join(&dir), "*");
            series.poster_id = art_id(poster.is_some(), &series.poster_id);
            series.fanart_id = art_id(fanart.is_some(), &series.fanart_id);
        }
        let id = series.id.clone();
        rows[index].parent_id = id.clone();
        created.insert(key, id);
        rows.push(series);
    }
    for mut series in pool {
        if created.values().any(|id| id == &series.id) {
            continue;
        }
        series.available = false;
        rows.push(series);
    }
    let live: HashSet<String> = rows
        .iter()
        .filter(|row| row.kind == "episode" && row.available)
        .map(|row| row.parent_id.clone())
        .collect();
    for row in rows.iter_mut().filter(|row| row.kind == "series") {
        row.available = live.contains(&row.id);
    }
}

fn blank_series(root_id: &str, key: &str, inode: Option<i64>) -> Row {
    Row {
        root_id: root_id.to_string(),
        id: Uuid::new_v4().to_string(),
        rel_path: key.to_string(),
        kind: "series".into(),
        series_key: key.to_string(),
        parent_id: String::new(),
        title: key
            .strip_prefix("name:")
            .or_else(|| key.strip_prefix("dir:"))
            .unwrap_or(key)
            .to_string(),
        has_year: false,
        year: 0,
        has_season: false,
        season: 0,
        has_episode: false,
        episode: 0,
        duration_us: -1,
        audio: String::new(),
        subtitle: String::new(),
        poster_id: String::new(),
        fanart_id: String::new(),
        available: true,
        inode,
        size: None,
        mtime_sec: None,
        mtime_nsec: None,
        probed: false,
    }
}

fn art_id(present: bool, current: &str) -> String {
    if !present {
        String::new()
    } else if current.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        current.to_string()
    }
}

fn series_inode(root: &Path, dir: &str) -> Option<i64> {
    if dir.is_empty() {
        return None;
    }
    let meta = fs::metadata(root.join(dir)).ok()?;
    i64::try_from(meta.ino()).ok()
}

fn probe_many(command: &Path, files: &[PathBuf]) -> Vec<Option<(i64, String, String)>> {
    if files.len() <= 1 {
        return files.iter().map(|path| probe_one(command, path)).collect();
    }
    let results = std::sync::Mutex::new(vec![None; files.len()]);
    let next = std::sync::atomic::AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if index >= files.len() {
                        break;
                    }
                    let probed = probe_one(command, &files[index]);
                    if let Ok(mut slot) = results.lock() {
                        slot[index] = probed;
                    }
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn probe_one(command: &Path, file: &Path) -> Option<(i64, String, String)> {
    let mut child = Command::new(command)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "--",
        ])
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut body = String::new();
                child.stdout.take()?.read_to_string(&mut body).ok()?;
                return parse_probe(&body);
            }
            Ok(None) if started.elapsed() > PROBE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

fn parse_probe(body: &str) -> Option<(i64, String, String)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let duration = value
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(|duration| duration.as_str())
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1_000_000.0).round())
        .filter(|micros| *micros <= i64::MAX as f64)
        .map(|micros| micros as i64)
        .unwrap_or(-1);
    let audio = stream_language(value.get("streams"), "audio");
    let subtitle = stream_language(value.get("streams"), "subtitle");
    Some((duration, audio, subtitle))
}

fn stream_language(streams: Option<&serde_json::Value>, kind: &str) -> String {
    let Some(streams) = streams.and_then(|value| value.as_array()) else {
        return String::new();
    };
    for stream in streams {
        if stream.get("codec_type").and_then(|value| value.as_str()) != Some(kind) {
            continue;
        }
        let Some(language) = stream
            .get("tags")
            .and_then(|tags| tags.get("language"))
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        let tag = language.trim().to_ascii_lowercase().replace('_', "-");
        if media::language_tag(&tag).is_ok() {
            return tag;
        }
    }
    String::new()
}
