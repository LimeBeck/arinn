// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Packaged search engines for typed searches (ADR-0066). Engine names and
//! URLs live in packaging data, never in core code (R-01).
use arinn_protocol::{browse::search_template, validation};
use serde::Deserialize;
use std::{collections::HashSet, io::Read, path::PathBuf};

pub const SEARCH_ENGINES_FILE: &str = "arinn/search-engines.toml";
const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_ENGINES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub id: String,
    pub title: String,
    pub template: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnginesFile {
    #[serde(default)]
    engine: Vec<EngineEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineEntry {
    id: String,
    title: String,
    template: String,
}

/// Engines from the first data root that has the file, in file order.
/// Invalid or duplicate entries are skipped; an unreadable file gives none.
pub fn load_engines(roots: &[PathBuf]) -> Vec<Engine> {
    let Some(path) = roots
        .iter()
        .map(|root| root.join(SEARCH_ENGINES_FILE))
        .find(|path| path.is_file())
    else {
        return Vec::new();
    };
    let mut text = String::new();
    let read = std::fs::File::open(&path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text));
    if read.is_err() || text.len() as u64 > MAX_FILE_BYTES {
        return Vec::new();
    }
    let Ok(file) = toml::from_str::<EnginesFile>(&text) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    file.engine
        .into_iter()
        .filter(|e| {
            validation::id(&e.id).is_ok()
                && validation::text(&e.title).is_ok()
                && !e.title.is_empty()
                && search_template(&e.template).is_ok()
                && seen.insert(e.id.clone())
        })
        .take(MAX_ENGINES)
        .map(|e| Engine {
            id: e.id,
            title: e.title,
            template: e.template,
        })
        .collect()
}

/// The configured engine, or the first one when the ID is empty or unknown.
pub fn selected<'a>(engines: &'a [Engine], configured: &str) -> Option<&'a Engine> {
    engines
        .iter()
        .find(|e| e.id == configured)
        .or_else(|| engines.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engines_come_from_the_first_root_and_skip_invalid_entries() {
        let base = std::env::temp_dir().join(format!("arinn-engines-{}", uuid::Uuid::new_v4()));
        let (first, second) = (base.join("home"), base.join("system"));
        for root in [&first, &second] {
            std::fs::create_dir_all(root.join("arinn")).unwrap();
        }
        let roots = [first.clone(), second.clone()];
        assert!(load_engines(&roots).is_empty());
        std::fs::write(
            second.join(SEARCH_ENGINES_FILE),
            r#"
[[engine]]
id = "one"
title = "One"
template = "https://one.example/search?q={query}"
[[engine]]
id = "plain"
title = "Plain"
template = "http://plain.example/?q={query}"
[[engine]]
id = "one"
title = "Duplicate"
template = "https://dup.example/?q={query}"
[[engine]]
id = "two"
title = "Two"
template = "https://two.example/?q={query}"
"#,
        )
        .unwrap();
        let engines = load_engines(&roots);
        assert_eq!(
            engines.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(selected(&engines, "two").unwrap().id, "two");
        assert_eq!(selected(&engines, "").unwrap().id, "one");
        assert_eq!(selected(&engines, "gone").unwrap().id, "one");
        assert!(selected(&[], "one").is_none());
        std::fs::write(first.join(SEARCH_ENGINES_FILE), "unknown = 1").unwrap();
        assert!(load_engines(&roots).is_empty());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn the_packaged_engines_load_with_google_first() {
        let base = std::env::temp_dir().join(format!("arinn-engines-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("arinn")).unwrap();
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../packaging/browser/search-engines.toml"
            ),
            base.join(SEARCH_ENGINES_FILE),
        )
        .unwrap();
        let engines = load_engines(std::slice::from_ref(&base));
        assert_eq!(engines.len(), 3);
        assert_eq!(engines[0].id, "google");
        std::fs::remove_dir_all(base).unwrap();
    }
}
