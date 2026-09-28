// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_plugin_sdk::runtime::{self, Action, ApplicationHandler, ApplicationService};
use arinn_protocol::{
    application::{Application, RunningApplication},
    validation::ErrorCode,
};
use std::{future::Future, pin::Pin, time::Duration};
struct Demo {
    delay: Duration,
    crash: bool,
}
impl ApplicationHandler for Demo {
    fn execute(
        &self,
        _action: Action,
        _id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            if self.crash {
                std::process::exit(1);
            }
            tokio::time::sleep(self.delay).await;
            Ok(())
        })
    }
}
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!(
            "{{\"level\":\"error\",\"component\":\"provider\",\"operation\":\"runtime\",\"provider_id\":null,\"request_id\":null,\"error\":\"Unavailable\",\"duration_ms\":0}}"
        );
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let fixture = match args.as_slice() {
        [] => "populated",
        [flag, value] if flag == "--fixture" => value,
        _ => return Err(ErrorCode::InvalidArgument.into()),
    };
    if ![
        "populated",
        "empty",
        "delayed",
        "invalid-id",
        "crash",
        "reconnect",
        "revision-race",
        "unavailable",
        "large",
    ]
    .contains(&fixture)
    {
        return Err(ErrorCode::InvalidArgument.into());
    }
    let count = if fixture == "empty" {
        0
    } else if ["large", "revision-race"].contains(&fixture) {
        401
    } else {
        1
    };
    let applications = (0..count)
        .map(|i| Application {
            id: if fixture == "invalid-id" {
                String::new()
            } else {
                format!("demo.application.{i}")
            },
            title: format!("Demo application {i}"),
            can_launch: true,
            // Cover every group; the single-entry fixtures stay "other".
            group: if count == 1 {
                "other".into()
            } else {
                arinn_protocol::application::GROUPS[i % 4].into()
            },
        })
        .collect();
    let running = if count == 0 {
        vec![]
    } else {
        vec![RunningApplication {
            id: "demo.activity".into(),
            application_id: "demo.application.0".into(),
            title: "Demo activity".into(),
            window_token: String::new(),
            can_activate: true,
            can_terminate: true,
        }]
    };
    let service = ApplicationService::new(
        Demo {
            delay: Duration::from_millis(if fixture == "delayed" { 7000 } else { 100 }),
            crash: fixture == "crash",
        },
        applications,
        running,
        if fixture == "unavailable" {
            "unavailable"
        } else {
            "available"
        },
    )?;
    if fixture == "revision-race" {
        let publisher = service.clone();
        tokio::spawn(async move {
            let mut counter = 0u64;
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                counter += 1;
                let apps = (0..401)
                    .map(|i| Application {
                        id: format!("demo.application.{i}"),
                        title: format!("Revision {counter}, application {i}"),
                        can_launch: true,
                        group: arinn_protocol::application::GROUPS[i % 4].into(),
                    })
                    .collect();
                let running = vec![RunningApplication {
                    id: "demo.activity".into(),
                    application_id: "demo.application.0".into(),
                    title: "Demo activity".into(),
                    window_token: String::new(),
                    can_activate: true,
                    can_terminate: true,
                }];
                let _ = publisher.replace(apps, running).await;
            }
        });
    }
    if fixture == "reconnect" {
        loop {
            let _ =
                tokio::time::timeout(Duration::from_secs(4), runtime::run(service.clone())).await;
        }
    }
    runtime::run(service).await
}
