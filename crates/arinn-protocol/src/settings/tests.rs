// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use zbus::zvariant::{LE, serialized::Context, to_bytes};

fn setting(id: &str, kind: &str, value: &str, constraints: &str) -> Setting {
    Setting {
        id: id.into(),
        category: "media".into(),
        title: "Title".into(),
        description: String::new(),
        kind: kind.into(),
        value: value.into(),
        constraints: constraints.into(),
        enabled: true,
    }
}
fn roots() -> Setting {
    setting(
        "roots",
        KIND_PATH,
        r#"["/srv/films"]"#,
        r#"{"directory":true,"multiple":true,"max_items":8}"#,
    )
}

#[test]
fn wire_signature_round_trips() {
    assert_eq!(Setting::SIGNATURE.to_string(), "(sssssssb)");
    let bytes = to_bytes(Context::new_dbus(LE, 0), &(&roots(),)).unwrap();
    let (back,): (Setting,) = bytes.deserialize().unwrap().0;
    assert_eq!(back, roots());
}

#[test]
fn every_kind_validates_its_default() {
    let good = [
        setting("scan", KIND_BOOLEAN, "true", ""),
        setting("interval", KIND_INTEGER, "60", r#"{"min":10,"max":3600}"#),
        setting("label", KIND_STRING, r#""Films""#, r#"{"max_length":32}"#),
        setting(
            "engine",
            KIND_ENUM,
            r#""google""#,
            r#"{"options":[{"value":"google","title":"Google"},{"value":"yandex","title":"Yandex"}]}"#,
        ),
        setting(
            "cache",
            KIND_PATH,
            r#""/var/cache/arinn""#,
            r#"{"directory":true}"#,
        ),
        roots(),
    ];
    for s in &good {
        assert_eq!(s.validate(), Ok(()), "{}", s.id);
    }
    assert_eq!(validate_schema(&good), Ok(()));
}

#[test]
fn values_are_checked_against_kind_and_constraints() {
    let interval = setting("interval", KIND_INTEGER, "60", r#"{"min":10,"max":3600}"#);
    assert!(interval.check_value("10").is_ok());
    for bad in ["9", "3601", "1.5", r#""60""#, "true", "not json"] {
        assert_eq!(
            interval.check_value(bad),
            Err(ErrorCode::InvalidArgument),
            "{bad}"
        );
    }
    let engine = setting(
        "engine",
        KIND_ENUM,
        r#""google""#,
        r#"{"options":[{"value":"google","title":"Google"}]}"#,
    );
    assert_eq!(
        engine.check_value(r#""bing""#),
        Err(ErrorCode::InvalidArgument)
    );
    let label = setting("label", KIND_STRING, r#""x""#, r#"{"max_length":3}"#);
    assert!(label.check_value(r#""abc""#).is_ok());
    assert!(label.check_value(r#""abcd""#).is_err());
    assert!(label.check_value(r#""a\nb""#).is_err());
    let roots = roots();
    assert!(
        roots
            .check_value(r#"["/srv/films","/mnt/nas/series"]"#)
            .is_ok()
    );
    for bad in [
        r#""/srv/films""#,
        r#"["relative/path"]"#,
        r#"["/srv/../etc"]"#,
        r#"["//server/share"]"#,
        r#"["/a","/b","/c","/d","/e","/f","/g","/h","/i"]"#,
    ] {
        assert_eq!(
            roots.check_value(bad),
            Err(ErrorCode::InvalidArgument),
            "{bad}"
        );
    }
}

#[test]
fn schemas_with_bad_shape_are_rejected() {
    let bad = [
        // Unknown kind or category.
        setting("x", "color", "1", ""),
        Setting {
            category: "games".into(),
            ..setting("x", KIND_BOOLEAN, "true", "")
        },
        // Constraints of another kind, unknown keys, inverted range.
        setting("x", KIND_BOOLEAN, "true", r#"{"min":1}"#),
        setting("x", KIND_INTEGER, "5", r#"{"min":9,"max":1}"#),
        setting("x", KIND_INTEGER, "5", r#"{"step":2}"#),
        // Enum without options or with duplicates.
        setting("x", KIND_ENUM, r#""a""#, r#"{"options":[]}"#),
        setting(
            "x",
            KIND_ENUM,
            r#""a""#,
            r#"{"options":[{"value":"a","title":"A"},{"value":"a","title":"B"}]}"#,
        ),
        // max_items without multiple; a default that breaks the constraints.
        setting("x", KIND_PATH, r#""/a""#, r#"{"max_items":2}"#),
        setting("x", KIND_INTEGER, "0", r#"{"min":1}"#),
    ];
    for s in &bad {
        assert_eq!(s.validate(), Err(ErrorCode::InvalidArgument), "{s:?}");
    }
    let duplicate = [roots(), roots()];
    assert_eq!(validate_schema(&duplicate), Err(ErrorCode::InvalidArgument));
}

#[test]
fn effective_values_keep_valid_stored_values_and_fill_defaults() {
    let schema = [
        roots(),
        setting("scan", KIND_BOOLEAN, "true", ""),
        setting("interval", KIND_INTEGER, "60", r#"{"min":10}"#),
    ];
    let stored: Map<String, Value> =
        serde_json::from_str(r#"{"roots":["/mnt/nas"],"interval":1,"obsolete":"x"}"#).unwrap();
    let values = effective_values(&schema, &stored);
    // A valid stored value wins, an invalid one falls back to the default,
    // a missing one takes the default, unknown ids are dropped.
    assert_eq!(
        Value::Object(values),
        serde_json::json!({"roots":["/mnt/nas"],"scan":true,"interval":60})
    );
}

#[test]
fn paths() {
    assert!(path("/"));
    assert!(path("/srv/films/"));
    for bad in [
        "", "srv", "//nas/x", "/a//b", "/a/./b", "/a/../b", "/a\u{0}b",
    ] {
        assert!(!path(bad), "{bad:?}");
    }
}

// ADR-0071: an optional display unit on integer settings.
#[test]
fn integer_unit_is_short_display_text_on_integers_only() {
    let minutes = setting(
        "idle",
        KIND_INTEGER,
        "10",
        r#"{"min":1,"max":60,"unit":"min"}"#,
    );
    assert_eq!(minutes.validate(), Ok(()));
    assert_eq!(minutes.constraints().unwrap().unit.as_deref(), Some("min"));
    // A unit does not change the value: it stays a plain integer.
    assert_eq!(minutes.check_value("15"), Ok(serde_json::json!(15)));
    assert_eq!(
        minutes.check_value(r#""15 min""#),
        Err(ErrorCode::InvalidArgument)
    );

    let longest = format!(r#"{{"unit":"{}"}}"#, "x".repeat(MAX_UNIT_BYTES));
    assert_eq!(setting("n", KIND_INTEGER, "1", &longest).validate(), Ok(()));
    for bad in [
        format!(r#"{{"unit":"{}"}}"#, "x".repeat(MAX_UNIT_BYTES + 1)),
        r#"{"unit":""}"#.into(),
        r#"{"unit":" min"}"#.into(),
        r#"{"unit":"m\nin"}"#.into(),
    ] {
        assert_eq!(
            setting("n", KIND_INTEGER, "1", &bad).validate(),
            Err(ErrorCode::InvalidArgument),
            "{bad}"
        );
    }
    // Multi-byte text counts in bytes.
    assert!(unit("мин"));
    assert!(!unit(&"ё".repeat(9)));

    for (kind, value) in [
        (KIND_BOOLEAN, "true"),
        (KIND_STRING, r#""x""#),
        (KIND_PATH, r#""/srv""#),
    ] {
        assert_eq!(
            setting("n", kind, value, r#"{"unit":"min"}"#).validate(),
            Err(ErrorCode::InvalidArgument),
            "{kind}"
        );
    }
    assert_eq!(
        setting(
            "n",
            KIND_ENUM,
            r#""a""#,
            r#"{"options":[{"value":"a","title":"A"}],"unit":"min"}"#
        )
        .validate(),
        Err(ErrorCode::InvalidArgument)
    );
}
