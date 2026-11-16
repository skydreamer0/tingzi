//! Finishing the text and putting it where the user was typing.

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::Globalization::{LCMapStringEx, LCMAP_SIMPLIFIED_CHINESE, LCMAP_TRADITIONAL_CHINESE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow};

/// Character-level script conversion from Windows; one-to-many characters follow the system table.
pub fn convert_script(text: &str, simplified: bool) -> String {
    let source: Vec<u16> = text.encode_utf16().collect();
    if source.is_empty() {
        return String::new();
    }
    let flag = if simplified { LCMAP_SIMPLIFIED_CHINESE } else { LCMAP_TRADITIONAL_CHINESE };
    let mut mapped = vec![0u16; source.len() * 2 + 1];
    // SAFETY: both buffers outlive the call and their lengths are passed with them.
    let written = unsafe { LCMapStringEx(PCWSTR::null(), flag, &source, Some(&mut mapped), None, None, None) };
    if written <= 0 {
        return text.to_string();
    }
    String::from_utf16_lossy(&mapped[..written as usize])
}

/// The window that had the keyboard when recording started, as a plain number so it can cross threads.
pub fn foreground() -> isize {
    // SAFETY: the call takes no arguments and only reads window manager state.
    unsafe { GetForegroundWindow().0 as isize }
}

fn held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the current key state; no pointers involved.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

fn key(code: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: code,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Copies the text and pastes it into `target`. Returns whether the paste was sent;
/// the text stays on the clipboard either way.
pub fn insert(text: &str, target: isize) -> Result<bool, String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .map_err(|_| "無法寫入剪貼簿".to_string())?;
    let window = HWND(target as *mut core::ffi::c_void);
    // SAFETY: IsWindow and GetForegroundWindow accept any handle value and only query state.
    let alive = target != 0 && unsafe { IsWindow(window).as_bool() };
    if !alive {
        return Ok(false);
    }
    // A shortcut still held down would turn Ctrl+V into something else.
    for _ in 0..40 {
        if ![VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].into_iter().any(held) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // SAFETY: the handle was checked above; failure only means focus stays where it is.
    unsafe {
        if GetForegroundWindow() != window {
            let _ = SetForegroundWindow(window);
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        if GetForegroundWindow() != window {
            return Ok(false);
        }
    }
    let strokes = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
    // SAFETY: the slice and the element size describe the same array.
    let sent = unsafe { SendInput(&strokes, std::mem::size_of::<INPUT>() as i32) };
    Ok(sent as usize == strokes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_conversion_maps_both_ways() {
        assert_eq!(convert_script("语音输入与体验", false), "語音輸入與體驗");
        assert_eq!(convert_script("語音輸入與體驗", true), "语音输入与体验");
        assert_eq!(convert_script("abc 123", false), "abc 123");
        assert_eq!(convert_script("", true), "");
    }
}
