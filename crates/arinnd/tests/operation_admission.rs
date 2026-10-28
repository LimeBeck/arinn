// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Concurrent work and retained deduplication history have separate budgets.
use arinn_protocol::{production::ProductionOperationProxy, validation::ErrorCode};
use arinnd::operations::{Label, Operation, Operations};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
use zbus::{Connection, zvariant::OwnedObjectPath};

struct Bus(Child, String);
impl Bus {
    fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self(child, address.trim().into())
    }
    async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn admit(
    book: &Operations,
    server: &Connection,
    caller: &str,
    index: usize,
    provider: &str,
    kind: &'static str,
) -> Result<(OwnedObjectPath, Operation, bool), ErrorCode> {
    let request = uuid::Uuid::from_u128(index as u128 + 1).to_string();
    let capability = if kind.starts_with("library.") {
        "library"
    } else {
        "media"
    };
    book.admit(
        server,
        caller,
        &request,
        &format!("{kind}:{provider}:{index}"),
        provider,
        Label::new(kind).provider(capability, "fixture"),
    )
    .await
}

#[tokio::test]
async fn completed_history_does_not_block_stop_or_next_launch() {
    let bus = Bus::start();
    let server = bus.connection().await;
    let client = bus.connection().await;
    let caller = client.unique_name().unwrap().as_str();
    let book = Operations::default();
    let mut paths = Vec::new();
    for index in 0..128 {
        let (path, operation, fresh) = admit(
            &book,
            &server,
            caller,
            index,
            "player",
            "media.command:seek",
        )
        .await
        .unwrap();
        assert!(fresh);
        let result = match index % 3 {
            0 => Ok(vec![]),
            1 => Err(ErrorCode::Unavailable),
            _ => Err(ErrorCode::Cancelled),
        };
        operation.finish(&server, &path, result).await;
        paths.push(path);
    }
    let (stop_path, stop, _) = admit(&book, &server, caller, 128, "player", "media.command:stop")
        .await
        .unwrap();
    stop.finish(&server, &stop_path, Ok(vec![])).await;
    admit(&book, &server, caller, 129, "library", "library.launch")
        .await
        .unwrap();
    // Retained terminal requests remain readable and deduplicate; admission
    // must not evict them to make room for the new controls.
    for (index, state) in ["succeeded", "failed", "cancelled"].iter().enumerate() {
        let proxy = ProductionOperationProxy::builder(&client)
            .destination(server.unique_name().unwrap().to_string())
            .unwrap()
            .path(paths[index].clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        let (info,) = proxy.get().await.unwrap();
        assert_eq!(&info.state, state);
        let (same, _, fresh) = admit(
            &book,
            &server,
            caller,
            index,
            "player",
            "media.command:seek",
        )
        .await
        .unwrap();
        assert_eq!(same, paths[index]);
        assert!(!fresh);
    }
}

#[tokio::test]
async fn caller_concurrency_is_still_bounded_and_completion_frees_it() {
    let bus = Bus::start();
    let server = bus.connection().await;
    let client = bus.connection().await;
    let caller = client.unique_name().unwrap().as_str();
    let book = Operations::default();
    let mut first = None;
    for index in 0..128 {
        let value = admit(
            &book,
            &server,
            caller,
            index,
            &format!("provider-{}", index / 32),
            "media.command:seek",
        )
        .await
        .unwrap();
        if index % 2 == 0 {
            value.1.value.lock().await.state = "running".into();
        }
        if index == 0 {
            first = Some(value);
        }
    }
    assert!(matches!(
        admit(&book, &server, caller, 128, "other", "media.command:stop").await,
        Err(ErrorCode::Unavailable)
    ));
    let (path, operation, _) = first.unwrap();
    operation.finish(&server, &path, Ok(vec![])).await;
    admit(&book, &server, caller, 128, "other", "media.command:stop")
        .await
        .unwrap();
    assert!(matches!(
        admit(&book, &server, caller, 129, "other", "media.command:stop").await,
        Err(ErrorCode::Unavailable)
    ));
}

#[tokio::test]
async fn provider_concurrency_is_shared_between_callers() {
    let bus = Bus::start();
    let server = bus.connection().await;
    let a = bus.connection().await;
    let b = bus.connection().await;
    let caller = a.unique_name().unwrap().as_str();
    let book = Operations::default();
    let mut first = None;
    for index in 0..32 {
        let value = admit(
            &book,
            &server,
            caller,
            index,
            "player",
            "media.command:seek",
        )
        .await
        .unwrap();
        if index % 2 == 0 {
            value.1.value.lock().await.state = "running".into();
        }
        if index == 0 {
            first = Some(value);
        }
    }
    let (same, _, fresh) = admit(&book, &server, caller, 0, "player", "media.command:seek")
        .await
        .unwrap();
    assert_eq!(same, first.as_ref().unwrap().0);
    assert!(!fresh);
    assert!(matches!(
        admit(
            &book,
            &server,
            b.unique_name().unwrap().as_str(),
            32,
            "player",
            "media.command:stop"
        )
        .await,
        Err(ErrorCode::Unavailable)
    ));
    let (path, operation, _) = first.unwrap();
    operation.finish(&server, &path, Ok(vec![])).await;
    admit(
        &book,
        &server,
        b.unique_name().unwrap().as_str(),
        32,
        "player",
        "media.command:stop",
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn global_retention_remains_bounded_and_duplicates_work_at_capacity() {
    let bus = Bus::start();
    let server = bus.connection().await;
    let book = Operations::default();
    let mut clients = Vec::new();
    for _ in 0..8 {
        clients.push(bus.connection().await);
    }
    let mut first = None;
    for index in 0..1024 {
        let caller = clients[index / 128].unique_name().unwrap().as_str();
        let (path, operation, _) = admit(
            &book,
            &server,
            caller,
            index,
            "player",
            "media.command:seek",
        )
        .await
        .unwrap();
        operation.finish(&server, &path, Ok(vec![])).await;
        if index == 0 {
            first = Some(path);
        }
    }
    let caller = clients[0].unique_name().unwrap().as_str();
    assert!(matches!(
        admit(&book, &server, caller, 1024, "player", "media.command:stop").await,
        Err(ErrorCode::Unavailable)
    ));
    assert!(matches!(
        admit(&book, &server, caller, 0, "player", "media.command:stop").await,
        Err(ErrorCode::Conflict)
    ));
    let (same, _, fresh) = admit(&book, &server, caller, 0, "player", "media.command:seek")
        .await
        .unwrap();
    assert_eq!(Some(same), first);
    assert!(!fresh);
}
