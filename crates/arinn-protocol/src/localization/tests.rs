// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use serde_json::json;

fn catalog() -> Catalog {
    Catalog::from_json(
        r#"{
      "formatVersion":1,"pluginId":"third-party-radio","sourceLocale":"en","locales":["ru"],
      "messages":{
        "connect.failed":{"revision":1,"source":"Could not connect to %1",
          "arguments":{"service":{"position":1,"type":"text"}}},
        "stations.count":{"revision":2,"source":"%n station(s)","numerus":true}
      }
    }"#,
    )
    .unwrap()
}
fn request() -> TextRequest {
    TextRequest {
        scope: Scope::DownloadJob,
        object_id: "job-1".into(),
        revision: 7,
        field: "error_message".into(),
        fallback_text: "Could not connect to Radio".into(),
    }
}
fn result() -> ResolvedText {
    ResolvedText {
        request_index: 0,
        fallback_text: request().fallback_text,
        message: Reference {
            message_key: "connect.failed".into(),
            message_revision: 1,
            arguments: BTreeMap::from([("service".into(), json!("Radio"))]),
            count: None,
        },
    }
}

#[test]
fn catalog_and_references_round_trip_without_changing_plain_fallback() {
    let c = catalog();
    assert_eq!(Catalog::from_json(&c.to_json().unwrap()).unwrap(), c);
    assert_eq!(
        c.translation_id("stations.count").unwrap(),
        "stations.count@2"
    );
    let requests = vec![request()];
    assert_eq!(
        requests_from_json(&requests_to_json(&requests).unwrap()).unwrap(),
        requests
    );
    let resolved = vec![result()];
    let json = resolved_to_json(&c, &requests, &resolved).unwrap();
    assert_eq!(resolved_from_json(&c, &requests, &json).unwrap(), resolved);
    // Content/unknown fields are omitted by the handler, not translated by
    // searching the observed text. An omitted response preserves the field.
    assert_eq!(resolved_from_json(&c, &requests, "[]").unwrap(), vec![]);
}

#[test]
fn schemas_and_qt_placeholders_agree() {
    let base = catalog().messages["connect.failed"].clone();
    for bad in [
        "%0",
        "%01",
        "%100",
        "%2",
        "%L1",
        "%n",
        "%L",
        "No parameters",
    ] {
        let mut message = base.clone();
        message.source = bad.into();
        assert!(message.validate().is_err(), "{bad}");
    }
    let mut numeric = base.clone();
    numeric.arguments.get_mut("service").unwrap().kind = ArgumentKind::Integer;
    numeric.source = "%L1 items (%1)".into();
    assert!(numeric.validate().is_ok());
    numeric
        .arguments
        .insert("duplicate".into(), numeric.arguments["service"].clone());
    assert!(numeric.validate().is_err());
    let mut gap = base.clone();
    gap.source = "%2".into();
    gap.arguments.get_mut("service").unwrap().position = 2;
    assert!(gap.validate().is_err());
    let mut zero = base;
    zero.revision = 0;
    assert!(zero.validate().is_err());
}

#[test]
fn references_require_exact_keys_revisions_types_and_arguments() {
    let c = catalog();
    let good = result().message;
    assert!(good.validate(&c).is_ok());
    let mut bad = good.clone();
    bad.message_key = "unknown".into();
    assert!(bad.validate(&c).is_err());
    let mut bad = good.clone();
    bad.message_revision = 2;
    assert!(bad.validate(&c).is_err());
    let mut bad = good.clone();
    bad.arguments.clear();
    assert!(bad.validate(&c).is_err());
    let mut bad = good.clone();
    bad.arguments.insert("extra".into(), json!("x"));
    assert!(bad.validate(&c).is_err());
    let mut bad = good.clone();
    bad.arguments.insert("service".into(), json!(42));
    assert!(bad.validate(&c).is_err());
    let mut bad = good.clone();
    bad.count = Some(1);
    assert!(bad.validate(&c).is_err());
    // Placeholder-like user data remains data for the one-pass renderer.
    let mut safe = good;
    safe.arguments
        .insert("service".into(), json!("%2 <b>Radio</b>"));
    assert!(safe.validate(&c).is_ok());
}

#[test]
fn numerus_count_fits_qt_without_the_minus_one_sentinel() {
    let c = catalog();
    let mut reference = Reference {
        message_key: "stations.count".into(),
        message_revision: 2,
        arguments: BTreeMap::new(),
        count: None,
    };
    assert!(reference.validate(&c).is_err());
    for n in [0, 1, 2, 5, 21, i32::MAX as u32] {
        reference.count = Some(n);
        assert!(reference.validate(&c).is_ok());
    }
    reference.count = Some(i32::MAX as u32 + 1);
    assert!(reference.validate(&c).is_err());
}

#[test]
fn result_indices_and_observed_fallback_are_bound_to_the_request() {
    let c = catalog();
    let requests = vec![request()];
    let mut bad = result();
    bad.request_index = 1;
    assert!(validate_resolved(&c, &requests, &[bad]).is_err());
    let mut bad = result();
    bad.fallback_text = "A newer failure".into();
    assert!(validate_resolved(&c, &requests, &[bad]).is_err());
    let mut other = request();
    other.object_id = "job-2".into();
    assert!(validate_resolved(&c, &[request(), other], &[result(), result()]).is_err());
    assert!(validate_requests(&[request(), request()]).is_err());
}

#[test]
fn scopes_bind_fields_to_their_snapshot_owner() {
    let mut r = request();
    for (scope, field) in [
        (Scope::MusicEntity, "title"),
        (Scope::MusicStation, "title"),
        (Scope::MediaSession, "title"),
        (Scope::DownloadJob, "error_message"),
    ] {
        r.scope = scope;
        r.field = field.into();
        r.revision = 1;
        assert!(r.validate().is_ok());
        r.revision = 0;
        assert!(r.validate().is_err());
    }
    r.scope = Scope::Provider;
    r.object_id = "self".into();
    r.field = "name".into();
    r.revision = 0;
    assert!(r.validate().is_ok());
    r.revision = 1;
    assert!(r.validate().is_err());
    r = request();
    r.field = "title".into();
    assert!(r.validate().is_err());
    r.field = "error_message".into();
    r.object_id.clear();
    assert!(r.validate().is_err());
}

#[test]
fn locale_keys_are_exact_and_do_not_allow_paths_or_process_suffixes() {
    for locale in ["en", "ru", "pt-BR", "zh-Hant", "zh-Hant-TW", "es-419"] {
        assert!(validate_locale(locale).is_ok(), "{locale}");
    }
    for locale in [
        "",
        "EN",
        "ru_RU",
        "../../ru",
        "ru.UTF-8",
        "en-us",
        "zh-hant",
        "en-US-x-test",
        "system",
    ] {
        assert!(validate_locale(locale).is_err(), "{locale}");
    }
}

#[test]
fn json_rejects_oversize_unknown_fields_and_duplicate_keys() {
    assert!(Catalog::from_json(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
    assert!(requests_from_json(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
    let good = catalog().to_json().unwrap();
    assert!(
        Catalog::from_json(&good.replacen(
            "\"formatVersion\":1",
            "\"formatVersion\":1,\"extra\":true",
            1
        ))
        .is_err()
    );
    assert!(
        Catalog::from_json(&good.replacen(
            "\"locales\":[\"ru\"]",
            "\"locales\":[\"ru\",\"ru\"]",
            1
        ))
        .is_err()
    );
    assert!(
        Catalog::from_json(&good.replacen(
            "\"service\":{",
            "\"service\":{\"position\":1,\"type\":\"text\"},\"service\":{",
            1
        ))
        .is_err()
    );
    let requests = vec![request(); MAX_REQUESTS + 1];
    assert!(validate_requests(&requests).is_err());
    let mut bad = catalog();
    bad.messages.get_mut("connect.failed").unwrap().source =
        "x".repeat(validation::MAX_TEXT_BYTES + 1);
    assert!(bad.validate().is_err());
    let mut bad = catalog();
    bad.messages.get_mut("connect.failed").unwrap().comment = "\u{1b}".into();
    assert!(bad.validate().is_err());
}
