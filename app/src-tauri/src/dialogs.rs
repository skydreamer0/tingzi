//! The Windows dialogs the app needs: picking a file, and asking before something that cannot be undone.

use std::path::PathBuf;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDOK, MB_DEFBUTTON2, MB_ICONQUESTION, MB_OKCANCEL, MB_TOPMOST, SW_SHOWNORMAL};

const TITLE: &str = "聽字・注音與語音輸入法";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The kinds of file the app opens or saves: a label, a pattern, and the extension added when saving.
#[derive(Clone, Copy)]
pub enum Kind {
    Wav,
    Text,
    Table,
}

impl Kind {
    fn filter(self) -> (&'static str, &'static str) {
        match self {
            Kind::Wav => ("WAV 音檔\0*.wav\0", "wav"),
            Kind::Text => ("文字檔 (*.txt)\0*.txt\0所有檔案 (*.*)\0*.*\0", "txt"),
            Kind::Table => ("Tab 分隔文字 (*.tsv)\0*.tsv\0所有檔案 (*.*)\0*.*\0", "tsv"),
        }
    }
}

/// Lets the user pick a file to open, or name one to save when `save_as` is given.
/// `owner` is the window the dialog belongs to, or 0. Blocks until it closes; call it off the UI thread.
pub fn pick(owner: isize, kind: Kind, save_as: Option<&str>) -> Option<PathBuf> {
    let (filter, extension) = kind.filter();
    let (filter, extension) = (wide(filter), wide(extension));
    let mut chosen = vec![0u16; 32768];
    if let Some(name) = save_as {
        let name: Vec<u16> = name.encode_utf16().collect();
        chosen[..name.len()].copy_from_slice(&name);
    }
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut core::ffi::c_void),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(chosen.as_mut_ptr()),
        nMaxFile: chosen.len() as u32,
        lpstrDefExt: PCWSTR(extension.as_ptr()),
        Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR | if save_as.is_some() { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST },
        ..Default::default()
    };
    // SAFETY: every pointer in the struct refers to a buffer that outlives the call.
    let picked = unsafe { if save_as.is_some() { GetSaveFileNameW(&mut dialog) } else { GetOpenFileNameW(&mut dialog) } };
    if !picked.as_bool() {
        return None;
    }
    let end = chosen.iter().position(|&unit| unit == 0)?;
    Some(PathBuf::from(String::from_utf16_lossy(&chosen[..end])))
}

/// Asks a yes-or-no question; Cancel is the default so Enter never confirms by accident.
pub fn confirm(owner: isize, question: &str) -> bool {
    let (text, title) = (wide(question), wide(TITLE));
    // SAFETY: both strings are null-terminated and outlive the call.
    let answer = unsafe {
        MessageBoxW(HWND(owner as *mut core::ffi::c_void), PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OKCANCEL | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_TOPMOST)
    };
    answer == IDOK
}

/// Opens a folder, a web address or a Windows settings page with whatever Windows uses for it.
pub fn open(target: &str) -> bool {
    let (verb, target) = (wide("open"), wide(target));
    // SAFETY: both strings are null-terminated and outlive the call.
    let result = unsafe { ShellExecuteW(HWND::default(), PCWSTR(verb.as_ptr()), PCWSTR(target.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    result.0 as usize > 32
}
