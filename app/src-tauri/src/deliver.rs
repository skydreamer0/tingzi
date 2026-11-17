//! Finishing the text and putting it where the user was typing.
//!
//! The rule that matters: text is pasted only into the place that had the keyboard when recording
//! started. If the user moved on, the text stays in the preview and on the clipboard instead.

use std::sync::Mutex;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Globalization::{LCMapStringEx, LCMAP_SIMPLIFIED_CHINESE, LCMAP_TRADITIONAL_CHINESE};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, IsWindowEnabled, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, IsWindow,
    SendMessageTimeoutW, SetForegroundWindow, GUITHREADINFO, GWL_STYLE, SMTO_ABORTIFHUNG,
};

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

pub fn copy(text: &str) -> bool {
    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.to_string())).is_ok()
}

pub fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()).ok()
}

/// A place the keyboard can be: a window, the process that owns it, and the control with the caret.
/// Handles are kept as plain numbers so a spot can cross threads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Spot {
    pub window: isize,
    pub process: u32,
    pub focus: isize,
}

fn handle(value: isize) -> HWND {
    HWND(value as *mut core::ffi::c_void)
}

impl Spot {
    /// Where the keyboard is right now.
    pub fn current() -> Self {
        // SAFETY: these calls only read window manager state; the struct is sized as the API requires.
        unsafe {
            let window = GetForegroundWindow();
            let mut process = 0u32;
            let thread = GetWindowThreadProcessId(window, Some(&mut process));
            let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
            let focus = if thread != 0 && GetGUIThreadInfo(thread, &mut info).is_ok() { info.hwndFocus.0 as isize } else { 0 };
            Self { window: window.0 as isize, process, focus }
        }
    }

    pub fn ours(&self) -> bool {
        self.process == std::process::id()
    }

    /// The control with the caret, when it is a standard Windows text box that can be written to.
    fn text_box(&self) -> Option<HWND> {
        let control = handle(self.focus);
        let mut name = [0u16; 64];
        // SAFETY: the buffer length travels with the slice; the other calls only read state.
        unsafe {
            if self.focus == 0 || !IsWindow(control).as_bool() || !IsWindowEnabled(control).as_bool() {
                return None;
            }
            let length = GetClassNameW(control, &mut name) as usize;
            let class = String::from_utf16_lossy(&name[..length]).to_ascii_lowercase();
            const READ_ONLY: i32 = 0x0800;
            const PASSWORD: i32 = 0x0020;
            let style = GetWindowLongW(control, GWL_STYLE);
            ((class == "edit" || class.starts_with("richedit")) && style & (READ_ONLY | PASSWORD) == 0).then_some(control)
        }
    }
}

struct Watch {
    /// The last place the keyboard was outside this app.
    outside: Spot,
    target: Option<Spot>,
    left: bool,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch { outside: Spot { window: 0, process: 0, focus: 0 }, target: None, left: false });

/// Follows the foreground window for the life of the app.
pub fn watch() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(100));
        let now = Spot::current();
        if now.window == 0 || now.ours() {
            continue;
        }
        if let Ok(mut watch) = WATCH.lock() {
            watch.outside = now;
            if watch.target.is_some_and(|target| target.window != now.window) {
                watch.left = true;
            }
        }
    });
}

/// Remembers where the user is typing as recording starts. Clicking the app's own mic button moves
/// the keyboard to this app, so in that case the last place outside it is used.
pub fn begin() -> Spot {
    let now = Spot::current();
    let mut watch = WATCH.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let target = if now.ours() || now.window == 0 { watch.outside } else { now };
    watch.target = Some(target);
    watch.left = false;
    target
}

/// The place recording started from; an empty spot when nothing is being recorded.
pub fn target() -> Spot {
    WATCH.lock().ok().and_then(|watch| watch.target).unwrap_or_default()
}

pub fn end() {
    if let Ok(mut watch) = WATCH.lock() {
        watch.target = None;
    }
}

fn left_target() -> bool {
    WATCH.lock().map(|watch| watch.left).unwrap_or(true)
}

fn held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the current key state; no pointers involved.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

fn modifier_held() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].into_iter().any(held)
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

/// What the user chose to happen when recognition finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Copy, then paste into the place recording started from.
    Insert,
    Copy,
    Preview,
}

/// How a delivery ended. `expand` asks the window to open the preview so the text is not lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub copied: bool,
    pub inserted: bool,
    pub cancelled: bool,
    pub expand: bool,
    pub message: &'static str,
}

impl Outcome {
    fn cancelled() -> Self {
        Self { copied: false, inserted: false, cancelled: true, expand: false, message: "已取消" }
    }
}

/// The clipboard, the keyboard and the target window, behind a seam so delivery can be tested.
pub trait Desk {
    /// Waits until the recording shortcut is released, or `cancelled` turns true.
    fn wait_released(&mut self, cancelled: &dyn Fn() -> bool);
    fn copy(&mut self, text: &str) -> bool;
    /// Whether the clipboard still holds exactly what `copy` put there.
    fn clipboard_holds(&mut self, text: &str) -> bool;
    /// Whether the keyboard is still where recording started.
    fn still_there(&mut self) -> bool;
    /// Writes straight into a standard text box, without the clipboard. False when the field is not one.
    fn write_directly(&mut self, text: &str) -> bool;
    fn paste(&mut self) -> bool;
}

/// Copies and inserts `text` according to `mode`. Never pastes more than once, and never pastes
/// when the user has moved away or the clipboard no longer holds this text.
pub fn deliver(text: &str, mode: Mode, desk: &mut dyn Desk, cancelled: &dyn Fn() -> bool) -> Outcome {
    if cancelled() {
        return Outcome::cancelled();
    }
    if text.trim().is_empty() {
        return Outcome { copied: false, inserted: false, cancelled: false, expand: false, message: "沒有辨識文字" };
    }
    if mode == Mode::Preview {
        return Outcome { copied: false, inserted: false, cancelled: false, expand: true, message: "辨識完成・可編輯後複製" };
    }
    if mode == Mode::Insert {
        desk.wait_released(cancelled);
    }
    if cancelled() {
        return Outcome::cancelled();
    }
    let copied = desk.copy(text);
    let mut inserted = false;
    if mode == Mode::Insert && !cancelled() && desk.still_there() {
        inserted = desk.write_directly(text)
            || (copied && desk.clipboard_holds(text) && !cancelled() && desk.still_there() && desk.paste());
    }
    let changed = copied && !desk.clipboard_holds(text);
    let message = match (changed, inserted, copied) {
        (true, true, _) => "已插入・剪貼簿已變更，結果保留預覽",
        (true, false, _) => "剪貼簿已變更・結果保留預覽",
        (false, true, true) => "已插入並複製文字",
        (false, true, false) => "已插入・剪貼簿忙碌，結果可取回",
        (false, false, true) => "已複製文字・可手動貼上",
        (false, false, false) => "剪貼簿忙碌・結果可取回",
    };
    Outcome { copied, inserted, cancelled: false, expand: changed || !copied || (mode == Mode::Insert && !inserted), message }
}

/// The real desktop, aimed at the place recording started from.
pub struct Desktop {
    target: Spot,
    /// The clipboard's change counter right after this app wrote to it.
    written: u32,
    /// False for the Codex draft shortcut, where the user is already in the editor they chose.
    watched: bool,
}

impl Desktop {
    pub fn new(target: Spot) -> Self {
        Self { target, written: 0, watched: true }
    }

    /// Aims at wherever the keyboard is now, for a paste the user asked for directly.
    pub fn here(target: Spot) -> Self {
        Self { target, written: 0, watched: false }
    }
}

impl Desk for Desktop {
    fn wait_released(&mut self, cancelled: &dyn Fn() -> bool) {
        // A shortcut still held down would turn Ctrl+V into something else.
        for _ in 0..40 {
            if !modifier_held() || cancelled() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn copy(&mut self, text: &str) -> bool {
        let copied = copy(text);
        // SAFETY: takes no arguments and only reads a counter.
        self.written = if copied { unsafe { GetClipboardSequenceNumber() } } else { 0 };
        copied
    }

    fn clipboard_holds(&mut self, text: &str) -> bool {
        // SAFETY: takes no arguments and only reads a counter.
        self.written != 0 && unsafe { GetClipboardSequenceNumber() } == self.written && clipboard_text().as_deref() == Some(text)
    }

    fn still_there(&mut self) -> bool {
        if self.target.window == 0 || (self.watched && left_target()) {
            return false;
        }
        let mut now = Spot::current();
        if now.ours() {
            // The user clicked this app's mic button; hand the keyboard back to where they were.
            // SAFETY: any handle value is accepted; failure only leaves focus where it is.
            unsafe {
                let _ = SetForegroundWindow(handle(self.target.window));
            }
            std::thread::sleep(Duration::from_millis(80));
            now = Spot::current();
        }
        now.window == self.target.window && now.process == self.target.process && (self.target.focus == 0 || now.focus == self.target.focus)
    }

    fn write_directly(&mut self, text: &str) -> bool {
        let Some(control) = self.target.text_box() else { return false };
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        const REPLACE_SELECTION: u32 = 0x00C2;
        // SAFETY: the text is null-terminated and outlives the call; Windows copies it across processes
        // for this message. A message sent to one control cannot land in another application.
        let done = unsafe {
            SendMessageTimeoutW(control, REPLACE_SELECTION, WPARAM(1), LPARAM(wide.as_ptr() as isize), SMTO_ABORTIFHUNG, 1000, None)
        };
        done.0 != 0
    }

    fn paste(&mut self) -> bool {
        if modifier_held() {
            return false;
        }
        let strokes = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
        // SAFETY: the slice and the element size describe the same array.
        let sent = unsafe { SendInput(&strokes, std::mem::size_of::<INPUT>() as i32) };
        sent as usize == strokes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A desk that records what was asked of it.
    #[derive(Default)]
    struct Fake {
        copies: Vec<String>,
        clipboard_busy: bool,
        /// How many times the clipboard still reads back the copied text.
        clipboard_good_for: Option<u32>,
        clipboard_reads: u32,
        /// How many times the target is still current.
        there_for: Option<u32>,
        there_asked: u32,
        text_box: bool,
        written: Option<String>,
        pastes: u32,
        waits: u32,
        cancel_on_wait: Option<std::rc::Rc<Cell<bool>>>,
    }

    impl Desk for Fake {
        fn wait_released(&mut self, _: &dyn Fn() -> bool) {
            self.waits += 1;
            if let Some(flag) = &self.cancel_on_wait {
                flag.set(true);
            }
        }
        fn copy(&mut self, text: &str) -> bool {
            if self.clipboard_busy {
                return false;
            }
            self.copies.push(text.to_string());
            true
        }
        fn clipboard_holds(&mut self, _: &str) -> bool {
            self.clipboard_reads += 1;
            !self.clipboard_busy && self.clipboard_good_for.map_or(true, |limit| self.clipboard_reads <= limit)
        }
        fn still_there(&mut self) -> bool {
            self.there_asked += 1;
            self.there_for.map_or(true, |limit| self.there_asked <= limit)
        }
        fn write_directly(&mut self, text: &str) -> bool {
            if self.text_box {
                self.written = Some(text.to_string());
            }
            self.text_box
        }
        fn paste(&mut self) -> bool {
            self.pastes += 1;
            true
        }
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn script_conversion_maps_both_ways() {
        assert_eq!(convert_script("语音输入与体验", false), "語音輸入與體驗");
        assert_eq!(convert_script("語音輸入與體驗", true), "语音输入与体验");
        assert_eq!(convert_script("abc 123", false), "abc 123");
        assert_eq!(convert_script("", true), "");
    }

    #[test]
    fn clipboard_only_copies_without_insertion() {
        let mut desk = Fake::default();
        let outcome = deliver("剪貼簿文字", Mode::Copy, &mut desk, &never);
        assert_eq!(desk.copies, ["剪貼簿文字"]);
        assert_eq!((desk.pastes, desk.there_asked, desk.waits), (0, 0, 0));
        assert!(outcome.copied && !outcome.inserted && !outcome.expand);
    }

    #[test]
    fn insert_copies_then_pastes_once() {
        let mut desk = Fake::default();
        let outcome = deliver("自動插入文字", Mode::Insert, &mut desk, &never);
        assert_eq!(desk.copies, ["自動插入文字"]);
        assert_eq!(desk.pastes, 1);
        assert!(outcome.copied && outcome.inserted && !outcome.expand);
        assert_eq!(outcome.message, "已插入並複製文字");
    }

    #[test]
    fn field_change_keeps_the_copy_and_opens_the_preview() {
        // The user leaves between the first check and the paste.
        let mut desk = Fake { there_for: Some(1), ..Fake::default() };
        let outcome = deliver("留存的文字", Mode::Insert, &mut desk, &never);
        assert_eq!(desk.copies, ["留存的文字"]);
        assert_eq!(desk.pastes, 0);
        assert!(outcome.copied && !outcome.inserted && outcome.expand);
        // The user had already left when recognition finished.
        let mut gone = Fake { there_for: Some(0), ..Fake::default() };
        let outcome = deliver("留存的文字", Mode::Insert, &mut gone, &never);
        assert_eq!((gone.pastes, gone.copies.len()), (0, 1));
        assert!(outcome.expand);
        assert_eq!(outcome.message, "已複製文字・可手動貼上");
    }

    #[test]
    fn changed_clipboard_blocks_a_stale_paste() {
        let mut desk = Fake { clipboard_good_for: Some(0), ..Fake::default() };
        let outcome = deliver("舊剪貼簿內容", Mode::Insert, &mut desk, &never);
        assert_eq!(desk.pastes, 0);
        assert!(outcome.copied && !outcome.inserted && outcome.expand);
        assert_eq!(outcome.message, "剪貼簿已變更・結果保留預覽");
    }

    #[test]
    fn empty_or_cancelled_delivery_does_not_copy() {
        let mut desk = Fake::default();
        let empty = deliver(" \t", Mode::Insert, &mut desk, &never);
        assert!(!empty.copied && !empty.cancelled && !empty.expand);
        let cancelled = deliver("取消文字", Mode::Insert, &mut desk, &|| true);
        assert!(cancelled.cancelled && !cancelled.copied);
        assert!(desk.copies.is_empty());
        assert_eq!((desk.pastes, desk.there_asked), (0, 0));
    }

    #[test]
    fn cancel_while_waiting_for_release_does_not_copy() {
        let flag = std::rc::Rc::new(Cell::new(false));
        let mut desk = Fake { cancel_on_wait: Some(flag.clone()), ..Fake::default() };
        let outcome = deliver("等待後不得複製", Mode::Insert, &mut desk, &|| flag.get());
        assert!(outcome.cancelled);
        assert!(desk.copies.is_empty());
        assert_eq!((desk.waits, desk.pastes), (1, 0));
    }

    #[test]
    fn clipboard_failure_prevents_a_paste() {
        let mut desk = Fake { clipboard_busy: true, ..Fake::default() };
        let outcome = deliver("仍可取回", Mode::Insert, &mut desk, &never);
        assert_eq!(desk.pastes, 0);
        assert!(!outcome.copied && !outcome.inserted && outcome.expand);
        assert_eq!(outcome.message, "剪貼簿忙碌・結果可取回");
    }

    #[test]
    fn a_text_box_still_receives_text_when_the_clipboard_fails() {
        let mut desk = Fake { clipboard_busy: true, text_box: true, ..Fake::default() };
        let outcome = deliver("原生欄位結果", Mode::Insert, &mut desk, &never);
        assert_eq!(desk.written.as_deref(), Some("原生欄位結果"));
        assert_eq!(desk.pastes, 0);
        assert!(!outcome.copied && outcome.inserted && outcome.expand);
    }

    #[test]
    fn text_reaches_a_real_text_box_without_the_clipboard() {
        use windows::Win32::Foundation::HINSTANCE;
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GetWindowTextW, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE};
        let class: Vec<u16> = "EDIT".encode_utf16().chain(std::iter::once(0)).collect();
        // Hidden text boxes owned by this test; nothing appears on screen and focus does not move.
        let make = |style: u32| unsafe {
            CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()), PCWSTR::null(), WINDOW_STYLE(style), 0, 0, 200, 40, HWND::default(), HMENU::default(), HINSTANCE::default(), None)
                .unwrap()
        };
        let read = |control: HWND| {
            let mut held = [0u16; 32];
            let length = unsafe { GetWindowTextW(control, &mut held) } as usize;
            String::from_utf16_lossy(&held[..length])
        };
        let spot = |control: HWND| Spot { window: control.0 as isize, process: std::process::id(), focus: control.0 as isize };
        let (open, locked) = (make(0), make(0x0800));
        assert!(Desktop::new(spot(open)).write_directly("語音"));
        assert!(Desktop::new(spot(open)).write_directly("輸入"));
        assert_eq!(read(open), "語音輸入");
        assert!(!Desktop::new(spot(locked)).write_directly("不該寫入"));
        assert_eq!(read(locked), "");
        unsafe {
            let _ = DestroyWindow(open);
            let _ = DestroyWindow(locked);
        }
    }

    #[test]
    fn preview_only_stays_open_without_copying() {
        let mut desk = Fake::default();
        let outcome = deliver("預覽文字", Mode::Preview, &mut desk, &never);
        assert!(desk.copies.is_empty());
        assert_eq!((desk.pastes, desk.there_asked), (0, 0));
        assert!(!outcome.copied && outcome.expand);
    }
}
