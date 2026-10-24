// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end client for the temporary desktop-provider private-bus suite.
use arinn_protocol::{
    application::ApplicationPage,
    kwin::KWinBridgeProxy,
    production::{
        ApplicationApiProxy, OperationInfo, ProductionDaemonProxy, ProductionOperationProxy,
    },
};
use std::time::{Duration, Instant};
use uuid::Uuid;
use zbus::{Connection, zvariant::OwnedObjectPath};
async fn page(
    app: &ApplicationApiProxy<'_>,
    predicate: impl Fn(&ApplicationPage) -> bool,
) -> ApplicationPage {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Ok((page,)) = app.get_applications("").await
                && predicate(&page)
            {
                return page;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}
async fn operation(c: &Connection, path: OwnedObjectPath, error: &str) {
    let state = if error.is_empty() {
        "succeeded"
    } else if error == "Cancelled" {
        "cancelled"
    } else {
        "failed"
    };
    let info = finished(c, path).await;
    assert_eq!((info.state.as_str(), info.error.as_str()), (state, error));
    assert!(!info.cancellable);
}
async fn finished(c: &Connection, path: OwnedObjectPath) -> OperationInfo {
    let proxy = ProductionOperationProxy::builder(c)
        .path(path)
        .unwrap()
        .build()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let (info,) = proxy.get().await.unwrap();
            if !["queued", "running"].contains(&info.state.as_str()) {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap()
}
/// A launch still starting: running and cancellable.
async fn starting(c: &Connection, path: &OwnedObjectPath) {
    let (info,) = ProductionOperationProxy::builder(c)
        .path(path.clone())
        .unwrap()
        .build()
        .await
        .unwrap()
        .get()
        .await
        .unwrap();
    assert_eq!((info.state.as_str(), info.cancellable), ("running", true));
}
fn pid(name: &str) -> Option<i32> {
    let dir = std::env::var_os("ARINN_DESKTOP_FIXTURE_DIR")?;
    std::fs::read_to_string(std::path::Path::new(&dir).join(format!("{name}.pid")))
        .ok()?
        .trim()
        .parse()
        .ok()
}
/// Alive and not a zombie.
fn alive(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z" && state != "X")
    })
}
async fn until(what: &str, predicate: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting until {what}"));
}
fn error(result: zbus::Result<impl std::fmt::Debug>, expected: &str) {
    assert!(
        matches!(result,Err(zbus::Error::MethodError(ref name,_,_)) if name.as_str()==format!("org.arinn.Error1.{expected}")),
        "{result:?}"
    );
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("ARINN_PRIVATE_TEST_BUS").as_deref() == Ok("1"));
    let c = Connection::session().await?;
    c.request_name("org.kde.KWin").await?;
    let daemon = ProductionDaemonProxy::new(&c).await?;
    let info = daemon
        .providers()
        .await?
        .into_iter()
        .find(|p| p.id == "desktop")
        .unwrap();
    let app = ApplicationApiProxy::builder(&c)
        .path(info.path)?
        .build()
        .await?;
    let bridge = KWinBridgeProxy::new(&c).await?;
    let generation = bridge.connect(&Uuid::new_v4().to_string()).await?;
    // Full inventories: each revision replaces the previous one.
    let base = [
        ("window-a", "fixture.desktop", 13),
        ("window-b", "fixture.desktop", 4),
        ("unknown", "unproven", 0),
        ("handoff-window", "handoff.desktop", 4),
    ];
    let inventory = |revision: i32, extra: Vec<(&'static str, &'static str, i32)>| {
        let bridge = bridge.clone();
        let generation = generation.clone();
        async move {
            let windows: Vec<_> = base.iter().copied().chain(extra).collect();
            bridge
                .begin_inventory(&generation, revision, windows.len() as i32)
                .await?;
            for (i, (token, desktop, flags)) in windows.into_iter().enumerate() {
                bridge
                    .put_window(&generation, revision, i as i32, token, desktop, "", flags)
                    .await?;
            }
            bridge.commit_inventory(&generation, revision).await
        }
    };
    inventory(1, vec![]).await?;
    let controller = bridge.clone();
    let control_generation = generation.clone();
    let poll = tokio::spawn(async move {
        loop {
            let command = controller.next_control(&control_generation).await?;
            if command.len() == 3 {
                controller
                    .complete_control(
                        &control_generation,
                        &command[0],
                        if command[1] == "close" {
                            "PermissionDenied"
                        } else {
                            ""
                        },
                    )
                    .await?;
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), zbus::Error>(())
    });
    let current = page(&app, |p| p.applications.len() == 7 && p.running.len() == 3).await;
    assert_eq!(current.running[0].id, "window-a");
    let launch = |id: &'static str| {
        let app = app.clone();
        let generation = current.generation.clone();
        async move {
            app.launch(&generation, id, &Uuid::new_v4().to_string())
                .await
        }
    };

    // #436: a launch whose window appears after the old 30 s deadline. It
    // runs while the other checks do; its unit keeps running with no window.
    let slow_started = Instant::now();
    let slow = launch("slow.desktop").await?;

    // A launch ends when its window appears, not when the launcher exits.
    let request = Uuid::new_v4().to_string();
    let fixture = app
        .launch(&current.generation, "fixture.desktop", &request)
        .await?;
    assert_eq!(
        app.launch(&current.generation, "fixture.desktop", &request)
            .await?,
        fixture
    );
    until("the fixture application runs", || pid("fixture").is_some()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    starting(&c, &fixture).await;
    inventory(2, vec![("window-new", "fixture.desktop", 4)]).await?;
    operation(&c, fixture, "").await;
    // Another application's new window does not end the slow launch.
    tokio::time::sleep(Duration::from_millis(500)).await;
    starting(&c, &slow).await;

    operation(&c, launch("broken.desktop").await?, "Unavailable").await;
    // The launcher and everything it started exited with no window: the
    // launch fails at once, not after a deadline.
    let at = Instant::now();
    operation(&c, launch("nowindow.desktop").await?, "Unavailable").await;
    assert!(at.elapsed() < Duration::from_secs(5), "{:?}", at.elapsed());
    // Handed to a running instance whose window already exists.
    operation(&c, launch("handoff.desktop").await?, "").await;

    // Cancel stops only that launch's unit, and only for its caller.
    let cancel = launch("cancel.desktop").await?;
    let keep = launch("keep.desktop").await?;
    until("both fixtures run", || {
        pid("cancel").is_some() && pid("keep").is_some()
    })
    .await;
    let (cancelled, kept) = (pid("cancel").unwrap(), pid("keep").unwrap());
    starting(&c, &cancel).await;
    let stranger = Connection::session().await?;
    error(
        ProductionOperationProxy::builder(&stranger)
            .path(cancel.clone())?
            .build()
            .await?
            .cancel()
            .await,
        "PermissionDenied",
    );
    let canceller = ProductionOperationProxy::builder(&c)
        .path(cancel.clone())?
        .build()
        .await?;
    canceller.cancel().await?;
    operation(&c, cancel.clone(), "Cancelled").await;
    until("the cancelled launch stopped", || !alive(cancelled)).await;
    // Cancelling an ended operation changes nothing.
    canceller.cancel().await?;
    operation(&c, cancel, "Cancelled").await;
    starting(&c, &keep).await;
    assert!(alive(kept), "cancel stopped another launch");
    // Another client launches the same application: CancelLaunch would stop
    // both, so a Cancel stops neither and answers Conflict (ADR-0110).
    let twin = ApplicationApiProxy::builder(&stranger)
        .path(app.inner().path().to_owned())?
        .build()
        .await?
        .launch(
            &current.generation,
            "keep.desktop",
            &Uuid::new_v4().to_string(),
        )
        .await?;
    let keep_canceller = ProductionOperationProxy::builder(&c)
        .path(keep.clone())?
        .build()
        .await?;
    error(keep_canceller.cancel().await, "Conflict");
    tokio::time::sleep(Duration::from_millis(500)).await;
    starting(&c, &keep).await;
    starting(&stranger, &twin).await;
    assert!(alive(kept), "a refused cancel stopped a launch");

    operation(
        &c,
        app.activate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await?,
        "",
    )
    .await;
    operation(
        &c,
        app.terminate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await?,
        "PermissionDenied",
    )
    .await;
    error(
        app.terminate(&current.generation, "window-b", &Uuid::new_v4().to_string())
            .await,
        "Unsupported",
    );
    error(
        app.activate(
            &Uuid::new_v4().to_string(),
            "window-a",
            &Uuid::new_v4().to_string(),
        )
        .await,
        "Conflict",
    );
    // The slow application shows its window after more than 30 s.
    let remaining = Duration::from_secs(31).saturating_sub(slow_started.elapsed());
    tokio::time::sleep(remaining).await;
    starting(&c, &slow).await;
    inventory(
        3,
        vec![
            ("window-new", "fixture.desktop", 4),
            ("slow-window", "slow.desktop", 4),
        ],
    )
    .await?;
    operation(&c, slow, "").await;
    assert!(slow_started.elapsed() > Duration::from_secs(30));
    starting(&c, &keep).await;

    bridge.begin_inventory(&generation, 4, 1).await?;
    bridge
        .put_window(&generation, 4, 0, "remapped", "fixture.desktop", "", 13)
        .await?;
    bridge.commit_inventory(&generation, 4).await?;
    page(&app, |p| {
        p.running.len() == 1 && p.running[0].id == "remapped"
    })
    .await;
    error(
        app.activate(&current.generation, "window-a", &Uuid::new_v4().to_string())
            .await,
        "NotFound",
    );
    // Lost authoritative inventory clears running instances, not the launch catalogue.
    c.release_name("org.kde.KWin").await?;
    page(&app, |p| p.running.is_empty() && p.applications.len() == 7).await;
    poll.abort();
    let _ = poll.await;
    println!(
        "PASS: desktop catalogue, launch until a window (also after 30 s), failure when the unit ends without one, cancel, controls, remap and inventory loss"
    );
    Ok(())
}
