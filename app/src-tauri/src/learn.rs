//! Learning from the user's fixes: a word typed with Zhuyin to put right what a dictation got
//! wrong is used for every later dictation.
//!
//! Nothing watches what the user types. Two things already kept on this computer are compared:
//! the recent dictations, and the words the Zhuyin input method learned because the user picked
//! their characters one by one. When such a word turns up, or is typed again, soon after a
//! dictation that wrote the same sounds with other characters, the recogniser is taken to have
//! misheard it, and the word joins the list used to correct recognised speech.

use crate::history::Item;
use crate::words::{self, Entry, Lexicon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// A word the Zhuyin input method learned, with how many times it was typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typed {
    pub reading: String,
    pub text: String,
    pub hits: u32,
}

/// What was known the last time the two lists were compared.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// Seconds since 1970; 0 before the first comparison.
    at: u64,
    /// How often each learned word had been typed, by `reading TAB text`.
    hits: BTreeMap<String, u32>,
    /// Words the user took off the list, which are not learned again.
    refused: Vec<String>,
}

fn key(reading: &str, text: &str) -> String {
    format!("{reading}\t{text}")
}

/// A fix is looked for in dictations no older than this before the last comparison.
const REACH_SECONDS: u64 = 600;
/// Only the latest dictations are looked at; a fix follows closely on what it fixes.
const REACH_ITEMS: usize = 5;

/// Compares the words typed since `seen` with the recent dictations, newest first, and returns
/// what is now known together with the words to start correcting speech with.
/// `known` holds the words already used for correction: the user's own and those learned before.
pub fn study(now: u64, typed: &[Typed], dictations: &[Item], seen: &Seen, known: &[Entry], lexicon: &Lexicon) -> (Seen, Vec<Entry>) {
    let next = Seen {
        at: now,
        hits: typed.iter().map(|word| (key(&word.reading, &word.text), word.hits)).collect(),
        refused: seen.refused.clone(),
    };
    // The first comparison only notes what is there; words typed long ago say nothing about today's dictations.
    if seen.at == 0 {
        return (next, Vec::new());
    }
    let recent: Vec<&Item> = dictations.iter().take(REACH_ITEMS).filter(|item| item.at + REACH_SECONDS >= seen.at).collect();
    let fresh = typed
        .iter()
        .filter(|word| word.hits > seen.hits.get(&key(&word.reading, &word.text)).copied().unwrap_or(0))
        .filter(|word| !seen.refused.contains(&key(&word.reading, &word.text)))
        .map(|word| Entry { reading: word.reading.clone(), word: word.text.clone() })
        .filter(|entry| !known.contains(entry))
        // The dictation wrote these sounds with other characters, so the word would change it.
        .filter(|entry| recent.iter().any(|item| words::correct(&item.text, std::slice::from_ref(entry), lexicon) != item.text))
        .collect();
    (next, fresh)
}

fn learned_file() -> PathBuf {
    crate::paths::data().join("voice-learned.txt")
}

fn seen_file() -> PathBuf {
    crate::paths::data().join("voice-learning.json")
}

fn load_seen() -> Seen {
    std::fs::read_to_string(seen_file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn store_seen(seen: &Seen) {
    let _ = std::fs::create_dir_all(crate::paths::data());
    let _ = serde_json::to_string(seen).map(|text| std::fs::write(seen_file(), text));
}

/// The words learned from fixes, in the order they were learned.
pub fn learned() -> Vec<Entry> {
    words::load(&learned_file(), false)
}

/// Words learned since the last dictation finished, for the window to mention once.
static FRESH: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// One comparison at a time, so two recordings started close together do not learn a word twice.
static STUDYING: Mutex<()> = Mutex::new(());

/// Looks for new fixes and keeps them. `typed` is None when the Zhuyin input method's learning
/// cannot be read, as on macOS; nothing is learned then. Blocks; call it off the UI thread.
pub fn catch_up(typed: Option<Vec<Typed>>) {
    let (Some(typed), Some(lexicon)) = (typed, words::lexicon()) else { return };
    let Ok(_one) = STUDYING.lock() else { return };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let mut kept = learned();
    let known: Vec<Entry> = words::load(&words::own_file(false), false).into_iter().chain(kept.iter().cloned()).collect();
    let (next, fresh) = study(now, &typed, &crate::history::load(), &load_seen(), &known, lexicon);
    if !fresh.is_empty() {
        kept.extend(fresh.iter().cloned());
        if words::save(&learned_file(), &kept, false).is_err() {
            // Not noted as seen, so the words are found again next time.
            return;
        }
        if let Ok(mut told) = FRESH.lock() {
            told.extend(fresh.into_iter().map(|entry| entry.word));
        }
    }
    store_seen(&next);
}

/// The words learned since this was last asked, to mention when a dictation finishes.
pub fn take_fresh() -> Vec<String> {
    FRESH.lock().map(|mut told| std::mem::take(&mut *told)).unwrap_or_default()
}

/// Puts the learned words wherever recognised Traditional text has the same sounds in other characters.
pub fn apply(text: &str) -> String {
    let kept = learned();
    if kept.is_empty() || text.trim().is_empty() {
        return text.to_string();
    }
    words::lexicon().map_or_else(|| text.to_string(), |lexicon| words::correct(text, &kept, lexicon))
}

/// Takes a word off the list for good: it is not learned again however often it is typed.
pub fn forget(reading: &str, word: &str) -> bool {
    let Ok(_one) = STUDYING.lock() else { return false };
    let mut kept = learned();
    let before = kept.len();
    kept.retain(|entry| !(entry.reading == reading && entry.word == word));
    if kept.len() == before || words::save(&learned_file(), &kept, false).is_err() {
        return false;
    }
    let mut seen = load_seen();
    let refused = key(reading, word);
    if !seen.refused.contains(&refused) {
        seen.refused.push(refused);
    }
    store_seen(&seen);
    true
}

/// Forgets every learned word, and which ones were refused. False when a file could not be removed.
pub fn clear() -> bool {
    let Ok(_one) = STUDYING.lock() else { return false };
    [learned_file(), seen_file()].into_iter().all(|file| match std::fs::remove_file(file) {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sounds() -> Lexicon {
        let dir = std::env::temp_dir().join(format!("tingzi-learn-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("phrases.txt");
        std::fs::write(&list, "ㄒㄧㄠˇ 小 -3.0\nㄒㄧㄠˇ 曉 -4.0\nㄇㄧㄥˊ 明 -3.0\nㄇㄧㄥˊ 名 -3.2\nㄌㄧˇ 李 -3.0\nㄌㄧˇ 理 -3.1\nㄊㄚ 他 -2.0\nㄊㄚ 她 -2.5\nㄇㄣ˙ 們 -2.0\n").unwrap();
        let lexicon = Lexicon::read(&list).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        lexicon
    }

    fn typed(reading: &str, text: &str, hits: u32) -> Typed {
        Typed { reading: reading.into(), text: text.into(), hits }
    }

    fn said(text: &str, at: u64) -> Item {
        Item { at, text: text.into(), inserted: true }
    }

    const NAME: &str = "ㄌㄧˇ-ㄒㄧㄠˇ-ㄇㄧㄥˊ";

    fn after(at: u64, words: &[Typed]) -> Seen {
        study(at, words, &[], &Seen::default(), &[], &sounds()).0
    }

    #[test]
    fn a_word_typed_after_a_misheard_dictation_is_learned() {
        let lexicon = sounds();
        let seen = after(1000, &[]);
        let dictations = [said("請理小名回電", 1005)];
        let (next, fresh) = study(1100, &[typed(NAME, "李曉明", 1)], &dictations, &seen, &[], &lexicon);
        assert_eq!(fresh, vec![Entry { reading: NAME.into(), word: "李曉明".into() }]);
        assert_eq!(next.at, 1100);
        // Nothing new was typed since, so the same word is not offered again.
        assert!(study(1200, &[typed(NAME, "李曉明", 1)], &dictations, &next, &[], &lexicon).1.is_empty());
        // Typing a word once more counts, too, when it was known before the dictation.
        let before = after(1000, &[typed(NAME, "李曉明", 3)]);
        assert_eq!(study(1100, &[typed(NAME, "李曉明", 4)], &dictations, &before, &[], &lexicon).1.len(), 1);
    }

    #[test]
    fn the_first_comparison_learns_nothing() {
        let dictations = [said("請理小名回電", 1005)];
        let (next, fresh) = study(1100, &[typed(NAME, "李曉明", 2)], &dictations, &Seen::default(), &[], &sounds());
        assert!(fresh.is_empty());
        assert_eq!(next.hits.get(&key(NAME, "李曉明")), Some(&2));
    }

    #[test]
    fn a_word_is_only_learned_from_a_dictation_it_would_change() {
        let lexicon = sounds();
        let seen = after(1000, &[]);
        let word = [typed(NAME, "李曉明", 1)];
        // The dictation already had it right, or said something else.
        assert!(study(1100, &word, &[said("請李曉明回電", 1005)], &seen, &[], &lexicon).1.is_empty());
        assert!(study(1100, &word, &[said("他們回電了", 1005)], &seen, &[], &lexicon).1.is_empty());
        assert!(study(1100, &word, &[], &seen, &[], &lexicon).1.is_empty());
        // A dictation from long before the last comparison is not what was being fixed.
        assert!(study(5000, &word, &[said("請理小名回電", 100)], &after(4000, &[]), &[], &lexicon).1.is_empty());
    }

    #[test]
    fn known_and_refused_words_are_left_alone() {
        let lexicon = sounds();
        let seen = after(1000, &[]);
        let dictations = [said("請理小名回電", 1005)];
        let word = [typed(NAME, "李曉明", 1)];
        let own = [Entry { reading: NAME.into(), word: "李曉明".into() }];
        assert!(study(1100, &word, &dictations, &seen, &own, &lexicon).1.is_empty());
        let refusing = Seen { refused: vec![key(NAME, "李曉明")], ..seen };
        let (next, fresh) = study(1100, &word, &dictations, &refusing, &[], &lexicon);
        assert!(fresh.is_empty());
        assert_eq!(next.refused, refusing.refused);
    }
}
