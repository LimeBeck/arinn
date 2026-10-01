// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use url::Url;
#[derive(Clone)]
pub(super) struct Candidate {
    pub url: Url,
    score: u32,
}
fn candidate(base: &Url, path: &str, sizes: &str) -> Option<Candidate> {
    let mut url = base.join(path).ok()?;
    url.set_fragment(None);
    super::network::eligible(&url).ok()?;
    let score = sizes
        .split_whitespace()
        .filter_map(|s| s.split_once('x'))
        .filter_map(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .filter(|(w, h)| w == h)
        .map(|(w, _)| w.abs_diff(256))
        .min()
        .unwrap_or(512);
    Some(Candidate { url, score })
}
pub(super) fn html(base: &Url, bytes: &[u8]) -> (Vec<Candidate>, Option<Url>, usize) {
    use html5ever::tokenizer::{
        BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, states::RawKind,
    };
    use std::cell::RefCell;
    // Tokenize only: no DOM, CSS selector engine, script execution or subresources.
    // Native raw-text states keep apparent link tags inside scripts/styles inert.
    struct Links(RefCell<Vec<html5ever::tokenizer::Tag>>);
    impl TokenSink for Links {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            if let Token::TagToken(tag) = token
                && tag.kind == TagKind::StartTag
            {
                match tag.name.as_ref() {
                    "script" => return TokenSinkResult::RawData(RawKind::ScriptData),
                    "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                    "title" | "textarea" => return TokenSinkResult::RawData(RawKind::Rcdata),
                    "plaintext" => return TokenSinkResult::Plaintext,
                    "link" if self.0.borrow().len() < 32 => self.0.borrow_mut().push(tag),
                    _ => {}
                }
            }
            TokenSinkResult::Continue
        }
    }
    let input = BufferQueue::default();
    input.push_back(String::from_utf8_lossy(bytes).as_ref().into());
    let tokenizer = Tokenizer::new(Links(RefCell::new(Vec::new())), Default::default());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let links = tokenizer.sink.0.into_inner();
    let count = links.len();
    let mut icons = Vec::new();
    let mut manifest = None;
    for link in links {
        let attribute = |name: &str| {
            link.attrs
                .iter()
                .find(|a| a.name.local.as_ref() == name)
                .map(|a| a.value.as_ref())
                .unwrap_or("")
        };
        let rel: Vec<_> = attribute("rel")
            .split_ascii_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        let href = attribute("href");
        if href.is_empty() {
            continue;
        }
        if rel.iter().any(|r| r == "icon" || r == "apple-touch-icon") {
            if let Some(icon) = candidate(base, href, attribute("sizes")) {
                icons.push(icon);
            }
        } else if manifest.is_none() && rel.iter().any(|r| r == "manifest") {
            manifest = base
                .join(href)
                .ok()
                .filter(|u| super::network::eligible(u).is_ok());
        }
    }
    (icons, manifest, count)
}
pub(super) fn manifest(base: &Url, bytes: &[u8], remaining: usize) -> Vec<Candidate> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    value
        .get("icons")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .take(remaining)
        .filter_map(|v| {
            candidate(
                base,
                v.get("src")?.as_str()?,
                v.get("sizes").and_then(|v| v.as_str()).unwrap_or(""),
            )
        })
        .collect()
}
pub(super) fn order(mut icons: Vec<Candidate>, base: &Url) -> Vec<Url> {
    icons.sort_by_key(|c| c.score);
    let mut out = Vec::new();
    for icon in icons {
        if !out.contains(&icon.url) {
            out.push(icon.url);
        }
    }
    if let Ok(fallback) = base.join("/favicon.ico") {
        out.retain(|url| *url != fallback);
        out.truncate(3);
        out.push(fallback);
    } else {
        out.truncate(4);
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inert_metadata_and_manifest_prefer_large_icons() {
        let base = Url::parse("https://example.com/page/").unwrap();
        let (mut icons,manifest_url,count) = html(&base,br#"<base href="https://evil.example"><link rel="icon" sizes="16x16" href="tiny.png"><link rel="apple-touch-icon" sizes="256x256" href="large.png"><link rel="manifest" href="app.json">"#);
        assert_eq!(
            manifest_url.unwrap().as_str(),
            "https://example.com/page/app.json"
        );
        icons.extend(manifest(
            &base,
            br#"{"icons":[{"src":"manifest.png","sizes":"128x128"}]}"#,
            32 - count,
        ));
        let urls = order(icons, &base);
        assert_eq!(urls[0].as_str(), "https://example.com/page/large.png");
        assert_eq!(urls.len(), 4);
        assert_eq!(urls[3].path(), "/favicon.ico");
    }
    #[test]
    fn script_text_base_and_extra_declarations_do_not_become_candidates() {
        let base = Url::parse("https://example.invalid/").unwrap();
        let text = format!(
            "<script>var s='<link rel=icon href=/script.png>';</script><style>x='<link rel=icon href=/style.png>'</style><base href='https://other.invalid/'>{}",
            "<link rel=icon href='/same.png#ignored'>".repeat(40)
        );
        let (icons, manifest, count) = html(&base, text.as_bytes());
        assert_eq!(count, 32);
        assert!(manifest.is_none());
        let urls = order(icons, &base);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].as_str(), "https://example.invalid/same.png");
        assert_eq!(urls[1].path(), "/favicon.ico");
    }
}
