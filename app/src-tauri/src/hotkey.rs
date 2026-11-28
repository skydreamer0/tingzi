//! The recording shortcut: which keys may make one, how it reads, and the keys the app keeps for itself.
//!
//! Keys are Windows virtual-key numbers and modifiers the bits RegisterHotKey uses, on macOS too,
//! so files from earlier versions, which stored a letter and one of three modifier pairs, read the same.

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Tracks a physical shortcut press through modifier release and key repeat.
pub fn manual_key(held: &std::sync::atomic::AtomicU32, key: u32, down: bool, shortcut: bool) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    if !down {
        let _ = held.compare_exchange(key, 0, Relaxed, Relaxed);
        return false;
    }
    if shortcut { held.store(key, Relaxed); }
    held.load(Relaxed) != key
}

pub const ALT: u32 = 1;
pub const CONTROL: u32 = 2;
pub const SHIFT: u32 = 4;
/// The Windows key; Command on macOS.
pub const SUPER: u32 = 8;
const ALL: u32 = ALT | CONTROL | SHIFT | SUPER;

const LETTERS: [Code; 26] = [
    Code::KeyA, Code::KeyB, Code::KeyC, Code::KeyD, Code::KeyE, Code::KeyF, Code::KeyG, Code::KeyH, Code::KeyI,
    Code::KeyJ, Code::KeyK, Code::KeyL, Code::KeyM, Code::KeyN, Code::KeyO, Code::KeyP, Code::KeyQ, Code::KeyR,
    Code::KeyS, Code::KeyT, Code::KeyU, Code::KeyV, Code::KeyW, Code::KeyX, Code::KeyY, Code::KeyZ,
];
const DIGITS: [Code; 10] =
    [Code::Digit0, Code::Digit1, Code::Digit2, Code::Digit3, Code::Digit4, Code::Digit5, Code::Digit6, Code::Digit7, Code::Digit8, Code::Digit9];
const FUNCTIONS: [Code; 24] = [
    Code::F1, Code::F2, Code::F3, Code::F4, Code::F5, Code::F6, Code::F7, Code::F8, Code::F9, Code::F10, Code::F11, Code::F12,
    Code::F13, Code::F14, Code::F15, Code::F16, Code::F17, Code::F18, Code::F19, Code::F20, Code::F21, Code::F22, Code::F23, Code::F24,
];
const FIRST_LETTER: u32 = 0x41;
const FIRST_DIGIT: u32 = 0x30;
const FIRST_FUNCTION: u32 = 0x70;

/// Ctrl+Alt+V pastes the draft into Codex; Control+Option+V on macOS.
pub const DRAFT: Hotkey = Hotkey { modifiers: CONTROL | ALT, key: 0x56 };
/// Alt+Shift+Z pastes the last dictation again; Option+Shift+Z on macOS.
pub const LAST: Hotkey = Hotkey { modifiers: ALT | SHIFT, key: 0x5A };
/// Ctrl+Alt+E rewrites the selected text as the user says, when that is turned on; Control+Option+E on macOS.
pub const REWRITE: Hotkey = Hotkey { modifiers: CONTROL | ALT, key: 0x45 };
/// What a new user gets: Ctrl+Alt+R.
pub const DEFAULT: Hotkey = Hotkey { modifiers: CONTROL | ALT, key: 0x52 };
/// What the page suggests when the chosen keys are taken, in this order.
const SUGGESTIONS: [Hotkey; 6] = [
    Hotkey { modifiers: 0, key: FIRST_FUNCTION + 8 },
    DEFAULT,
    Hotkey { modifiers: CONTROL | SHIFT, key: 0x52 },
    Hotkey { modifiers: ALT | SHIFT, key: 0x52 },
    Hotkey { modifiers: 0, key: FIRST_FUNCTION + 9 },
    Hotkey { modifiers: CONTROL | ALT, key: 0x44 },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Hotkey {
    pub modifiers: u32,
    /// 0 turns the shortcut off.
    pub key: u32,
}

enum Kind {
    Letter(usize),
    Digit(usize),
    Function(usize),
}

impl Hotkey {
    fn kind(&self) -> Option<Kind> {
        let within = |first: u32, count: usize| self.key.checked_sub(first).map(|index| index as usize).filter(|index| *index < count);
        within(FIRST_LETTER, 26)
            .map(Kind::Letter)
            .or_else(|| within(FIRST_DIGIT, 10).map(Kind::Digit))
            .or_else(|| within(FIRST_FUNCTION, 24).map(Kind::Function))
    }

    pub fn off(&self) -> bool {
        self.key == 0
    }

    /// Whether an earlier version can read these keys: a letter with Ctrl+Alt, Alt+Shift or Ctrl+Shift.
    pub fn fits_earlier_versions(&self) -> bool {
        matches!(self.modifiers, 3 | 5 | 6) && matches!(self.kind(), Some(Kind::Letter(_)))
    }

    /// Why these keys cannot record, in words for the settings page; None when they can, or when the shortcut is off.
    pub fn problem(&self) -> Option<String> {
        if self.off() {
            return None;
        }
        let [control, alt, _, super_key] = crate::sys::MODIFIER_KEYS;
        let kind = self.kind();
        if self.modifiers & !ALL != 0 || kind.is_none() {
            return Some("錄音快捷鍵只能用字母、數字或 F1 到 F24，可以搭配輔助鍵。".into());
        }
        if matches!(kind, Some(Kind::Letter(_) | Kind::Digit(_))) && self.modifiers & (CONTROL | ALT | SUPER) == 0 {
            return Some(format!("字母和數字要搭配 {control}、{alt} 或 {super_key}，否則打字時就會開始錄音。也可以用單獨的功能鍵，例如 F9。"));
        }
        if *self == DRAFT {
            return Some(format!("{} 用於貼入草稿，請選其他錄音鍵。", DRAFT.label()));
        }
        if *self == LAST {
            return Some(format!("{} 用於貼上上一則，請選其他錄音鍵。", LAST.label()));
        }
        None
    }

    /// The keys as the user reads them, such as `Ctrl+Alt+R` or `F9`; "停用" when off.
    pub fn label(&self) -> String {
        let key = match self.kind() {
            _ if self.off() => return "停用".into(),
            Some(Kind::Letter(index)) => char::from(b'A' + index as u8).to_string(),
            Some(Kind::Digit(index)) => index.to_string(),
            Some(Kind::Function(index)) => format!("F{}", index + 1),
            None => "?".into(),
        };
        let names = crate::sys::MODIFIER_KEYS;
        let held = [CONTROL, ALT, SHIFT, SUPER].iter().zip(names).filter(|(bit, _)| self.modifiers & **bit != 0).map(|(_, name)| name);
        held.chain(std::iter::once(key.as_str())).collect::<Vec<_>>().join("+")
    }

    /// The shortcut for keys the app keeps for itself, which `problem` turns away as a recording shortcut.
    pub fn shortcut_unchecked(&self) -> Shortcut {
        let code = match self.kind() {
            Some(Kind::Letter(index)) => LETTERS[index],
            Some(Kind::Digit(index)) => DIGITS[index],
            Some(Kind::Function(index)) => FUNCTIONS[index],
            None => Code::F24,
        };
        let mut modifiers = Modifiers::empty();
        for (bit, flag) in [(ALT, Modifiers::ALT), (CONTROL, Modifiers::CONTROL), (SHIFT, Modifiers::SHIFT), (SUPER, Modifiers::SUPER)] {
            if self.modifiers & bit != 0 {
                modifiers |= flag;
            }
        }
        Shortcut::new((!modifiers.is_empty()).then_some(modifiers), code)
    }

    /// The shortcut to register; None when off or not usable.
    pub fn shortcut(&self) -> Option<Shortcut> {
        if self.problem().is_some() || self.kind().is_none() {
            return None;
        }
        Some(self.shortcut_unchecked())
    }
}

/// Usable keys to offer instead of `taken`, in order of preference; `free` says whether the system would accept them.
pub fn suggestion(taken: Hotkey, free: impl Fn(&Hotkey) -> bool) -> Option<Hotkey> {
    SUGGESTIONS.into_iter().find(|keys| *keys != taken && keys.problem().is_none() && free(keys))
}

#[cfg(test)]
mod tests {
    #[test]
    fn recording_shortcut_repeat_and_release_are_not_manual_edits() {
        let held = std::sync::atomic::AtomicU32::new(0);
        assert!(!super::manual_key(&held, 82, true, true));
        assert!(!super::manual_key(&held, 82, true, false));
        assert!(super::manual_key(&held, 37, true, false));
        assert!(!super::manual_key(&held, 82, false, false));
        assert!(super::manual_key(&held, 82, true, false));
    }

    use super::*;

    fn keys(modifiers: u32, key: u32) -> Hotkey {
        Hotkey { modifiers, key }
    }

    #[test]
    fn the_rewrite_keys_are_ctrl_alt_e() {
        assert_eq!(REWRITE.shortcut_unchecked(), Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyE));
        assert_eq!(DRAFT.shortcut_unchecked(), Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV));
    }

    #[test]
    fn reads_like_the_keys_pressed() {
        let [control, alt, shift, super_key] = crate::sys::MODIFIER_KEYS;
        assert_eq!(DEFAULT.label(), format!("{control}+{alt}+R"));
        assert_eq!(keys(SHIFT | CONTROL, u32::from(b'T')).label(), format!("{control}+{shift}+T"));
        assert_eq!(keys(0, FIRST_FUNCTION + 8).label(), "F9");
        assert_eq!(keys(SUPER | SHIFT, FIRST_FUNCTION + 23).label(), format!("{shift}+{super_key}+F24"));
        assert_eq!(keys(ALT, u32::from(b'7')).label(), format!("{alt}+7"));
        assert_eq!(keys(3, 0).label(), "停用");
    }

    #[test]
    fn letters_need_a_modifier_but_function_keys_do_not() {
        assert!(keys(0, u32::from(b'R')).problem().is_some());
        assert!(keys(SHIFT, u32::from(b'R')).problem().is_some());
        assert!(keys(SHIFT, u32::from(b'1')).problem().is_some());
        assert_eq!(keys(SUPER | SHIFT, u32::from(b'R')).problem(), None);
        assert_eq!(keys(0, FIRST_FUNCTION).problem(), None);
        assert_eq!(keys(SHIFT, FIRST_FUNCTION + 11).problem(), None);
        // Space, Enter and the numeric keypad are not offered.
        assert!(keys(CONTROL, 0x20).problem().is_some());
        assert!(keys(CONTROL, 0x60).problem().is_some());
        assert!(keys(16, u32::from(b'R')).problem().is_some());
        assert_eq!(keys(0, 0).problem(), None);
    }

    #[test]
    fn the_app_keeps_its_own_keys() {
        assert!(DRAFT.problem().is_some());
        assert!(LAST.problem().is_some());
        assert_eq!(keys(CONTROL | SHIFT, u32::from(b'V')).problem(), None);
        assert!(DRAFT.shortcut().is_none());
    }

    #[test]
    fn only_the_old_choices_fit_earlier_versions() {
        assert!(DEFAULT.fits_earlier_versions());
        assert!(keys(5, u32::from(b'Q')).fits_earlier_versions());
        assert!(!keys(0, FIRST_FUNCTION + 8).fits_earlier_versions());
        assert!(!keys(CONTROL, u32::from(b'R')).fits_earlier_versions());
        assert!(!keys(3, u32::from(b'1')).fits_earlier_versions());
    }

    #[test]
    fn builds_the_shortcut_to_register() {
        assert_eq!(DEFAULT.shortcut(), Some(Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyR)));
        assert_eq!(keys(0, FIRST_FUNCTION + 8).shortcut(), Some(Shortcut::new(None, Code::F9)));
        assert_eq!(keys(SUPER, u32::from(b'0')).shortcut(), Some(Shortcut::new(Some(Modifiers::SUPER), Code::Digit0)));
        assert_eq!(keys(3, 0).shortcut(), None);
    }

    #[test]
    fn suggests_free_keys_other_than_the_taken_ones() {
        let f9 = keys(0, FIRST_FUNCTION + 8);
        assert_eq!(suggestion(DEFAULT, |_| true), Some(f9));
        assert_eq!(suggestion(f9, |_| true), Some(DEFAULT));
        assert_eq!(suggestion(DEFAULT, |keys| *keys != f9), Some(keys(CONTROL | SHIFT, u32::from(b'R'))));
        assert_eq!(suggestion(DEFAULT, |_| false), None);
    }
}
