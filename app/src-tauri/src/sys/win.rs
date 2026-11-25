//! Windows: Win32 for windows, keys and dialogs, the registry for choices shared with the Zhuyin
//! text service, and Credential Manager for the service key.

use crate::deliver::Spot;
use crate::dialogs::Kind;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, BOOL, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, HWND, LPARAM, TRUE, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Globalization::{LCMapStringEx, LCMAP_SIMPLIFIED_CHINESE, LCMAP_TRADITIONAL_CHINESE};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use windows::Win32::Storage::Packaging::Appx::{GetPackageFamilyName, GetPackagesByPackageFamily};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, OpenProcess, QueryFullProcessImageNameW, SetEvent, WaitForMultipleObjects,
    EVENT_MODIFY_STATE, INFINITE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, UIA_EditControlTypeId};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, IsWindowEnabled, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_C, VK_V,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, MessageBoxW, SendMessageTimeoutW, SetForegroundWindow, GUITHREADINFO, GWL_STYLE, IDOK, MB_DEFBUTTON2,
    MB_ICONQUESTION, MB_OKCANCEL, MB_TOPMOST, SMTO_ABORTIFHUNG, SW_SHOWNORMAL, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, WS_EX_TOPMOST,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// The system's name, for messages that send the user to its settings.
pub const SYSTEM: &str = "Windows";
/// What the settings page is told, so it can word itself for this system.
pub const PLATFORM: &str = "windows";
/// How the installer for this system is named on the Releases page: its start and end. None when there is none yet, so the page is opened instead.
pub const UPDATE_INSTALLER: Option<(&str, &str)> = Some(("Tingzi-Setup-", "-x64.exe"));
/// Where the service key is kept, as the user knows it.
pub const KEY_STORE: &str = "Windows 認證管理員";
/// The modifier keys as the user reads them, in the order a shortcut is written: Control, Alt, Shift and the Windows key.
pub const MODIFIER_KEYS: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];
/// The system page that lists keyboards and input languages.
pub const LANGUAGE_SETTINGS: &str = "ms-settings:regionlanguage";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn handle(value: isize) -> HWND {
    HWND(value as *mut core::ffi::c_void)
}

// ---- Text

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

// ---- Where the keyboard is

/// The foreground window, the process that owns it, and the control with the caret.
pub fn spot() -> Spot {
    // SAFETY: these calls only read window manager state; the struct is sized as the API requires.
    unsafe {
        let window = GetForegroundWindow();
        let mut process = 0u32;
        let thread = GetWindowThreadProcessId(window, Some(&mut process));
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let focus = if thread != 0 && GetGUIThreadInfo(thread, &mut info).is_ok() { info.hwndFocus.0 as isize } else { 0 };
        Spot { window: window.0 as isize, process, focus }
    }
}

/// The control with the caret, when it is a standard Windows text box that can be written to.
fn text_box(spot: &Spot) -> Option<HWND> {
    let control = handle(spot.focus);
    let mut name = [0u16; 64];
    // SAFETY: the buffer length travels with the slice; the other calls only read state.
    unsafe {
        if spot.focus == 0 || !IsWindow(control).as_bool() || !IsWindowEnabled(control).as_bool() {
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

/// Writes straight into a standard text box, without the clipboard. False when the field is not one.
pub fn write_to_text_box(spot: &Spot, text: &str) -> bool {
    let Some(control) = text_box(spot) else { return false };
    let wide = wide(text);
    const REPLACE_SELECTION: u32 = 0x00C2;
    // SAFETY: the text is null-terminated and outlives the call; Windows copies it across processes
    // for this message. A message sent to one control cannot land in another application.
    let done = unsafe {
        SendMessageTimeoutW(control, REPLACE_SELECTION, WPARAM(1), LPARAM(wide.as_ptr() as isize), SMTO_ABORTIFHUNG, 1000, None)
    };
    done.0 != 0
}

/// Hands the keyboard back to the window recording started from.
pub fn bring_forward(spot: &Spot) {
    // SAFETY: any handle value is accepted; failure only leaves focus where it is.
    unsafe {
        let _ = SetForegroundWindow(handle(spot.window));
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
}

fn held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the current key state; no pointers involved.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

pub fn modifier_held() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].into_iter().any(held)
}

/// Whether the main mouse button is down, as it is for the length of a drag.
pub fn mouse_held() -> bool {
    held(VK_LBUTTON)
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

/// Presses Ctrl+V. False when the keys could not be sent.
pub fn press_paste() -> bool {
    press_with_control(VK_V)
}

/// Presses Ctrl+C, so the program in front copies what is selected. False when the keys could not be sent.
pub fn press_copy() -> bool {
    press_with_control(VK_C)
}

fn press_with_control(letter: VIRTUAL_KEY) -> bool {
    let strokes = [key(VK_CONTROL, false), key(letter, false), key(letter, true), key(VK_CONTROL, true)];
    // SAFETY: the slice and the element size describe the same array.
    let sent = unsafe { SendInput(&strokes, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == strokes.len()
}

/// A number that changes whenever any program writes to the clipboard.
pub fn clipboard_serial() -> u64 {
    // SAFETY: takes no arguments and only reads a counter.
    u64::from(unsafe { GetClipboardSequenceNumber() })
}

// ---- Programs

/// The file name of the program a process runs, such as Code.exe. None when Windows will not say.
pub fn program(process: u32) -> Option<String> {
    if process == 0 {
        return None;
    }
    let mut name = vec![0u16; 1024];
    let mut length = name.len() as u32;
    // SAFETY: the buffer and its length travel together; the handle is closed before returning.
    let found = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process).ok()?;
        let found = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(name.as_mut_ptr()), &mut length);
        let _ = CloseHandle(handle);
        found
    };
    found.ok()?;
    let path = PathBuf::from(String::from_utf16_lossy(&name[..length as usize]));
    path.file_name().map(|file| file.to_string_lossy().into_owned())
}

unsafe extern "system" fn note_window(window: HWND, found: LPARAM) -> BOOL {
    // SAFETY: `found` is the vector `processes_on_screen` passed in, alive for the whole enumeration.
    let processes = &mut *(found.0 as *mut Vec<u32>);
    if IsWindowVisible(window).as_bool() {
        let mut process = 0u32;
        GetWindowThreadProcessId(window, Some(&mut process));
        if process != 0 && !processes.contains(&process) {
            processes.push(process);
        }
    }
    TRUE
}

/// The processes that have a window on screen now.
pub fn processes_on_screen() -> Vec<u32> {
    let mut processes: Vec<u32> = Vec::new();
    // SAFETY: the callback only reads window state and writes to the vector, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(note_window), LPARAM(&mut processes as *mut Vec<u32> as isize));
    }
    processes
}

const CODEX_PACKAGE: &str = "OpenAI.Codex_2p2nqsd0c76g0";

/// The version of the Codex desktop app installed for this user, as Codex writes it; the newest
/// when there is more than one. None when it is not installed.
pub fn codex_version() -> Option<String> {
    let family = wide(CODEX_PACKAGE);
    let (mut count, mut length) = (0u32, 0u32);
    // SAFETY: the first call only asks for sizes; the second is given buffers of exactly those sizes,
    // and the names it points to live in `text`, which outlives their use.
    let names: Vec<String> = unsafe {
        let _ = GetPackagesByPackageFamily(PCWSTR(family.as_ptr()), &mut count, None, &mut length, PWSTR::null());
        if count == 0 || length == 0 {
            return None;
        }
        let mut found = vec![PWSTR::null(); count as usize];
        let mut text = vec![0u16; length as usize];
        if GetPackagesByPackageFamily(PCWSTR(family.as_ptr()), &mut count, Some(found.as_mut_ptr()), &mut length, PWSTR(text.as_mut_ptr())) != ERROR_SUCCESS {
            return None;
        }
        found.iter().take(count as usize).filter(|name| !name.is_null()).filter_map(|name| name.to_string().ok()).collect()
    };
    names.iter().filter_map(|name| crate::codex::package_version(name)).max().map(crate::codex::version_text)
}

/// Whether `process` is the Codex desktop app, by the package Windows installed it from.
fn is_codex(process: u32) -> bool {
    // SAFETY: the handle is closed before returning and the buffer length is passed with the buffer.
    unsafe {
        let Ok(opened) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else { return false };
        let mut name = [0u16; 128];
        let mut length = name.len() as u32;
        let found = GetPackageFamilyName(opened, &mut length, PWSTR(name.as_mut_ptr())) == ERROR_SUCCESS;
        let _ = CloseHandle(opened);
        let end = name.iter().position(|&unit| unit == 0).unwrap_or(0);
        found && String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case(CODEX_PACKAGE)
    }
}

/// Whether the keyboard is in Codex's message box rather than, say, its search field.
pub fn in_codex_message_box(process: u32) -> bool {
    if !is_codex(process) {
        return false;
    }
    // SAFETY: COM is initialised on this thread before use; every call returns an owned object or an error.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(automation) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { return false };
        let Ok(focused) = automation.GetFocusedElement() else { return false };
        let editable = focused.CurrentControlType().is_ok_and(|kind| kind == UIA_EditControlTypeId)
            && focused.CurrentIsEnabled().is_ok_and(|enabled| enabled.as_bool())
            && focused.CurrentIsPassword().is_ok_and(|password| !password.as_bool());
        let class = focused.CurrentClassName().map(|name| name.to_string()).unwrap_or_default();
        let id = focused.CurrentAutomationId().map(|name| name.to_string()).unwrap_or_default();
        editable && crate::codex::is_composer(&class, &id)
    }
}

/// The Windows edition and release as people know it, such as `Windows 11 24H2（組建 26100）`.
pub fn system_version() -> String {
    let key = RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let read = |name: &str| key.as_ref().ok().and_then(|key| key.get_value::<String, _>(name).ok()).unwrap_or_default();
    let build = read("CurrentBuild");
    // Windows 11 still calls itself Windows 10 in ProductName; its builds start at 22000.
    let name = if build.parse::<u32>().is_ok_and(|number| number >= 22000) { "Windows 11" } else { "Windows 10" };
    let release = read("DisplayVersion");
    match (release.is_empty(), build.is_empty()) {
        (_, true) => "Windows".into(),
        (true, false) => format!("{name}（組建 {build}）"),
        (false, false) => format!("{name} {release}（組建 {build}）"),
    }
}

// ---- Dialogs

/// The product's name on this system, for window titles and dialogs.
pub const TITLE: &str = "聽字・注音與語音輸入法";

fn filter(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Wav => ("WAV 音檔\0*.wav\0", "wav"),
        Kind::Text => ("文字檔 (*.txt)\0*.txt\0所有檔案 (*.*)\0*.*\0", "txt"),
        Kind::Table => ("Tab 分隔文字 (*.tsv)\0*.tsv\0所有檔案 (*.*)\0*.*\0", "tsv"),
    }
}

/// Lets the user pick a file to open, or name one to save when `save_as` is given.
/// `owner` is the window the dialog belongs to, or 0. Blocks until it closes; call it off the UI thread.
pub fn pick(owner: isize, kind: Kind, save_as: Option<&str>) -> Option<PathBuf> {
    let (filter, extension) = filter(kind);
    let (filter, extension) = (wide(filter), wide(extension));
    let mut chosen = vec![0u16; 32768];
    if let Some(name) = save_as {
        let name: Vec<u16> = name.encode_utf16().collect();
        chosen[..name.len()].copy_from_slice(&name);
    }
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: handle(owner),
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
        MessageBoxW(handle(owner), PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OKCANCEL | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_TOPMOST)
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

/// The window a dialog should belong to, as a plain number; 0 when there is none.
pub fn owner(window: Option<tauri::WebviewWindow>) -> isize {
    window.and_then(|window| window.hwnd().ok()).map_or(0, |handle| handle.0 as isize)
}

/// Puts the window back above ordinary windows when something has taken that away, and says whether it had to.
/// The window is created always on top, but other programs, such as window managers, can clear the flag,
/// and asking Tauri again does nothing because it still believes the flag is set.
pub fn keep_on_top(window: &tauri::WebviewWindow) -> bool {
    let Ok(hwnd) = window.hwnd() else { return false };
    let hwnd = handle(hwnd.0 as isize);
    // SAFETY: the handle belongs to a window of this program; both calls only read or reorder it.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 {
            return false;
        }
        // Asynchronous, so a busy window thread never holds up the caller.
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS).is_ok()
    }
}

// ---- One running copy

fn local(name: &str) -> Vec<u16> {
    wide(&format!("Local\\{name}"))
}

/// Claims the single-instance name. False means another copy is already running.
pub fn first_instance(name: &str) -> bool {
    let name = local(name);
    // SAFETY: the name is null-terminated; the handle is kept for the life of the process on purpose.
    unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())).is_ok() && GetLastError() != ERROR_ALREADY_EXISTS }
}

/// Raises a named signal in the copy that is already running. False when it could not be reached.
pub fn signal(name: &str) -> bool {
    let name = local(name);
    // SAFETY: the name is null-terminated and the handle is only used to raise the signal.
    unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())).is_ok_and(|event| SetEvent(event).is_ok()) }
}

/// Listens for the named signals for the life of the app, and calls `raised` with a signal's place in `names`.
pub fn listen(names: Vec<String>, raised: impl Fn(usize) + Send + 'static) {
    // Handles are plain numbers here so the list can move to the listening thread.
    let events: Vec<isize> = names
        .iter()
        .filter_map(|name| {
            let name = local(name);
            // SAFETY: the name is null-terminated; auto-reset events start unsignalled.
            unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())).ok() }.map(|event| event.0 as isize)
        })
        .collect();
    if events.len() != names.len() {
        return;
    }
    std::thread::spawn(move || {
        let events: Vec<HANDLE> = events.into_iter().map(|event| HANDLE(event as *mut core::ffi::c_void)).collect();
        loop {
            // SAFETY: the handles stay open for the life of the process.
            let woken = unsafe { WaitForMultipleObjects(&events, false, INFINITE) };
            let index = woken.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
            if index >= events.len() {
                return;
            }
            raised(index);
        }
    });
}

// ---- The service key

/// Where the key called `name` is kept in Credential Manager, such as `Tingzi/RecognitionServiceKey`.
fn secret_target(name: &str) -> String {
    format!("Tingzi/{name}")
}

pub fn secret_read(name: &str) -> Option<String> {
    let target = HSTRING::from(secret_target(name));
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW fills `found` with a buffer Windows owns, which is read and then freed with CredFree.
    unsafe {
        CredReadW(&target, CRED_TYPE_GENERIC, 0, &mut found).ok()?;
        let blob = std::slice::from_raw_parts((*found).CredentialBlob, (*found).CredentialBlobSize as usize).to_vec();
        CredFree(found as *const _);
        String::from_utf8(blob).ok().filter(|key| !key.is_empty())
    }
}

/// Stores `key` under `name` for this Windows user, replacing any earlier one.
pub fn secret_store(name: &str, key: &str) -> bool {
    let mut target = wide(&secret_target(name));
    let mut blob = key.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `credential` points into buffers that live until the call returns.
    unsafe { CredWriteW(&credential, 0) }.is_ok()
}

pub fn secret_clear(name: &str) {
    // SAFETY: the target name is a valid string for the length of the call.
    let _ = unsafe { CredDeleteW(&HSTRING::from(secret_target(name)), CRED_TYPE_GENERIC, 0) };
}

// ---- Choices kept by the system

const ENGINE_KEY: &str = r"Software\Tingzi\Engine";
const LOOK_KEY: &str = r"Software\Tingzi\Appearance";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The Zhuyin text service's choices, which it reads from the registry when an application loads it:
/// keyboard layout, staged segmentation, candidate size and composition line size. None for one not set.
pub fn zhuyin_choices() -> [Option<u32>; 4] {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let number = |key: &str, name: &str| user.open_subkey(key).and_then(|key| key.get_value::<u32, _>(name)).ok();
    [number(ENGINE_KEY, "KeyboardLayout"), number(ENGINE_KEY, "StagedSegmentation"), number(LOOK_KEY, "CandidateDIP"), number(LOOK_KEY, "CompositionDIP")]
}

pub fn store_zhuyin_choices(layout: u32, staged: u32, candidate_size: u32, line_size: u32) -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (engine, _) = user.create_subkey(ENGINE_KEY)?;
    engine.set_value("KeyboardLayout", &layout)?;
    engine.set_value("StagedSegmentation", &staged)?;
    let (look, _) = user.create_subkey(LOOK_KEY)?;
    look.set_value("CandidateDIP", &candidate_size)?;
    look.set_value("CompositionDIP", &line_size)
}

/// Tells the text service the choices it shares with the voice window: it converts committed Zhuyin
/// text, and draws its windows, by the same ones.
pub fn share_with_text_service(simplified: bool, appearance: u32) -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    user.create_subkey(ENGINE_KEY)?.0.set_value("SimplifiedOutput", &u32::from(simplified))?;
    user.create_subkey(LOOK_KEY)?.0.set_value("Theme", &appearance)
}

pub fn starts_at_login() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).and_then(|run| run.get_value::<String, _>("Tingzi")).is_ok()
}

pub fn set_login_start(enabled: bool) -> std::io::Result<()> {
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if enabled {
        let program = std::env::current_exe()?;
        run.set_value("Tingzi", &format!("\"{}\"", program.display()))
    } else {
        run.delete_value("Tingzi").or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) })
    }
}

/// The Windows accent colour as red, green and blue.
pub fn accent() -> Option<(u32, u32, u32)> {
    let stored: u32 = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\DWM").ok()?.get_value("AccentColor").ok()?;
    // Windows stores the colour as alpha, blue, green, red.
    Some((stored & 0xff, (stored >> 8) & 0xff, (stored >> 16) & 0xff))
}

/// The proxy set in Windows' proxy settings for HTTPS traffic to `host`.
pub fn system_proxy(host: &str) -> Option<String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let settings = user.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings").ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    let server: String = settings.get_value("ProxyServer").ok()?;
    let bypass: String = settings.get_value("ProxyOverride").unwrap_or_default();
    crate::recognise::https_proxy(enabled != 0, &server, &bypass, host)
}

// ---- Folders and programs

/// The user's settings, words and window position.
pub fn data() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Tingzi")
}

/// The user's home folder.
pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
}

/// The installed folder that holds the word list and the Zhuyin tools.
/// The app sits one level below it when installed; a development build uses the newest installed version.
pub fn install() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.to_path_buf()));
    if let Some(folder) = beside.filter(|folder| folder.join("phrases.txt").is_file()) {
        return Some(folder);
    }
    let programs = PathBuf::from(std::env::var_os("ProgramFiles")?).join("Tingzi");
    let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(programs)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let numbers: Option<Vec<u32>> = name.strip_prefix('v')?.split('.').map(|part| part.parse().ok()).collect();
            Some((numbers?, entry.path()))
        })
        .collect();
    versions.sort();
    versions.pop().map(|(_, folder)| folder)
}

/// The Whisper program inside one of the local recogniser's folders. With `gpu`, only the build that
/// uses an NVIDIA card; without, the plain build the app downloaded or the one set up by hand.
pub fn whisper_program(root: &Path, gpu: bool) -> Option<PathBuf> {
    let folders: &[&str] = if gpu { &["engine-gpu"] } else { &["engine-cpu", "whisper-v1.9.2"] };
    folders.iter().map(|folder| root.join(folder).join("Release").join("whisper-cli.exe")).find(|program| program.is_file())
}

/// The Visual C++ runtime that whisper.cpp's Windows builds need but do not include. The voice window carries it.
const WHISPER_RUNTIME: [&str; 4] = ["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll", "vcomp140.dll"];

/// Puts the runtime the voice window carries into `folder`, beside the Whisper program, where Windows looks
/// before the system folder. The program then runs on a computer without the Visual C++ Redistributable,
/// or with an older one.
pub fn whisper_runtime(folder: &Path) {
    if let Some(carried) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.to_path_buf())) {
        copy_runtime(&carried, folder);
    }
}

/// Copies the runtime from `carried` into `folder`. A file already there with the same size is left alone.
pub fn copy_runtime(carried: &Path, folder: &Path) {
    for name in WHISPER_RUNTIME {
        let (from, to) = (carried.join(name), folder.join(name));
        let Ok(size) = std::fs::metadata(&from).map(|meta| meta.len()) else { continue };
        if std::fs::metadata(&to).map(|meta| meta.len()).ok() != Some(size) {
            let _ = std::fs::copy(&from, &to);
        }
    }
}

fn system_file(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32").join(name)
}

/// Whether this computer has an NVIDIA driver, which the downloadable graphics card build needs.
pub fn gpu_driver() -> bool {
    system_file("nvcuda.dll").is_file()
}

/// The archive tool that ships with Windows; it reads zip files.
pub fn tar() -> PathBuf {
    system_file("tar.exe")
}

/// Keeps a console program from flashing a window of its own.
pub fn quiet(command: &mut Command) -> &mut Command {
    const NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(NO_WINDOW)
}

/// Nothing to prepare on Windows.
pub fn prepare(_app: &mut tauri::App) {}

/// The Zhuyin input method's learning file, which the voice window shares with it; its tables are
/// described in `docs/specs/learning-data.md`. Windows' own SQLite is used, as the input method does.
mod learning {
    use std::ffi::{c_char, c_int, c_void, CStr, CString};

    #[link(name = "winsqlite3", kind = "raw-dylib")]
    extern "system" {
        fn sqlite3_open_v2(name: *const c_char, db: *mut *mut c_void, flags: c_int, vfs: *const c_char) -> c_int;
        fn sqlite3_close(db: *mut c_void) -> c_int;
        fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
        fn sqlite3_exec(db: *mut c_void, sql: *const c_char, callback: *const c_void, argument: *mut c_void, error: *mut *mut c_char) -> c_int;
        fn sqlite3_prepare_v2(db: *mut c_void, sql: *const c_char, bytes: c_int, statement: *mut *mut c_void, tail: *mut *const c_char) -> c_int;
        fn sqlite3_bind_text(statement: *mut c_void, index: c_int, text: *const c_char, bytes: c_int, destructor: isize) -> c_int;
        fn sqlite3_bind_int64(statement: *mut c_void, index: c_int, value: i64) -> c_int;
        fn sqlite3_step(statement: *mut c_void) -> c_int;
        fn sqlite3_column_int64(statement: *mut c_void, column: c_int) -> i64;
        fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char;
        fn sqlite3_finalize(statement: *mut c_void) -> c_int;
    }

    const OK: c_int = 0;
    const ROW: c_int = 100;
    const DONE: c_int = 101;
    const OPEN_READWRITE: c_int = 0x2;
    /// SQLite copies the text before the call returns.
    const TRANSIENT: isize = -1;

    pub enum Value<'a> {
        Number(i64),
        Text(&'a str),
    }

    pub struct Db(*mut c_void);

    impl Db {
        /// Opens the file only when the input method has made it; the voice window never creates it.
        pub fn open() -> Option<Db> {
            let file = super::data().join("preferences.sqlite3");
            if !file.is_file() {
                return None;
            }
            let name = CString::new(file.to_str()?).ok()?;
            let mut db = std::ptr::null_mut();
            let opened = unsafe { sqlite3_open_v2(name.as_ptr(), &mut db, OPEN_READWRITE, std::ptr::null()) };
            let db = Db(db);
            if opened != OK {
                return None;
            }
            unsafe { sqlite3_busy_timeout(db.0, 350) };
            Some(db)
        }

        pub fn exec(&self, sql: &str) -> bool {
            let Ok(sql) = CString::new(sql) else { return false };
            unsafe { sqlite3_exec(self.0, sql.as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut()) == OK }
        }

        /// Runs one statement, handing each row's columns to `row`. False when it failed.
        pub fn query(&self, sql: &str, values: &[Value], mut row: impl FnMut(&Row)) -> bool {
            let Ok(sql) = CString::new(sql) else { return false };
            let mut statement = std::ptr::null_mut();
            if unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut statement, std::ptr::null_mut()) } != OK {
                unsafe { sqlite3_finalize(statement) };
                return false;
            }
            let mut bound = true;
            for (index, value) in values.iter().enumerate() {
                let index = index as c_int + 1;
                let done = match value {
                    Value::Number(number) => unsafe { sqlite3_bind_int64(statement, index, *number) },
                    Value::Text(text) => unsafe { sqlite3_bind_text(statement, index, text.as_ptr().cast(), text.len() as c_int, TRANSIENT) },
                };
                bound &= done == OK;
            }
            let finished = bound
                && loop {
                    match unsafe { sqlite3_step(statement) } {
                        ROW => row(&Row(statement)),
                        DONE => break true,
                        _ => break false,
                    }
                };
            unsafe { sqlite3_finalize(statement) };
            finished
        }
    }

    impl Drop for Db {
        fn drop(&mut self) {
            unsafe { sqlite3_close(self.0) };
        }
    }

    pub struct Row(*mut c_void);

    impl Row {
        pub fn number(&self, column: c_int) -> i64 {
            unsafe { sqlite3_column_int64(self.0, column) }
        }

        pub fn text(&self, column: c_int) -> String {
            let text = unsafe { sqlite3_column_text(self.0, column) };
            if text.is_null() {
                return String::new();
            }
            unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
        }
    }

    pub fn enabled(db: &Db) -> bool {
        let mut on = false;
        db.query("SELECT enabled FROM settings", &[], |row| on = row.number(0) == 1);
        on
    }
}

fn seconds_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}

/// The words the Zhuyin input method committed over part of a dictation, newest first.
/// None when its learning file cannot be read or learning is off.
pub fn typed_fixes() -> Option<Vec<crate::learn::Fix>> {
    let db = learning::Db::open()?;
    if !learning::enabled(&db) {
        return None;
    }
    let mut fixes = Vec::new();
    // No table yet means no fix has been typed yet.
    db.query("SELECT at,said,typed,reading FROM fixes ORDER BY at DESC", &[], |row| {
        fixes.push(crate::learn::Fix { at: row.number(0).max(0) as u64, said: row.text(1), typed: row.text(2), reading: row.text(3) })
    });
    Some(fixes)
}

/// Tells the Zhuyin input method about text just dictated into a program, so that a word typed
/// over part of it can be recognised as a fix. Only the five latest from the last ten minutes are
/// kept, and nothing while its learning is off.
pub fn note_dictation(text: &str) {
    use learning::Value::{Number, Text};
    let Some(db) = learning::Db::open() else { return };
    if !learning::enabled(&db) || !db.exec("CREATE TABLE IF NOT EXISTS dictations(at INTEGER NOT NULL,text TEXT NOT NULL)") {
        return;
    }
    let now = seconds_now();
    db.query("INSERT INTO dictations VALUES(?,?)", &[Number(now), Text(text)], |_| {});
    db.query(
        "DELETE FROM dictations WHERE at<? OR rowid NOT IN (SELECT rowid FROM dictations ORDER BY at DESC,rowid DESC LIMIT 5)",
        &[Number(now - 600)],
        |_| {},
    );
}

/// Forgets the dictations noted for the input method, as when the user stops learning from fixes.
pub fn forget_dictations() {
    if let Some(db) = learning::Db::open() {
        db.exec("DELETE FROM dictations");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(write_to_text_box(&spot(open), "語音"));
        assert!(write_to_text_box(&spot(open), "輸入"));
        assert_eq!(read(open), "語音輸入");
        assert!(!write_to_text_box(&spot(locked), "不該寫入"));
        assert_eq!(read(locked), "");
        unsafe {
            let _ = DestroyWindow(open);
            let _ = DestroyWindow(locked);
        }
    }

    #[test]
    fn the_installed_codex_version_reads_as_a_version() {
        // Codex may or may not be installed where the tests run; when it is, the answer is numbers and dots.
        if let Some(version) = codex_version() {
            assert!(version.split('.').count() >= 3 && version.split('.').all(|part| part.parse::<u32>().is_ok()), "{version}");
        }
    }

    #[test]
    fn this_test_program_is_not_codex() {
        assert!(!is_codex(std::process::id()));
        assert!(program(std::process::id()).unwrap().to_lowercase().ends_with(".exe"));
    }
}

pub fn preferred_language() -> Option<String> {
    sys_locale::get_locale()
}


// Count events rather than polling held keys: a tap can start and finish between updates.
static INPUT_WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INPUT_ACTIVITY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

unsafe extern "system" fn watch_keyboard(
    code: i32,
    message: WPARAM,
    data: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    if code >= 0 && matches!(message.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        let event = &*(data.0 as *const KBDLLHOOKSTRUCT);
        // All injected keys are excluded, including Tingzi's Unicode and shortcut events.
        let modifier = matches!(event.vkCode, 0x10..=0x12 | 0x5B..=0x5C | 0xA0..=0xA5);
        if !modifier && event.flags.0 & LLKHF_INJECTED.0 == 0 && !is_stream_shortcut(event.vkCode) {
            INPUT_ACTIVITY.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, message, data)
}

unsafe extern "system" fn watch_mouse(
    code: i32,
    message: WPARAM,
    data: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    if code >= 0
        && matches!(
            message.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
    {
        let event = &*(data.0 as *const MSLLHOOKSTRUCT);
        if event.flags & 1 == 0 {
            INPUT_ACTIVITY.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, message, data)
}

/// Changes on physical non-modifier key presses and mouse clicks. Failure changes every read,
/// so callers abandon an insertion rather than assuming an unobserved caret stayed still.
pub fn input_activity() -> u64 {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let ready = READY.get_or_init(|| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("input-activity".into())
            .spawn(move || {
                use windows::Win32::UI::WindowsAndMessaging::*;
                // SAFETY: callbacks are static and execute on this dedicated message-loop thread.
                unsafe {
                    let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(watch_keyboard), None, 0);
                    let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(watch_mouse), None, 0);
                    INPUT_WATCHING.store(
                        keyboard.is_ok() && mouse.is_ok(),
                        std::sync::atomic::Ordering::SeqCst,
                    );
                    let _ = send.send(keyboard.is_ok() && mouse.is_ok());
                    match (keyboard, mouse) {
                        (Ok(keyboard), Ok(mouse)) => {
                            let mut message = MSG::default();
                            while GetMessageW(&mut message, None, 0, 0).0 > 0 {}
                            let _ = UnhookWindowsHookEx(keyboard);
                            let _ = UnhookWindowsHookEx(mouse);
                        }
                        (keyboard, mouse) => {
                            if let Ok(hook) = keyboard {
                                let _ = UnhookWindowsHookEx(hook);
                            }
                            if let Ok(hook) = mouse {
                                let _ = UnhookWindowsHookEx(hook);
                            }
                        }
                    }
                    INPUT_WATCHING.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            })
            .is_err()
        {
            return false;
        }
        receive.recv().unwrap_or(false)
    });
    if !ready || !INPUT_WATCHING.load(std::sync::atomic::Ordering::SeqCst) {
        INPUT_ACTIVITY.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    INPUT_ACTIVITY.load(std::sync::atomic::Ordering::SeqCst)
}

/// Sends Unicode keyboard input without changing the clipboard.
pub fn type_text(text: &str) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_UNICODE;
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        for up in [false, true] {
            let mut event = key(VIRTUAL_KEY(0), up);
            // SAFETY: key() initializes the keyboard member of the INPUT union.
            unsafe {
                event.Anonymous.ki.wScan = unit;
                event.Anonymous.ki.dwFlags |= KEYEVENTF_UNICODE;
            }
            inputs.push(event);
        }
    }
    inputs.is_empty()
        || unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len()
        }
}

/// Deletes `count` preceding characters using Backspace key presses.
pub fn backspace(count: usize) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_BACK;
    let inputs: Vec<_> = (0..count)
        .flat_map(|_| [key(VK_BACK, false), key(VK_BACK, true)])
        .collect();
    inputs.is_empty()
        || unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize == inputs.len()
        }
}

/// Windows focus is already identified by the window/control handles in Spot.
pub fn stream_focus() -> Option<u64> {
    Some(0)
}

static STREAM_SHORTCUT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Excludes only the registered recording shortcut from manual-input cancellation.
pub fn set_stream_shortcut(shortcut: crate::hotkey::Hotkey) {
    STREAM_SHORTCUT.store(
        (u64::from(shortcut.modifiers) << 32) | u64::from(shortcut.key),
        std::sync::atomic::Ordering::SeqCst,
    );
}

fn is_stream_shortcut(key: u32) -> bool {
    use crate::hotkey;
    let shortcut = STREAM_SHORTCUT.load(std::sync::atomic::Ordering::SeqCst);
    if key == 0 || key != shortcut as u32 {
        return false;
    }
    let modifiers = if held(VK_MENU) { hotkey::ALT } else { 0 }
        | if held(VK_CONTROL) { hotkey::CONTROL } else { 0 }
        | if held(VK_SHIFT) { hotkey::SHIFT } else { 0 }
        | if held(VK_LWIN) || held(VK_RWIN) {
            hotkey::SUPER
        } else {
            0
        };
    modifiers == (shortcut >> 32) as u32
}
