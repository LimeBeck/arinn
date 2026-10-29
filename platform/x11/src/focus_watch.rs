// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wakes the adapter's loop when `_NET_ACTIVE_WINDOW` changes (#507), so the
//! remote's Menu grabs follow the shell's focus at once rather than at the
//! next 0.5 s tick (ADR-0106).
//!
//! The watch has its own X connection that only listens. The adapter's own
//! connection is no use for this: every snapshot makes round trips on it, so
//! waking on its readability would wake on its own replies, and x11rb may
//! read an event into its queue while it waits for a reply. A wake is only
//! a hint: the loop refreshes as on a tick, and the tick stays, so a watch
//! that fails or misses an event costs no more than the tick did.
use anyhow::Result;
use std::{
    os::fd::AsRawFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
use tokio::sync::Notify;
use x11rb::{
    connection::Connection as _,
    protocol::{Event, xproto::*},
    rust_connection::RustConnection,
};

/// How long the watch thread blocks before it looks at its stop flag.
const POLL_MS: i32 = 250;

/// The running watch; dropping it stops the thread within `POLL_MS`.
pub struct FocusWatch {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for FocusWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Never block the async loop: the thread ends by itself.
        drop(self.thread.take());
    }
}

/// Starts watching `_NET_ACTIVE_WINDOW` on `display` (`None`: `DISPLAY`).
/// Each change notifies `changed`; changes before the loop waits collapse
/// into one wake.
pub fn start(display: Option<&str>, changed: Arc<Notify>) -> Result<FocusWatch> {
    let (c, screen) = x11rb::connect(display)?;
    let root = c.setup().roots[screen].root;
    let active = c.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
    // Another client's selection on the root window: each client chooses its
    // own events there, so the adapter's mask is untouched.
    c.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let thread = std::thread::spawn(move || {
        let _ = watch(&c, active, &stopped, &changed);
    });
    Ok(FocusWatch {
        stop,
        thread: Some(thread),
    })
}

fn watch(c: &RustConnection, active: Atom, stop: &AtomicBool, changed: &Notify) -> Result<()> {
    let fd = c.stream().as_raw_fd();
    while !stop.load(Ordering::SeqCst) {
        // Drain first: the selection's reply may have queued an event.
        while let Some(event) = c.poll_for_event()? {
            if let Event::PropertyNotify(e) = event
                && e.atom == active
            {
                changed.notify_one();
            }
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd for the connection's open socket, which
        // `c` keeps open for the whole call.
        let ready = unsafe { libc::poll(&mut poll, 1, POLL_MS) };
        if ready < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            anyhow::bail!("focus watch poll failed");
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            anyhow::bail!("focus watch connection closed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::fixture::Desktop;
    use std::time::{Duration, Instant};
    use x11rb::wrapper::ConnectionExt as _;

    /// #507: a change of `_NET_ACTIVE_WINDOW` wakes the loop at once, other
    /// root property changes do not, and the watch ends with its handle.
    #[tokio::test]
    #[ignore = "requires Xvfb; spawns an isolated X server, never uses the real DISPLAY"]
    async fn a_focus_change_wakes_the_loop_at_once() -> Result<()> {
        let desktop = Desktop::new()?;
        let window = desktop.client("org.example.Player.desktop", "")?;
        let changed = Arc::new(Notify::new());
        let watch = start(Some(&desktop.display), changed.clone())?;
        // Another root property: no wake.
        desktop.inventory(&[window], 0)?;
        assert!(
            tokio::time::timeout(Duration::from_millis(300), changed.notified())
                .await
                // `inventory` also writes `_NET_ACTIVE_WINDOW`: take that wake.
                .is_ok()
        );
        desktop.c.change_property32(
            PropMode::REPLACE,
            desktop.root,
            desktop.atoms._NET_CLIENT_LIST,
            AtomEnum::WINDOW,
            &[window],
        )?;
        desktop.c.sync()?;
        assert!(
            tokio::time::timeout(Duration::from_millis(300), changed.notified())
                .await
                .is_err()
        );
        let start = Instant::now();
        desktop.set_net_active_window(&[window])?;
        tokio::time::timeout(Duration::from_secs(5), changed.notified()).await?;
        // Far below the 0.5 s tick it replaces for focus changes.
        assert!(
            start.elapsed() < Duration::from_millis(400),
            "{:?}",
            start.elapsed()
        );
        let stop = watch.stop.clone();
        drop(watch);
        assert!(stop.load(Ordering::SeqCst));
        Ok(())
    }
}
