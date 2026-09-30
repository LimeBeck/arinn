// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Resume rules from the media MVP (ADR-0056).
//! A long item completes in the final 90 seconds only when playback steps
//! into that window. A seek, a repeated sample, and short items do not.
//! Confirmed EOF always completes.

use arinn_protocol::{media, validation::ErrorCode};

/// Items longer than this use the 90-second credit window.
pub const LONG_US: i64 = 180 * 1_000_000;
/// Completion window at the end of a long item.
pub const CREDIT_US: i64 = 90 * 1_000_000;
/// Largest forward step still treated as playback, not a seek.
pub const STEP_US: i64 = 15 * 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub position_us: i64,
    pub duration_us: i64,
    pub completed: bool,
}

pub fn apply_progress(
    previous: &Progress,
    position_us: i64,
    duration_us: i64,
    ended: bool,
) -> Result<Progress, ErrorCode> {
    media::position_us(position_us, duration_us)?;
    let mut next = Progress {
        position_us,
        duration_us,
        completed: previous.completed,
    };
    if ended {
        next.completed = true;
        if duration_us >= 0 {
            next.position_us = duration_us;
        }
        return Ok(next);
    }
    if !previous.completed && entered_credits(previous.position_us, position_us, duration_us) {
        next.completed = true;
    }
    Ok(next)
}

pub fn restart(previous: &Progress) -> Progress {
    Progress {
        position_us: 0,
        duration_us: previous.duration_us,
        completed: false,
    }
}

fn entered_credits(previous: i64, position: i64, duration: i64) -> bool {
    if duration <= LONG_US || position < duration - CREDIT_US {
        return false;
    }
    let step = position - previous;
    step > 0 && step <= STEP_US && previous + STEP_US >= duration - CREDIT_US
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(position: i64, duration: i64) -> Progress {
        Progress {
            position_us: position,
            duration_us: duration,
            completed: false,
        }
    }

    #[test]
    fn long_item_completes_on_a_playback_step_into_the_credits() {
        let duration = 2 * 60 * 60 * 1_000_000;
        let previous = at(duration - 100_000_000, duration);
        let next = apply_progress(&previous, duration - 86_000_000, duration, false).unwrap();
        assert!(next.completed);
        assert_eq!(next.position_us, duration - 86_000_000);
    }

    #[test]
    fn a_seek_to_the_end_does_not_complete() {
        let duration = 2 * 60 * 60 * 1_000_000;
        let next = apply_progress(&at(0, duration), duration - 1_000_000, duration, false).unwrap();
        assert!(!next.completed);
        let paused = apply_progress(&next, next.position_us, duration, false).unwrap();
        assert!(!paused.completed);
    }

    #[test]
    fn short_and_unknown_durations_need_eof() {
        let short = apply_progress(&at(0, 60_000_000), 60_000_000, 60_000_000, false).unwrap();
        assert!(!short.completed);
        let ended = apply_progress(&at(1_000, 60_000_000), 60_000_000, 60_000_000, true).unwrap();
        assert!(ended.completed);
        assert_eq!(ended.position_us, 60_000_000);
        let unknown = apply_progress(&at(0, -1), 50_000_000, -1, false).unwrap();
        assert!(!unknown.completed);
        let unknown_end = apply_progress(&unknown, 50_000_000, -1, true).unwrap();
        assert!(unknown_end.completed);
        assert_eq!(unknown_end.position_us, 50_000_000);
    }

    #[test]
    fn restart_clears_completion_and_position() {
        let done = Progress {
            position_us: 100,
            duration_us: LONG_US + 1,
            completed: true,
        };
        assert_eq!(
            restart(&done),
            Progress {
                position_us: 0,
                duration_us: LONG_US + 1,
                completed: false,
            }
        );
    }
}
