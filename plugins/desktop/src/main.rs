// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_plugin_desktop::{
    discovery::{Entry, Settings},
    instances::Instances,
    launcher,
    launches::{self, Baseline, Launches},
};
use arinn_plugin_sdk::{
    platform::{WindowClient, WindowSnapshot},
    runtime::{self, Action, ApplicationHandler, ApplicationService, RegistrationContext},
};
use arinn_protocol::{production, validation::ErrorCode};
use futures_util::StreamExt;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::sync::watch;

struct Session {
    registration: String,
    client: Arc<WindowClient>,
    snapshot: WindowSnapshot,
}
#[derive(Default)]
struct Shared {
    session: Mutex<Option<Session>>,
    registrations: OnceLock<watch::Receiver<Option<RegistrationContext>>>,
    launches: Launches,
}
struct Handler {
    settings: Settings,
    shared: Arc<Shared>,
}
async fn discover(settings: Settings) -> Result<BTreeMap<String, Entry>, ErrorCode> {
    tokio::task::spawn_blocking(move || settings.discover())
        .await
        .map_err(|_| ErrorCode::Internal)?
}
fn code(error: production::Error) -> ErrorCode {
    match error {
        production::Error::NotFound(_) => ErrorCode::NotFound,
        production::Error::Unsupported(_) => ErrorCode::Unsupported,
        production::Error::Conflict(_) => ErrorCode::Conflict,
        production::Error::PermissionDenied(_) => ErrorCode::PermissionDenied,
        production::Error::Timeout(_) => ErrorCode::Timeout,
        production::Error::InvalidArgument(_) => ErrorCode::InvalidArgument,
        _ => ErrorCode::Unavailable,
    }
}
impl Handler {
    /// The current window inventory of this registration, if the platform
    /// layer reports one.
    fn windows(&self, registration: &str) -> Option<(Arc<WindowClient>, WindowSnapshot)> {
        self.shared
            .session
            .lock()
            .unwrap()
            .as_ref()
            .filter(|s| s.registration == registration)
            .map(|s| (s.client.clone(), s.snapshot.clone()))
    }
    /// Launch through GIO and keep the launch pending while its application
    /// unit runs with no window (spec §6.5, §7.2, ADR-0110).
    async fn launch(
        &self,
        registration: &str,
        id: &str,
        entry: &Entry,
        entries: &BTreeMap<String, Entry>,
    ) -> Result<(), ErrorCode> {
        let mut ticket = self.shared.launches.begin(id);
        let start = self.windows(registration);
        // A dedicated connection owns the AddRef reference. Losing the
        // registration drops it, which releases the reference and never
        // stops the application unit.
        let connection = zbus::Connection::session()
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        let unit = launcher::start_on(&connection, &entry.path).await;
        if ticket.cancelled() {
            let result = match &unit {
                Ok(unit) => launches::stop(unit).await,
                Err(_) => Ok(()),
            };
            ticket.stopped(result);
            if let Ok(unit) = unit {
                unit.release().await;
            }
            return result.and(Err(ErrorCode::Cancelled));
        }
        let unit = unit?;
        // No window inventory right now (an adapter reconnecting): the wait
        // gives it the same grace as a gap during the launch, and counts the
        // launch as dispatched only if it stays missing.
        let baseline = start.map(|(_, snapshot)| Baseline::new(&snapshot));
        let result = launches::wait(
            id,
            entries,
            baseline,
            || self.windows(registration).map(|(_, snapshot)| snapshot),
            || async {
                match self.windows(registration) {
                    Some((client, _)) => client.snapshot().await.ok(),
                    None => None,
                }
            },
            &unit,
            &mut ticket.cancel,
            launches::INVENTORY_GRACE,
        )
        .await;
        if ticket.cancelled() {
            ticket.stopped(match result {
                Err(ErrorCode::Cancelled) => Ok(()),
                Err(error) => Err(error),
                // The window won the race with the cancellation.
                Ok(()) => Err(ErrorCode::NotFound),
            });
        }
        unit.release().await;
        result
    }
}
impl ApplicationHandler for Handler {
    fn execute(
        &self,
        action: Action,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move {
            let mut registrations = self
                .shared
                .registrations
                .get()
                .ok_or(ErrorCode::Unavailable)?
                .clone();
            let context = registrations
                .borrow_and_update()
                .clone()
                .ok_or(ErrorCode::Unavailable)?;
            let work = async {
                match action {
                    Action::Launch => {
                        // Resolve again at dispatch: a removed/hidden override must
                        // not launch an obsolete lower-priority entry.
                        let entries = discover(self.settings.clone()).await?;
                        let entry = entries.get(&id).ok_or(ErrorCode::NotFound)?;
                        if entry.dbus_name.is_some() {
                            // The bus starts the application outside any Arinn
                            // unit: dispatch is all the provider can observe.
                            return launcher::dispatch(entry).await;
                        }
                        self.launch(&context.generation, &id, entry, &entries).await
                    }
                    Action::Activate | Action::Terminate => {
                        let (client, generation) = {
                            let state = self.shared.session.lock().unwrap();
                            let state = state
                                .as_ref()
                                .filter(|s| s.registration == context.generation)
                                .ok_or(ErrorCode::Unavailable)?;
                            if !state.snapshot.windows.iter().any(|w| w.token == id) {
                                return Err(ErrorCode::NotFound);
                            }
                            (state.client.clone(), state.snapshot.generation.clone())
                        };
                        let request = uuid::Uuid::new_v4().to_string();
                        let path = match action {
                            Action::Activate => client.activate(&generation, &id, &request).await,
                            _ => client.close(&generation, &id, &request).await,
                        }
                        .map_err(code)?;
                        client.wait_operation(&path).await.map_err(code)
                    }
                }
            };
            tokio::select! { biased;
                _=registrations.changed()=>Err(ErrorCode::Unavailable),
                result=work=>result,
            }
        })
    }
    fn cancel_launch(
        &self,
        id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ErrorCode>> + Send + '_>> {
        Box::pin(async move { self.shared.launches.cancel(&id).await })
    }
}
async fn synchronize(
    service: ApplicationService,
    settings: Settings,
    shared: Arc<Shared>,
) -> Result<(), ErrorCode> {
    let mut registrations = service.registrations();
    let mut entries = discover(settings.clone()).await.unwrap_or_default();
    loop {
        *shared.session.lock().unwrap() = None;
        service
            .replace(
                entries.values().map(|e| e.application.clone()).collect(),
                Vec::new(),
            )
            .await?;
        let context = registrations.borrow_and_update().clone();
        let Some(context) = context else {
            registrations
                .changed()
                .await
                .map_err(|_| ErrorCode::Unavailable)?;
            continue;
        };
        let client = loop {
            let attempt = async {
                let client = Arc::new(WindowClient::new(&context.connection).await.map_err(code)?);
                let changes = client
                    .proxy()
                    .receive_windows_changed()
                    .await
                    .map_err(|_| ErrorCode::Unavailable)?;
                Ok::<_, ErrorCode>((client, changes))
            };
            let result = tokio::select! {biased;_=registrations.changed()=>None,result=attempt=>Some(result)};
            match result {
                None => break None,
                Some(Ok((client, stream))) => {
                    break Some((client, stream));
                }
                Some(Err(_)) => {
                    tokio::select! {_=registrations.changed()=>break None,_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                }
            }
        };
        let Some((client, mut changes)) = client else {
            continue;
        };
        let mut model = Instances::default();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut scan_at = tokio::time::Instant::now();
        loop {
            let update = async {
                if tokio::time::Instant::now() >= scan_at {
                    entries = discover(settings.clone()).await.unwrap_or_default();
                    scan_at = tokio::time::Instant::now() + Duration::from_secs(2);
                }
                let snapshot = client.snapshot().await;
                let running = if let Ok(snapshot) = snapshot {
                    let running = model.reconcile(&entries, &snapshot);
                    *shared.session.lock().unwrap() = Some(Session {
                        registration: context.generation.clone(),
                        client: client.clone(),
                        snapshot,
                    });
                    running
                } else {
                    *shared.session.lock().unwrap() = None;
                    Vec::new()
                };
                service
                    .replace(
                        entries.values().map(|e| e.application.clone()).collect(),
                        running,
                    )
                    .await
            };
            tokio::select! {biased;_=registrations.changed()=>break,result=update=>result?}
            tokio::select! {biased;_=registrations.changed()=>break,event=changes.next()=>{if event.is_none(){break;}},_=tick.tick()=>{}}
        }
    }
}
async fn run() -> Result<(), ErrorCode> {
    let settings = Settings::from_env()?;
    let entries = discover(settings.clone()).await.unwrap_or_default();
    let shared = Arc::new(Shared::default());
    let service = ApplicationService::new(
        Handler {
            settings: settings.clone(),
            shared: shared.clone(),
        },
        entries.values().map(|e| e.application.clone()).collect(),
        Vec::new(),
        "available",
    )?;
    shared
        .registrations
        .set(service.registrations())
        .map_err(|_| ErrorCode::Internal)?;
    tokio::select! {
        result=runtime::run(service.clone())=>result.map_err(|_|ErrorCode::Unavailable),
        result=synchronize(service,settings,shared)=>result,
        _=tokio::signal::ctrl_c()=>Ok(()),
    }
}
#[tokio::main]
async fn main() {
    if std::env::args_os().len() != 1 {
        std::process::exit(2);
    }
    if let Err(error) = run().await {
        eprintln!(
            "{{\"level\":\"error\",\"component\":\"arinn-plugin-desktop\",\"operation\":\"runtime\",\"error\":\"{error}\"}}"
        );
        std::process::exit(1);
    }
}
