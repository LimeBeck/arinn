// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pre-stable library contract (ADR-0024, ADR-0054).
//!
//! File URIs are visible only to the daemon and the provider. They never
//! reach the shell and are never logged (spec §13).
use crate::{
    media::{self, PlaybackSource},
    production::EntityRef,
    validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use zbus::zvariant::{LE, OwnedObjectPath, Type, serialized::Context, to_bytes};

pub const PROVIDER_INTERFACE: &str = "org.arinn.LibraryProvider1";
pub const INTERFACE: &str = "org.arinn.Library1";
pub const RESUME_INTERFACE: &str = "org.arinn.LibraryResume1";
pub const IMPORT_INTERFACE: &str = "org.arinn.LibraryImport1";
pub const ROOTS_INTERFACE: &str = "org.arinn.LibraryRoots1";

pub const FEATURE_RESUME: &str = "LibraryResume1";
pub const FEATURE_IMPORT: &str = "LibraryImport1";
pub const FEATURE_ROOTS: &str = "LibraryRoots1";

pub const COLLECTION_KINDS: &[&str] = &["film", "series", "other"];
pub const ITEM_KINDS: &[&str] = &["film", "episode", "series", "other"];
pub const MAX_IMPORTS: usize = 32;
const MAX_TOKEN: usize = 32;

/// Unknown kinds are valid on the wire and display as `other`.
pub fn display_collection_kind(value: &str) -> &'static str {
    COLLECTION_KINDS
        .iter()
        .copied()
        .find(|kind| *kind == value)
        .unwrap_or("other")
}

pub fn display_item_kind(value: &str) -> &'static str {
    ITEM_KINDS
        .iter()
        .copied()
        .find(|kind| *kind == value)
        .unwrap_or("other")
}

fn token(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty()
        || value.len() > MAX_TOKEN
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Err(ErrorCode::InvalidArgument)
    } else {
        Ok(())
    }
}

fn optional_year(has: bool, year: i32) -> Result<(), ErrorCode> {
    let valid = if has {
        (1..=9999).contains(&year)
    } else {
        year == 0
    };
    if valid {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

fn optional_index(has: bool, value: u32) -> Result<(), ErrorCode> {
    let valid = if has {
        (1..10_000).contains(&value)
    } else {
        value == 0
    };
    if valid {
        Ok(())
    } else {
        Err(ErrorCode::InvalidArgument)
    }
}

fn asset_ref(value: &str) -> Result<(), ErrorCode> {
    if value.is_empty() {
        Ok(())
    } else {
        validation::id(value)
    }
}

/// A catalogue tab. `kind` is a short token; known values are film, series
/// and other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub kind: String,
}

/// One library entry. `parent_id` is empty or another item in the same
/// snapshot (an episode points at its series). Missing files stay in the
/// catalogue with `available` false. Artwork fields are asset ids or empty.
/// Year, season and episode use an explicit has-value flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[allow(clippy::struct_excessive_bools)]
pub struct Item {
    pub id: String,
    pub collection_id: String,
    pub parent_id: String,
    pub title: String,
    pub kind: String,
    pub has_year: bool,
    pub year: i32,
    pub has_season: bool,
    pub season: u32,
    pub has_episode: bool,
    pub episode: u32,
    pub duration_us: i64,
    pub poster: String,
    pub fanart: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LibraryPage {
    pub generation: String,
    pub revision: u64,
    pub collections: Vec<Collection>,
    pub items: Vec<Item>,
    pub next_token: String,
}

/// Resume owned by the library. `observed_at_us` is Unix time in microseconds,
/// not the monotonic media anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Resume {
    pub content_id: String,
    pub position_us: i64,
    pub duration_us: i64,
    pub completed: bool,
    pub observed_at_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ResumePage {
    pub generation: String,
    pub revision: u64,
    pub resumes: Vec<Resume>,
    pub next_token: String,
}

/// Whether one configured library root can be read now (ADR-0071). `path`
/// is the root exactly as configured in the provider's `path` setting, so
/// the shell can match it to the value it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RootState {
    pub path: String,
    pub available: bool,
}

/// All configured roots of a provider with their availability. Small and
/// bounded by `settings::MAX_PATHS`, so it is not paged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Roots {
    pub generation: String,
    pub revision: u64,
    pub roots: Vec<RootState>,
}

/// One completed download output mapped onto a local file. Repeating an
/// import with the same completion identity must not create a second item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ImportCandidate {
    pub source: EntityRef,
    pub file_uri: String,
    pub completion_id: String,
}

impl Collection {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::text(&self.title)?;
        token(&self.kind)
    }
}

impl Item {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.id)?;
        validation::id(&self.collection_id)?;
        if !self.parent_id.is_empty() {
            validation::id(&self.parent_id)?;
        }
        validation::text(&self.title)?;
        token(&self.kind)?;
        optional_year(self.has_year, self.year)?;
        optional_index(self.has_season, self.season)?;
        optional_index(self.has_episode, self.episode)?;
        media::duration_us(self.duration_us)?;
        asset_ref(&self.poster)?;
        asset_ref(&self.fanart)
    }
}

impl LibraryPage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        uuid::Uuid::parse_str(&self.generation).map_err(|_| ErrorCode::InvalidArgument)?;
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
        }
        if self.revision == 0 || self.collections.len() + self.items.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut collections = HashSet::new();
        for collection in &self.collections {
            collection.validate()?;
            if !collections.insert(collection.id.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        let mut items = HashSet::new();
        for item in &self.items {
            item.validate()?;
            if !items.insert(item.id.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// Unique ids, every item in a listed collection, and acyclic parents.
pub fn validate_entries(collections: &[Collection], items: &[Item]) -> Result<(), ErrorCode> {
    let mut collection_ids = HashSet::new();
    for collection in collections {
        collection.validate()?;
        if !collection_ids.insert(collection.id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    let mut by_id = HashMap::new();
    for item in items {
        item.validate()?;
        if !collection_ids.contains(item.collection_id.as_str())
            || by_id.insert(item.id.as_str(), item).is_some()
        {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    for item in items {
        if item.parent_id.is_empty() {
            continue;
        }
        if item.parent_id == item.id || !by_id.contains_key(item.parent_id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut seen = HashSet::new();
        let mut current = item.parent_id.as_str();
        loop {
            if !seen.insert(current) {
                return Err(ErrorCode::InvalidArgument);
            }
            let parent = by_id[current];
            if parent.parent_id.is_empty() {
                break;
            }
            current = parent.parent_id.as_str();
        }
    }
    Ok(())
}

impl Resume {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.content_id)?;
        media::position_us(self.position_us, self.duration_us)
    }
}

impl ResumePage {
    pub fn encoded_len(&self) -> Result<usize, ErrorCode> {
        to_bytes(Context::new_dbus(LE, 0), &(self,))
            .map(|bytes| bytes.len())
            .map_err(|_| ErrorCode::InvalidArgument)
    }

    pub fn validate(&self) -> Result<(), ErrorCode> {
        uuid::Uuid::parse_str(&self.generation).map_err(|_| ErrorCode::InvalidArgument)?;
        if !self.next_token.is_empty() {
            validation::id(&self.next_token)?;
        }
        if self.revision == 0 || self.resumes.len() > MAX_PAGE_ENTRIES {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut ids = HashSet::new();
        for resume in &self.resumes {
            resume.validate()?;
            if !ids.insert(resume.content_id.as_str()) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        if self.encoded_len()? > MAX_BODY_BYTES {
            return Err(ErrorCode::InvalidArgument);
        }
        Ok(())
    }
}

/// At most `settings::MAX_PATHS` roots, each an absolute normal path, no
/// path twice.
pub fn validate_roots(roots: &[RootState]) -> Result<(), ErrorCode> {
    let mut paths = HashSet::new();
    let ok = roots.len() as u64 <= crate::settings::MAX_PATHS
        && roots
            .iter()
            .all(|root| crate::settings::path(&root.path) && paths.insert(root.path.as_str()));
    ok.then_some(()).ok_or(ErrorCode::InvalidArgument)
}

pub fn validate_resumes(resumes: &[Resume]) -> Result<(), ErrorCode> {
    let mut ids = HashSet::new();
    for resume in resumes {
        resume.validate()?;
        if !ids.insert(resume.content_id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

impl ImportCandidate {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.source.provider_id)?;
        validation::capability(&self.source.capability)?;
        validation::id(&self.source.local_id)?;
        media::file_uri(&self.file_uri)?;
        validation::id(&self.completion_id)
    }
}

pub fn validate_imports(candidates: &[ImportCandidate]) -> Result<(), ErrorCode> {
    if candidates.is_empty() || candidates.len() > MAX_IMPORTS {
        return Err(ErrorCode::InvalidArgument);
    }
    let mut ids = HashSet::new();
    for candidate in candidates {
        candidate.validate()?;
        if !ids.insert(candidate.completion_id.as_str()) {
            return Err(ErrorCode::InvalidArgument);
        }
    }
    Ok(())
}

/// `Launch` returns a [`PlaybackSource`] the daemon keeps off the shell.
pub mod provider {
    use super::{LibraryPage, PlaybackSource};
    #[zbus::proxy(
        interface = "org.arinn.LibraryProvider1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait LibraryProvider {
        fn get_library(&self, token: &str) -> zbus::Result<(LibraryPage,)>;
        fn get_item(&self, generation: &str, id: &str) -> zbus::Result<(super::Item,)>;
        fn backend_state(&self) -> zbus::Result<String>;
        fn features(&self) -> zbus::Result<Vec<String>>;
        fn launch(&self, generation: &str, id: &str) -> zbus::Result<(PlaybackSource,)>;
        #[zbus(signal, name = "StateChanged")]
        fn provider_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use provider::LibraryProviderProxy;

pub mod resume {
    use super::ResumePage;
    #[zbus::proxy(
        interface = "org.arinn.LibraryResume1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait LibraryResume {
        fn get_resumes(&self, token: &str) -> zbus::Result<(ResumePage,)>;
        fn set_progress(
            &self,
            generation: &str,
            content_id: &str,
            position_us: i64,
            duration_us: i64,
            ended: bool,
        ) -> zbus::Result<()>;
        fn restart_from_beginning(&self, generation: &str, content_id: &str) -> zbus::Result<()>;
        #[zbus(signal, name = "StateChanged")]
        fn resume_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use resume::LibraryResumeProxy;

pub mod import {
    use super::ImportCandidate;
    #[zbus::proxy(
        interface = "org.arinn.LibraryImport1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait LibraryImport {
        fn import_completed(
            &self,
            generation: &str,
            candidates: Vec<ImportCandidate>,
        ) -> zbus::Result<Vec<String>>;
    }
}
pub use import::LibraryImportProxy;

/// Optional provider extension (feature `LibraryRoots1`): the availability
/// of the configured roots, for example a disconnected drive. The provider
/// emits `StateChanged` with a new revision when any root changes.
pub mod roots {
    use super::Roots;
    #[zbus::proxy(
        interface = "org.arinn.LibraryRoots1",
        default_path = "/org/arinn/Provider1"
    )]
    pub trait LibraryRoots {
        fn get_roots(&self, generation: &str) -> zbus::Result<(Roots,)>;
        #[zbus(signal, name = "StateChanged")]
        fn roots_state_changed(&self, generation: &str, revision: u64) -> zbus::Result<()>;
    }
}
pub use roots::LibraryRootsProxy;

/// Shell-facing library API. `GetResumes` and `RestartFromBeginning` return
/// `Unsupported` unless `Features` lists `LibraryResume1`. Import and
/// `SetProgress` are not on this interface: the daemon calls the provider.
#[zbus::proxy(
    interface = "org.arinn.Library1",
    default_service = "org.arinn.Daemon1"
)]
pub trait LibraryApi {
    fn get_library(&self, token: &str) -> zbus::Result<(LibraryPage,)>;
    fn get_item(&self, generation: &str, id: &str) -> zbus::Result<(Item,)>;
    fn features(&self) -> zbus::Result<Vec<String>>;
    fn launch(&self, generation: &str, id: &str, request_id: &str)
    -> zbus::Result<OwnedObjectPath>;
    fn get_resumes(&self, token: &str) -> zbus::Result<(ResumePage,)>;
    fn restart_from_beginning(
        &self,
        generation: &str,
        id: &str,
        request_id: &str,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection() -> Collection {
        Collection {
            id: "films".into(),
            title: "Films".into(),
            kind: "film".into(),
        }
    }

    fn film() -> Item {
        Item {
            id: "film-1".into(),
            collection_id: "films".into(),
            parent_id: String::new(),
            title: "Film".into(),
            kind: "film".into(),
            has_year: true,
            year: 2024,
            has_season: false,
            season: 0,
            has_episode: false,
            episode: 0,
            duration_us: 90 * 60 * 1_000_000,
            poster: "poster-1".into(),
            fanart: String::new(),
            available: true,
        }
    }

    #[test]
    fn wire_signatures_round_trip() {
        assert_eq!(Collection::SIGNATURE.to_string(), "(sss)");
        assert_eq!(Item::SIGNATURE.to_string(), "(sssssbibubuxssb)");
        assert_eq!(
            LibraryPage::SIGNATURE.to_string(),
            "(sta(sss)a(sssssbibubuxssb)s)"
        );
        assert_eq!(Resume::SIGNATURE.to_string(), "(sxxbt)");
        assert_eq!(RootState::SIGNATURE.to_string(), "(sb)");
        assert_eq!(Roots::SIGNATURE.to_string(), "(sta(sb))");
        assert_eq!(ResumePage::SIGNATURE.to_string(), "(sta(sxxbt)s)");
        assert_eq!(ImportCandidate::SIGNATURE.to_string(), "((sss)ss)");
        assert_eq!(PlaybackSource::SIGNATURE.to_string(), "(ssxss)");
        let page = LibraryPage {
            generation: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            collections: vec![collection()],
            items: vec![film()],
            next_token: String::new(),
        };
        page.validate().unwrap();
        let bytes = to_bytes(Context::new_dbus(LE, 0), &(&page,)).unwrap();
        let (decoded,): (LibraryPage,) = bytes.deserialize().unwrap().0;
        assert_eq!(decoded, page);
        assert_eq!(display_collection_kind("nope"), "other");
        assert_eq!(display_item_kind("episode"), "episode");
    }

    #[test]
    fn entries_link_collections_and_reject_cycles() {
        let mut episode = film();
        episode.id = "ep".into();
        episode.kind = "episode".into();
        episode.parent_id = "film-1".into();
        episode.has_season = true;
        episode.season = 1;
        episode.has_episode = true;
        episode.episode = 2;
        episode.has_year = false;
        episode.year = 0;
        assert_eq!(
            validate_entries(&[collection()], &[film(), episode.clone()]),
            Ok(())
        );
        episode.parent_id = "missing".into();
        assert_eq!(
            validate_entries(&[collection()], &[film(), episode.clone()]),
            Err(ErrorCode::InvalidArgument)
        );
        let mut a = film();
        a.parent_id = "b".into();
        let mut b = film();
        b.id = "b".into();
        b.parent_id = "film-1".into();
        assert_eq!(
            validate_entries(&[collection()], &[a, b]),
            Err(ErrorCode::InvalidArgument)
        );
    }

    #[test]
    fn optional_numbers_use_flags_and_imports_are_bounded() {
        let mut item = film();
        item.has_year = false;
        assert_eq!(item.validate(), Err(ErrorCode::InvalidArgument));
        item.year = 0;
        assert_eq!(item.validate(), Ok(()));
        let candidate = ImportCandidate {
            source: EntityRef {
                provider_id: "downloads".into(),
                capability: "download".into(),
                local_id: "job-1".into(),
            },
            file_uri: "file:///media/film.mkv".into(),
            completion_id: "done-1".into(),
        };
        validate_imports(std::slice::from_ref(&candidate)).unwrap();
        assert_eq!(
            validate_imports(&[candidate.clone(), candidate]),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(validate_imports(&[]), Err(ErrorCode::InvalidArgument));
    }

    // ADR-0071: configured roots and whether each can be read now.
    #[test]
    fn roots_are_bounded_unique_absolute_paths() {
        let root = |path: &str, available| RootState {
            path: path.into(),
            available,
        };
        assert_eq!(
            validate_roots(&[
                root("/srv/films", true),
                root("/run/media/fixture/Disk", false)
            ]),
            Ok(())
        );
        assert_eq!(validate_roots(&[]), Ok(()));
        for bad in [
            vec![root("relative", true)],
            vec![root("/srv/../etc", true)],
            vec![root("/srv", true), root("/srv", false)],
        ] {
            assert_eq!(
                validate_roots(&bad),
                Err(ErrorCode::InvalidArgument),
                "{bad:?}"
            );
        }
        let many: Vec<_> = (0..=crate::settings::MAX_PATHS)
            .map(|i| root(&format!("/srv/{i}"), true))
            .collect();
        assert_eq!(validate_roots(&many), Err(ErrorCode::InvalidArgument));
        assert_eq!(validate_roots(&many[1..]), Ok(()));
    }
}
