// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Provider-private construction helpers, not a wire or SDK extension.
use arinn_protocol::localization::{Catalog, Message, Reference, ResolvedText, Scope, TextRequest};
use std::collections::BTreeMap;

pub fn catalog(plugin: &str, messages: &[(&str, &str, &str)]) -> Catalog {
    Catalog {
        format_version: 1,
        plugin_id: plugin.into(),
        source_locale: "en".into(),
        locales: vec!["ru".into()],
        messages: messages
            .iter()
            .map(|(key, source, comment)| {
                (
                    (*key).into(),
                    Message {
                        revision: 1,
                        source: (*source).into(),
                        arguments: BTreeMap::new(),
                        numerus: false,
                        comment: (*comment).into(),
                    },
                )
            })
            .collect(),
    }
}
pub fn provider_name(request: &TextRequest, original: &str) -> bool {
    request.scope == Scope::Provider
        && request.object_id == "self"
        && request.revision == 0
        && request.field == "name"
        && request.fallback_text == original
}
pub fn resolved(index: usize, request: &TextRequest, key: &str) -> ResolvedText {
    ResolvedText {
        request_index: index as u32,
        fallback_text: request.fallback_text.clone(),
        message: Reference {
            message_key: key.into(),
            message_revision: 1,
            arguments: BTreeMap::new(),
            count: None,
        },
    }
}
