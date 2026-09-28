// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use arinn_protocol::{
    application::{Application, ApplicationPage, RunningApplication, validate_entries},
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

struct Lease {
    owner: String,
    created: Instant,
    pages: Arc<Vec<ApplicationPage>>,
}

/// Atomically published snapshots with immutable, caller-bound paging leases.
/// Keep one store per (provider instance, application capability) registration.
pub struct Snapshots {
    generation: String,
    revision: u64,
    pages: Arc<Vec<ApplicationPage>>,
    leases: HashMap<String, Lease>,
}

impl Snapshots {
    pub fn new(generation: &str) -> Result<Self, ErrorCode> {
        Uuid::parse_str(generation).map_err(|_| ErrorCode::InvalidArgument)?;
        Ok(Self {
            generation: generation.into(),
            revision: 0,
            pages: Arc::new(Vec::new()),
            leases: HashMap::new(),
        })
    }

    /// Validate and build the complete replacement before committing. A failed
    /// publish leaves both the current snapshot and existing leases untouched.
    pub fn publish(
        &mut self,
        applications: Vec<Application>,
        running: Vec<RunningApplication>,
    ) -> Result<u64, ErrorCode> {
        if applications.len().saturating_add(running.len()) > MAX_SNAPSHOT_ENTRIES {
            return Err(ErrorCode::Unavailable);
        }
        validate_entries(&applications, &running)?;
        // Avoid allocating encoded copies of an unbounded snapshot.
        let text_bytes: usize = applications
            .iter()
            .map(|a| a.id.len() + a.title.len())
            .sum::<usize>()
            + running
                .iter()
                .map(|r| r.id.len() + r.application_id.len() + r.title.len() + r.window_token.len())
                .sum::<usize>();
        if text_bytes > MAX_SNAPSHOT_BYTES {
            return Err(ErrorCode::Unavailable);
        }
        let old_applications = self.pages.iter().flat_map(|p| p.applications.iter());
        let old_running = self.pages.iter().flat_map(|p| p.running.iter());
        if self.revision > 0
            && old_applications.eq(applications.iter())
            && old_running.eq(running.iter())
        {
            return Ok(self.revision);
        }
        let revision = self.revision.checked_add(1).ok_or(ErrorCode::Conflict)?;
        let mut page = ApplicationPage {
            generation: self.generation.clone(),
            revision,
            applications: Vec::new(),
            running: Vec::new(),
            // Reserve the maximum actual token size while sizing each page.
            next_token: "0".repeat(43),
        };
        let mut pages = Vec::new();
        let mut bytes = 0;
        for entry in applications
            .into_iter()
            .map(Entry::Application)
            .chain(running.into_iter().map(Entry::Running))
        {
            entry.push(&mut page);
            if page.applications.len() + page.running.len() > MAX_PAGE_ENTRIES
                || page.encoded_len()? > MAX_BODY_BYTES
            {
                let overflow = entry.pop(&mut page);
                bytes += page.encoded_len()?;
                pages.push(page.clone());
                page.applications.clear();
                page.running.clear();
                overflow.push(&mut page);
            }
            if bytes > MAX_SNAPSHOT_BYTES {
                return Err(ErrorCode::Unavailable);
            }
        }
        bytes += page.encoded_len()?;
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
        applications: Vec<Application>,
        running: Vec<RunningApplication>,
    ) -> Result<(), ErrorCode> {
        if revision == 0 || revision < self.revision {
            return Err(ErrorCode::Conflict);
        }
        let same = self
            .pages
            .iter()
            .flat_map(|p| p.applications.iter())
            .eq(applications.iter())
            && self
                .pages
                .iter()
                .flat_map(|p| p.running.iter())
                .eq(running.iter());
        if revision == self.revision && !same {
            return Err(ErrorCode::Conflict);
        }
        self.publish(applications, running)?;
        self.revision = revision;
        for page in Arc::make_mut(&mut self.pages) {
            page.revision = revision;
        }
        Ok(())
    }

    pub fn page(
        &mut self,
        owner: &str,
        token: &str,
        now: Instant,
    ) -> Result<ApplicationPage, ErrorCode> {
        validation::id(owner)?;
        self.leases
            .retain(|_, lease| now.saturating_duration_since(lease.created) < TOKEN_TTL);
        if token.is_empty() {
            if self.revision == 0 {
                return Err(ErrorCode::Unavailable);
            }
            if self.pages.len() == 1 {
                let mut page = self.pages[0].clone();
                page.next_token.clear();
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
            let first = leased_page(&key, &lease, 0)?;
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
        leased_page(key, lease, index)
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

fn leased_page(key: &str, lease: &Lease, index: usize) -> Result<ApplicationPage, ErrorCode> {
    let mut page = lease.pages.get(index).ok_or(ErrorCode::Conflict)?.clone();
    page.next_token = if index + 1 < lease.pages.len() {
        format!("{key}:{}", index + 1)
    } else {
        String::new()
    };
    Ok(page)
}

enum Entry {
    Application(Application),
    Running(RunningApplication),
}
impl Entry {
    fn push(&self, page: &mut ApplicationPage) {
        match self {
            Self::Application(a) => page.applications.push(a.clone()),
            Self::Running(r) => page.running.push(r.clone()),
        }
    }
    fn pop(&self, page: &mut ApplicationPage) -> Self {
        match self {
            Self::Application(_) => Self::Application(page.applications.pop().unwrap()),
            Self::Running(_) => Self::Running(page.running.pop().unwrap()),
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
}
