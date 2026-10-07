// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional media-session ↔ browser-tab association (ADR-0094). Bindings and
//! validation only; the join rules are in docs/architecture/media-association.md.
use crate::{
    platform,
    validation::{self, ErrorCode},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use zbus::zvariant::Type;

/// Daemon-root snapshot of verified links.
pub const INTERFACE: &str = "org.arinn.MediaAssociation1";
/// Optional media-provider interface: the process that owns a session.
pub const MEDIA_PROCESS_INTERFACE: &str = "org.arinn.MediaProcess1";
/// Optional browser-provider interface: the browser process of a window.
pub const BROWSER_PROCESS_INTERFACE: &str = "org.arinn.BrowserProcess1";
/// Upper bound of links in one snapshot.
pub const MAX_LINKS: usize = 64;
/// A process ID a provider could not establish.
pub const UNKNOWN_PROCESS: u32 = 0;

/// One verified link. Every field is opaque; generations are the providers'
/// registration generations, so a link never outlives a provider restart.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
pub struct MediaTabLink {
    pub media_provider: String,
    pub media_generation: String,
    pub session: String,
    pub browser_provider: String,
    pub browser_generation: String,
    pub tab: String,
}
impl MediaTabLink {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        validation::id(&self.media_provider)?;
        platform::generation(&self.media_generation)?;
        validation::id(&self.session)?;
        validation::id(&self.browser_provider)?;
        platform::generation(&self.browser_generation)?;
        validation::id(&self.tab)
    }
}

/// One coherent daemon-owned snapshot. An empty `links` is the normal state:
/// no browser media session is verifiably tied to a tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MediaAssociation {
    pub epoch: String,
    pub revision: u64,
    pub links: Vec<MediaTabLink>,
}
impl MediaAssociation {
    /// Structural validation: a session or a tab appears in at most one link.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        platform::generation(&self.epoch)?;
        if self.revision == 0 || self.links.len() > MAX_LINKS {
            return Err(ErrorCode::InvalidArgument);
        }
        let mut sessions = BTreeSet::new();
        let mut tabs = BTreeSet::new();
        for link in &self.links {
            link.validate()?;
            let session = (&link.media_provider, &link.media_generation, &link.session);
            let tab = (&link.browser_provider, &link.browser_generation, &link.tab);
            if !sessions.insert(session) || !tabs.insert(tab) {
                return Err(ErrorCode::InvalidArgument);
            }
        }
        Ok(())
    }
}

/// Arguments of `MediaProcess1.GetProcessId` and `BrowserProcess1.GetProcessId`.
pub fn validate_process_query(generation: &str, id: &str) -> Result<(), ErrorCode> {
    platform::generation(generation)?;
    validation::id(id)
}

/// A current media session and the process that owns it (join input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSession {
    pub media_provider: String,
    pub media_generation: String,
    pub session: String,
    pub process: u32,
}

/// A current browser window and its browser process (join input). Every
/// window counts, with or without tabs, for provider uniqueness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessWindow {
    pub browser_provider: String,
    pub browser_generation: String,
    pub window: String,
    pub process: u32,
}

/// A current browser tab of one of the provider's windows (join input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTab {
    pub browser_provider: String,
    pub browser_generation: String,
    pub tab: String,
    pub window: String,
    pub audible: bool,
}

/// A published link with the process ID it was verified with. The ID stays
/// with the producer; only `link` is published.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeldLink {
    pub link: MediaTabLink,
    pub process: u32,
}

/// The reference join of docs/architecture/media-association.md. `previous`
/// is the last result; it only lets an established link hold, with the same
/// process ID, while no tab is the single audible one. Never picks the first
/// of several candidates.
pub fn join(
    sessions: &[ProcessSession],
    windows: &[ProcessWindow],
    tabs: &[ProcessTab],
    previous: &[HeldLink],
) -> Vec<HeldLink> {
    let mut links = Vec::new();
    for session in sessions {
        let process = session.process;
        if process == UNKNOWN_PROCESS
            || sessions.iter().filter(|s| s.process == process).count() != 1
        {
            continue;
        }
        let owned: Vec<&ProcessWindow> = windows.iter().filter(|w| w.process == process).collect();
        let Some(first) = owned.first() else {
            continue;
        };
        let (provider, generation) = (&first.browser_provider, &first.browser_generation);
        if owned
            .iter()
            .any(|w| &w.browser_provider != provider || &w.browser_generation != generation)
        {
            continue;
        }
        let candidates: Vec<&ProcessTab> = tabs
            .iter()
            .filter(|t| {
                &t.browser_provider == provider
                    && &t.browser_generation == generation
                    && owned.iter().any(|w| w.window == t.window)
            })
            .collect();
        let audible: Vec<&&ProcessTab> = candidates.iter().filter(|t| t.audible).collect();
        let tab = if audible.len() == 1 {
            Some(audible[0].tab.clone())
        } else {
            previous
                .iter()
                .find(|h| {
                    h.process == process
                        && h.link.media_provider == session.media_provider
                        && h.link.media_generation == session.media_generation
                        && h.link.session == session.session
                        && &h.link.browser_provider == provider
                        && &h.link.browser_generation == generation
                })
                .filter(|h| candidates.iter().any(|t| t.tab == h.link.tab))
                .map(|h| h.link.tab.clone())
        };
        if let Some(tab) = tab {
            links.push(HeldLink {
                link: MediaTabLink {
                    media_provider: session.media_provider.clone(),
                    media_generation: session.media_generation.clone(),
                    session: session.session.clone(),
                    browser_provider: provider.clone(),
                    browser_generation: generation.clone(),
                    tab,
                },
                process,
            });
        }
    }
    links.sort();
    links.truncate(MAX_LINKS);
    links
}

#[zbus::proxy(
    interface = "org.arinn.MediaAssociation1",
    default_service = "org.arinn.Daemon1",
    default_path = "/org/arinn/Daemon1"
)]
pub trait MediaAssociationApi {
    fn get(&self) -> zbus::Result<(MediaAssociation,)>;
    #[zbus(signal)]
    fn changed(&self, epoch: &str, revision: u64) -> zbus::Result<()>;
}

/// Provider side. 0 means unknown; the daemon never infers a process from IDs.
#[zbus::proxy(
    interface = "org.arinn.MediaProcess1",
    default_path = "/org/arinn/Provider1"
)]
pub trait MediaProcess {
    fn get_process_id(&self, generation: &str, id: &str) -> zbus::Result<u32>;
}

/// Provider side, keyed by the provider's opaque browser window ID.
#[zbus::proxy(
    interface = "org.arinn.BrowserProcess1",
    default_path = "/org/arinn/Provider1"
)]
pub trait BrowserProcess {
    fn get_process_id(&self, generation: &str, window_id: &str) -> zbus::Result<u32>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{LE, serialized::Context, to_bytes};

    fn link(n: u8) -> MediaTabLink {
        MediaTabLink {
            media_provider: "media".into(),
            media_generation: "10000000-0000-4000-8000-000000000001".into(),
            session: format!("opaque:session {n}"),
            browser_provider: "browser".into(),
            browser_generation: "20000000-0000-4000-8000-000000000002".into(),
            tab: format!("c1:t{n}"),
        }
    }
    fn sample() -> MediaAssociation {
        MediaAssociation {
            epoch: "30000000-0000-4000-8000-000000000003".into(),
            revision: 1,
            links: vec![link(1), link(2)],
        }
    }

    #[test]
    fn signatures_and_roundtrip() {
        assert_eq!(MediaTabLink::SIGNATURE.to_string(), "(ssssss)");
        assert_eq!(MediaAssociation::SIGNATURE.to_string(), "(sta(ssssss))");
        let mut value = sample();
        for empty in [false, true] {
            if empty {
                value.links.clear();
            }
            value.validate().unwrap();
            let bytes = to_bytes(Context::new_dbus(LE, 0), &(&value,)).unwrap();
            let (decoded,): (MediaAssociation,) = bytes.deserialize().unwrap().0;
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn malformed_snapshots_are_rejected() {
        let good = sample();
        let mut invalid = vec![];
        let mut v = good.clone();
        v.revision = 0;
        invalid.push(v);
        let mut v = good.clone();
        v.epoch = "not-an-epoch".into();
        invalid.push(v);
        let mut v = good.clone();
        v.links = (0..=MAX_LINKS as u8).map(link).collect();
        invalid.push(v);
        // One session, two tabs.
        let mut v = good.clone();
        v.links[1].session = v.links[0].session.clone();
        invalid.push(v);
        // One tab, two sessions.
        let mut v = good.clone();
        v.links[1].tab = v.links[0].tab.clone();
        invalid.push(v);
        for field in 0..6 {
            let mut v = good.clone();
            let target = &mut v.links[0];
            match field {
                0 => target.media_provider.clear(),
                1 => target.media_generation = uuid::Uuid::nil().to_string(),
                2 => target.session.clear(),
                3 => target.browser_provider = "bad\0id".into(),
                4 => target.browser_generation.clear(),
                _ => target.tab = "x".repeat(4097),
            }
            invalid.push(v);
        }
        for v in invalid {
            assert_eq!(v.validate(), Err(ErrorCode::InvalidArgument), "{v:?}");
        }
        // The same session ID under another generation is a different session.
        let mut v = good.clone();
        v.links[1].session = v.links[0].session.clone();
        v.links[1].media_generation = "40000000-0000-4000-8000-000000000004".into();
        v.validate().unwrap();
    }

    const MG: &str = "10000000-0000-4000-8000-000000000001";
    const BG: &str = "20000000-0000-4000-8000-000000000002";
    fn session(id: &str, process: u32) -> ProcessSession {
        ProcessSession {
            media_provider: "mpris".into(),
            media_generation: MG.into(),
            session: id.into(),
            process,
        }
    }
    fn window(id: &str, process: u32) -> ProcessWindow {
        ProcessWindow {
            browser_provider: "chrome".into(),
            browser_generation: BG.into(),
            window: id.into(),
            process,
        }
    }
    fn tab(id: &str, window: &str, audible: bool) -> ProcessTab {
        ProcessTab {
            browser_provider: "chrome".into(),
            browser_generation: BG.into(),
            tab: id.into(),
            window: window.into(),
            audible,
        }
    }
    fn tabs_of(links: &[HeldLink]) -> Vec<&str> {
        links.iter().map(|l| l.link.tab.as_str()).collect()
    }
    /// Windows w7 and w8 of browser process 7, w9 of process 8.
    fn windows() -> Vec<ProcessWindow> {
        vec![window("w7", 7), window("w8", 7), window("w9", 8)]
    }

    #[test]
    fn join_links_the_single_audible_tab_of_the_same_process() {
        let sessions = [session("browser", 7), session("player", 9)];
        let tabs = [
            tab("t1", "w7", false),
            tab("t2", "w8", true),
            tab("t3", "w9", true),
        ];
        let links = join(&sessions, &windows(), &tabs, &[]);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].link.session, "browser");
        assert_eq!(links[0].link.tab, "t2");
        assert_eq!(links[0].process, 7);
        MediaAssociation {
            epoch: MG.into(),
            revision: 1,
            links: links.iter().map(|l| l.link.clone()).collect(),
        }
        .validate()
        .unwrap();
        // Unknown process, no audible tab, no browser windows: nothing.
        assert!(join(&[session("browser", 0)], &windows(), &tabs, &[]).is_empty());
        assert!(join(&sessions, &windows(), &[tab("t1", "w7", false)], &[]).is_empty());
        assert!(join(&sessions, &[], &tabs, &[]).is_empty());
    }

    #[test]
    fn join_refuses_ambiguity_and_never_picks_the_first() {
        // Two audible tabs.
        let tabs = [tab("t1", "w7", true), tab("t2", "w8", true)];
        assert!(join(&[session("s", 7)], &windows(), &tabs, &[]).is_empty());
        // Two sessions of one process.
        let tabs = [tab("t1", "w7", true)];
        assert!(join(&[session("a", 7), session("b", 7)], &windows(), &tabs, &[]).is_empty());
        // A second browser provider claims the process, even with an empty
        // window and no tabs at all.
        let mut other = window("x1", 7);
        other.browser_provider = "other".into();
        let mut all = windows();
        all.push(other);
        assert!(join(&[session("s", 7)], &all, &tabs, &[]).is_empty());
    }

    #[test]
    fn join_holds_through_a_pause_and_moves_to_the_next_audible_tab() {
        let sessions = [session("s", 7)];
        let w = windows();
        let playing = join(
            &sessions,
            &w,
            &[tab("t1", "w7", true), tab("t2", "w8", false)],
            &[],
        );
        assert_eq!(tabs_of(&playing), ["t1"]);
        // Paused: no tab is audible, the link holds.
        let paused = join(
            &sessions,
            &w,
            &[tab("t1", "w7", false), tab("t2", "w8", false)],
            &playing,
        );
        assert_eq!(paused, playing);
        // Both audible: an established link holds, a new one is not made.
        let both = join(
            &sessions,
            &w,
            &[tab("t1", "w7", true), tab("t2", "w8", true)],
            &paused,
        );
        assert_eq!(both, playing);
        // Another tab becomes the single audible one: the link moves.
        let moved = join(
            &sessions,
            &w,
            &[tab("t1", "w7", false), tab("t2", "w8", true)],
            &both,
        );
        assert_eq!(tabs_of(&moved), ["t2"]);
        // The linked tab closes while nothing is audible: the link goes.
        assert!(join(&sessions, &w, &[tab("t1", "w7", false)], &moved).is_empty());
    }

    #[test]
    fn join_drops_links_on_generation_or_process_change() {
        let sessions = [session("s", 7)];
        let tabs = [tab("t1", "w7", false)];
        let previous = join(&sessions, &windows(), &[tab("t1", "w7", true)], &[]);
        assert_eq!(previous.len(), 1);
        // A provider restart gives new generations; nothing carries over.
        let mut restarted_window = window("w7", 7);
        restarted_window.browser_generation = "30000000-0000-4000-8000-000000000003".into();
        let mut restarted_tab = tab("t1", "w7", false);
        restarted_tab.browser_generation = restarted_window.browser_generation.clone();
        assert!(join(&sessions, &[restarted_window], &[restarted_tab], &previous).is_empty());
        let mut renewed = session("s", 7);
        renewed.media_generation = "40000000-0000-4000-8000-000000000004".into();
        assert!(join(&[renewed], &windows(), &tabs, &previous).is_empty());
        // The session now belongs to another process.
        assert!(join(&[session("s", 8)], &windows(), &tabs, &previous).is_empty());
        assert!(join(&[session("s", 0)], &windows(), &tabs, &previous).is_empty());
        // Session and window both move to process 8 with nothing audible:
        // the old evidence is for process 7, so the link is not held.
        let moved = [window("w7", 8)];
        assert!(join(&[session("s", 8)], &moved, &tabs, &previous).is_empty());
    }

    #[test]
    fn process_queries_are_bounded() {
        let g = "10000000-0000-4000-8000-000000000001";
        validate_process_query(g, "c1:w3").unwrap();
        assert!(validate_process_query(g, "").is_err());
        assert!(validate_process_query("x", "c1:w3").is_err());
        assert!(validate_process_query(g, "bad\0").is_err());
    }
}
