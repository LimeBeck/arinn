// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Managed playback preferences through the existing provider settings contract.
use arinn_protocol::{settings::Setting, validation::ErrorCode};
use serde_json::{Map, Value, json};

pub fn schema() -> Vec<Setting> {
    vec![Setting {
        id: "hardware_decoding".into(),
        category: "playback".into(),
        title: "Video decoding".into(),
        description: "Choose automatic hardware decoding or software decoding. Applies to the next video opened.".into(),
        kind: "enum".into(),
        value: json!("no").to_string(),
        constraints: json!({"options": [
            {"value": "auto", "title": "Automatic (hardware when available)"},
            {"value": "no", "title": "Software"}
        ]}).to_string(),
        enabled: true,
    }]
}

pub fn hardware_decoding(values: &Map<String, Value>) -> Result<bool, ErrorCode> {
    match values.get("hardware_decoding").and_then(Value::as_str) {
        Some("auto") => Ok(true),
        Some("no") => Ok(false),
        _ => Err(ErrorCode::InvalidArgument),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arinn_plugin_sdk::runtime::settings_from_json;
    use arinn_protocol::settings::validate_schema;

    #[test]
    fn validated_settings_preserve_software_default_and_sanitize_startup() {
        let schema = schema();
        validate_schema(&schema).unwrap();
        for stored in [
            "{}",
            "not-json",
            r#"{"hardware_decoding":true}"#,
            r#"{"hardware_decoding":"arbitrary-backend"}"#,
        ] {
            assert!(!hardware_decoding(&settings_from_json(&schema, stored)).unwrap());
        }
        assert!(
            hardware_decoding(&settings_from_json(
                &schema,
                r#"{"hardware_decoding":"auto","unknown":true}"#
            ))
            .unwrap()
        );
        for bad in [
            json!({}),
            json!({"hardware_decoding": null}),
            json!({"hardware_decoding":"vaapi"}),
        ] {
            assert_eq!(
                hardware_decoding(bad.as_object().unwrap()),
                Err(ErrorCode::InvalidArgument)
            );
        }
        assert!(schema[0].check_value(r#""auto""#).is_ok());
        assert!(schema[0].check_value(r#""no""#).is_ok());
        assert!(schema[0].check_value(r#""auto,unsafe""#).is_err());
    }
}
