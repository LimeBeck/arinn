// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The provider's settings schema for `ProviderSettings1` (ADR-0065): the
//! library folders and the two scan switches of ADR-0056. Values are read at
//! start from `ARINN_PROVIDER_SETTINGS`; a change applies at the next
//! provider start. Paths are not logged.

use crate::store::{MAX_ROOTS, normalize_root};
use arinn_protocol::{settings::Setting, validation::ErrorCode};
use serde_json::{Map, Value};
use std::path::Path;

/// Titles and descriptions are provider text, shown as sent (§5.6).
pub fn schema() -> Vec<Setting> {
    vec![
        Setting {
            id: "roots".into(),
            category: "media".into(),
            title: "Library folders".into(),
            description: "Folders with films and series. Removing a folder never deletes files."
                .into(),
            kind: "path".into(),
            value: "[]".into(),
            constraints: format!(r#"{{"directory":true,"multiple":true,"max_items":{MAX_ROOTS}}}"#),
            enabled: true,
        },
        Setting {
            id: "autoscan".into(),
            category: "media".into(),
            title: "Rescan regularly".into(),
            description: "Look through the library folders again every 10 minutes.".into(),
            kind: "boolean".into(),
            value: "true".into(),
            constraints: "{}".into(),
            enabled: true,
        },
        Setting {
            id: "probe".into(),
            category: "media".into(),
            title: "Read video details".into(),
            description: "Read the duration and languages of new video files.".into(),
            kind: "boolean".into(),
            value: "true".into(),
            constraints: "{}".into(),
            enabled: true,
        },
    ]
}

/// Checks complete effective values beyond the schema: every root must be a
/// folder the provider accepts at start (ADR-0056: absolute, not `/`). A
/// valid change applies at the next start, so the answer is `Unsupported`.
pub fn check(values: &Map<String, Value>) -> Result<(), ErrorCode> {
    let roots = values
        .get("roots")
        .and_then(Value::as_array)
        .ok_or(ErrorCode::InvalidArgument)?;
    if roots.len() > MAX_ROOTS {
        return Err(ErrorCode::InvalidArgument);
    }
    for root in roots {
        let text = root.as_str().ok_or(ErrorCode::InvalidArgument)?;
        normalize_root(Path::new(text))?;
    }
    Err(ErrorCode::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arinn_protocol::settings::{effective_values, validate_schema};

    #[test]
    fn the_schema_is_valid_and_its_defaults_are_the_start_defaults() {
        let schema = schema();
        validate_schema(&schema).unwrap();
        let defaults = effective_values(&schema, &Map::new());
        let settings = crate::store::Settings::parse(&Value::Object(defaults).to_string()).unwrap();
        let start = crate::store::Settings::default();
        assert!(settings.roots.is_empty());
        assert_eq!(settings.autoscan, start.autoscan);
        assert_eq!(settings.probe, start.probe);
        let roots = schema.iter().find(|s| s.id == "roots").unwrap();
        roots
            .check_value(r#"["/srv/films","/mnt/share/"]"#)
            .unwrap();
        let many: Vec<String> = (0..=MAX_ROOTS).map(|i| format!("/srv/{i}")).collect();
        assert!(
            roots
                .check_value(&serde_json::to_string(&many).unwrap())
                .is_err()
        );
        assert!(roots.check_value(r#"["films"]"#).is_err());
    }

    #[test]
    fn a_valid_change_applies_at_the_next_start() {
        let values = |roots: &str| {
            serde_json::from_str::<Map<String, Value>>(&format!(
                r#"{{"roots":{roots},"autoscan":true,"probe":false}}"#
            ))
            .unwrap()
        };
        assert_eq!(
            check(&values(r#"["/srv/films"]"#)),
            Err(ErrorCode::Unsupported)
        );
        assert_eq!(check(&values("[]")), Err(ErrorCode::Unsupported));
        // The schema accepts `/`; the provider does not (ADR-0056).
        assert_eq!(check(&values(r#"["/"]"#)), Err(ErrorCode::InvalidArgument));
        assert_eq!(check(&Map::new()), Err(ErrorCode::InvalidArgument));
    }
}
