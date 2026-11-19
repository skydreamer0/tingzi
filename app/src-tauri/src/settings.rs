//! The user's preferences, in the same file the earlier voice window used, so an upgrade keeps them.

use crate::deliver::Mode;
use crate::hotkey::Hotkey;
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

/// The languages refined text may come out in; anything else reads as "as spoken".
pub const REFINE_LANGUAGES: [&str; 3] = ["", "en", "zh-TW"];

fn lenient_language<'de, D: Deserializer<'de>>(source: D) -> Result<String, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(source)?;
    let code = value.as_ref().and_then(serde_json::Value::as_str).unwrap_or_default();
    Ok(if REFINE_LANGUAGES.contains(&code) { code.to_string() } else { String::new() })
}

/// Keys that are not a shortcut this version understands are read as unset, so the rest of the user's choices still load.
fn lenient_hotkey<'de, D: Deserializer<'de>>(source: D) -> Result<Option<Hotkey>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(source)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    /// What earlier versions read: true sends recordings to Codex. Kept in step with `engine`
    /// so going back to an earlier version never sends audio to a service the user did not pick.
    pub online: bool,
    /// Where recordings are recognised; None in files from before there was a choice of three.
    #[serde(deserialize_with = "lenient_engine")]
    pub engine: Option<Engine>,
    pub streaming_asr: bool,
    /// The service's address, up to and including the version, such as `https://api.openai.com/v1`.
    pub api_url: String,
    pub api_model: String,
    /// The Whisper model file chosen on this computer; None uses the one installed first.
    pub local_model: Option<String>,
    /// Whether local recognition uses the graphics card, when a program that can is in place.
    pub gpu: bool,
    /// What earlier versions read for the recording shortcut: 3 is Ctrl+Alt, 5 is Alt+Shift, 6 is Ctrl+Shift;
    /// on macOS, Control and Option take the place of Ctrl and Alt. `hotkey` takes its place when set.
    pub modifiers: u32,
    /// What earlier versions read: a Windows virtual key from A to Z, or 0 when the shortcut is off.
    pub record_key: u32,
    /// The recording shortcut, which may be any keys `Hotkey` allows; None in files from before it could.
    #[serde(deserialize_with = "lenient_hotkey")]
    pub hotkey: Option<Hotkey>,
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
    /// The language refined text comes out in: "" as spoken, "en" English, "zh-TW" Taiwanese Traditional Chinese.
    #[serde(deserialize_with = "lenient_language")]
    pub refine_language: String,
    /// What the rule for the program being dictated into adds to the instructions, such as a tone. Comes only from
    /// the rule, so it is never saved with the main settings.
    #[serde(skip)]
    pub refine_app_note: String,
    /// Whether Ctrl+Alt+E rewrites the selected text as the user says, with the refinement service. Off unless turned on,
    /// since the selected text is sent.
    pub rewrite_selection: bool,
    /// Whether GitHub is asked once a day for a newer version. Off means no request is made unless the user presses the button.
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            online: false,
            engine: None,
            streaming_asr: false,
            api_url: DEFAULT_API_URL.into(),
            api_model: DEFAULT_API_MODEL.into(),
            local_model: None,
            gpu: false,
            modifiers: 3,
            record_key: u32::from(b'R'),
            hotkey: None,
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
            refine_language: String::new(),
            refine_app_note: String::new(),
            rewrite_selection: false,
            check_updates: true,
        }
    }
}

impl Settings {
    pub fn segment_dictation(&self, rewriting: bool) -> bool {
        self.streaming_asr && self.engine() == Engine::Codex && !rewriting
    }

    /// The engine in use, reading files from before the choice of three by their `Online` flag.
    pub fn engine(&self) -> Engine {
        self.engine.unwrap_or(if self.online { Engine::Codex } else { Engine::Local })
    }

    /// The recording shortcut, reading files from before any keys could be chosen by their letter and modifier pair.
    pub fn hotkey(&self) -> Hotkey {
        self.hotkey.unwrap_or(Hotkey { modifiers: self.modifiers, key: self.record_key })
    }

    /// The choices as written: the engine named, and `Online` only for Codex. Earlier versions read
    /// the shortcut from `Modifiers` and `RecordKey`; keys they cannot use are written there as Ctrl+Alt+R.
    fn as_saved(&self) -> Self {
        let keys = self.hotkey();
        let (modifiers, record_key) = if keys.fits_earlier_versions() {
            (keys.modifiers, keys.key)
        } else if keys.off() {
            (crate::hotkey::DEFAULT.modifiers, 0)
        } else {
            (crate::hotkey::DEFAULT.modifiers, crate::hotkey::DEFAULT.key)
        };
        Self { online: self.engine() == Engine::Codex, engine: Some(self.engine()), hotkey: Some(keys), modifiers, record_key, ..self.clone() }
    }

    fn file() -> PathBuf {
        crate::paths::data().join("voice-settings.json")
    }

    /// Whether choices have ever been saved on this computer, by this version or an earlier one.
    pub fn saved() -> bool {
        Self::file().exists()
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::file())
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .filter(|saved| saved.problem().is_none())
            .unwrap_or_default()
    }

    /// Why these choices cannot be used, in words for the settings page; None when they are fine.
    pub fn problem(&self) -> Option<String> {
        if let Some(problem) = self.hotkey().problem() {
            return Some(problem);
        }
        if self.rewrite_selection && self.hotkey() == crate::hotkey::REWRITE {
            return Some(format!("{} 用於改寫選取文字，請選其他錄音鍵，或關閉「用語音改寫選取的文字」。", crate::hotkey::REWRITE.label()));
        }
        if !matches!(self.text_size, 18 | 20 | 24) || self.appearance > 2 || self.delivery_mode.is_some_and(|mode| mode > 2) {
            return Some("設定內容不完整，請重新選擇。".into());
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
        self.hotkey().label()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_asr_is_opt_in_for_codex_dictation_only() {
        let older: Settings = serde_json::from_str(r#"{"Online":true}"#).unwrap();
        assert!(!older.streaming_asr);
        assert!(!older.segment_dictation(false));
        let enabled: Settings = serde_json::from_str(r#"{"StreamingAsr":true,"Engine":"codex"}"#).unwrap();
        assert!(enabled.segment_dictation(false));
        assert!(!enabled.segment_dictation(true));
        assert_eq!(serde_json::to_value(enabled.as_saved()).unwrap()["StreamingAsr"], true);
        for engine in [Engine::Local, Engine::Api] {
            assert!(!Settings { engine: Some(engine), ..enabled.clone() }.segment_dictation(false));
        }
    }

    #[test]
    fn saved_file_is_understood() {
        let saved: Settings = serde_json::from_str(
            r#"{"Online":true,"Modifiers":6,"RecordKey":84,"TextSize":24,"MicrophoneName":"耳機","Simplified":true,"DeliveryMode":1,"Valid":true,"Gesture":"Ctrl+Shift+T"}"#,
        )
        .unwrap();
        assert_eq!((saved.modifiers, saved.record_key, saved.simplified, saved.online), (6, 84, true, true));
        assert_eq!(saved.mode(), Mode::Copy);
        let [control, _, shift, _] = crate::sys::MODIFIER_KEYS;
        assert_eq!(saved.gesture(), format!("{control}+{shift}+T"));
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
    fn the_output_language_is_kept_and_an_unknown_one_reads_as_spoken() {
        let english: Settings = serde_json::from_str(r#"{"RefineLanguage":"en"}"#).unwrap();
        assert_eq!(english.refine_language, "en");
        let odd: Settings = serde_json::from_str(r#"{"RefineLanguage":"klingon"}"#).unwrap();
        assert_eq!(odd.refine_language, "");
        let older: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(older.refine_language, "");
        // A program's note is never written to the settings file.
        let with_note = Settings { refine_app_note: "口語".into(), ..Settings::default() };
        assert!(!serde_json::to_string(&with_note).unwrap().contains("口語"));
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
    fn files_from_before_the_update_check_have_it_on() {
        let earlier: Settings = serde_json::from_str(r#"{"RecordKey":84}"#).unwrap();
        assert!(earlier.check_updates);
        let off: Settings = serde_json::from_str(r#"{"CheckUpdates":false}"#).unwrap();
        assert!(!off.check_updates);
    }

    #[test]
    fn any_keys_are_kept_and_earlier_versions_still_get_a_shortcut() {
        use crate::hotkey::{Hotkey, SHIFT};
        let f9 = Hotkey { modifiers: 0, key: 0x78 };
        let written = serde_json::to_string(&Settings { hotkey: Some(f9), ..Settings::default() }.as_saved()).unwrap();
        assert!(written.contains(r#""Hotkey":{"Modifiers":0,"Key":120}"#) && written.contains(r#""Modifiers":3,"RecordKey":82"#));
        let back: Settings = serde_json::from_str(&written).unwrap();
        assert_eq!((back.hotkey(), back.problem()), (f9, None));
        // A shortcut earlier versions can use is written for them as it is.
        let letter = Hotkey { modifiers: 6, key: u32::from(b'T') };
        let written = serde_json::to_string(&Settings { hotkey: Some(letter), ..Settings::default() }.as_saved()).unwrap();
        assert!(written.contains(r#""Modifiers":6,"RecordKey":84"#));
        // Turned off stays off for them too.
        let off = Settings { hotkey: Some(Hotkey { modifiers: 0, key: 0 }), ..Settings::default() }.as_saved();
        assert_eq!((off.modifiers, off.record_key), (3, 0));
        // Keys this version cannot use are ignored, keeping the earlier choice and everything else.
        let odd: Settings = serde_json::from_str(r#"{"Hotkey":"F9","Modifiers":5,"RecordKey":81,"Tidy":false}"#).unwrap();
        assert_eq!((odd.hotkey(), odd.tidy), (Hotkey { modifiers: 5, key: 81 }, false));
        let letter_alone = Settings { hotkey: Some(Hotkey { modifiers: SHIFT, key: u32::from(b'R') }), ..Settings::default() };
        assert!(letter_alone.problem().is_some());
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
