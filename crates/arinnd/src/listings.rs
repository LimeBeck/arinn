// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use arinn_protocol::{
    production::{ProviderInfo, ProviderPage},
    validation::{self, ErrorCode},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use uuid::Uuid;
struct Lease {
    owner: String,
    created: Instant,
    providers: Vec<ProviderInfo>,
}
#[derive(Default)]
pub struct Listings {
    leases: HashMap<String, Lease>,
}
impl Listings {
    pub fn page(
        &mut self,
        owner: &str,
        token: &str,
        values: impl FnOnce() -> Vec<ProviderInfo>,
    ) -> Result<ProviderPage, ErrorCode> {
        validation::id(owner)?;
        let now = Instant::now();
        self.leases
            .retain(|_, lease| now.duration_since(lease.created) < Duration::from_secs(30));
        let (key, index) = if token.is_empty() {
            if self.leases.len() >= 32 {
                return Err(ErrorCode::Unavailable);
            }
            let key = Uuid::new_v4().simple().to_string();
            self.leases.insert(
                key.clone(),
                Lease {
                    owner: owner.into(),
                    created: now,
                    providers: values(),
                },
            );
            (key, 0)
        } else {
            validation::id(token)?;
            let (key, index) = token.split_once(':').ok_or(ErrorCode::Conflict)?;
            (
                key.to_owned(),
                index.parse::<usize>().map_err(|_| ErrorCode::Conflict)?,
            )
        };
        let lease = self.leases.get(&key).ok_or(ErrorCode::Conflict)?;
        if lease.owner != owner {
            return Err(ErrorCode::PermissionDenied);
        }
        if index > lease.providers.len() {
            return Err(ErrorCode::Conflict);
        }
        let mut page = ProviderPage {
            snapshot_id: key.clone(),
            providers: Vec::new(),
            next_token: "0".repeat(43),
        };
        for provider in lease.providers.iter().skip(index).take(200) {
            page.providers.push(provider.clone());
            if zbus::zvariant::to_bytes(
                zbus::zvariant::serialized::Context::new_dbus(zbus::zvariant::LE, 0),
                &(&page,),
            )
            .map_err(|_| ErrorCode::Internal)?
            .len()
                > validation::MAX_BODY_BYTES
            {
                page.providers.pop();
                break;
            }
        }
        if page.providers.is_empty() && index < lease.providers.len() {
            return Err(ErrorCode::InvalidArgument);
        }
        page.next_token = if index + page.providers.len() < lease.providers.len() {
            format!("{key}:{}", index + page.providers.len())
        } else {
            String::new()
        };
        if index == 0 && page.next_token.is_empty() {
            self.leases.remove(&key);
        }
        Ok(page)
    }
    pub fn disconnect(&mut self, owner: &str) {
        self.leases.retain(|_, l| l.owner != owner);
    }
}
