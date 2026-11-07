// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Result, bail, ensure};
use arinn_protocol::localization::{Catalog, validate_locale};
use quick_xml::{
    Reader, Writer, XmlVersion,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_TS_BYTES: usize = 4 * 1024 * 1024;
#[derive(Default, Clone)]
pub struct Message {
    pub context: String,
    pub id: Option<String>,
    pub source: String,
    pub comment: String,
    pub note: String,
    pub numerus: bool,
    pub finished: bool,
    pub obsolete: bool,
    pub forms: Vec<String>,
}
pub struct Document {
    pub locale: String,
    pub messages: Vec<Message>,
}
#[derive(Serialize)]
pub struct Report {
    pub locale: String,
    pub finished: usize,
    pub unfinished: usize,
    pub id_based: bool,
}
fn text(value: &str, empty: bool) -> bool {
    bounded_text(value, 8192, empty)
}
fn bounded_text(value: &str, maximum: usize, empty: bool) -> bool {
    (empty || !value.is_empty())
        && value.len() <= maximum
        && !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}
fn attributes(start: &BytesStart<'_>) -> Result<BTreeMap<String, String>> {
    start
        .attributes()
        .map(|attribute| {
            let a = attribute?;
            Ok((
                String::from_utf8(a.key.as_ref().to_vec())?,
                a.normalized_value(XmlVersion::Implicit1_0)?.into_owned(),
            ))
        })
        .collect()
}
pub fn parse(input: &str) -> Result<Document> {
    ensure!(input.len() <= MAX_TS_BYTES, "TS catalogue is too large");
    let mut reader = Reader::from_str(input);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = Vec::<String>::new();
    let mut document = Document {
        locale: String::new(),
        messages: Vec::new(),
    };
    let mut context = String::new();
    let mut message: Option<Message> = None;
    let mut fields = BTreeSet::new();
    let mut named_context = false;
    let mut root = false;
    loop {
        let value = match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8(e.name().as_ref().to_vec())?;
                ensure!(stack.len() < 16, "TS nesting is too deep");
                ensure!(
                    matches!(
                        name.as_str(),
                        "TS" | "context"
                            | "name"
                            | "message"
                            | "source"
                            | "comment"
                            | "extracomment"
                            | "translation"
                            | "numerusform"
                            | "location"
                            | "translatorcomment"
                            | "oldsource"
                            | "oldcomment"
                    ) || name.starts_with("extra-"),
                    "Unsupported TS element: {name}"
                );
                let attrs = attributes(&e)?;
                if ["source", "comment", "extracomment", "translation"].contains(&name.as_str()) {
                    ensure!(
                        stack.last().map(String::as_str) == Some("message")
                            && fields.insert(name.clone()),
                        "Duplicate or misplaced message field"
                    );
                }
                match name.as_str() {
                    "TS" => {
                        ensure!(!root && stack.is_empty(), "Expected a single TS root");
                        root = true;
                        document.locale = attrs
                            .get("language")
                            .cloned()
                            .unwrap_or_default()
                            .replace('_', "-");
                    }
                    "context" => {
                        ensure!(
                            stack.last().map(String::as_str) == Some("TS"),
                            "Invalid context"
                        );
                        context.clear();
                        named_context = false;
                    }
                    "name" => {
                        ensure!(
                            !named_context && stack.last().map(String::as_str) == Some("context"),
                            "Invalid context name"
                        );
                        named_context = true;
                    }
                    "message" => {
                        ensure!(
                            message.is_none()
                                && named_context
                                && stack.last().map(String::as_str) == Some("context"),
                            "Invalid message"
                        );
                        fields.clear();
                        message = Some(Message {
                            context: context.clone(),
                            id: attrs.get("id").cloned(),
                            numerus: attrs.get("numerus").is_some_and(|v| v == "yes"),
                            ..Default::default()
                        });
                    }
                    "translation" => {
                        let m = message
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("Translation outside a message"))?;
                        let kind = attrs.get("type").map(String::as_str);
                        ensure!(
                            matches!(kind, None | Some("unfinished" | "vanished" | "obsolete")),
                            "Unsupported translation type"
                        );
                        m.finished = kind.is_none();
                        m.obsolete = matches!(kind, Some("vanished" | "obsolete"));
                        if !m.numerus {
                            m.forms.push(String::new());
                        }
                    }
                    "numerusform" => {
                        let m = message
                            .as_mut()
                            .ok_or_else(|| anyhow::anyhow!("Plural form outside a message"))?;
                        ensure!(
                            m.numerus && stack.last().map(String::as_str) == Some("translation"),
                            "Unexpected plural form"
                        );
                        m.forms.push(String::new());
                    }
                    _ => {}
                }
                stack.push(name);
                continue;
            }
            Event::End(e) => {
                let name = String::from_utf8(e.name().as_ref().to_vec())?;
                ensure!(
                    stack.pop().as_deref() == Some(name.as_str()),
                    "Unbalanced TS elements"
                );
                if name == "message" {
                    ensure!(document.messages.len() < 4096, "Too many TS messages");
                    document.messages.push(
                        message
                            .take()
                            .ok_or_else(|| anyhow::anyhow!("Invalid message end"))?,
                    );
                }
                continue;
            }
            // Qt's ordinary <!DOCTYPE TS> is harmless; external/inline entities
            // and dependencies are not part of the editable format we accept.
            Event::DocType(e) => {
                ensure!(
                    e.decode()?.trim() == "TS",
                    "External or inline XML declarations are forbidden"
                );
                continue;
            }
            Event::Text(e) => {
                quick_xml::escape::unescape(&e.xml_content(XmlVersion::Implicit1_0)?)?.into_owned()
            }
            Event::CData(e) => e.xml_content(XmlVersion::Implicit1_0)?.into_owned(),
            Event::GeneralRef(e) => {
                quick_xml::escape::unescape(&format!("&{};", e.decode()?))?.into_owned()
            }
            Event::Eof => break,
            Event::Decl(e) => {
                ensure!(e.version()?.as_ref() == b"1.0", "Only XML 1.0 is supported");
                continue;
            }
            Event::Comment(_) => continue,
            _ => bail!("Unsupported XML event"),
        };
        ensure!(
            !stack.is_empty() || value.trim().is_empty(),
            "Text outside the TS root"
        );
        if stack.last().map(String::as_str) == Some("name") && message.is_none() {
            context.push_str(&value);
        } else if let Some(m) = message.as_mut() {
            match stack.last().map(String::as_str) {
                Some("source") => m.source.push_str(&value),
                Some("comment") => m.comment.push_str(&value),
                Some("extracomment") => m.note.push_str(&value),
                Some("numerusform") => m.forms.last_mut().unwrap().push_str(&value),
                Some("translation") if !m.numerus => m.forms.last_mut().unwrap().push_str(&value),
                _ => {}
            }
        }
    }
    ensure!(root && stack.is_empty(), "Incomplete TS document");
    validate_locale(&document.locale).map_err(|_| anyhow::anyhow!("Invalid TS language tag"))?;
    Ok(document)
}

// Match Qt positions rather than replacing them successively. Translation
// reordering is allowed; missing/unknown/leading-zero positions are errors.
#[derive(PartialEq, Eq)]
struct Placeholders {
    positions: BTreeSet<(u8, bool)>,
    counts: BTreeSet<bool>,
}
fn placeholders(value: &str) -> Result<Placeholders> {
    let bytes = value.as_bytes();
    let mut positions = BTreeSet::new();
    let mut counts = BTreeSet::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            at += 1;
            continue;
        }
        at += 1;
        let localized = bytes.get(at) == Some(&b'L');
        if localized {
            at += 1;
        }
        if bytes.get(at) == Some(&b'n') {
            counts.insert(localized);
            at += 1;
            continue;
        }
        let begin = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at > begin {
            ensure!(
                bytes[begin] != b'0' && at - begin <= 2,
                "Invalid Qt placeholder"
            );
            positions.insert((value[begin..at].parse()?, localized));
        } else {
            ensure!(!localized, "Malformed localized Qt placeholder");
        }
    }
    Ok(Placeholders { positions, counts })
}
pub fn check(document: &Document) -> Result<Report> {
    let mut report = Report {
        locale: document.locale.clone(),
        finished: 0,
        unfinished: 0,
        id_based: false,
    };
    let mut source_based = false;
    let mut keys = BTreeSet::new();
    for m in document.messages.iter().filter(|m| !m.obsolete) {
        ensure!(
            text(&m.context, false)
                && text(&m.source, false)
                && text(&m.comment, true)
                && bounded_text(&m.note, 16 * 1024, true),
            "Invalid or oversized source text"
        );
        let key = if let Some(id) = &m.id {
            report.id_based = true;
            let (key, revision) = id
                .rsplit_once('@')
                .ok_or_else(|| anyhow::anyhow!("Provider ids must be key@revision"))?;
            ensure!(
                !key.is_empty()
                    && key.len() <= 128
                    && key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    && revision
                        .parse::<u32>()
                        .is_ok_and(|r| r > 0 && r.to_string() == revision),
                "Invalid provider message id"
            );
            id.clone()
        } else {
            source_based = true;
            format!("{}\0{}\0{}", m.context, m.source, m.comment)
        };
        ensure!(keys.insert(key), "Duplicate TS lookup key");
        let expected = placeholders(&m.source)?;
        ensure!(
            !expected.counts.is_empty() == m.numerus,
            "Source numerus metadata does not match its placeholders"
        );
        if !m.finished {
            report.unfinished += 1;
            continue;
        }
        ensure!(!m.forms.is_empty(), "A finished translation has no forms");
        for form in &m.forms {
            ensure!(
                text(form, false),
                "A finished translation is empty or oversized"
            );
            ensure!(
                placeholders(form)? == expected,
                "A translation changes Qt placeholders"
            );
        }
        report.finished += 1;
    }
    ensure!(
        !(report.id_based && source_based),
        "Keep shell and provider messages in separate catalogues"
    );
    Ok(report)
}
fn element(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> Result<()> {
    writer.write_event(Event::Start(BytesStart::new(name)))?;
    writer.write_event(Event::Text(BytesText::new(value)))?;
    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}
pub fn template(locale: &str, messages: &[Message]) -> Result<String> {
    validate_locale(locale).map_err(|_| anyhow::anyhow!("Invalid language tag"))?;
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;
    writer.write_event(Event::Start(BytesStart::new("TS").with_attributes([
        ("version", "2.1"),
        ("language", locale),
        ("sourcelanguage", "en"),
    ])))?;
    let mut contexts = BTreeMap::<&str, Vec<&Message>>::new();
    for message in messages.iter().filter(|m| !m.obsolete) {
        contexts.entry(&message.context).or_default().push(message);
    }
    for (context, messages) in contexts {
        writer.write_event(Event::Start(BytesStart::new("context")))?;
        element(&mut writer, "name", context)?;
        for m in messages {
            let mut start = BytesStart::new("message");
            if let Some(id) = &m.id {
                start.push_attribute(("id", id.as_str()));
            }
            if m.numerus {
                start.push_attribute(("numerus", "yes"));
            }
            writer.write_event(Event::Start(start))?;
            element(&mut writer, "source", &m.source)?;
            if !m.comment.is_empty() {
                element(&mut writer, "comment", &m.comment)?;
            }
            if !m.note.is_empty() {
                element(&mut writer, "extracomment", &m.note)?;
            }
            writer.write_event(Event::Empty(
                BytesStart::new("translation").with_attributes([("type", "unfinished")]),
            ))?;
            writer.write_event(Event::End(BytesEnd::new("message")))?;
        }
        writer.write_event(Event::End(BytesEnd::new("context")))?;
    }
    writer.write_event(Event::End(BytesEnd::new("TS")))?;
    Ok(String::from_utf8(writer.into_inner())?)
}
pub fn shell(domain: &str, locale: &str) -> Result<String> {
    let source = match domain {
        "arinn" => include_str!("../../../../apps/arinn-shell/i18n/arinn_ru.ts"),
        "music" => include_str!("../../../../apps/arinn-shell/i18n/music_ru.ts"),
        _ => bail!("Shell domain must be arinn or music"),
    };
    template(locale, &parse(source)?.messages)
}
pub fn provider(catalog: &Catalog, locale: &str) -> Result<String> {
    catalog
        .validate()
        .map_err(|_| anyhow::anyhow!("Invalid provider catalogue"))?;
    let messages = catalog
        .messages
        .iter()
        .map(|(key, value)| Message {
            // pluginId is opaque UTF-8 and may contain XML-forbidden controls.
            // The trusted directory/catalog binds identity; id-based QM ignores context.
            context: "ProviderMessages".into(),
            id: Some(format!("{key}@{}", value.revision)),
            source: value.source.clone(),
            numerus: value.numerus,
            note: format!(
                "{}{}",
                value.comment,
                value
                    .arguments
                    .iter()
                    .map(|(name, argument)| format!(
                        "\n{name}: position {}, {:?}",
                        argument.position, argument.kind
                    ))
                    .collect::<String>()
            ),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    template(locale, &messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_export_can_add_a_language_and_preserves_revision_roles_and_numerus() {
        use arinn_protocol::localization::{Argument, ArgumentKind, Message as ProviderMessage};
        let catalog = Catalog {
            format_version: 1,
            plugin_id: "third.party/radio".into(),
            source_locale: "en".into(),
            locales: vec!["ru".into()],
            messages: BTreeMap::from([(
                "connect.failed".into(),
                ProviderMessage {
                    revision: 2,
                    source: "Could not connect to %1 (%n attempt(s))".into(),
                    arguments: BTreeMap::from([(
                        "service".into(),
                        Argument {
                            position: 1,
                            kind: ArgumentKind::Text,
                        },
                    )]),
                    numerus: true,
                    comment: "Fixed connection error".into(),
                },
            )]),
        };
        let parsed = parse(&provider(&catalog, "de").unwrap()).unwrap();
        assert_eq!(parsed.messages[0].id.as_deref(), Some("connect.failed@2"));
        assert_eq!(parsed.messages[0].context, "ProviderMessages");
        assert!(parsed.messages[0].numerus);
        assert!(
            parsed.messages[0]
                .note
                .contains("service: position 1, Text")
        );
        let report = check(&parsed).unwrap();
        assert!(report.id_based);
        assert_eq!((report.finished, report.unfinished), (0, 1));
        assert_eq!(report.locale, "de");
        for plugin in ["third.party/radio", "радио", "\u{1}radio"] {
            let mut opaque = catalog.clone();
            opaque.plugin_id = plugin.into();
            assert!(opaque.validate().is_ok());
            let exported = provider(&opaque, "de").unwrap();
            assert!(!exported.contains('\u{1}'));
            assert!(check(&parse(&exported).unwrap()).is_ok());
        }
    }
    #[test]
    fn localized_qualifiers_are_preserved_and_malformed_tokens_are_rejected() {
        for (source, translation) in [("%1", "%L1"), ("Hello", "Hallo %Lx"), ("Hello", "Hallo %L")]
        {
            let xml = format!(
                "<TS language='de'><context><name>X</name><message><source>{source}</source><translation>{translation}</translation></message></context></TS>"
            );
            assert!(check(&parse(&xml).unwrap()).is_err());
        }
        let xml = "<TS language='de'><context><name>X</name><message><source>%L1 %2</source><translation>%2 %L1</translation></message></context></TS>";
        assert!(check(&parse(xml).unwrap()).is_ok());
    }
    #[test]
    fn installed_shell_exports_unfinished_context_source_pairs() {
        for domain in ["arinn", "music"] {
            let exported = shell(domain, "sr-Latn").unwrap();
            let parsed = parse(&exported).unwrap();
            let report = check(&parsed).unwrap();
            assert_eq!(report.finished, 0);
            assert!(report.unfinished > 100);
            assert_eq!(report.locale, "sr-Latn");
            assert!(!report.id_based);
        }
    }
    #[test]
    fn xml_escaping_and_disambiguation_round_trip() {
        let messages = [Message {
            context: "A & B".into(),
            source: "<Open> %1".into(),
            comment: "a \"label\"".into(),
            ..Default::default()
        }];
        let parsed = parse(&template("de", &messages).unwrap()).unwrap();
        assert_eq!(parsed.messages[0].source, messages[0].source);
        assert_eq!(parsed.messages[0].comment, messages[0].comment);
        assert_eq!(parsed.messages[0].context, messages[0].context);
    }
    #[test]
    fn checker_rejects_entities_dependencies_duplicates_and_broken_placeholders() {
        for input in [
            "<!DOCTYPE TS SYSTEM 'file:///synthetic'><TS language='de'/>",
            "<TS language='de'><dependencies/></TS>",
            "<TS language='de'><context><name>X</name><message><source>%1</source><translation>%2</translation></message></context></TS>",
            "<TS language='de'><context><name>X</name><message><source>%01</source><translation>One</translation></message></context></TS>",
            "<TS language='de'><context><name>X</name><message><source>A</source><translation>A</translation></message><message><source>A</source><translation>B</translation></message></context></TS>",
            "<TS language='de'><context>",
        ] {
            assert!(parse(input).and_then(|d| check(&d)).is_err(), "{input}");
        }
    }
}
