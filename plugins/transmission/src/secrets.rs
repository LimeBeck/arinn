// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Credential lookup on the freedesktop secret service. The secret bytes are
//! `username:password`. A locked item is left locked so this provider never
//! raises a prompt. The bytes are not logged.

use arinn_protocol::validation::ErrorCode;
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Type};

const SERVICE: &str = "org.freedesktop.secrets";
const PATH: &str = "/org/freedesktop/secrets";
const INTERFACE: &str = "org.freedesktop.Secret.Service";

#[derive(Debug, serde::Deserialize, Type)]
struct ItemSecret(
    #[allow(dead_code)] OwnedObjectPath,
    #[allow(dead_code)] Vec<u8>,
    Vec<u8>,
    #[allow(dead_code)] String,
);

pub async fn lookup(
    connection: &zbus::Connection,
    reference: &str,
) -> Result<(String, String), ErrorCode> {
    let reply = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "OpenSession",
            &("plain", OwnedValue::from(Str::from(""))),
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let (_output, session): (OwnedValue, OwnedObjectPath) = reply
        .body()
        .deserialize()
        .map_err(|_| ErrorCode::Unavailable)?;
    let mut attributes = HashMap::new();
    attributes.insert("arinn".to_string(), "transmission".to_string());
    attributes.insert("credential".to_string(), reference.to_string());
    let reply = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "SearchItems",
            &attributes,
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) = reply
        .body()
        .deserialize()
        .map_err(|_| ErrorCode::Unavailable)?;
    let Some(item) = unlocked.first() else {
        let _ = locked;
        return Err(ErrorCode::PermissionDenied);
    };
    let items = [item.as_ref()];
    let reply = connection
        .call_method(
            Some(SERVICE),
            PATH,
            Some(INTERFACE),
            "GetSecrets",
            &(items.as_slice(), session.as_ref()),
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
    let secrets: HashMap<OwnedObjectPath, ItemSecret> = reply
        .body()
        .deserialize()
        .map_err(|_| ErrorCode::Unavailable)?;
    let Some(secret) = secrets.get(item) else {
        return Err(ErrorCode::PermissionDenied);
    };
    parse_secret(&secret.2)
}

fn parse_secret(bytes: &[u8]) -> Result<(String, String), ErrorCode> {
    let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::PermissionDenied)?;
    let (user, password) = text.split_once(':').ok_or(ErrorCode::PermissionDenied)?;
    if user.contains('\0') || password.contains('\0') {
        return Err(ErrorCode::PermissionDenied);
    }
    Ok((user.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_text_is_user_then_password() {
        assert_eq!(
            parse_secret(b"fixture:secret:more").unwrap(),
            ("fixture".into(), "secret:more".into())
        );
        assert_eq!(parse_secret(b"nosep"), Err(ErrorCode::PermissionDenied));
    }
}
