//! The user's replacement rules: what the recogniser keeps hearing, and what the user wants written instead.
//!
//! Recognisers often get English names and mixed Chinese-English terms wrong in the same way every time,
//! such as "github" for "GitHub". A rule fixes that one spelling without touching anything else.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One rule: text as it was heard, and the text to put in its place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub heard: String,
    pub wanted: String,
}

pub fn own_file() -> PathBuf {
    crate::paths::data().join("user-replacements.txt")
}

/// One rule per line, `heard TAB wanted`. Lines missing either side are skipped.
pub fn load(file: &Path) -> Vec<Replacement> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let (heard, wanted) = line.split_once('\t')?;
            let (heard, wanted) = (heard.trim(), wanted.trim());
            (!heard.is_empty() && !wanted.is_empty()).then(|| Replacement { heard: heard.to_string(), wanted: wanted.to_string() })
        })
        .collect()
}

/// Replaces the file in one step, so a crash never leaves half a list.
pub fn save(file: &Path, rules: &[Replacement]) -> std::io::Result<()> {
    let flat = |text: &str| text.replace(['\t', '\r', '\n'], " ").trim().to_string();
    let lines: Vec<String> = rules
        .iter()
        .map(|rule| (flat(&rule.heard), flat(&rule.wanted)))
        .filter(|(heard, wanted)| !heard.is_empty() && !wanted.is_empty())
        .map(|(heard, wanted)| format!("{heard}\t{wanted}"))
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

/// Letters and digits only, so a Chinese character next to an English word still counts as a boundary.
fn word_character(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

/// Where a match of `heard` starting at `at` ends, if it matches there.
/// ASCII letters match in either case; a space in `heard` matches any run of whitespace, or none.
fn match_at(text: &[char], at: usize, heard: &[char]) -> Option<usize> {
    let mut here = at;
    for &wanted in heard {
        if wanted.is_whitespace() {
            while text.get(here).is_some_and(|c| c.is_whitespace()) {
                here += 1;
            }
            continue;
        }
        let &found = text.get(here)?;
        if !found.eq_ignore_ascii_case(&wanted) {
            return None;
        }
        here += 1;
    }
    // A rule for "git" must not fire inside "github" or "digit".
    let starts_word = heard.first().is_some_and(|c| word_character(*c));
    let ends_word = heard.last().is_some_and(|c| word_character(*c));
    let cut_before = starts_word && at > 0 && word_character(text[at - 1]);
    let cut_after = ends_word && text.get(here).is_some_and(|c| word_character(*c));
    (here > at && !cut_before && !cut_after).then_some(here)
}

/// Applies every rule, longest `heard` first. Text a rule has already written is not looked at again.
pub fn replace(text: &str, rules: &[Replacement]) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut longest_first: Vec<(Vec<char>, &str)> = rules
        .iter()
        .map(|rule| (rule.heard.trim().chars().collect::<Vec<char>>(), rule.wanted.as_str()))
        .filter(|(heard, wanted)| !heard.is_empty() && !wanted.is_empty())
        .collect();
    longest_first.sort_by_key(|(heard, _)| std::cmp::Reverse(heard.len()));
    // Matches as (start, end, wanted) over the original text, never overlapping.
    let mut taken: Vec<(usize, usize, &str)> = Vec::new();
    for (heard, wanted) in &longest_first {
        let mut at = 0;
        while at < characters.len() {
            let end = match_at(&characters, at, heard).filter(|end| !taken.iter().any(|(start, stop, _)| at < *stop && *start < *end));
            match end {
                Some(end) => {
                    taken.push((at, end, wanted));
                    at = end;
                }
                None => at += 1,
            }
        }
    }
    taken.sort_by_key(|(start, _, _)| *start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end, wanted) in taken {
        out.extend(&characters[at..start]);
        out.push_str(wanted);
        at = end;
    }
    out.extend(&characters[at..]);
    out
}

/// Applies the user's rules from their file.
pub fn apply(text: &str) -> String {
    let rules = load(&own_file());
    if rules.is_empty() {
        return text.to_string();
    }
    replace(text, &rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(heard: &str, wanted: &str) -> Replacement {
        Replacement { heard: heard.into(), wanted: wanted.into() }
    }

    #[test]
    fn english_words_match_only_whole_words() {
        let rules = [rule("git", "Git")];
        assert_eq!(replace("用git推上去", &rules), "用Git推上去");
        assert_eq!(replace("use git now", &rules), "use Git now");
        assert_eq!(replace("github", &rules), "github");
        assert_eq!(replace("digit", &rules), "digit");
        assert_eq!(replace("git2", &rules), "git2");
    }

    #[test]
    fn letters_match_in_either_case() {
        let rules = [rule("deadline", "deadline")];
        assert_eq!(replace("這個DeadLine很趕", &rules), "這個deadline很趕");
        assert_eq!(replace("GITHUB上面", &[rule("github", "GitHub")]), "GitHub上面");
    }

    #[test]
    fn a_space_in_the_rule_matches_any_spacing_or_none() {
        let rules = [rule("git hub", "GitHub")];
        assert_eq!(replace("GitHub", &rules), "GitHub");
        assert_eq!(replace("放到git  hub上", &rules), "放到GitHub上");
        assert_eq!(replace("git hub", &rules), "GitHub");
        assert_eq!(replace("githup", &rules), "githup");
    }

    #[test]
    fn longer_rules_win_and_replaced_text_is_left_alone() {
        let rules = [rule("hub", "Hub"), rule("git hub", "GitHub")];
        assert_eq!(replace("git hub 和 hub", &rules), "GitHub 和 Hub");
        // The text a rule writes is not fed to the next rule.
        let chain = [rule("ai", "AI 助理"), rule("助理", "幫手")];
        assert_eq!(replace("問ai", &chain), "問AI 助理");
        assert_eq!(replace("a", &[rule("a", "aa")]), "aa");
    }

    #[test]
    fn chinese_matches_exactly() {
        let rules = [rule("賴皮", "Lambda")];
        assert_eq!(replace("用賴皮函式", &rules), "用Lambda函式");
        assert_eq!(replace("無賴皮膚", &rules), "無Lambda膚");
        assert_eq!(replace("癩皮", &rules), "癩皮");
    }

    #[test]
    fn empty_rules_are_ignored() {
        let rules = [rule("", "空"), rule("字", ""), rule("  ", "空白")];
        assert_eq!(replace("一個字", &rules), "一個字");
        assert_eq!(replace("", &[rule("a", "b")]), "");
    }

    #[test]
    fn rules_survive_saving() {
        let dir = std::env::temp_dir().join(format!("tingzi-replace-{}-{:?}", std::process::id(), std::thread::current().id()));
        let file = dir.join("user-replacements.txt");
        save(&file, &[rule(" github ", "GitHub"), rule("截止\t日", "deadline\n前"), rule("", "略過")]).unwrap();
        assert_eq!(load(&file), vec![rule("github", "GitHub"), rule("截止 日", "deadline 前")]);
        std::fs::write(&file, "沒有分隔\n\tGitHub\ngit\tGit\r\n").unwrap();
        assert_eq!(load(&file), vec![rule("git", "Git")]);
        assert!(load(&dir.join("missing.txt")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
