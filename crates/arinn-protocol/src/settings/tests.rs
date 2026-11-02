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

fn translated_schema() -> Vec<Setting> {
    vec![
        roots(),
        setting(
            "decoding",
            KIND_ENUM,
            r#""no""#,
            r#"{"options":[{"value":"auto","title":"Automatic"},{"value":"no","title":"Software"}]}"#,
        ),
        setting(
            "delay",
            KIND_INTEGER,
            "5",
            r#"{"min":0,"max":60,"unit":"s"}"#,
        ),
        setting("count", KIND_INTEGER, "5", r#"{"min":0,"max":60}"#),
    ]
}
fn title(text: &str) -> Text {
    Text {
        title: Some(text.into()),
        ..Text::default()
    }
}

/// ADR-0129: a table round-trips as JSON and validates against its schema.
#[test]
fn translations_round_trip_and_validate() {
    let table = Translations::default()
        .with("ru", "roots", title("Папки медиатеки"))
        .with(
            "ru",
            "decoding",
            Text {
                description: Some("Как декодировать видео".into()),
                options: [("auto".to_owned(), "Автоматически".to_owned())].into(),
                ..Text::default()
            },
        )
        .with(
            "ru",
            "delay",
            Text {
                unit: Some("с".into()),
                ..Text::default()
            },
        )
        .with("pt_BR", "roots", title("Pastas da biblioteca"));
    assert_eq!(table.validate(&translated_schema()), Ok(()));
    assert_eq!(Translations::from_json(&table.to_json()).unwrap(), table);
    assert_eq!(Translations::default().to_json(), "{}");
    assert!(Translations::default().is_empty());
}

#[test]
fn translations_that_do_not_fit_the_schema_are_rejected() {
    let schema = translated_schema();
    let bad = [
        // Unknown setting, option value, field and locale key.
        r#"{"ru":{"missing":{"title":"Нет"}}}"#,
        r#"{"ru":{"decoding":{"options":{"vaapi":"VA-API"}}}}"#,
        r#"{"ru":{"roots":{"label":"Папки"}}}"#,
        r#"{"russian":{"roots":{"title":"Папки"}}}"#,
        r#"{"RU":{"roots":{"title":"Папки"}}}"#,
        r#"{"ru_ru":{"roots":{"title":"Папки"}}}"#,
        r#"{"ru-RU":{"roots":{"title":"Папки"}}}"#,
        // A unit only where the integer has one; options only on an enum.
        r#"{"ru":{"count":{"unit":"шт"}}}"#,
        r#"{"ru":{"roots":{"unit":"шт"}}}"#,
        r#"{"ru":{"roots":{"options":{"auto":"Авто"}}}}"#,
        r#"{"ru":{"delay":{"unit":" с"}}}"#,
        // No empty titles, no NUL.
        r#"{"ru":{"roots":{"title":""}}}"#,
        r#"{"ru":{"decoding":{"options":{"auto":""}}}}"#,
        "{\"ru\":{\"roots\":{\"title\":\"a\\u0000b\"}}}",
        // Not a table.
        r#"["ru"]"#,
        "not json",
    ];
    for json in bad {
        let checked = Translations::from_json(json).and_then(|t| t.validate(&schema));
        assert_eq!(checked, Err(ErrorCode::InvalidArgument), "{json}");
    }
    let mut many = Translations::default();
    for i in 0..=translations::MAX_LOCALES {
        let key = format!(
            "{}{}",
            (b'a' + (i / 26) as u8) as char,
            (b'a' + (i % 26) as u8) as char
        );
        many = many.with(&key, "roots", title("x"));
    }
    assert_eq!(many.validate(&schema), Err(ErrorCode::InvalidArgument));
    let huge = "x".repeat(translations::MAX_TRANSLATION_BYTES + 1);
    assert_eq!(
        Translations::from_json(&huge),
        Err(ErrorCode::InvalidArgument)
    );
}

/// Each field comes from the full locale, then its language; what neither
/// has stays absent so the caller shows the source text.
#[test]
fn lookup_falls_back_from_territory_to_language_to_source() {
    let table = Translations::default()
        .with(
            "pt",
            "decoding",
            Text {
                title: Some("Decodificação".into()),
                description: Some("Como decodificar".into()),
                options: [
                    ("auto".to_owned(), "Automático".to_owned()),
                    ("no".to_owned(), "Software".to_owned()),
                ]
                .into(),
                ..Text::default()
            },
        )
        .with(
            "pt_BR",
            "decoding",
            Text {
                title: Some("Decodificação de vídeo".into()),
                options: [("auto".to_owned(), "Automática".to_owned())].into(),
                ..Text::default()
            },
        );
    let brazil = table.lookup("pt_BR", "decoding");
    assert_eq!(brazil.title.as_deref(), Some("Decodificação de vídeo"));
    assert_eq!(brazil.description.as_deref(), Some("Como decodificar"));
    assert_eq!(brazil.options["auto"], "Automática");
    assert_eq!(brazil.options["no"], "Software");
    assert_eq!(
        table.lookup("pt_PT", "decoding").title.as_deref(),
        Some("Decodificação")
    );
    assert_eq!(table.lookup("pt", "decoding").options["auto"], "Automático");
    assert_eq!(table.lookup("ru_RU", "decoding"), Text::default());
    assert_eq!(table.lookup("pt_BR", "roots"), Text::default());
}

#[test]
fn locale_keys() {
    for good in ["ru", "en", "fil", "pt_BR", "ru_RU"] {
        assert!(translations::locale(good), "{good}");
    }
    for bad in [
        "", "r", "russ", "RU", "ru_", "ru_R", "ru_RUS", "ru-RU", "ru_ru", "ru.UTF-8", "C",
    ] {
        assert!(!translations::locale(bad), "{bad}");
    }
}
