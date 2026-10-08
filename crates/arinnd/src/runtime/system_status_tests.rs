// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon export of ADR-0100 with fake sources on private buses.
use super::*;
use crate::{
    status_sources::{
        Sources,
        tests::{Bus, emit_state_changed, fake_pactl, network_manager, pactl_event, wifi},
    },
    storage::Paths,
};
use arinn_protocol::system_status::{Link, Network, SystemStatusApiProxy};
use futures_util::StreamExt;
use std::time::Duration;
use uuid::Uuid;

async fn wait_for(
    client: &SystemStatusApiProxy<'_>,
    wanted: impl Fn(&SystemStatus) -> bool,
) -> SystemStatus {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (status,) = client.get().await.unwrap();
            status.validate().unwrap();
            if wanted(&status) {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("expected status")
}

#[tokio::test]
async fn the_export_follows_its_sources_and_matches_the_xml() {
    let session = Bus::new();
    let system = Bus::new();
    let (network_manager, fake) = network_manager(&system, wifi()).await;
    let server = session.connect().await;
    let client = session.connect().await;
    let folder = std::env::temp_dir().join(format!("arinn-status-{}", Uuid::new_v4()));
    let pactl = fake_pactl(&folder.join("pactl"));
    let core = Core::new(
        &server,
        Paths {
            config: folder.join("config"),
            data: folder.join("data"),
            state: folder.join("state"),
        },
        folder.clone(),
    );
    let proxy = SystemStatusApiProxy::builder(&client)
        .destination(server.unique_name().unwrap().to_string())
        .unwrap()
        .build()
        .await
        .unwrap();
    // Readers install the Changed match before the first Get.
    let mut changes = proxy.receive_changed().await.unwrap();
    core.export_system_status(Sources {
        system_bus: Some(system.1.clone()),
        pactl,
        poll: Duration::from_secs(60),
        retry: Duration::from_millis(200),
    })
    .await
    .unwrap();

    let live = zbus::fdo::IntrospectableProxy::builder(&client)
        .destination(server.unique_name().unwrap().to_string())
        .unwrap()
        .path(production::PATH)
        .unwrap()
        .build()
        .await
        .unwrap()
        .introspect()
        .await
        .unwrap();
    let file = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/plugin-api/system-status1.xml"
    ))
    .unwrap();
    let methods = super::super::media_routing::tests::methods;
    assert_eq!(
        methods(&live, "org.arinn.SystemStatus1"),
        methods(&file, "org.arinn.SystemStatus1")
    );

    let known = wait_for(&proxy, |s| {
        s.audio() == Audio::Unmuted && s.network() == Network::Connected
    })
    .await;
    assert_eq!(known.epoch, core.epoch);
    assert_eq!((known.link(), known.signal()), (Link::Wifi, 3));
    let signal = changes.next().await.unwrap();
    assert_eq!(signal.args().unwrap().epoch(), &core.epoch);

    // Muting raises the revision and announces it.
    pactl_event(&folder.join("pactl"), "yes", "Event 'change' on sink #3");
    let muted = wait_for(&proxy, |s| s.audio() == Audio::Muted).await;
    assert!(muted.revision > known.revision);
    let announced = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let signal = changes.next().await.unwrap();
            if *signal.args().unwrap().revision() == muted.revision {
                return;
            }
        }
    })
    .await;
    assert!(announced.is_ok(), "Changed for revision {}", muted.revision);

    // NetworkManager goes away: the network fields fall back to unknown and
    // the audio field is untouched.
    fake.lock().unwrap().state = 20;
    emit_state_changed(&network_manager).await;
    wait_for(&proxy, |s| s.network() == Network::Disconnected).await;
    drop(network_manager);
    drop(system);
    let lost = wait_for(&proxy, |s| s.network() == Network::Unknown).await;
    assert_eq!(lost.audio(), Audio::Muted);
    assert_eq!((lost.link, lost.signal), ("unknown".to_string(), 0));
    let _ = std::fs::remove_dir_all(&folder);
}

#[tokio::test]
async fn unchanged_updates_keep_the_revision() {
    let export = StatusExport::new("10000000-0000-4000-8000-000000000001");
    assert_eq!(export.apply(Update::Audio(Audio::Unknown)).await, None);
    assert_eq!(export.apply(Update::Audio(Audio::Muted)).await, Some(2));
    assert_eq!(export.apply(Update::Audio(Audio::Muted)).await, None);
    assert_eq!(
        export.apply(Update::Network(NetworkState::UNKNOWN)).await,
        None
    );
    // At the last revision nothing more is published, and the snapshot
    // falls back to unknown.
    export.published.lock().await.revision = u64::MAX;
    assert_eq!(export.apply(Update::Audio(Audio::Unmuted)).await, None);
    let (status,) = export.get().await.unwrap();
    assert_eq!(status.audio(), Audio::Unknown);
    assert_eq!(export.apply(Update::Audio(Audio::Muted)).await, None);
}
