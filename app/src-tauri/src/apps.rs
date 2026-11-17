//! Per-program rules: a terminal, a code editor or a chat box wants different handling than a document.
//!
//! A rule names a program by its file name and overrides some of the main choices while dictating into it.
//! Anything a rule leaves out follows the main settings.

use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct AppRule {
    /// The program's file name, such as Code.exe, or Safari.app on macOS; compared without regard to case.
    pub program: String,
    pub tidy: Option<bool>,
    pub simplified: Option<bool>,
    /// 0 inserts and copies, 1 only copies, 2 only shows the text.
    pub delivery_mode: Option<u32>,
    /// Chat boxes and search fields do not want a sentence's closing 「。」.
    pub drop_final_stop: bool,
}

pub fn own_file() -> PathBuf {
    crate::paths::data().join("app-rules.json")
}

/// Rules without a program are dropped, and an unknown delivery mode falls back to the main setting.
fn cleaned(rules: Vec<AppRule>) -> Vec<AppRule> {
    let mut kept: Vec<AppRule> = Vec::new();
    for mut rule in rules {
        rule.program = rule.program.trim().to_string();
        rule.delivery_mode = rule.delivery_mode.filter(|mode| *mode <= 2);
        if rule.program.is_empty() {
            continue;
        }
        // One rule per program; the later one wins.
        kept.retain(|have| have.program.to_lowercase() != rule.program.to_lowercase());
        kept.push(rule);
    }
    kept
}

pub fn load(file: &Path) -> Vec<AppRule> {
    let saved = std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    cleaned(saved)
}

/// Replaces the file in one step, so a crash never leaves half a list.
pub fn save(file: &Path, rules: &[AppRule]) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let draft = file.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&draft, serde_json::to_string_pretty(&cleaned(rules.to_vec()))?)?;
    std::fs::rename(&draft, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&draft);
    })
}

/// The rule for a program file name, if the user made one.
pub fn find<'a>(rules: &'a [AppRule], program: &str) -> Option<&'a AppRule> {
    let program = program.to_lowercase();
    rules.iter().find(|rule| rule.program.to_lowercase() == program)
}

/// The main choices with the rule's overrides on top.
pub fn effective(prefs: &Settings, rule: Option<&AppRule>) -> Settings {
    let Some(rule) = rule else { return prefs.clone() };
    Settings {
        tidy: rule.tidy.unwrap_or(prefs.tidy),
        simplified: rule.simplified.unwrap_or(prefs.simplified),
        delivery_mode: rule.delivery_mode.or(prefs.delivery_mode),
        ..prefs.clone()
    }
}

/// The last step of a dictation: takes off a closing 「。」 when the rule asks for that.
pub fn finish(text: &str, rule: Option<&AppRule>) -> String {
    match rule {
        Some(rule) if rule.drop_final_stop => {
            let text = text.trim_end();
            text.strip_suffix('。').unwrap_or(text).trim_end().to_string()
        }
        _ => text.to_string(),
    }
}

pub use crate::sys::program;

/// The programs that have a window on screen now, so the user can pick one by name. Leaves this app out.
pub fn running() -> Vec<String> {
    let mut names: Vec<String> = crate::sys::processes_on_screen().into_iter().filter(|process| *process != std::process::id()).filter_map(program).collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(program: &str) -> AppRule {
        AppRule { program: program.into(), ..AppRule::default() }
    }

    #[test]
    fn a_rule_is_found_by_file_name_in_any_case() {
        let rules = [rule("Code.exe"), rule("WindowsTerminal.exe")];
        assert_eq!(find(&rules, "code.EXE").map(|found| found.program.as_str()), Some("Code.exe"));
        assert!(find(&rules, "Code").is_none());
        assert!(find(&[], "Code.exe").is_none());
    }

    #[test]
    fn overrides_replace_only_what_the_rule_sets() {
        let prefs = Settings { tidy: true, simplified: false, delivery_mode: Some(0), keep_history: false, ..Settings::default() };
        assert_eq!(effective(&prefs, None), prefs);
        assert_eq!(effective(&prefs, Some(&rule("Code.exe"))), prefs);
        let chat = AppRule { tidy: Some(false), simplified: Some(true), delivery_mode: Some(1), ..rule("LINE.exe") };
        let changed = effective(&prefs, Some(&chat));
        assert_eq!((changed.tidy, changed.simplified, changed.delivery_mode), (false, true, Some(1)));
        assert_eq!(changed.mode(), crate::deliver::Mode::Copy);
        assert!(!changed.keep_history);
    }

    #[test]
    fn the_closing_full_stop_goes_only_when_asked() {
        let chat = AppRule { drop_final_stop: true, ..rule("LINE.exe") };
        assert_eq!(finish("明天見。", Some(&chat)), "明天見");
        assert_eq!(finish("好，明天見。 ", Some(&chat)), "好，明天見");
        assert_eq!(finish("真的嗎？", Some(&chat)), "真的嗎？");
        assert_eq!(finish("第一句。第二句", Some(&chat)), "第一句。第二句");
        assert_eq!(finish("明天見。", Some(&rule("Code.exe"))), "明天見。");
        assert_eq!(finish("明天見。", None), "明天見。");
    }

    #[test]
    fn rules_survive_saving_and_older_files_load() {
        let dir = std::env::temp_dir().join(format!("tingzi-apps-{}-{:?}", std::process::id(), std::thread::current().id()));
        let file = dir.join("app-rules.json");
        let terminal = AppRule { tidy: Some(false), delivery_mode: Some(1), ..rule("WindowsTerminal.exe") };
        // A second rule for the same program replaces the first; a bad mode and a nameless rule are dropped.
        save(&file, &[rule("code.exe"), terminal.clone(), AppRule { delivery_mode: Some(7), ..rule(" Code.exe ") }, rule("  ")]).unwrap();
        assert_eq!(load(&file), vec![terminal, rule("Code.exe")]);
        std::fs::write(&file, r#"[{"Program":"LINE.exe","DropFinalStop":true}]"#).unwrap();
        assert_eq!(load(&file), vec![AppRule { drop_final_stop: true, ..rule("LINE.exe") }]);
        std::fs::write(&file, "not json").unwrap();
        assert!(load(&file).is_empty());
        assert!(load(&dir.join("missing.json")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn this_test_program_has_a_name() {
        assert!(!program(std::process::id()).unwrap().is_empty());
        assert!(program(0).is_none());
    }
}
