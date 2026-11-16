//! Reads the preferences the existing voice window saves, so both apps follow the same choices.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    /// 3 is Ctrl+Alt, 5 is Alt+Shift, 6 is Ctrl+Shift.
    pub modifiers: u32,
    /// A Windows virtual key from A to Z, or 0 when the shortcut is off.
    pub record_key: u32,
    pub simplified: bool,
    /// 0 inserts and copies, 1 only copies, 2 only shows the text.
    pub delivery_mode: Option<u32>,
    pub auto_insert: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { modifiers: 3, record_key: u32::from(b'R'), simplified: false, delivery_mode: None, auto_insert: true }
    }
}

impl Settings {
    pub fn load() -> Self {
        let file = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Tingzi").join("voice-settings.json");
        std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn inserts(&self) -> bool {
        match self.delivery_mode {
            Some(mode) => mode == 0,
            None => self.auto_insert,
        }
    }

    pub fn copies(&self) -> bool {
        self.delivery_mode != Some(2) && (self.delivery_mode.is_some() || self.auto_insert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_file_is_understood() {
        let saved: Settings = serde_json::from_str(r#"{"Online":true,"Modifiers":6,"RecordKey":84,"TextSize":24,"Simplified":true,"DeliveryMode":1}"#).unwrap();
        assert_eq!((saved.modifiers, saved.record_key, saved.simplified), (6, 84, true));
        assert!(!saved.inserts() && saved.copies());
    }

    #[test]
    fn older_file_keeps_inserting() {
        let saved: Settings = serde_json::from_str(r#"{"Online":false,"Modifiers":3,"RecordKey":82}"#).unwrap();
        assert!(saved.inserts() && saved.copies());
        let preview: Settings = serde_json::from_str(r#"{"AutoInsert":false}"#).unwrap();
        assert!(!preview.inserts() && !preview.copies());
    }
}
