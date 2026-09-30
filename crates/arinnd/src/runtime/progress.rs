// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Playback progress forwarding (ADR-0054, ADR-0061): the daemon passes the
//! position of media sessions opened from a library item to that library's
//! `LibraryResume1.SetProgress`. The provider applies the resume rules and is
//! the only persistent store.
use super::*;
use arinn_protocol::{
    library::{FEATURE_RESUME, LibraryResumeProxy},
    media::Session,
};
use std::collections::HashMap;

/// While playing, progress is forwarded at least this often (media-mvp.md).
pub(super) const PROGRESS_INTERVAL: Duration = Duration::from_secs(5);
/// A position this far from the extrapolated one is a seek.
const SEEK_THRESHOLD_US: i64 = 2_000_000;
const SET_PROGRESS_TIMEOUT: Duration = Duration::from_secs(2);

/// What was last forwarded for one session.
#[derive(Clone, Debug)]
pub(super) struct Sent {
    at: Instant,
    position_us: i64,
    duration_us: i64,
    playing: bool,
    library: String,
    content: String,
}
impl Sent {
    fn expected(&self, now: Instant) -> i64 {
        if self.playing {
            clamp(
                self.position_us + now.duration_since(self.at).as_micros() as i64,
                self.duration_us,
            )
        } else {
            self.position_us
        }
    }
}
/// Forwarding state per (media provider, session).
pub(super) type Progress = HashMap<(String, String), Sent>;

fn monotonic_us() -> u64 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime only writes the timespec it is given.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    if rc != 0 {
        return 0;
    }
    time.tv_sec as u64 * 1_000_000 + time.tv_nsec as u64 / 1_000
}
fn clamp(position: i64, duration: i64) -> i64 {
    let position = position.max(0);
    if duration >= 0 {
        position.min(duration)
    } else {
        position
    }
}
/// The session's position now: a playing session advances from its anchor.
fn current_position(session: &Session) -> i64 {
    if session.state == "playing" {
        let elapsed = monotonic_us().saturating_sub(session.anchor_us) as i64;
        clamp(session.position_us + elapsed, session.duration_us)
    } else {
        session.position_us
    }
}

impl Core {
    /// Decide which sessions of media provider `media` need a progress
    /// update and send them. Called after every session import and tick.
    pub(super) async fn forward_progress(&self, media: &str, sessions: &[Session]) {
        let now = Instant::now();
        let mut updates = Vec::new();
        {
            let mut progress = self.progress.lock().await;
            let linked: Vec<&Session> = sessions
                .iter()
                .filter(|s| s.has_content && s.content.capability == "library")
                .collect();
            for session in &linked {
                let key = (media.to_owned(), session.id.clone());
                let position = current_position(session);
                let playing = session.state == "playing";
                let send = match progress.get(&key) {
                    None => true,
                    Some(sent) => {
                        sent.playing != playing
                            || sent.content != session.content.local_id
                            || (position - sent.expected(now)).abs() > SEEK_THRESHOLD_US
                            || (playing && now.duration_since(sent.at) >= PROGRESS_INTERVAL)
                    }
                };
                if send {
                    let sent = Sent {
                        at: now,
                        position_us: position,
                        duration_us: session.duration_us,
                        playing,
                        library: session.content.provider_id.clone(),
                        content: session.content.local_id.clone(),
                    };
                    updates.push(sent.clone());
                    progress.insert(key, sent);
                }
            }
            // A session that is gone was stopped: forward where it ended.
            let gone: Vec<_> = progress
                .keys()
                .filter(|(m, s)| m == media && !linked.iter().any(|l| &l.id == s))
                .cloned()
                .collect();
            for key in gone {
                if let Some(mut sent) = progress.remove(&key) {
                    sent.position_us = sent.expected(now);
                    sent.playing = false;
                    updates.push(sent);
                }
            }
        }
        for sent in updates {
            self.set_progress(sent).await;
        }
    }

    /// Best effort: the resume store is the library provider's; a missed
    /// update is corrected by the next one.
    async fn set_progress(&self, sent: Sent) {
        let target = {
            let state = self.state.lock().await;
            state
                .providers
                .get(&sent.library)
                .filter(|p| {
                    p.capability() == "library"
                        && p.info.runtime_state == "ready"
                        && !p.owner.is_empty()
                        && p.features.iter().any(|f| f == FEATURE_RESUME)
                })
                .map(|p| (p.owner.clone(), p.info.generation.clone()))
        };
        let Some((owner, generation)) = target else {
            return;
        };
        let Ok(builder) = LibraryResumeProxy::builder(&self.connection).destination(owner) else {
            return;
        };
        let Ok(proxy) = builder.build().await else {
            return;
        };
        // No confirmed end-of-file signal exists in the session contract yet
        // (ADR-0061), so `ended` is never claimed.
        let _ = tokio::time::timeout(
            SET_PROGRESS_TIMEOUT,
            proxy.set_progress(
                &generation,
                &sent.content,
                sent.position_us,
                sent.duration_us,
                false,
            ),
        )
        .await;
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
