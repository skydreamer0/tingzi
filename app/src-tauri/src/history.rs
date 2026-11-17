//! The most recent dictations, kept on this computer so text that did not land can be got back.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many dictations are kept; older ones fall off the end.
const LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Seconds since 1970, for the window to show as a local time.
    pub at: u64,
    pub text: String,
    /// For a refined dictation, the text as it would have gone in without the model, when that differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// False when the text stayed in the window instead of going into an application.
    pub inserted: bool,
}

fn file() -> PathBuf {
    crate::paths::data().join("voice-history.json")
}

/// Newest first.
pub fn load() -> Vec<Item> {
    std::fs::read_to_string(file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

/// Puts `item` at the front. Saying the same thing twice in a row keeps one entry.
fn push(list: &mut Vec<Item>, item: Item) {
    if list.first().is_some_and(|latest| latest.text == item.text) {
        list.remove(0);
    }
    list.insert(0, item);
    list.truncate(LIMIT);
}

pub fn add(text: &str, before: Option<String>, inserted: bool) {
    let mut list = load();
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    push(&mut list, Item { at, text: text.to_string(), before, inserted });
    let _ = std::fs::create_dir_all(crate::paths::data());
    let _ = serde_json::to_string(&list).map(|text| std::fs::write(file(), text));
}

pub fn clear() -> bool {
    match std::fs::remove_file(file()) {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str, at: u64) -> Item {
        Item { at, text: text.into(), before: None, inserted: true }
    }

    #[test]
    fn newest_comes_first_and_the_list_stays_short() {
        let mut list = Vec::new();
        for number in 0..60 {
            push(&mut list, item(&format!("第 {number} 句"), number));
        }
        assert_eq!(list.len(), LIMIT);
        assert_eq!(list[0].text, "第 59 句");
        assert_eq!(list[LIMIT - 1].text, "第 10 句");
    }

    #[test]
    fn a_repeat_replaces_the_previous_entry() {
        let mut list = vec![item("早安", 1)];
        push(&mut list, Item { at: 2, text: "早安".into(), before: None, inserted: false });
        assert_eq!(list, vec![Item { at: 2, text: "早安".into(), before: None, inserted: false }]);
    }

    #[test]
    fn older_files_without_the_unrefined_text_still_load() {
        let list: Vec<Item> = serde_json::from_str(r#"[{"at":1,"text":"早安","inserted":true}]"#).unwrap();
        assert_eq!(list, vec![item("早安", 1)]);
        let refined = Item { before: Some("嗯早安".into()), ..item("早安。", 2) };
        let written = serde_json::to_string(&vec![refined.clone(), item("午安", 3)]).unwrap();
        assert_eq!(written.matches("before").count(), 1);
        assert_eq!(serde_json::from_str::<Vec<Item>>(&written).unwrap(), vec![refined, item("午安", 3)]);
    }
}
