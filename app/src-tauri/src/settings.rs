//! The user's preferences, in the same file the earlier voice window used, so an upgrade keeps them.

use crate::deliver::Mode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    /// True sends recordings to Codex; false uses the recogniser on this computer.
    pub online: bool,
    /// 3 is Ctrl+Alt, 5 is Alt+Shift, 6 is Ctrl+Shift.
    pub modifiers: u32,
    /// A Windows virtual key from A to Z, or 0 when the shortcut is off.
    pub record_key: u32,
    /// Preview text size in points: 18, 20 or 24.
    pub text_size: u32,
    pub microphone_id: Option<String>,
    pub microphone_name: Option<String>,
    pub auto_insert: bool,
    pub simplified: bool,
    /// Whether recognised speech is corrected with the user's own words.
    pub use_phrases: bool,
    /// 0 follows Windows, 1 is light, 2 is dark.
    pub appearance: u32,
    pub system_accent: bool,
    /// 0 inserts and copies, 1 only copies, 2 only shows the text.
    pub delivery_mode: Option<u32>,
    /// True records while the shortcut is held and sends on release; a quick tap still latches.
    pub hold_to_talk: bool,
    /// Whether hesitation sounds are dropped and punctuation repaired.
    pub tidy: bool,
    /// Whether recent dictations are kept on this computer.
    pub keep_history: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            online: false,
            modifiers: 3,
            record_key: u32::from(b'R'),
            text_size: 20,
            microphone_id: None,
            microphone_name: None,
            auto_insert: true,
            simplified: false,
            use_phrases: true,
            appearance: 0,
            system_accent: false,
            delivery_mode: None,
            hold_to_talk: false,
            tidy: true,
            keep_history: true,
        }
    }
}

impl Settings {
    fn file() -> PathBuf {
        crate::paths::data().join("voice-settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .filter(|saved| saved.problem().is_none())
            .unwrap_or_default()
    }

    /// Why these choices cannot be used, in words for the settings page; None when they are fine.
    pub fn problem(&self) -> Option<&'static str> {
        let key_ok = self.record_key == 0 || (u32::from(b'A')..=u32::from(b'Z')).contains(&self.record_key);
        if !matches!(self.modifiers, 3 | 5 | 6) || !key_ok {
            return Some("請選擇 A 到 Z 的錄音鍵。");
        }
        if self.modifiers == 3 && self.record_key == u32::from(b'V') {
            return Some("Ctrl+Alt+V 用於貼入草稿，請選其他錄音鍵。");
        }
        if self.modifiers == 5 && self.record_key == u32::from(b'Z') {
            return Some("Alt+Shift+Z 用於貼上上一則，請選其他錄音鍵。");
        }
        if !matches!(self.text_size, 18 | 20 | 24) || self.appearance > 2 || self.delivery_mode.is_some_and(|mode| mode > 2) {
            return Some("設定內容不完整，請重新選擇。");
        }
        None
    }

    /// Writes the file in one step, and tells the Zhuyin text service the choices it shares.
    pub fn store(&self) -> std::io::Result<()> {
        let file = Self::file();
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let draft = file.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&draft, serde_json::to_string(self)?)?;
        std::fs::rename(&draft, &file).inspect_err(|_| {
            let _ = std::fs::remove_file(&draft);
        })?;
        let user = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        // The text service converts committed Zhuyin text, and draws its windows, by the same choices.
        user.create_subkey(r"Software\Tingzi\Engine")?.0.set_value("SimplifiedOutput", &u32::from(self.simplified))?;
        user.create_subkey(r"Software\Tingzi\Appearance")?.0.set_value("Theme", &self.appearance)
    }

    pub fn mode(&self) -> Mode {
        match self.delivery_mode {
            Some(0) => Mode::Insert,
            Some(1) => Mode::Copy,
            Some(_) => Mode::Preview,
            None if self.auto_insert => Mode::Insert,
            None => Mode::Preview,
        }
    }

    /// The recording shortcut as the user reads it.
    pub fn gesture(&self) -> String {
        if self.record_key == 0 {
            return "停用".into();
        }
        let modifiers = match self.modifiers {
            5 => "Alt+Shift",
            6 => "Ctrl+Shift",
            _ => "Ctrl+Alt",
        };
        format!("{modifiers}+{}", char::from_u32(self.record_key).unwrap_or('?'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_file_is_understood() {
        let saved: Settings = serde_json::from_str(
            r#"{"Online":true,"Modifiers":6,"RecordKey":84,"TextSize":24,"MicrophoneName":"耳機","Simplified":true,"DeliveryMode":1,"Valid":true,"Gesture":"Ctrl+Shift+T"}"#,
        )
        .unwrap();
        assert_eq!((saved.modifiers, saved.record_key, saved.simplified, saved.online), (6, 84, true, true));
        assert_eq!(saved.mode(), Mode::Copy);
        assert_eq!(saved.gesture(), "Ctrl+Shift+T");
        assert_eq!(saved.problem(), None);
        assert!(saved.use_phrases);
    }

    #[test]
    fn older_file_keeps_inserting() {
        let saved: Settings = serde_json::from_str(r#"{"Online":false,"Modifiers":3,"RecordKey":82,"TextSize":20}"#).unwrap();
        assert_eq!(saved.mode(), Mode::Insert);
        let preview: Settings = serde_json::from_str(r#"{"AutoInsert":false}"#).unwrap();
        assert_eq!(preview.mode(), Mode::Preview);
    }

    #[test]
    fn the_draft_shortcut_cannot_be_the_record_key() {
        let clash = Settings { modifiers: 3, record_key: u32::from(b'V'), ..Settings::default() };
        assert!(clash.problem().is_some());
        assert_eq!(Settings { modifiers: 6, record_key: u32::from(b'V'), ..Settings::default() }.problem(), None);
        assert_eq!(Settings { record_key: 0, ..Settings::default() }.gesture(), "停用");
        assert!(Settings { modifiers: 5, record_key: u32::from(b'Z'), ..Settings::default() }.problem().is_some());
    }
}
