// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Configurable season and episode recognition rules (ADR-0128, #658).
//!
//! A rule is a short template matched word by word against a cleaned name
//! (`.` and `_` are spaces, `[...]` groups dropped). It has exactly one number
//! slot and at least one letter in its literal text:
//!
//! - `{n}`: 1 to 4 digits, the season or episode number;
//! - `{n:ord}`: an English ordinal, the digits followed by `st`, `nd`, `rd`
//!   or `th` (`2nd`).
//!
//! Text may surround the slot inside one word (`S{n}`, `{n}-й`), and a
//! template has at most four words (`{n:ord} Season`, `Сезон {n}`). Matching
//! ignores case (Unicode) and always covers whole words, so `Season 2` never
//! matches inside `Seasonal`.
//!
//! Season rules are tried after the built-in markers (`SxxEyy`, `NxNN`,
//! `TVn`, `Snn`, `Season N`), in order: the user's rules first, then the
//! enabled defaults. Episode rules are tried before the loose episode number
//! of a season folder. The first matching rule wins, at its leftmost match.

use serde::Deserialize;
use serde_json::Value;

/// Built-in default rules: (id, template). Users disable them by id.
const DEFAULT_SEASON: &[(&str, &str)] = &[("ordinal-season", "{n:ord} Season")];
const DEFAULT_EPISODE: &[(&str, &str)] = &[];

const MAX_RULES: usize = 32;
const MAX_TEMPLATE_BYTES: usize = 64;
const MAX_WORDS: usize = 4;
const MAX_NUMBER: u32 = 9_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Number,
    Ordinal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Word {
    Literal(String),
    Slot {
        prefix: String,
        slot: Slot,
        suffix: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub template: String,
    words: Vec<Word>,
}

/// A rule's match in a text: the number and the byte range it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub number: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    season: Vec<Rule>,
    episode: Vec<Rule>,
}

impl Default for Rules {
    fn default() -> Self {
        Self::build(&Config::default()).expect("the default rules are valid")
    }
}

/// The `recognition` table of the provider settings (ADR-0064).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub season: Vec<String>,
    #[serde(default)]
    pub episode: Vec<String>,
    #[serde(default)]
    pub disable: Vec<String>,
}

impl Rules {
    /// Rules from the settings' `recognition` value. An error names the
    /// faulty rule and why, without any path.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let config: Config = serde_json::from_value(value.clone())
            .map_err(|error| format!("recognition settings: {error}"))?;
        Self::build(&config)
    }

    pub fn build(config: &Config) -> Result<Self, String> {
        for (name, list) in [
            ("season", &config.season),
            ("episode", &config.episode),
            ("disable", &config.disable),
        ] {
            if list.len() > MAX_RULES {
                return Err(format!("{name}: at most {MAX_RULES} entries"));
            }
        }
        let known = |id: &str| {
            DEFAULT_SEASON
                .iter()
                .chain(DEFAULT_EPISODE)
                .any(|(known, _)| *known == id)
        };
        for id in &config.disable {
            if !known(id) {
                return Err(format!("disable: unknown default rule \"{id}\""));
            }
        }
        let list = |kind: &str, user: &[String], defaults: &[(&str, &str)]| {
            let mut rules = Vec::new();
            for (index, template) in user.iter().enumerate() {
                let id = format!("{kind}:{}", index + 1);
                let words = parse_template(template)
                    .map_err(|why| format!("{id} \"{template}\": {why}"))?;
                rules.push(Rule {
                    id,
                    template: template.clone(),
                    words,
                });
            }
            for (id, template) in defaults {
                if config.disable.iter().any(|off| off == id) {
                    continue;
                }
                let words = parse_template(template).map_err(|why| format!("{id}: {why}"))?;
                rules.push(Rule {
                    id: (*id).into(),
                    template: (*template).into(),
                    words,
                });
            }
            Ok::<_, String>(rules)
        };
        Ok(Self {
            season: list("season", &config.season, DEFAULT_SEASON)?,
            episode: list("episode", &config.episode, DEFAULT_EPISODE)?,
        })
    }

    pub fn season_rules(&self) -> &[Rule] {
        &self.season
    }

    pub fn episode_rules(&self) -> &[Rule] {
        &self.episode
    }

    /// The first season rule, in precedence order, that matches `text`.
    pub fn season_in(&self, text: &str) -> Option<Match> {
        self.season.iter().find_map(|rule| rule.find(text))
    }

    /// The first season rule, in precedence order, with a match that ends
    /// `text`: the marker of a `<Title> <marker>` folder name.
    pub fn season_at_end(&self, text: &str) -> Option<Match> {
        self.season.iter().find_map(|rule| rule.find_at_end(text))
    }

    /// The first episode rule, in precedence order, that matches `text`.
    pub fn episode_in(&self, text: &str) -> Option<Match> {
        self.episode.iter().find_map(|rule| rule.find(text))
    }

    /// Every season rule that matches `text`, with its number, for the dry
    /// run's ambiguity report.
    pub fn season_matches<'a>(&'a self, text: &str) -> Vec<(&'a str, u32)> {
        self.season
            .iter()
            .filter_map(|rule| {
                rule.find(text)
                    .map(|found| (rule.id.as_str(), found.number))
            })
            .collect()
    }
}

impl Rule {
    /// The match that ends `text`, if any.
    pub fn find_at_end(&self, text: &str) -> Option<Match> {
        let words = split_words(text);
        let first = words.len().checked_sub(self.words.len())?;
        let tail = &text[words.get(first)?.0..];
        self.find(tail)
            .filter(|found| found.start == 0)
            .map(|found| Match {
                number: found.number,
                start: found.start + words[first].0,
                end: found.end + words[first].0,
            })
    }

    /// The leftmost match in `text`, a cleaned name whose words are separated
    /// by single spaces.
    pub fn find(&self, text: &str) -> Option<Match> {
        let words = split_words(text);
        if words.len() < self.words.len() {
            return None;
        }
        'start: for first in 0..=words.len() - self.words.len() {
            let mut number = None;
            for (offset, word) in self.words.iter().enumerate() {
                let (_, _, actual) = &words[first + offset];
                match word {
                    Word::Literal(literal) => {
                        if actual != literal {
                            continue 'start;
                        }
                    }
                    Word::Slot {
                        prefix,
                        slot,
                        suffix,
                    } => {
                        let Some(middle) = actual
                            .strip_prefix(prefix.as_str())
                            .and_then(|rest| rest.strip_suffix(suffix.as_str()))
                        else {
                            continue 'start;
                        };
                        let Some(value) = slot_value(middle, *slot) else {
                            continue 'start;
                        };
                        number = Some(value);
                    }
                }
            }
            let number = number?;
            let start = words[first].0;
            let end = words[first + self.words.len() - 1].1;
            return Some(Match { number, start, end });
        }
        None
    }
}

/// Words of `text` with their byte ranges and lowercase form.
fn split_words(text: &str) -> Vec<(usize, usize, String)> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(from) = start.take() {
                words.push((from, index, text[from..index].to_lowercase()));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        words.push((from, text.len(), text[from..].to_lowercase()));
    }
    words
}

fn slot_value(text: &str, slot: Slot) -> Option<u32> {
    let digits = match slot {
        Slot::Number => text,
        Slot::Ordinal => {
            let (digits, suffix) = text.split_at(text.find(|ch: char| !ch.is_ascii_digit())?);
            let expected = ordinal_suffix(digits.parse().ok()?);
            (suffix == expected).then_some(digits)?
        }
    };
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: u32 = digits.parse().ok()?;
    (1..=MAX_NUMBER).contains(&value).then_some(value)
}

/// `1st`, `2nd`, `3rd`, `4th`, `11th`, `12th`, `13th`, `21st`.
fn ordinal_suffix(value: u32) -> &'static str {
    match (value % 10, value % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

fn parse_template(template: &str) -> Result<Vec<Word>, String> {
    if template.len() > MAX_TEMPLATE_BYTES {
        return Err(format!("longer than {MAX_TEMPLATE_BYTES} bytes"));
    }
    let raw: Vec<&str> = template.split_whitespace().collect();
    if raw.is_empty() {
        return Err("empty".into());
    }
    if raw.len() > MAX_WORDS {
        return Err(format!("more than {MAX_WORDS} words"));
    }
    let mut words = Vec::new();
    let mut slots = 0;
    let mut letters = false;
    for word in raw {
        let lower = word.to_lowercase();
        let found = [("{n:ord}", Slot::Ordinal), ("{n}", Slot::Number)]
            .into_iter()
            .find_map(|(marker, slot)| lower.find(marker).map(|at| (at, marker, slot)));
        let (literal_parts, parsed) = match found {
            Some((at, marker, slot)) => {
                slots += 1;
                let prefix = lower[..at].to_string();
                let suffix = lower[at + marker.len()..].to_string();
                (
                    vec![prefix.clone(), suffix.clone()],
                    Word::Slot {
                        prefix,
                        slot,
                        suffix,
                    },
                )
            }
            None => (vec![lower.clone()], Word::Literal(lower)),
        };
        for part in &literal_parts {
            if part.contains(['{', '}']) {
                return Err("a word has more than one slot or a stray brace".into());
            }
            if part.chars().any(char::is_alphabetic) {
                letters = true;
            }
        }
        if let Word::Slot {
            suffix,
            slot: Slot::Ordinal,
            ..
        } = &parsed
            && suffix.starts_with(|ch: char| ch.is_ascii_alphanumeric())
        {
            return Err("text right after {n:ord} would join its suffix".into());
        }
        if let Word::Slot {
            prefix,
            suffix,
            slot: Slot::Number,
        } = &parsed
            && (prefix.ends_with(|ch: char| ch.is_ascii_digit())
                || suffix.starts_with(|ch: char| ch.is_ascii_digit()))
        {
            return Err("a digit right next to {n} makes the number ambiguous".into());
        }
        words.push(parsed);
    }
    match slots {
        0 => Err("no {n} or {n:ord} slot".into()),
        1 if letters => Ok(words),
        1 => Err("needs a letter outside the slot, or it matches any number".into()),
        _ => Err("more than one slot".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rules(value: Value) -> Result<Rules, String> {
        Rules::from_value(&value)
    }

    #[test]
    fn the_ordinal_default_matches_whole_words() {
        let rules = Rules::default();
        let found = rules.season_in("Synthetic Show 2nd Season - 01").unwrap();
        assert_eq!(found.number, 2);
        assert_eq!(
            &"Synthetic Show 2nd Season - 01"[found.start..found.end],
            "2nd Season"
        );
        assert_eq!(rules.season_in("Show 21ST season").unwrap().number, 21);
        assert_eq!(rules.season_in("Show 11th Season").unwrap().number, 11);
        // A wrong ordinal suffix, a joined word and no season word.
        for text in [
            "Show 2th Season",
            "Show 2nd Seasonal",
            "The 2nd Coming",
            "Show 2nd",
            "Show 0th Season",
        ] {
            assert!(rules.season_in(text).is_none(), "{text}");
        }
    }

    #[test]
    fn user_rules_come_first_and_defaults_can_be_disabled() {
        let rules = rules(json!({"season": ["{n}-й сезон", "Part {n}"], "episode": ["Серия {n}"]}))
            .unwrap();
        let ids: Vec<_> = rules
            .season_rules()
            .iter()
            .map(|rule| rule.id.as_str())
            .collect();
        assert_eq!(ids, ["season:1", "season:2", "ordinal-season"]);
        assert_eq!(rules.season_in("Шоу 3-й Сезон").unwrap().number, 3);
        assert_eq!(rules.episode_in("Шоу серия 07").unwrap().number, 7);
        let off = super::Rules::from_value(&json!({"disable": ["ordinal-season"]})).unwrap();
        assert!(off.season_in("Show 2nd Season").is_none());
        // Ambiguity is visible: two rules, two numbers.
        let both = rules_with(&["Part {n}", "{n:ord} Season"]);
        assert_eq!(
            both.season_matches("Show Part 1 2nd Season"),
            [("season:1", 1), ("season:2", 2), ("ordinal-season", 2)]
        );
    }

    fn rules_with(season: &[&str]) -> Rules {
        Rules::from_value(&json!({"season": season})).unwrap()
    }

    #[test]
    fn invalid_rules_are_rejected_with_a_reason() {
        for (value, why) in [
            (json!({"season": ["{n}"]}), "needs a letter"),
            (json!({"season": ["Season"]}), "no {n}"),
            (json!({"season": ["{n} and {n}"]}), "more than one slot"),
            (
                json!({"season": ["S{n}{n}"]}),
                "more than one slot or a stray brace",
            ),
            (json!({"season": ["a b c d e {n}"]}), "more than 4 words"),
            (
                json!({"season": ["x".repeat(65) + "{n}"]}),
                "longer than 64 bytes",
            ),
            (
                json!({"season": ["{n:ord}x Season"]}),
                "would join its suffix",
            ),
            (json!({"season": ["S1{n}"]}), "ambiguous"),
            (json!({"season": [""]}), "empty"),
            (json!({"disable": ["no-such-rule"]}), "unknown default rule"),
            (json!({"seasons": []}), "unknown field"),
            (json!({"season": vec!["S{n}"; 33]}), "at most 32"),
        ] {
            let error = rules(value.clone()).unwrap_err();
            assert!(error.contains(why), "{value}: {error}");
        }
    }

    #[test]
    fn numbers_are_bounded() {
        let rules = rules_with(&["Vol {n}"]);
        assert_eq!(rules.season_in("Show Vol 0007").unwrap().number, 7);
        assert!(rules.season_in("Show Vol 0").is_none());
        assert!(rules.season_in("Show Vol 12345").is_none());
        assert!(rules.season_in("Show Vol 2a").is_none());
    }
}
