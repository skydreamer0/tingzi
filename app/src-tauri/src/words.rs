//! The user's own words and text templates, and the readings needed to work with them.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A word or template the user added. `reading` has one Zhuyin syllable per character, joined with hyphens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub reading: String,
    pub word: String,
}

/// Every reading each character has in the installed word list, most common first.
#[derive(Default)]
pub struct Lexicon {
    readings: HashMap<char, Vec<(String, f64)>>,
}

impl Lexicon {
    pub fn read(word_list: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(word_list).ok()?;
        let mut lexicon = Self::default();
        for line in text.lines() {
            // Single characters only; lines whose reading is ASCII are control entries.
            let mut parts = line.split_whitespace();
            let (Some(reading), Some(word), Some(score)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let mut characters = word.chars();
            let (Some(character), None) = (characters.next(), characters.next()) else { continue };
            let Ok(score) = score.parse::<f64>() else { continue };
            if reading.is_ascii() || reading.contains('-') {
                continue;
            }
            let known = lexicon.readings.entry(character).or_default();
            if !known.iter().any(|(seen, _)| seen == reading) {
                known.push((reading.to_string(), score));
            }
        }
        for known in lexicon.readings.values_mut() {
            known.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        Some(lexicon)
    }

    pub fn readings(&self, character: char) -> Vec<String> {
        self.readings.get(&character).map(|known| known.iter().map(|(reading, _)| reading.clone()).collect()).unwrap_or_default()
    }

    fn sounds_like(&self, character: char, reading: &str) -> bool {
        self.readings.get(&character).is_some_and(|known| known.iter().any(|(seen, _)| seen == reading))
    }
}

/// The installed word list's readings, read once and kept. None when the word list is missing.
pub fn lexicon() -> Option<&'static Lexicon> {
    static KEPT: OnceLock<Option<Lexicon>> = OnceLock::new();
    KEPT.get_or_init(|| crate::paths::word_list().and_then(|file| Lexicon::read(&file))).as_ref()
}

pub fn own_file(templates: bool) -> PathBuf {
    crate::paths::data().join(if templates { "user-templates.txt" } else { "user-phrases.txt" })
}

/// Words are `reading word score`; templates are `reading TAB text`, where the text may contain spaces.
pub fn load(file: &Path, templates: bool) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let (reading, word) = if templates {
                line.split_once('\t')?
            } else {
                let mut parts = line.split_whitespace();
                (parts.next()?, parts.next()?)
            };
            (reading.contains('-') && !word.is_empty()).then(|| Entry { reading: reading.to_string(), word: word.to_string() })
        })
        .collect()
}

/// Replaces the file in one step, so a crash never leaves half a list.
pub fn save(file: &Path, entries: &[Entry], templates: bool) -> std::io::Result<()> {
    let lines: Vec<String> = entries
        .iter()
        .filter(|entry| entry.reading.contains('-') && !entry.reading.contains(char::is_whitespace))
        .map(|entry| {
            if templates {
                format!("{}\t{}", entry.reading, entry.word.replace(['\t', '\r', '\n'], " "))
            } else {
                format!("{} {} 0", entry.reading, entry.word.split_whitespace().collect::<String>())
            }
        })
        .collect();
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let draft = file.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&draft, lines.join("\r\n") + if lines.is_empty() { "" } else { "\r\n" })?;
    std::fs::rename(&draft, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&draft);
    })
}

/// Puts the user's word wherever the heard characters have the same sounds but differ in writing.
pub fn correct(text: &str, own: &[Entry], lexicon: &Lexicon) -> String {
    let mut characters: Vec<char> = text.chars().collect();
    let mut longest_first: Vec<&Entry> = own.iter().collect();
    longest_first.sort_by_key(|entry| std::cmp::Reverse(entry.word.chars().count()));
    for entry in longest_first {
        let word: Vec<char> = entry.word.chars().collect();
        let sounds: Vec<&str> = entry.reading.split('-').collect();
        if word.len() < 2 || sounds.len() != word.len() {
            continue;
        }
        let mut at = 0;
        while at + word.len() <= characters.len() {
            let heard = &characters[at..at + word.len()];
            let same_sound = heard.iter().zip(&word).zip(&sounds).all(|((h, w), s)| h == w || lexicon.sounds_like(*h, s));
            if same_sound && heard != word.as_slice() {
                characters[at..at + word.len()].copy_from_slice(&word);
                at += word.len();
            } else {
                at += 1;
            }
        }
    }
    characters.into_iter().collect()
}

/// Applies the user's words to Traditional text.
pub fn apply(text: &str) -> String {
    let own = load(&own_file(false), false);
    if own.is_empty() || text.trim().is_empty() {
        return text.to_string();
    }
    lexicon().map_or_else(|| text.to_string(), |lexicon| correct(text, &own, lexicon))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tingzi-words-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(reading: &str, word: &str) -> Entry {
        Entry { reading: reading.into(), word: word.into() }
    }

    #[test]
    fn own_words_replace_same_sounding_characters() {
        let dir = folder();
        let list = dir.join("phrases.txt");
        std::fs::write(
            &list,
            "_ctrl_punctuation_, ， 0.0\nㄒㄧㄠˇ 小 -3.0\nㄒㄧㄠˇ 曉 -4.0\nㄇㄧㄥˊ 明 -3.0\nㄇㄧㄥˊ 名 -3.2\nㄇㄧㄥˋ 命 -3.2\nㄌㄧˇ 李 -3.0\nㄌㄧˇ 理 -3.1\nㄌㄧˇ-ㄒㄧㄤˇ 理想 -4.0\n",
        )
        .unwrap();
        let sounds = Lexicon::read(&list).unwrap();
        let own = [entry("ㄌㄧˇ-ㄒㄧㄠˇ-ㄇㄧㄥˊ", "李曉明")];
        assert_eq!(correct("請理小名回電", &own, &sounds), "請李曉明回電");
        assert_eq!(correct("請李曉明回電", &own, &sounds), "請李曉明回電");
        // 命 is read with a different tone, so it is left alone.
        assert_eq!(correct("理小命", &own, &sounds), "理小命");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn readings_come_most_common_first() {
        let dir = folder();
        let list = dir.join("phrases.txt");
        std::fs::write(&list, "ㄔㄨㄥˊ 重 -3.5\nㄓㄨㄥˋ 重 -3.0\nㄧㄠˋ 要 -3.1\nㄓㄨㄥˋ-ㄧㄠˋ 重要 -4.0\n").unwrap();
        let sounds = Lexicon::read(&list).unwrap();
        assert_eq!(sounds.readings('重'), ["ㄓㄨㄥˋ", "ㄔㄨㄥˊ"]);
        assert!(sounds.readings('好').is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn words_and_templates_survive_saving() {
        let dir = folder();
        let file = dir.join("user-phrases.txt");
        save(&file, &[entry("ㄔㄨㄥˊ-ㄧㄠˋ", "重要"), entry("ㄓㄨㄥˋ", "重")], false).unwrap();
        assert_eq!(load(&file, false), vec![entry("ㄔㄨㄥˊ-ㄧㄠˋ", "重要")]);
        assert!(load(&dir.join("missing.txt"), false).is_empty());
        let templates = dir.join("user-templates.txt");
        save(&templates, &[entry("ㄉㄧˋ-ㄓˇ", "台北市 信義區\n1 號")], true).unwrap();
        assert_eq!(load(&templates, true), vec![entry("ㄉㄧˋ-ㄓˇ", "台北市 信義區 1 號")]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
