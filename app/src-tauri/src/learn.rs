//! Learning from the user's fixes: a word typed with Zhuyin to put right what a dictation got
//! wrong is used for every later dictation.
//!
//! Nothing watches what the user types. The voice window notes its latest dictations in the Zhuyin
//! input method's learning file, and the input method, when a word it commits replaces part of one
//! of them, notes what it replaced (see `docs/specs/learning-data.md`). When the replaced part has
//! the same sounds as the word, the recogniser is taken to have misheard it, and the word joins
//! the list used to correct recognised speech. Words typed anywhere else are not learned.

use crate::history::Item;
use crate::words::{self, Entry, Lexicon};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// A word the Zhuyin input method committed over part of a dictation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Seconds since 1970.
    pub at: u64,
    /// The part of the dictation it replaced; empty when the program it was typed in would not
    /// let the input method read the text around it.
    pub said: String,
    pub typed: String,
    /// Syllables joined with hyphens.
    pub reading: String,
}

/// What was known the last time the fixes were looked at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    /// Seconds since 1970; 0 before the first look.
    at: u64,
    /// Words the user took off the list, which are not learned again.
    refused: Vec<String>,
}

fn key(reading: &str, text: &str) -> String {
    format!("{reading}\t{text}")
}

/// A fix is looked for in dictations no older than this before it was typed.
const REACH_SECONDS: u64 = 600;
/// Only the latest dictations are looked at; a fix follows closely on what it fixes.
const REACH_ITEMS: usize = 5;

/// Goes through the fixes noted since `seen` and returns what is now known together with the
/// words to start correcting speech with. `dictations` are the recent ones, newest first, used
/// only for fixes whose surroundings could not be read. `known` holds the words already used for
/// correction: the user's own and those learned before.
pub fn study(now: u64, fixes: &[Fix], dictations: &[Item], seen: &Seen, known: &[Entry], lexicon: &Lexicon) -> (Seen, Vec<Entry>) {
    let next = Seen { at: now, refused: seen.refused.clone() };
    // The first look reaches back as far as a fix can, so the first fix is not lost.
    let since = if seen.at == 0 { now.saturating_sub(REACH_SECONDS) } else { seen.at };
    let mut fresh: Vec<Entry> = Vec::new();
    for fix in fixes.iter().filter(|fix| fix.at > since) {
        let entry = Entry { reading: fix.reading.clone(), word: fix.typed.clone() };
        // One character would change too much of later dictations.
        if entry.word.chars().count() < 2
            || seen.refused.contains(&key(&entry.reading, &entry.word))
            || known.contains(&entry)
            || fresh.contains(&entry)
        {
            continue;
        }
        let one = std::slice::from_ref(&entry);
        let misheard = if fix.said.is_empty() {
            // The word would change a dictation made shortly before it was typed.
            dictations
                .iter()
                .take(REACH_ITEMS)
                .filter(|item| item.inserted && item.at <= fix.at && item.at + REACH_SECONDS >= fix.at)
                .any(|item| words::correct(&item.text, one, lexicon) != item.text)
        } else {
            // The replaced part had the same sounds, written with other characters.
            words::correct(&fix.said, one, lexicon) == fix.typed
        };
        if misheard {
            fresh.push(entry);
        }
    }
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

/// Looks for new fixes and keeps them. Nothing is learned when the Zhuyin input method's learning
/// cannot be read, as on macOS. Blocks; call it off the UI thread.
pub fn catch_up() {
    let (Some(fixes), Some(lexicon)) = (crate::sys::typed_fixes(), words::lexicon()) else { return };
    let Ok(_one) = STUDYING.lock() else { return };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let mut kept = learned();
    let known: Vec<Entry> = words::load(&words::own_file(false), false).into_iter().chain(kept.iter().cloned()).collect();
    let (next, fresh) = study(now, &fixes, &crate::history::load(), &load_seen(), &known, lexicon);
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

    const NAME: &str = "ㄌㄧˇ-ㄒㄧㄠˇ-ㄇㄧㄥˊ";

    fn fix(at: u64, said: &str, typed: &str) -> Fix {
        Fix { at, said: said.into(), typed: typed.into(), reading: NAME.into() }
    }

    fn said(text: &str, at: u64) -> Item {
        Item { at, text: text.into(), before: None, inserted: true }
    }

    fn looked(at: u64) -> Seen {
        Seen { at, refused: Vec::new() }
    }

    fn name() -> Entry {
        Entry { reading: NAME.into(), word: "李曉明".into() }
    }

    #[test]
    fn a_word_typed_over_a_misheard_part_is_learned() {
        let lexicon = sounds();
        let fixes = [fix(1100, "理小名", "李曉明")];
        let (next, fresh) = study(1200, &fixes, &[], &looked(1000), &[], &lexicon);
        assert_eq!(fresh, vec![name()]);
        assert_eq!(next.at, 1200);
        // A fix already looked at is not offered again.
        assert!(study(1300, &fixes, &[], &next, &[], &lexicon).1.is_empty());
        // The very first look still finds a fix made just before it.
        assert_eq!(study(1200, &fixes, &[], &Seen::default(), &[], &lexicon).1, vec![name()]);
    }

    #[test]
    fn a_word_that_does_not_sound_like_what_it_replaced_is_not_learned() {
        let fixes = [fix(1100, "王大明", "李曉明")];
        assert!(study(1200, &fixes, &[], &looked(1000), &[], &sounds()).1.is_empty());
    }

    #[test]
    fn without_its_surroundings_a_fix_is_checked_against_recent_dictations() {
        let lexicon = sounds();
        let fixes = [fix(1100, "", "李曉明")];
        assert_eq!(study(1200, &fixes, &[said("請理小名回電", 1005)], &looked(1000), &[], &lexicon).1, vec![name()]);
        // The dictation already had it right, said something else, or came long before.
        assert!(study(1200, &fixes, &[said("請李曉明回電", 1005)], &looked(1000), &[], &lexicon).1.is_empty());
        assert!(study(1200, &fixes, &[said("他們回電了", 1005)], &looked(1000), &[], &lexicon).1.is_empty());
        assert!(study(1200, &fixes, &[said("請理小名回電", 100)], &looked(1000), &[], &lexicon).1.is_empty());
    }

    #[test]
    fn single_characters_known_and_refused_words_are_left_alone() {
        let lexicon = sounds();
        let one = [Fix { at: 1100, said: "他".into(), typed: "她".into(), reading: "ㄊㄚ".into() }];
        assert!(study(1200, &one, &[], &looked(1000), &[], &lexicon).1.is_empty());
        let fixes = [fix(1100, "理小名", "李曉明"), fix(1150, "理小名", "李曉明")];
        assert!(study(1200, &fixes, &[], &looked(1000), &[name()], &lexicon).1.is_empty());
        let refusing = Seen { at: 1000, refused: vec![key(NAME, "李曉明")] };
        let (next, fresh) = study(1200, &fixes, &[], &refusing, &[], &lexicon);
        assert!(fresh.is_empty());
        assert_eq!(next.refused, refusing.refused);
        // The same fix made twice is learned once.
        assert_eq!(study(1200, &fixes, &[], &looked(1000), &[], &lexicon).1, vec![name()]);
    }
}
