// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The recognition dry run (ADR-0128): how the rules read sample names,
//! before they are applied to the catalogue.
//!
//! ```text
//! arinn-plugin-library --check-recognition [--rules FILE] NAME...
//! ```
//!
//! `NAME` is a path relative to a library folder, such as
//! `Show 2nd Season/Show 2nd Season - 01.mkv`. `FILE` is TOML with the keys
//! of the `recognition` settings table (`season`, `episode`, `disable`).
//! Without it, the rules come from `ARINN_PROVIDER_SETTINGS`, or the
//! defaults. Nothing is read from or written to disk apart from `FILE`;
//! the output is JSON lines on stdout.

use crate::{
    parse::{Kind, parse_video_with, rule_text},
    recognition::Rules,
};
use serde_json::{Value, json};
use std::{io::Write, path::Path};

const MAX_RULES_FILE: u64 = 64 * 1024;

/// Runs the dry run and returns the process exit code: 0 when every rule is
/// valid, 1 when the rules are invalid, 2 for a usage error.
pub fn check_recognition(args: &[String], out: &mut impl Write) -> i32 {
    let mut rules_file = None;
    let mut names = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--rules" => match rest.next() {
                Some(file) if rules_file.is_none() => rules_file = Some(file.clone()),
                _ => return usage(out),
            },
            other if other.starts_with("--") => return usage(out),
            name => names.push(name.to_string()),
        }
    }
    if names.is_empty() {
        return usage(out);
    }
    let rules = match load_rules(rules_file.as_deref()) {
        Ok(rules) => rules,
        Err(detail) => {
            let _ = writeln!(
                out,
                "{}",
                json!({"error": "InvalidArgument", "detail": detail})
            );
            return 1;
        }
    };
    let listed: Vec<Value> = rules
        .season_rules()
        .iter()
        .map(|rule| json!({"id": rule.id, "kind": "season", "template": rule.template}))
        .chain(
            rules
                .episode_rules()
                .iter()
                .map(|rule| json!({"id": rule.id, "kind": "episode", "template": rule.template})),
        )
        .collect();
    let _ = writeln!(out, "{}", json!({ "rules": listed }));
    let root = Path::new("/library");
    for name in names {
        let path = root.join(name.trim_start_matches('/'));
        let mut line = json!({ "name": name });
        match parse_video_with(root, &path, &rules) {
            None => line["detail"] = json!("not a video file name"),
            Some(parsed) => {
                line["kind"] = json!(match parsed.kind {
                    Kind::Film => "film",
                    Kind::Episode => "episode",
                });
                line["title"] = json!(parsed.title);
                if parsed.kind == Kind::Episode {
                    line["series"] = json!(parsed.series_title);
                }
                if parsed.has_season {
                    line["season"] = json!(parsed.season);
                }
                if parsed.has_episode {
                    line["episode"] = json!(parsed.episode);
                }
                let ambiguous = ambiguity(&rules, &path);
                if !ambiguous.is_empty() {
                    line["ambiguous"] = Value::Array(ambiguous);
                }
            }
        }
        let _ = writeln!(out, "{line}");
    }
    0
}

/// Season rules that read a different number from the same folder or file
/// name. The `kind`, `season` and `episode` fields show what applies.
fn ambiguity(rules: &Rules, path: &Path) -> Vec<Value> {
    let mut found = Vec::new();
    for part in path.iter().skip(2) {
        let name = Path::new(part)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        let matches = rules.season_matches(&rule_text(name));
        let first = matches.first().map(|(_, season)| *season);
        if matches.iter().any(|(_, season)| Some(*season) != first) {
            found.extend(
                matches
                    .into_iter()
                    .map(|(rule, season)| json!({"part": name, "rule": rule, "season": season})),
            );
        }
    }
    found
}

fn load_rules(file: Option<&str>) -> Result<Rules, String> {
    let Some(file) = file else {
        let text = std::env::var("ARINN_PROVIDER_SETTINGS").unwrap_or_else(|_| "{}".into());
        let settings: Value = serde_json::from_str(&text)
            .map_err(|_| "ARINN_PROVIDER_SETTINGS is not JSON".to_string())?;
        return match settings.get("recognition") {
            Some(value) => Rules::from_value(value),
            None => Ok(Rules::default()),
        };
    };
    let size = std::fs::metadata(file)
        .map_err(|_| "the rules file cannot be read".to_string())?
        .len();
    if size > MAX_RULES_FILE {
        return Err("the rules file is larger than 64 KiB".into());
    }
    let text =
        std::fs::read_to_string(file).map_err(|_| "the rules file cannot be read".to_string())?;
    let table: toml::Table = toml::from_str(&text)
        .map_err(|error| format!("the rules file is not TOML: {}", error.message()))?;
    let value =
        serde_json::to_value(table).map_err(|_| "the rules file has no JSON form".to_string())?;
    Rules::from_value(&value)
}

fn usage(out: &mut impl Write) -> i32 {
    let _ = writeln!(
        out,
        "{}",
        json!({"error": "usage", "detail": "arinn-plugin-library --check-recognition [--rules FILE] NAME..."})
    );
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> (i32, Vec<Value>) {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let mut out = Vec::new();
        let code = check_recognition(&args, &mut out);
        let lines = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (code, lines)
    }

    fn rules_file(text: &str) -> tempfile_path::TempPath {
        tempfile_path::TempPath::with(text)
    }

    #[test]
    fn the_dry_run_reports_kind_series_season_and_episode() {
        let file = rules_file("season = [\"{n}-й сезон\"]\nepisode = [\"серия {n}\"]\n");
        let (code, lines) = run(&[
            "--check-recognition",
            "--rules",
            file.path(),
            "Show 2nd Season/Show 2nd Season - 01.mkv",
            "Шоу 2-й сезон/Шоу серия 03.mkv",
            "The 2nd Coming (2001).mkv",
            "notes.txt",
        ][1..]);
        assert_eq!(code, 0);
        let ids: Vec<_> = lines[0]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| rule["id"].clone())
            .collect();
        assert_eq!(ids, ["season:1", "ordinal-season", "episode:1"]);
        assert_eq!(lines[1]["kind"], "episode");
        assert_eq!(lines[1]["series"], "Show");
        assert_eq!(
            (lines[1]["season"].clone(), lines[1]["episode"].clone()),
            (json!(2), json!(1))
        );
        assert_eq!(
            (
                lines[2]["series"].clone(),
                lines[2]["season"].clone(),
                lines[2]["episode"].clone()
            ),
            (json!("Шоу"), json!(2), json!(3))
        );
        assert_eq!(lines[3]["kind"], "film");
        assert_eq!(lines[4]["detail"], "not a video file name");
    }

    #[test]
    fn invalid_and_ambiguous_rules_are_reported() {
        let bad = rules_file("season = [\"{n}\"]\n");
        let (code, lines) = run(&["--rules", bad.path(), "Show.mkv"]);
        assert_eq!(code, 1);
        assert_eq!(lines[0]["error"], "InvalidArgument");
        assert!(lines[0]["detail"].as_str().unwrap().contains("season:1"));
        let both = rules_file("season = [\"Part {n}\"]\n");
        let (code, lines) = run(&[
            "--rules",
            both.path(),
            "Show Part 1 2nd Season/Show - 01.mkv",
        ]);
        assert_eq!(code, 0);
        let ambiguous = lines[1]["ambiguous"].as_array().unwrap();
        assert_eq!(ambiguous.len(), 2);
        // `<Title> <marker>`: the rule whose match ends the folder name.
        assert_eq!(lines[1]["season"], 2);
        assert_eq!(lines[1]["series"], "Show Part 1");
        let (code, _) = run(&["--rules"]);
        assert_eq!(code, 2);
        let (code, _) = run(&[]);
        assert_eq!(code, 2);
    }

    /// A rules file in the temporary directory, removed when dropped.
    mod tempfile_path {
        pub struct TempPath(std::path::PathBuf);
        impl TempPath {
            pub fn with(text: &str) -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "arinn-recognition-{}-{}.toml",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::write(&path, text).unwrap();
                Self(path)
            }
            pub fn path(&self) -> &str {
                self.0.to_str().unwrap()
            }
        }
        impl Drop for TempPath {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }
}
