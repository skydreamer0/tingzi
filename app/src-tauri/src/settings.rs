//! The user's preferences, in the same file the earlier voice window used, so an upgrade keeps them.

use crate::deliver::Mode;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

/// Where recordings are recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// The Whisper program on this computer; nothing is uploaded.
    Local,
    /// Codex's built-in recogniser, through the user's Codex login.
    Codex,
    /// A transcription service that speaks the OpenAI API, with the user's own key.
    Api,
}

/// An engine this version does not know, from a newer version's file, is read as unset
/// so the rest of the user's choices still load.
fn lenient_engine<'de, D: Deserializer<'de>>(source: D) -> Result<Option<Engine>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(source)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

pub const DEFAULT_API_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_API_MODEL: &str = "whisper-1";
pub const DEFAULT_REFINE_URL: &str = "https://api.openai.com/v1";
/// The most characters the user's note to the refinement service may have; the rest is cut off.
pub const LONGEST_REFINE_NOTE: usize = 500;

/// The first 500 characters of a note to the refinement service.
pub fn short_note(note: &str) -> String {
    note.chars().take(LONGEST_REFINE_NOTE).collect()
}

/// A note that is not text reads as empty, and a long one is cut to size, from the file and from the page alike.
fn lenient_note<'de, D: Deserializer<'de>>(source: D) -> Result<String, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(source)?;
    Ok(value.as_ref().and_then(serde_json::Value::as_str).map(short_note).unwrap_or_default())
}

#[cfg(not(target_os = "macos"))]
const DRAFT_CLASH: &str = "Ctrl+Alt+V 用於貼入草稿，請選其他錄音鍵。";
#[cfg(target_os = "macos")]
const DRAFT_CLASH: &str = "Control+Option+V 用於貼入草稿，請選其他錄音鍵。";
#[cfg(not(target_os = "macos"))]
const LAST_CLASH: &str = "Alt+Shift+Z 用於貼上上一則，請選其他錄音鍵。";
#[cfg(target_os = "macos")]
const LAST_CLASH: &str = "Option+Shift+Z 用於貼上上一則，請選其他錄音鍵。";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    /// What earlier versions read: true sends recordings to Codex. Kept in step with `engine`
    /// so going back to an earlier version never sends audio to a service the user did not pick.
    pub online: bool,
    /// Where recordings are recognised; None in files from before there was a choice of three.
    #[serde(deserialize_with = "lenient_engine")]
    pub engine: Option<Engine>,
    /// The service's address, up to and including the version, such as `https://api.openai.com/v1`.
    pub api_url: String,
    pub api_model: String,
    /// The Whisper model file chosen on this computer; None uses the one installed first.
    pub local_model: Option<String>,
    /// Whether local recognition uses the graphics card, when a program that can is in place.
    pub gpu: bool,
    /// 3 is Ctrl+Alt, 5 is Alt+Shift, 6 is Ctrl+Shift; on macOS, Control and Option take the place of Ctrl and Alt.
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
    /// Whether the user's "heard → wanted" replacement rules are applied to recognised speech.
    pub use_replacements: bool,
    /// 0 follows the system, 1 is light, 2 is dark.
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
    /// Whether a word typed with Zhuyin to fix a dictation is used to correct later dictations.
    pub learn_fixes: bool,
    /// Whether recognised text is sent to a language model to be tidied into written text. Off unless the user turns it on.
    pub refine: bool,
    /// The refinement service's address, up to and including the version, such as `https://api.openai.com/v1`.
    pub refine_url: String,
    pub refine_model: String,
    /// What the user adds to the instructions for the refinement service, at most 500 characters.
    #[serde(deserialize_with = "lenient_note")]
    pub refine_note: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            online: false,
            engine: None,
            api_url: DEFAULT_API_URL.into(),
            api_model: DEFAULT_API_MODEL.into(),
            local_model: None,
            gpu: false,
            modifiers: 3,
            record_key: u32::from(b'R'),
            text_size: 20,
            microphone_id: None,
            microphone_name: None,
            auto_insert: true,
            simplified: false,
            use_phrases: true,
            use_replacements: true,
            appearance: 0,
            system_accent: false,
            delivery_mode: None,
            hold_to_talk: false,
            tidy: true,
            keep_history: true,
            learn_fixes: true,
            refine: false,
            refine_url: DEFAULT_REFINE_URL.into(),
            refine_model: String::new(),
            refine_note: String::new(),
        }
    }
}

impl Settings {
    /// The engine in use, reading files from before the choice of three by their `Online` flag.
    pub fn engine(&self) -> Engine {
        self.engine.unwrap_or(if self.online { Engine::Codex } else { Engine::Local })
    }

    /// The choices as written: the engine named, and `Online` only for Codex.
    fn as_saved(&self) -> Self {
        Self { online: self.engine() == Engine::Codex, engine: Some(self.engine()), ..self.clone() }
    }

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
            return Some(DRAFT_CLASH);
        }
        if self.modifiers == 5 && self.record_key == u32::from(b'Z') {
            return Some(LAST_CLASH);
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
        std::fs::write(&draft, serde_json::to_string(&self.as_saved())?)?;
        std::fs::rename(&draft, &file).inspect_err(|_| {
            let _ = std::fs::remove_file(&draft);
        })?;
        crate::sys::share_with_text_service(self.simplified, self.appearance)
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
        let [control_alt, alt_shift, control_shift] = crate::sys::MODIFIER_NAMES;
        let modifiers = match self.modifiers {
            5 => alt_shift,
            6 => control_shift,
            _ => control_alt,
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
        assert_eq!(saved.gesture(), format!("{}+T", crate::sys::MODIFIER_NAMES[2]));
        assert_eq!(saved.problem(), None);
        assert!(saved.use_phrases);
        assert_eq!(saved.engine(), Engine::Codex);
    }

    #[test]
    fn the_engine_follows_older_files_and_survives_newer_ones() {
        let local: Settings = serde_json::from_str(r#"{"Online":false,"RecordKey":84}"#).unwrap();
        assert_eq!(local.engine(), Engine::Local);
        let chosen: Settings = serde_json::from_str(r#"{"Online":false,"Engine":"api","ApiModel":"m"}"#).unwrap();
        assert_eq!((chosen.engine(), chosen.api_model.as_str()), (Engine::Api, "m"));
        // An engine from a newer version leaves the other choices in place.
        let newer: Settings = serde_json::from_str(r#"{"Online":true,"Engine":"someday","RecordKey":84}"#).unwrap();
        assert_eq!((newer.engine(), newer.record_key), (Engine::Codex, 84));
        let defaults: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!((defaults.api_url.as_str(), defaults.engine()), (DEFAULT_API_URL, Engine::Local));
    }

    #[test]
    fn only_codex_is_written_as_online() {
        for (engine, online) in [(Engine::Api, false), (Engine::Codex, true), (Engine::Local, false)] {
            let prefs = Settings { engine: Some(engine), online: !online, ..Settings::default() };
            let back: Settings = serde_json::from_str(&serde_json::to_string(&prefs.as_saved()).unwrap()).unwrap();
            assert_eq!((back.online, back.engine()), (online, engine));
        }
    }

    #[test]
    fn older_file_keeps_inserting() {
        let saved: Settings = serde_json::from_str(r#"{"Online":false,"Modifiers":3,"RecordKey":82,"TextSize":20}"#).unwrap();
        assert_eq!(saved.mode(), Mode::Insert);
        // Files saved before replacement rules existed turn them on.
        assert!(saved.use_replacements);
        let preview: Settings = serde_json::from_str(r#"{"AutoInsert":false}"#).unwrap();
        assert_eq!(preview.mode(), Mode::Preview);
    }

    #[test]
    fn refinement_choices_survive_saving_and_older_files_leave_it_off() {
        let older: Settings = serde_json::from_str(r#"{"Online":false,"RecordKey":82,"Tidy":true}"#).unwrap();
        assert_eq!((older.refine, older.refine_url.as_str(), older.refine_model.as_str(), older.refine_note.as_str()), (false, DEFAULT_REFINE_URL, "", ""));
        let chosen = Settings { refine: true, refine_url: "http://localhost:11434/v1".into(), refine_model: "qwen2.5".into(), refine_note: "用詞正式".into(), ..Settings::default() };
        let written = serde_json::to_string(&chosen.as_saved()).unwrap();
        assert!(written.contains(r#""Refine":true"#) && written.contains(r#""RefineUrl":"http://localhost:11434/v1""#));
        assert!(written.contains(r#""RefineModel":"qwen2.5""#) && written.contains(r#""RefineNote":"用詞正式""#));
        let back: Settings = serde_json::from_str(&written).unwrap();
        assert_eq!(back, chosen.as_saved());
        assert_eq!(back.problem(), None);
    }

    #[test]
    fn a_long_note_is_cut_and_a_strange_one_is_dropped() {
        let long = "字".repeat(LONGEST_REFINE_NOTE + 20);
        let saved: Settings = serde_json::from_str(&format!(r#"{{"RefineNote":"{long}","RecordKey":84}}"#)).unwrap();
        assert_eq!((saved.refine_note.chars().count(), saved.record_key), (LONGEST_REFINE_NOTE, 84));
        let odd: Settings = serde_json::from_str(r#"{"RefineNote":7,"RecordKey":84}"#).unwrap();
        assert_eq!((odd.refine_note.as_str(), odd.record_key), ("", 84));
        let empty: Settings = serde_json::from_str(r#"{"RefineNote":null}"#).unwrap();
        assert_eq!(empty.refine_note, "");
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
