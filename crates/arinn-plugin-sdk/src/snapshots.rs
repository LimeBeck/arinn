// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use arinn_protocol::{
    application::{self, Application, ApplicationPage, RunningApplication},
    browser::{self, BrowserWindow, Tab, TabPage},
    download::{self, Job, JobPage},
    library::{self, Collection, Item, LibraryPage, Resume, ResumePage},
    media::{self, Session, SessionPage},
    validation::{self, ErrorCode, MAX_BODY_BYTES, MAX_PAGE_ENTRIES},
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

const TOKEN_TTL: Duration = Duration::from_secs(30);
const MAX_LEASES: usize = 32;
// Provider-side safeguards, not a wire limit on a complete catalogue.
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SNAPSHOT_ENTRIES: usize = 100_000;

/// A paged snapshot kind: two entry lists in one page type, as in
/// `(generation, revision, first[], second[], next_token)`.
pub trait Catalog: Send + Sync + 'static {
    type First: Clone + PartialEq + Send + Sync;
    type Second: Clone + PartialEq + Send + Sync;
    type Page: Clone + Send + Sync;
    fn validate(first: &[Self::First], second: &[Self::Second]) -> Result<(), ErrorCode>;
    /// Text bytes of the entries, checked before encoding anything.
    fn text_bytes(first: &[Self::First], second: &[Self::Second]) -> usize;
    fn empty_page(generation: &str, revision: u64) -> Self::Page;
    fn lists(page: &Self::Page) -> (&[Self::First], &[Self::Second]);
    fn lists_mut(page: &mut Self::Page) -> (&mut Vec<Self::First>, &mut Vec<Self::Second>);
    fn set_revision(page: &mut Self::Page, revision: u64);
    fn next_token(page: &mut Self::Page) -> &mut String;
    fn encoded_len(page: &Self::Page) -> Result<usize, ErrorCode>;
}

/// Applications and running instances (`org.arinn.Provider1`).
pub struct ApplicationCatalog;
impl Catalog for ApplicationCatalog {
    type First = Application;
    type Second = RunningApplication;
    type Page = ApplicationPage;
    fn validate(first: &[Application], second: &[RunningApplication]) -> Result<(), ErrorCode> {
        application::validate_entries(first, second)
    }
    fn text_bytes(first: &[Application], second: &[RunningApplication]) -> usize {
        first
            .iter()
            .map(|a| a.id.len() + a.title.len() + a.icon.len())
            .sum::<usize>()
            + second
                .iter()
                .map(|r| r.id.len() + r.application_id.len() + r.title.len() + r.window_token.len())
                .sum::<usize>()
    }
    fn empty_page(generation: &str, revision: u64) -> ApplicationPage {
        ApplicationPage {
            generation: generation.into(),
            revision,
            applications: Vec::new(),
            running: Vec::new(),
            next_token: String::new(),
        }
    }
    fn lists(page: &ApplicationPage) -> (&[Application], &[RunningApplication]) {
        (&page.applications, &page.running)
    }
    fn lists_mut(
        page: &mut ApplicationPage,
    ) -> (&mut Vec<Application>, &mut Vec<RunningApplication>) {
        (&mut page.applications, &mut page.running)
    }
    fn set_revision(page: &mut ApplicationPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut ApplicationPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &ApplicationPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

/// Browser windows and tabs (`org.arinn.BrowserProvider1`, ADR-0048).
pub struct BrowserCatalog;
impl Catalog for BrowserCatalog {
    type First = BrowserWindow;
    type Second = Tab;
    type Page = TabPage;
    fn validate(first: &[BrowserWindow], second: &[Tab]) -> Result<(), ErrorCode> {
        browser::validate_entries(first, second)
    }
    fn text_bytes(first: &[BrowserWindow], second: &[Tab]) -> usize {
        first
            .iter()
            .map(|w| w.id.len() + w.window_token.len())
            .sum::<usize>()
            + second
                .iter()
                .map(|t| t.id.len() + t.window_id.len() + t.title.len() + t.url.len())
                .sum::<usize>()
    }
    fn empty_page(generation: &str, revision: u64) -> TabPage {
        TabPage {
            generation: generation.into(),
            revision,
            windows: Vec::new(),
            tabs: Vec::new(),
            next_token: String::new(),
        }
    }
    fn lists(page: &TabPage) -> (&[BrowserWindow], &[Tab]) {
        (&page.windows, &page.tabs)
    }
    fn lists_mut(page: &mut TabPage) -> (&mut Vec<BrowserWindow>, &mut Vec<Tab>) {
        (&mut page.windows, &mut page.tabs)
    }
    fn set_revision(page: &mut TabPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut TabPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &TabPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

/// Library collections and items (`org.arinn.LibraryProvider1`, ADR-0054).
pub struct LibraryCatalog;
impl Catalog for LibraryCatalog {
    type First = Collection;
    type Second = Item;
    type Page = LibraryPage;
    fn validate(first: &[Collection], second: &[Item]) -> Result<(), ErrorCode> {
        library::validate_entries(first, second)
    }
    fn text_bytes(first: &[Collection], second: &[Item]) -> usize {
        first
            .iter()
            .map(|c| c.id.len() + c.title.len() + c.kind.len())
            .sum::<usize>()
            + second
                .iter()
                .map(|item| {
                    item.id.len()
                        + item.collection_id.len()
                        + item.parent_id.len()
                        + item.title.len()
                        + item.kind.len()
                        + item.poster.len()
                        + item.fanart.len()
                })
                .sum::<usize>()
    }
    fn empty_page(generation: &str, revision: u64) -> LibraryPage {
        LibraryPage {
            generation: generation.into(),
            revision,
            collections: Vec::new(),
            items: Vec::new(),
            next_token: String::new(),
        }
    }
    fn lists(page: &LibraryPage) -> (&[Collection], &[Item]) {
        (&page.collections, &page.items)
    }
    fn lists_mut(page: &mut LibraryPage) -> (&mut Vec<Collection>, &mut Vec<Item>) {
        (&mut page.collections, &mut page.items)
    }
    fn set_revision(page: &mut LibraryPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut LibraryPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &LibraryPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

/// A paged snapshot with one entry list, for media sessions and resumes.
pub trait Row: Send + Sync + 'static {
    type Entry: Clone + PartialEq + Send + Sync;
    type Page: Clone + Send + Sync;
    fn validate(entries: &[Self::Entry]) -> Result<(), ErrorCode>;
    fn text_bytes(entries: &[Self::Entry]) -> usize;
    fn empty_page(generation: &str, revision: u64) -> Self::Page;
    fn entries(page: &Self::Page) -> &[Self::Entry];
    fn entries_mut(page: &mut Self::Page) -> &mut Vec<Self::Entry>;
    fn set_revision(page: &mut Self::Page, revision: u64);
    fn next_token(page: &mut Self::Page) -> &mut String;
    fn encoded_len(page: &Self::Page) -> Result<usize, ErrorCode>;
}

pub struct SessionRows;
impl Row for SessionRows {
    type Entry = Session;
    type Page = SessionPage;
    fn validate(entries: &[Session]) -> Result<(), ErrorCode> {
        media::validate_sessions(entries)
    }
    fn text_bytes(entries: &[Session]) -> usize {
        entries
            .iter()
            .map(|session| {
                session.id.len()
                    + session.title.len()
                    + session.subtitle.len()
                    + session.artwork.len()
                    + session.state.len()
                    + session.actions.iter().map(String::len).sum::<usize>()
                    + session.content.provider_id.len()
                    + session.content.capability.len()
                    + session.content.local_id.len()
            })
            .sum()
    }
    fn empty_page(generation: &str, revision: u64) -> SessionPage {
        SessionPage {
            generation: generation.into(),
            revision,
            sessions: Vec::new(),
            next_token: String::new(),
        }
    }
    fn entries(page: &SessionPage) -> &[Session] {
        &page.sessions
    }
    fn entries_mut(page: &mut SessionPage) -> &mut Vec<Session> {
        &mut page.sessions
    }
    fn set_revision(page: &mut SessionPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut SessionPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &SessionPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

pub struct ResumeRows;
impl Row for ResumeRows {
    type Entry = Resume;
    type Page = ResumePage;
    fn validate(entries: &[Resume]) -> Result<(), ErrorCode> {
        library::validate_resumes(entries)
    }
    fn text_bytes(entries: &[Resume]) -> usize {
        entries.iter().map(|resume| resume.content_id.len()).sum()
    }
    fn empty_page(generation: &str, revision: u64) -> ResumePage {
        ResumePage {
            generation: generation.into(),
            revision,
            resumes: Vec::new(),
            next_token: String::new(),
        }
    }
    fn entries(page: &ResumePage) -> &[Resume] {
        &page.resumes
    }
    fn entries_mut(page: &mut ResumePage) -> &mut Vec<Resume> {
        &mut page.resumes
    }
    fn set_revision(page: &mut ResumePage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut ResumePage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &ResumePage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

pub struct JobRows;
impl Row for JobRows {
    type Entry = Job;
    type Page = JobPage;
    fn validate(entries: &[Job]) -> Result<(), ErrorCode> {
        download::validate_jobs(entries)
    }
    fn text_bytes(entries: &[Job]) -> usize {
        entries
            .iter()
            .map(|job| {
                job.id.len()
                    + job.title.len()
                    + job.state.len()
                    + job.actions.iter().map(String::len).sum::<usize>()
                    + job.error_code.len()
                    + job.error_message.len()
                    + job
                        .outputs
                        .iter()
                        .map(|o| o.id.len() + o.title.len() + o.file_uri.len())
                        .sum::<usize>()
            })
            .sum()
    }
    fn empty_page(generation: &str, revision: u64) -> JobPage {
        JobPage {
            generation: generation.into(),
            revision,
            jobs: Vec::new(),
            next_token: String::new(),
        }
    }
    fn entries(page: &JobPage) -> &[Job] {
        &page.jobs
    }
    fn entries_mut(page: &mut JobPage) -> &mut Vec<Job> {
        &mut page.jobs
    }
    fn set_revision(page: &mut JobPage, revision: u64) {
        page.revision = revision;
    }
    fn next_token(page: &mut JobPage) -> &mut String {
        &mut page.next_token
    }
    fn encoded_len(page: &JobPage) -> Result<usize, ErrorCode> {
        page.encoded_len()
    }
}

pub type LibrarySnapshots = Store<LibraryCatalog>;
pub type JobSnapshots = RowStore<JobRows>;
pub type SessionSnapshots = RowStore<SessionRows>;
pub type ResumeSnapshots = RowStore<ResumeRows>;

struct Lease<P> {
    owner: String,
    created: Instant,
    pages: Arc<Vec<P>>,
}

/// Atomically published snapshots with immutable, caller-bound paging leases.
/// Keep one store per (provider instance, capability) registration.
pub struct Store<C: Catalog> {
    generation: String,
    revision: u64,
    pages: Arc<Vec<C::Page>>,
    leases: HashMap<String, Lease<C::Page>>,
}
/// Application snapshots, the original SDK store.
pub type Snapshots = Store<ApplicationCatalog>;
/// Browser window and tab snapshots.
pub type TabSnapshots = Store<BrowserCatalog>;

impl<C: Catalog> Store<C> {
    pub fn new(generation: &str) -> Result<Self, ErrorCode> {
        Uuid::parse_str(generation).map_err(|_| ErrorCode::InvalidArgument)?;
        Ok(Self {
            generation: generation.into(),
            revision: 0,
            pages: Arc::new(Vec::new()),
            leases: HashMap::new(),
        })
    }

    fn same(&self, first: &[C::First], second: &[C::Second]) -> bool {
        self.pages
            .iter()
            .flat_map(|p| C::lists(p).0.iter())
            .eq(first.iter())
            && self
                .pages
                .iter()
                .flat_map(|p| C::lists(p).1.iter())
                .eq(second.iter())
    }

    /// Validate and build the complete replacement before committing. A failed
    /// publish leaves both the current snapshot and existing leases untouched.
    pub fn publish(
        &mut self,
        first: Vec<C::First>,
        second: Vec<C::Second>,
    ) -> Result<u64, ErrorCode> {
        self.build(first, second, false)
    }

    /// The revision of the current snapshot; 0 before the first publish.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Publish the unchanged entries under a new revision, for a change that
    /// lives outside the entries (such as the backend state).
    pub fn touch(&mut self) -> Result<u64, ErrorCode> {
        if self.revision == 0 {
            return Err(ErrorCode::Unavailable);
        }
        let first = self
            .pages
            .iter()
            .flat_map(|p| C::lists(p).0.iter().cloned())
            .collect();
        let second = self
            .pages
            .iter()
            .flat_map(|p| C::lists(p).1.iter().cloned())
            .collect();
        self.build(first, second, true)
    }

    fn build(
        &mut self,
        first: Vec<C::First>,
        second: Vec<C::Second>,
        force: bool,
    ) -> Result<u64, ErrorCode> {
        if first.len().saturating_add(second.len()) > MAX_SNAPSHOT_ENTRIES {
            return Err(ErrorCode::Unavailable);
        }
        C::validate(&first, &second)?;
        // Avoid allocating encoded copies of an unbounded snapshot.
        if C::text_bytes(&first, &second) > MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        if !force && self.revision > 0 && self.same(&first, &second) {
            return Ok(self.revision);
        }
        let revision = self.revision.checked_add(1).ok_or(ErrorCode::Conflict)?;
        let mut page = C::empty_page(&self.generation, revision);
        // Reserve the maximum actual token size while sizing each page.
        *C::next_token(&mut page) = "0".repeat(43);
        let mut pages = Vec::new();
        let mut bytes = 0;
        for entry in first
            .into_iter()
            .map(Entry::First)
            .chain(second.into_iter().map(Entry::Second))
        {
            entry.push::<C>(&mut page);
            let (a, b) = C::lists(&page);
            if a.len() + b.len() > MAX_PAGE_ENTRIES || C::encoded_len(&page)? > MAX_BODY_BYTES {
                let overflow = entry.pop::<C>(&mut page);
                bytes += C::encoded_len(&page)?;
                pages.push(page.clone());
                let (a, b) = C::lists_mut(&mut page);
                a.clear();
                b.clear();
                overflow.push::<C>(&mut page);
            }
            if bytes > MAX_SNAPSHOT_BYTES {
                return Err(ErrorCode::Unavailable);
            }
        }
        bytes += C::encoded_len(&page)?;
        if bytes > MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        pages.push(page);
        self.pages = Arc::new(pages);
        self.revision = revision;
        Ok(revision)
    }

    /// Import an already versioned provider snapshot into a daemon-side cache.
    pub fn import(
        &mut self,
        revision: u64,
        first: Vec<C::First>,
        second: Vec<C::Second>,
    ) -> Result<(), ErrorCode> {
        if revision == 0 || revision < self.revision {
            return Err(ErrorCode::Conflict);
        }
        if revision == self.revision && !self.same(&first, &second) {
            return Err(ErrorCode::Conflict);
        }
        self.publish(first, second)?;
        self.revision = revision;
        for page in Arc::make_mut(&mut self.pages) {
            C::set_revision(page, revision);
        }
        Ok(())
    }

    pub fn page(&mut self, owner: &str, token: &str, now: Instant) -> Result<C::Page, ErrorCode> {
        validation::id(owner)?;
        self.leases
            .retain(|_, lease| now.saturating_duration_since(lease.created) < TOKEN_TTL);
        if token.is_empty() {
            if self.revision == 0 {
                return Err(ErrorCode::Unavailable);
            }
            if self.pages.len() == 1 {
                let mut page = self.pages[0].clone();
                C::next_token(&mut page).clear();
                return Ok(page);
            }
            if self.leases.len() >= MAX_LEASES {
                return Err(ErrorCode::Unavailable);
            }
            let key = Uuid::new_v4().simple().to_string();
            let lease = Lease {
                owner: owner.into(),
                created: now,
                pages: self.pages.clone(),
            };
            let first = leased_page::<C>(&key, &lease, 0)?;
            self.leases.insert(key, lease);
            return Ok(first);
        }
        validation::id(token)?;
        let (key, index) = token.split_once(':').ok_or(ErrorCode::Conflict)?;
        let index = index.parse::<usize>().map_err(|_| ErrorCode::Conflict)?;
        let lease = self.leases.get(key).ok_or(ErrorCode::Conflict)?;
        if lease.owner != owner {
            return Err(ErrorCode::PermissionDenied);
        }
        leased_page::<C>(key, lease, index)
    }

    /// Drop all old generations and leases immediately on re-registration.
    pub fn reset(&mut self, generation: &str) -> Result<(), ErrorCode> {
        *self = Self::new(generation)?;
        Ok(())
    }

    pub fn disconnect(&mut self, owner: &str) {
        self.leases.retain(|_, lease| lease.owner != owner);
    }
}

fn leased_page<C: Catalog>(
    key: &str,
    lease: &Lease<C::Page>,
    index: usize,
) -> Result<C::Page, ErrorCode> {
    let mut page = lease.pages.get(index).ok_or(ErrorCode::Conflict)?.clone();
    *C::next_token(&mut page) = if index + 1 < lease.pages.len() {
        format!("{key}:{}", index + 1)
    } else {
        String::new()
    };
    Ok(page)
}

pub struct RowStore<R: Row> {
    generation: String,
    revision: u64,
    pages: Arc<Vec<R::Page>>,
    leases: HashMap<String, Lease<R::Page>>,
}

impl<R: Row> RowStore<R> {
    pub fn new(generation: &str) -> Result<Self, ErrorCode> {
        Uuid::parse_str(generation).map_err(|_| ErrorCode::InvalidArgument)?;
        Ok(Self {
            generation: generation.into(),
            revision: 0,
            pages: Arc::new(Vec::new()),
            leases: HashMap::new(),
        })
    }

    fn same(&self, entries: &[R::Entry]) -> bool {
        self.pages
            .iter()
            .flat_map(|page| R::entries(page).iter())
            .eq(entries.iter())
    }

    pub fn publish(&mut self, entries: Vec<R::Entry>) -> Result<u64, ErrorCode> {
        self.build(entries, false)
    }

    pub fn touch(&mut self) -> Result<u64, ErrorCode> {
        if self.revision == 0 {
            return Err(ErrorCode::Unavailable);
        }
        let entries = self
            .pages
            .iter()
            .flat_map(|page| R::entries(page).iter().cloned())
            .collect();
        self.build(entries, true)
    }

    fn build(&mut self, entries: Vec<R::Entry>, force: bool) -> Result<u64, ErrorCode> {
        if entries.len() > MAX_SNAPSHOT_ENTRIES {
            return Err(ErrorCode::Unavailable);
        }
        R::validate(&entries)?;
        if R::text_bytes(&entries) > MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        if !force && self.revision > 0 && self.same(&entries) {
            return Ok(self.revision);
        }
        let revision = self.revision.checked_add(1).ok_or(ErrorCode::Conflict)?;
        let mut page = R::empty_page(&self.generation, revision);
        *R::next_token(&mut page) = "0".repeat(43);
        let mut pages = Vec::new();
        let mut bytes = 0;
        for entry in entries {
            R::entries_mut(&mut page).push(entry);
            if R::entries(&page).len() > MAX_PAGE_ENTRIES || R::encoded_len(&page)? > MAX_BODY_BYTES
            {
                let overflow = R::entries_mut(&mut page).pop().unwrap();
                bytes += R::encoded_len(&page)?;
                pages.push(page.clone());
                R::entries_mut(&mut page).clear();
                R::entries_mut(&mut page).push(overflow);
            }
            if bytes > MAX_SNAPSHOT_BYTES {
                return Err(ErrorCode::Unavailable);
            }
        }
        bytes += R::encoded_len(&page)?;
        if bytes > MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        pages.push(page);
        self.pages = Arc::new(pages);
        self.revision = revision;
        Ok(revision)
    }

    pub fn import(&mut self, revision: u64, entries: Vec<R::Entry>) -> Result<(), ErrorCode> {
        if revision == 0 || revision < self.revision {
            return Err(ErrorCode::Conflict);
        }
        if revision == self.revision && !self.same(&entries) {
            return Err(ErrorCode::Conflict);
        }
        self.publish(entries)?;
        self.revision = revision;
        for page in Arc::make_mut(&mut self.pages) {
            R::set_revision(page, revision);
        }
        Ok(())
    }

    pub fn page(&mut self, owner: &str, token: &str, now: Instant) -> Result<R::Page, ErrorCode> {
        validation::id(owner)?;
        self.leases
            .retain(|_, lease| now.saturating_duration_since(lease.created) < TOKEN_TTL);
        if token.is_empty() {
            if self.revision == 0 {
                return Err(ErrorCode::Unavailable);
            }
            if self.pages.len() == 1 {
                let mut page = self.pages[0].clone();
                R::next_token(&mut page).clear();
                return Ok(page);
            }
            if self.leases.len() >= MAX_LEASES {
                return Err(ErrorCode::Unavailable);
            }
            let key = Uuid::new_v4().simple().to_string();
            let lease = Lease {
                owner: owner.into(),
                created: now,
                pages: self.pages.clone(),
            };
            let first = leased_row::<R>(&key, &lease, 0)?;
            self.leases.insert(key, lease);
            return Ok(first);
        }
        validation::id(token)?;
        let (key, index) = token.split_once(':').ok_or(ErrorCode::Conflict)?;
        let index = index.parse::<usize>().map_err(|_| ErrorCode::Conflict)?;
        let lease = self.leases.get(key).ok_or(ErrorCode::Conflict)?;
        if lease.owner != owner {
            return Err(ErrorCode::PermissionDenied);
        }
        leased_row::<R>(key, lease, index)
    }
}

fn leased_row<R: Row>(
    key: &str,
    lease: &Lease<R::Page>,
    index: usize,
) -> Result<R::Page, ErrorCode> {
    let mut page = lease.pages.get(index).ok_or(ErrorCode::Conflict)?.clone();
    *R::next_token(&mut page) = if index + 1 < lease.pages.len() {
        format!("{key}:{}", index + 1)
    } else {
        String::new()
    };
    Ok(page)
}

enum Entry<F, S> {
    First(F),
    Second(S),
}
impl<F: Clone, S: Clone> Entry<F, S> {
    fn push<C: Catalog<First = F, Second = S>>(&self, page: &mut C::Page) {
        let (a, b) = C::lists_mut(page);
        match self {
            Self::First(x) => a.push(x.clone()),
            Self::Second(x) => b.push(x.clone()),
        }
    }
    fn pop<C: Catalog<First = F, Second = S>>(&self, page: &mut C::Page) -> Self {
        let (a, b) = C::lists_mut(page);
        match self {
            Self::First(_) => Self::First(a.pop().unwrap()),
            Self::Second(_) => Self::Second(b.pop().unwrap()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Snapshots {
        Snapshots::new(&Uuid::new_v4().to_string()).unwrap()
    }
    fn apps(count: usize, title_len: usize) -> Vec<Application> {
        (0..count)
            .map(|i| Application {
                id: format!("opaque/{i}"),
                title: "x".repeat(title_len),
                can_launch: true,
                group: "other".into(),
                icon: String::new(),
            })
            .collect()
    }

    #[test]
    fn g13_pages_obey_entry_and_encoded_byte_limits() {
        for (count, title_len) in [(401, 10), (100, 8192)] {
            let mut store = store();
            assert_eq!(store.publish(apps(count, title_len), vec![]), Ok(1));
            let now = Instant::now();
            let mut token = String::new();
            let mut actual = Vec::new();
            loop {
                let page = store.page(":1.42", &token, now).unwrap();
                page.validate().unwrap();
                assert!(page.applications.len() <= MAX_PAGE_ENTRIES);
                assert!(page.encoded_len().unwrap() <= MAX_BODY_BYTES);
                actual.extend(page.applications);
                token = page.next_token;
                if token.is_empty() {
                    break;
                }
            }
            assert_eq!(actual, apps(count, title_len));
        }
    }

    #[test]
    fn g13_immutable_revision_owner_expiry_and_reset() {
        let mut store = store();
        let now = Instant::now();
        store.publish(apps(201, 10), vec![]).unwrap();
        let first = store.page(":1.1", "", now).unwrap();
        store.publish(apps(1, 20), vec![]).unwrap();
        let last = store.page(":1.1", &first.next_token, now).unwrap();
        assert_eq!((last.revision, last.applications.len()), (1, 1));
        assert_eq!(
            store.page(":1.2", &first.next_token, now),
            Err(ErrorCode::PermissionDenied)
        );
        assert_eq!(
            store.page(":1.1", &first.next_token, now + TOKEN_TTL),
            Err(ErrorCode::Conflict)
        );
        assert_eq!(store.page(":1.1", "", now + TOKEN_TTL).unwrap().revision, 2);
        store.reset(&Uuid::new_v4().to_string()).unwrap();
        assert_eq!(store.page(":1.1", "", now), Err(ErrorCode::Unavailable));
    }

    #[test]
    fn g13_invalid_snapshot_does_not_replace_valid_state() {
        let mut store = store();
        assert_eq!(store.publish(vec![], vec![]), Ok(1));
        assert_eq!(store.publish(vec![], vec![]), Ok(1));
        assert_eq!(
            store.publish(apps(1, 8193), vec![]),
            Err(ErrorCode::InvalidArgument)
        );
        let duplicate = vec![apps(1, 1)[0].clone(); 2];
        assert_eq!(
            store.publish(duplicate, vec![]),
            Err(ErrorCode::InvalidArgument)
        );
        assert_eq!(store.page(":1.1", "", Instant::now()).unwrap().revision, 1);
        store.revision = u64::MAX;
        assert_eq!(store.publish(apps(1, 1), vec![]), Err(ErrorCode::Conflict));
    }

    #[test]
    fn g13_lease_budget_and_disconnection() {
        let mut store = store();
        store.publish(apps(201, 1), vec![]).unwrap();
        let now = Instant::now();
        for _ in 0..MAX_LEASES {
            store.page(":1.1", "", now).unwrap();
        }
        assert_eq!(store.page(":1.1", "", now), Err(ErrorCode::Unavailable));
        store.disconnect(":1.1");
        assert!(store.page(":1.2", "", now).is_ok());
    }

    #[test]
    fn import_accepts_a_late_subscriber_but_rejects_revision_reuse() {
        let mut store = store();
        store.import(7, apps(201, 1), vec![]).unwrap();
        let now = Instant::now();
        let first = store.page(":1.1", "", now).unwrap();
        assert_eq!(first.revision, 7);
        assert_eq!(
            store.import(7, apps(1, 1), vec![]),
            Err(ErrorCode::Conflict)
        );
        assert_eq!(
            store.import(6, apps(1, 1), vec![]),
            Err(ErrorCode::Conflict)
        );
        store.import(8, apps(1, 1), vec![]).unwrap();
        assert_eq!(
            store.page(":1.1", &first.next_token, now).unwrap().revision,
            7
        );
    }

    #[test]
    fn tab_snapshots_page_and_validate_like_applications() {
        let mut store = TabSnapshots::new(&Uuid::new_v4().to_string()).unwrap();
        let windows = vec![BrowserWindow {
            id: "w".into(),
            window_token: String::new(),
            focused: true,
        }];
        let tabs: Vec<Tab> = (0..250)
            .map(|i| Tab {
                id: format!("t{i}"),
                window_id: "w".into(),
                title: "Tab".into(),
                url: "https://example.org/".into(),
                active: i == 0,
                audible: false,
            })
            .collect();
        assert_eq!(store.publish(windows.clone(), tabs.clone()), Ok(1));
        assert_eq!(store.touch(), Ok(2));
        let now = Instant::now();
        let first = store.page(":1.1", "", now).unwrap();
        first.validate().unwrap();
        assert_eq!(first.revision, 2);
        let last = store.page(":1.1", &first.next_token, now).unwrap();
        last.validate().unwrap();
        assert!(last.next_token.is_empty());
        let all: Vec<Tab> = first.tabs.into_iter().chain(last.tabs).collect();
        assert_eq!(all, tabs);
        let orphan = vec![Tab {
            window_id: "missing".into(),
            ..tabs[0].clone()
        }];
        assert_eq!(
            store.publish(windows, orphan),
            Err(ErrorCode::InvalidArgument)
        );
    }

    #[test]
    fn application_icon_references_are_validated_and_counted() {
        let mut store = store();
        let mut entries = apps(1, 1);
        entries[0].icon = "https://example.invalid/icon.png".into();
        assert_eq!(
            store.publish(entries, vec![]),
            Err(ErrorCode::InvalidArgument)
        );

        let mut entries = apps(1, 1);
        entries[0].icon = "applications-games".into();
        assert_eq!(store.publish(entries, vec![]), Ok(1));
        let page = store.page(":1.42", "", Instant::now()).unwrap();
        page.validate().unwrap();
        assert_eq!(page.applications[0].icon, "applications-games");

        // The catalogue budget counts the reference, not only id and title.
        let huge = "a".repeat(arinn_protocol::application::MAX_ICON_REFERENCE_BYTES);
        let mut entries = apps(20_000, 1);
        for entry in &mut entries {
            entry.icon.clone_from(&huge);
        }
        assert!(
            ApplicationCatalog::text_bytes(&entries, &[]) > 16 * 1024 * 1024,
            "icon bytes must be part of the catalogue budget"
        );
        assert_eq!(store.publish(entries, vec![]), Err(ErrorCode::Unavailable));
    }

    #[test]
    fn running_instances_can_land_on_later_pages() {
        let mut store = store();
        let running = RunningApplication {
            id: "instance".into(),
            application_id: "opaque/0".into(),
            title: "Window".into(),
            window_token: "token".into(),
            can_activate: true,
            can_terminate: false,
        };
        store.publish(apps(200, 1), vec![running.clone()]).unwrap();
        let now = Instant::now();
        let first = store.page(":1.1", "", now).unwrap();
        let last = store.page(":1.1", &first.next_token, now).unwrap();
        last.validate().unwrap();
        assert_eq!(last.running, vec![running]);
    }

    #[test]
    fn session_and_library_pages_follow_the_same_limits() {
        let mut sessions = SessionSnapshots::new(&Uuid::new_v4().to_string()).unwrap();
        let session = Session {
            id: "s".into(),
            title: "Film".into(),
            subtitle: String::new(),
            artwork: String::new(),
            state: "paused".into(),
            duration_us: -1,
            position_us: 0,
            anchor_us: 1,
            actions: vec!["play".into()],
            has_content: false,
            content: arinn_protocol::production::EntityRef {
                provider_id: String::new(),
                capability: String::new(),
                local_id: String::new(),
            },
        };
        let many = (0..250)
            .map(|i| Session {
                id: format!("s{i}"),
                ..session.clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(sessions.publish(many.clone()), Ok(1));
        assert_eq!(sessions.touch(), Ok(2));
        let now = Instant::now();
        let first = sessions.page(":1.1", "", now).unwrap();
        first.validate().unwrap();
        let last = sessions.page(":1.1", &first.next_token, now).unwrap();
        let all: Vec<_> = first.sessions.into_iter().chain(last.sessions).collect();
        assert_eq!(all.len(), many.len());

        let mut library = LibrarySnapshots::new(&Uuid::new_v4().to_string()).unwrap();
        let collection = Collection {
            id: "films".into(),
            title: "Films".into(),
            kind: "film".into(),
        };
        let item = Item {
            id: "film-1".into(),
            collection_id: "films".into(),
            parent_id: String::new(),
            title: "Film".into(),
            kind: "film".into(),
            has_year: false,
            year: 0,
            has_season: false,
            season: 0,
            has_episode: false,
            episode: 0,
            duration_us: -1,
            poster: String::new(),
            fanart: String::new(),
            available: true,
        };
        assert_eq!(library.publish(vec![collection], vec![item.clone()]), Ok(1));
        let page = library.page(":1.1", "", now).unwrap();
        page.validate().unwrap();
        assert_eq!(page.items, vec![item]);
    }
}
