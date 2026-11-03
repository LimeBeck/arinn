// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A bounded source/session-owned queue. Only a confirmed EOF advances itself.
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_UPCOMING: usize = 64;
pub const MAX_HISTORY: usize = 32;
const RECENT_LIMIT: usize = 256;
static OWNERS: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub occurrence: u64,
    pub track: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    owner: u64,
    session: u64,
    request: u64,
    replace: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Stale,
    Empty,
    Full,
    Unsupported,
}

pub struct Queue {
    owner: u64,
    entries: VecDeque<Entry>,
    cursor: usize,
    serial: u64,
    session: u64,
    request: u64,
    pending: bool,
    station: Option<String>,
    recent: VecDeque<String>,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            owner: OWNERS.fetch_add(1, Ordering::Relaxed),
            entries: VecDeque::new(),
            cursor: 0,
            serial: 0,
            session: 0,
            request: 0,
            pending: false,
            station: None,
            recent: VecDeque::new(),
        }
    }
}

impl Queue {
    pub fn entries(&self) -> &VecDeque<Entry> {
        &self.entries
    }
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }
    pub fn station(&self) -> Option<&str> {
        self.station.as_deref()
    }
    pub fn pending(&self) -> bool {
        self.pending
    }
    pub fn upcoming(&self) -> usize {
        self.entries.len().saturating_sub(self.cursor + 1)
    }

    /// Explicit starts replace the source session and invalidate outstanding work.
    pub fn start(&mut self, tracks: Vec<String>, station: Option<String>) -> Result<(), Error> {
        if (tracks.is_empty() && station.is_none())
            || station.as_deref() == Some("")
            || tracks.iter().any(String::is_empty)
        {
            return Err(Error::Empty);
        }
        if tracks.len() > MAX_UPCOMING + 1 {
            return Err(Error::Full);
        }
        self.stop();
        self.station = station;
        self.recent.clear();
        for track in tracks {
            let entry = self.entry(track);
            self.entries.push_back(entry);
        }
        self.remember_current();
        Ok(())
    }

    pub fn stop(&mut self) {
        self.session += 1;
        self.request += 1;
        self.entries.clear();
        self.cursor = 0;
        self.station = None;
        self.pending = false;
    }

    /// No looping at the loaded end, including when a radio refill fails.
    pub fn advance(&mut self) -> Result<(), Error> {
        if self.upcoming() == 0 {
            return Err(Error::Empty);
        }
        self.cursor += 1;
        self.remember_current();
        self.trim();
        Ok(())
    }

    pub fn previous(&mut self) -> Result<(), Error> {
        if self.cursor == 0 {
            return Err(Error::Empty);
        }
        self.cursor -= 1;
        Ok(())
    }
    pub fn append_finite(&mut self, tracks: Vec<String>) -> Result<(), Error> {
        if self.station.is_some() {
            return Err(Error::Unsupported);
        }
        if tracks.iter().any(String::is_empty)
            || tracks.len() > MAX_UPCOMING.saturating_sub(self.upcoming())
        {
            return Err(Error::Full);
        }
        for track in tracks {
            let entry = self.entry(track);
            self.entries.push_back(entry);
        }
        Ok(())
    }

    pub fn confirmed_end(&mut self, occurrence: u64) -> Result<(), Error> {
        if self.current().map(|e| e.occurrence) != Some(occurrence) {
            return Err(Error::Stale);
        }
        self.advance()
    }

    pub fn remove(&mut self, occurrence: u64) -> Result<(), Error> {
        if self.station.is_some() {
            return Err(Error::Unsupported);
        }
        let index = self
            .entries
            .iter()
            .position(|e| e.occurrence == occurrence)
            .ok_or(Error::Stale)?;
        // Removing the current item is a playback command, not a queue edit.
        if index <= self.cursor {
            return Err(Error::Unsupported);
        }
        self.entries.remove(index);
        Ok(())
    }

    pub fn begin_refill(&mut self) -> Result<Request, Error> {
        if self.station.is_none() {
            return Err(Error::Unsupported);
        }
        if self.pending {
            return Err(Error::Stale);
        }
        self.request += 1;
        self.pending = true;
        Ok(Request {
            owner: self.owner,
            session: self.session,
            request: self.request,
            replace: false,
        })
    }

    /// Feedback replacement supersedes any in-flight append but preserves current.
    pub fn begin_replacement(&mut self) -> Result<Request, Error> {
        self.pending = false;
        self.begin_refill().map(|mut request| {
            request.replace = true;
            request
        })
    }

    pub fn fail_refill(&mut self, request: Request) -> Result<(), Error> {
        self.check(request)?;
        self.pending = false;
        Ok(())
    }

    pub fn complete_refill(
        &mut self,
        request: Request,
        tracks: Vec<String>,
        replace: bool,
    ) -> Result<(), Error> {
        self.check(request)?;
        if request.replace != replace {
            return Err(Error::Unsupported);
        }
        self.pending = false;
        // Validate before discarding even one loaded item.
        if tracks.len() > MAX_UPCOMING || tracks.iter().any(String::is_empty) {
            return Err(Error::Full);
        }
        let preserved = if replace {
            self.cursor + 1
        } else {
            self.entries.len()
        };
        let mut seen: HashSet<String> = self
            .entries
            .iter()
            .take(preserved)
            .map(|e| e.track.clone())
            .collect();
        seen.extend(self.recent.iter().cloned());
        let mut accepted: Vec<String> = tracks
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        if accepted.is_empty() {
            return Err(Error::Empty);
        }
        let capacity = MAX_UPCOMING.saturating_sub(if replace { 0 } else { self.upcoming() });
        if capacity == 0 {
            return Err(Error::Full);
        }
        accepted.truncate(capacity);
        let initially_empty = self.entries.is_empty();
        if replace {
            self.entries.truncate(self.cursor + 1);
        }
        for track in accepted {
            let entry = self.entry(track);
            self.entries.push_back(entry);
        }
        self.trim();
        if initially_empty {
            self.remember_current();
        }
        Ok(())
    }

    fn check(&self, request: Request) -> Result<(), Error> {
        if !self.pending
            || request.owner != self.owner
            || request.session != self.session
            || request.request != self.request
        {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
    fn entry(&mut self, track: String) -> Entry {
        self.serial += 1;
        Entry {
            occurrence: self.serial,
            track,
        }
    }
    fn trim(&mut self) {
        while self.cursor > MAX_HISTORY {
            self.entries.pop_front();
            self.cursor -= 1;
        }
    }
    fn remember_current(&mut self) {
        if let Some(entry) = self.current() {
            self.recent.push_back(entry.track.clone());
        }
        while self.recent.len() > RECENT_LIMIT {
            self.recent.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn finite_refill_preserves_duplicate_occurrences_and_window_bound() {
        let mut q = Queue::default();
        q.start(ids(&["a", "b"]), None).unwrap();
        q.append_finite(ids(&["a", "a"])).unwrap();
        assert_eq!(q.entries.len(), 4);
        let occurrences: std::collections::HashSet<_> =
            q.entries.iter().map(|e| e.occurrence).collect();
        assert_eq!(occurrences.len(), 4);
        assert!(q.append_finite(vec!["overflow".into(); 64]).is_err());
        assert_eq!(q.entries.len(), 4);
    }
    #[test]
    fn duplicate_tracks_have_distinct_occurrences_and_removed_items_are_not_played() {
        let mut q = Queue::default();
        q.start(ids(&["a", "a", "b"]), None).unwrap();
        assert_ne!(q.entries[0].occurrence, q.entries[1].occurrence);
        q.remove(q.entries[1].occurrence).unwrap();
        q.advance().unwrap();
        assert_eq!(q.current().unwrap().track, "b");
        assert_eq!(q.advance(), Err(Error::Empty));
        assert_eq!(q.current().unwrap().track, "b");
    }

    #[test]
    fn late_batches_and_feedback_replies_cannot_revive_or_modify_another_session() {
        let mut q = Queue::default();
        q.start(ids(&["a", "b"]), Some("station".into())).unwrap();
        let old = q.begin_refill().unwrap();
        let replacement = q.begin_replacement().unwrap();
        assert_eq!(
            q.complete_refill(old, ids(&["old"]), false),
            Err(Error::Stale)
        );
        let current = q.current().cloned();
        q.complete_refill(replacement, ids(&["new"]), true).unwrap();
        assert_eq!(q.current().cloned(), current);
        assert_eq!(
            q.entries
                .iter()
                .map(|e| e.track.as_str())
                .collect::<Vec<_>>(),
            ["a", "new"]
        );
        let old = q.begin_refill().unwrap();
        q.stop();
        assert_eq!(
            q.complete_refill(old, ids(&["late"]), false),
            Err(Error::Stale)
        );
        assert!(q.entries.is_empty());
    }

    #[test]
    fn retry_keeps_current_and_loaded_tail_and_rejects_repeated_end_events() {
        let mut q = Queue::default();
        q.start(ids(&["a", "b"]), Some("station".into())).unwrap();
        let current = q.current().unwrap().occurrence;
        let request = q.begin_refill().unwrap();
        q.fail_refill(request).unwrap();
        assert_eq!(q.current().unwrap().occurrence, current);
        assert_eq!(q.upcoming(), 1);
        let retry = q.begin_refill().unwrap();
        q.complete_refill(retry, ids(&["a", "b", "c", "c"]), false)
            .unwrap();
        assert_eq!(q.upcoming(), 2);
        q.confirmed_end(current).unwrap();
        assert_eq!(q.confirmed_end(current), Err(Error::Stale));
    }

    #[test]
    fn repeated_replacements_keep_bounded_history_without_cutting_off_current() {
        let mut q = Queue::default();
        q.start(ids(&["first"]), Some("station".into())).unwrap();
        for i in 0..1000 {
            let current = q.current().cloned();
            let request = q.begin_replacement().unwrap();
            q.complete_refill(request, vec![format!("t{i}")], true)
                .unwrap();
            assert_eq!(q.current().cloned(), current);
            q.advance().unwrap();
            assert!(q.entries.len() <= MAX_HISTORY + MAX_UPCOMING + 1);
            assert!(q.recent.len() <= RECENT_LIMIT);
        }
    }

    #[test]
    fn an_initially_empty_station_can_refill_but_an_append_cannot_replace_its_tail() {
        let mut q = Queue::default();
        q.start(vec![], Some("empty-station".into())).unwrap();
        assert!(q.current().is_none());
        let request = q.begin_refill().unwrap();
        assert_eq!(
            q.complete_refill(request, ids(&["a"]), true),
            Err(Error::Unsupported)
        );
        q.complete_refill(request, ids(&["a", "b"]), false).unwrap();
        assert_eq!(q.current().unwrap().track, "a");
        assert_eq!(q.upcoming(), 1);
    }
    #[test]
    fn replacing_the_queue_object_cannot_revalidate_an_old_request_token() {
        let mut first = Queue::default();
        first.start(ids(&["a"]), Some("station".into())).unwrap();
        let old = first.begin_refill().unwrap();
        let mut replacement = Queue::default();
        replacement
            .start(ids(&["a"]), Some("station".into()))
            .unwrap();
        replacement.begin_refill().unwrap();
        assert_eq!(
            replacement.complete_refill(old, ids(&["stale"]), false),
            Err(Error::Stale)
        );
        assert_eq!(replacement.upcoming(), 0);
    }

    #[test]
    fn an_empty_stations_first_track_stays_recent_after_history_is_trimmed() {
        let mut q = Queue::default();
        q.start(vec![], Some("station".into())).unwrap();
        let request = q.begin_refill().unwrap();
        q.complete_refill(request, ids(&["initial"]), false)
            .unwrap();
        for i in 0..MAX_HISTORY + 1 {
            let request = q.begin_replacement().unwrap();
            q.complete_refill(request, vec![format!("track-{i}")], true)
                .unwrap();
            q.advance().unwrap();
        }
        let request = q.begin_refill().unwrap();
        assert_eq!(
            q.complete_refill(request, ids(&["initial"]), false),
            Err(Error::Empty)
        );
    }
}
