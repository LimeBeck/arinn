// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded discovery of local language choices; the daemon never parses QM
//! or selects the language of provider data. Called on a blocking worker.
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 512;
const MAX_CHOICES: usize = 64;
const MAX_QM_BYTES: u64 = 1024 * 1024;
const MAX_PROVIDER_CHECKS: usize = 8192;

pub fn valid_language(value: &str) -> bool {
    if value == "system" {
        return true;
    }
    let mut parts = value.split('-').peekable();
    let language = parts.next().unwrap_or_default();
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    if let Some(script) = parts.peek()
        && script.len() == 4
    {
        if !script.as_bytes()[0].is_ascii_uppercase()
            || !script.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
        {
            return false;
        }
        parts.next();
    }
    if let Some(region) = parts.next()
        && !((region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase()))
            || (region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit())))
    {
        return false;
    }
    parts.next().is_none()
}

pub fn roots(user_data: &Path) -> Vec<PathBuf> {
    roots_with_dirs(user_data, std::env::var_os("XDG_DATA_DIRS"))
}
fn roots_with_dirs(user_data: &Path, configured: Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let mut result = vec![user_data.join("translations")];
    let dirs = configured
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for directory in std::env::split_paths(&dirs)
        .take(15)
        .filter(|p| p.is_absolute())
    {
        let path = directory.join("arinn/translations");
        if !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

fn entries(path: &Path) -> Vec<PathBuf> {
    let Ok(dir) = fs::read_dir(path) else {
        return vec![];
    };
    let mut entries: Vec<_> = dir
        .take(MAX_ENTRIES + 1)
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    if entries.len() > MAX_ENTRIES {
        return vec![];
    }
    entries.sort();
    entries
}
fn safe_file(root: &Path, path: &Path, maximum: u64) -> bool {
    let Ok(info) = fs::metadata(path) else {
        return false;
    };
    if !info.is_file() || info.len() == 0 || info.len() > maximum {
        return false;
    }
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}
fn native_name(root: &Path, locale: &str) -> Option<String> {
    let path = root.join("overrides").join(locale).join("language.json");
    if !safe_file(root, &path, 4096) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(4097)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if value.get("formatVersion")?.as_u64()? != 1 || value.get("locale")?.as_str()? != locale {
        return None;
    }
    let name = value.get("nativeName")?.as_str()?;
    (!name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control))
        .then(|| name.into())
}
fn provider_directories(root: &Path, directory: &Path) -> Vec<PathBuf> {
    let contained = |path: &Path| match (root.canonicalize(), path.canonicalize()) {
        (Ok(root), Ok(path)) => path.starts_with(root) && path.is_dir(),
        _ => false,
    };
    if !contained(directory) {
        return vec![];
    }
    let mut result: Vec<_> = entries(directory)
        .into_iter()
        .filter(|path| contained(path))
        .collect();
    let hashed = directory.join("by-id");
    if contained(&hashed) {
        result.extend(entries(&hashed).into_iter().filter(|path| {
            contained(path)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.len() == 64
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
        }));
    }
    result
}
fn provider_override(root: &Path, directory: &Path, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    for provider in provider_directories(root, directory) {
        for (name, maximum) in [("settings.json", 256 * 1024), ("messages.qm", 240 * 1024)] {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            if safe_file(root, &provider.join(name), maximum) {
                return true;
            }
        }
    }
    false
}

/// The saved, well-formed choice survives package removal. This makes it a
/// valid enum value without overwriting the user's desired preference.
pub fn languages(directories: &[PathBuf], saved: &str) -> Vec<(String, String)> {
    let mut remaining_provider_checks = MAX_PROVIDER_CHECKS;
    let mut found = BTreeMap::from([
        ("en".to_string(), "English".to_string()),
        ("ru".to_string(), "Русский".to_string()),
    ]);
    for root in directories.iter().take(16) {
        for path in entries(root) {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let locale = ["arinn_", "music_"]
                .into_iter()
                .find_map(|prefix| name.strip_prefix(prefix)?.strip_suffix(".qm"));
            if let Some(locale) = locale {
                let locale = locale.replace('_', "-");
                if locale != "system"
                    && valid_language(&locale)
                    && safe_file(root, &path, MAX_QM_BYTES)
                {
                    found
                        .entry(locale.clone())
                        .or_insert_with(|| native_name(root, &locale).unwrap_or(locale));
                }
            }
        }
        for path in entries(&root.join("overrides")) {
            let Some(locale) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if locale == "system" || !valid_language(locale) {
                continue;
            }
            let shell = ["arinn.qm", "music.qm"]
                .iter()
                .any(|name| safe_file(root, &path.join("shell").join(name), MAX_QM_BYTES));
            let provider = provider_override(
                root,
                &path.join("providers"),
                &mut remaining_provider_checks,
            );
            if shell || provider || native_name(root, locale).is_some() {
                found
                    .entry(locale.into())
                    .or_insert_with(|| native_name(root, locale).unwrap_or_else(|| locale.into()));
            }
        }
        for provider in provider_directories(root, &root.join("providers")) {
            if remaining_provider_checks == 0 {
                break;
            }
            for path in entries(&provider) {
                if remaining_provider_checks == 0 {
                    break;
                }
                remaining_provider_checks -= 1;
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                let candidate = name
                    .strip_prefix("messages_")
                    .and_then(|s| s.strip_suffix(".qm"))
                    .map(|s| (s, 240 * 1024))
                    .or_else(|| {
                        name.strip_prefix("settings_")
                            .and_then(|s| s.strip_suffix(".json"))
                            .map(|s| (s, 256 * 1024))
                    });
                let Some((locale, maximum)) = candidate else {
                    continue;
                };
                let locale = locale.replace('_', "-");
                if locale != "system" && valid_language(&locale) && safe_file(root, &path, maximum)
                {
                    found
                        .entry(locale.clone())
                        .or_insert_with(|| native_name(root, &locale).unwrap_or(locale));
                }
            }
        }
    }
    if saved != "system" && valid_language(saved) {
        found.entry(saved.into()).or_insert_with(|| saved.into());
    }
    let mut choices = vec![
        ("system".into(), "As in the system".into()),
        ("en".into(), "English".into()),
        ("ru".into(), "Русский".into()),
    ];
    choices.extend(
        found
            .into_iter()
            .filter(|(locale, _)| locale != "en" && locale != "ru")
            .take(MAX_CHOICES - 3),
    );
    if saved != "system"
        && valid_language(saved)
        && !choices.iter().any(|(locale, _)| locale == saved)
    {
        choices.pop();
        choices.push((saved.into(), saved.into()));
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_xdg_data_dirs_uses_the_same_defaults_as_qt() {
        let directory = Path::new("/example/user/arinn");
        let defaults = roots_with_dirs(directory, None);
        assert_eq!(defaults, roots_with_dirs(directory, Some("".into())));
        assert!(defaults.contains(&PathBuf::from("/usr/share/arinn/translations")));
        assert_eq!(
            roots_with_dirs(directory, Some("relative:/example/system".into())),
            vec![
                directory.join("translations"),
                PathBuf::from("/example/system/arinn/translations")
            ]
        );
    }
    #[test]
    fn language_tags_and_saved_missing_choices() {
        for language in [
            "system",
            "en",
            "ru",
            "pt-BR",
            "zh-Hant",
            "zh-Hant-TW",
            "es-419",
        ] {
            assert!(valid_language(language));
        }
        for language in [
            "",
            "../../ru",
            "ru_RU",
            "EN",
            "en-us",
            "en-US-x-test",
            "ru.UTF-8",
        ] {
            assert!(!valid_language(language));
        }
        assert!(languages(&[], "de").contains(&("de".into(), "de".into())));
        assert!(
            !languages(&[], "../de")
                .iter()
                .any(|(locale, _)| locale == "../de")
        );
    }
    #[test]
    fn local_partial_catalogues_are_discovered_without_parsing_qm() {
        let root = std::env::temp_dir().join(format!("arinn-languages-{}", uuid::Uuid::new_v4()));
        let directory = root.join("overrides/de/shell");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("arinn.qm"), b"opaque fixture").unwrap();
        fs::write(
            root.join("overrides/de/language.json"),
            br#"{"formatVersion":1,"locale":"de","nativeName":"Deutsch"}"#,
        )
        .unwrap();
        let choices = languages(std::slice::from_ref(&root), "system");
        assert!(choices.contains(&("de".into(), "Deutsch".into())));
        fs::write(root.join("music_fr.qm"), b"opaque fixture").unwrap();
        assert!(
            languages(std::slice::from_ref(&root), "system")
                .iter()
                .any(|(locale, _)| locale == "fr")
        );
        let hashed = root
            .join("overrides/it/providers/by-id")
            .join("a".repeat(64));
        fs::create_dir_all(&hashed).unwrap();
        fs::write(
            hashed.join("settings.json"),
            br#"{"formatVersion":1,"pluginId":"example/provider","locale":"it","settings":{}}"#,
        )
        .unwrap();
        assert!(
            languages(std::slice::from_ref(&root), "system")
                .iter()
                .any(|(locale, _)| locale == "it")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_choices_keep_builtin_and_saved_languages() {
        let root = std::env::temp_dir().join(format!("arinn-languages-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        for first in b'a'..=b'd' {
            for second in b'a'..=b'z' {
                fs::write(
                    root.join(format!(
                        "arinn_{}{}.qm",
                        char::from(first),
                        char::from(second)
                    )),
                    b"fixture",
                )
                .unwrap();
            }
        }
        let choices = languages(std::slice::from_ref(&root), "zz");
        assert_eq!(choices.len(), MAX_CHOICES);
        for expected in ["system", "en", "ru", "zz"] {
            assert!(choices.iter().any(|(code, _)| code == expected));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn provider_only_personal_and_community_languages_are_discovered() {
        let root =
            std::env::temp_dir().join(format!("arinn-provider-languages-{}", uuid::Uuid::new_v4()));
        for path in [
            "overrides/es/providers/radio/messages.qm".to_string(),
            format!(
                "overrides/pt-BR/providers/by-id/{}/messages.qm",
                "a".repeat(64)
            ),
            "providers/radio/messages_fr_CA.qm".to_string(),
            format!("providers/by-id/{}/settings_qaa.json", "b".repeat(64)),
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"bounded opaque resource").unwrap();
        }
        fs::create_dir_all(root.join("overrides/de")).unwrap();
        fs::write(
            root.join("overrides/de/language.json"),
            br#"{"formatVersion":1,"locale":"de","nativeName":"Deutsch"}"#,
        )
        .unwrap();
        let choices = languages(std::slice::from_ref(&root), "system");
        for locale in ["es", "pt-BR", "fr-CA", "qaa"] {
            assert!(choices.iter().any(|(code, _)| code == locale), "{locale}");
        }
        assert!(choices.contains(&("de".into(), "Deutsch".into())));
        fs::write(
            root.join("providers/radio/messages_tr.qm"),
            vec![0; 240 * 1024 + 1],
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::temp_dir(), root.join("providers/outside")).unwrap();
        assert!(
            !languages(std::slice::from_ref(&root), "system")
                .iter()
                .any(|(code, _)| code == "tr")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
