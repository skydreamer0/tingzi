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
use windows::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
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
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, IsWindow,
    IsWindowVisible, MessageBoxW, SendMessageTimeoutW, SetForegroundWindow, GUITHREADINFO, GWL_STYLE, IDOK, MB_DEFBUTTON2,
    MB_ICONQUESTION, MB_OKCANCEL, MB_TOPMOST, SMTO_ABORTIFHUNG, SW_SHOWNORMAL,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// The system's name, for messages that send the user to its settings.
pub const SYSTEM: &str = "Windows";
/// What the settings page is told, so it can word itself for this system.
pub const PLATFORM: &str = "windows";
/// Where the service key is kept, as the user knows it.
pub const KEY_STORE: &str = "Windows 認證管理員";
/// The modifier pairs a shortcut may use, as the user reads them: Ctrl+Alt, Alt+Shift, Ctrl+Shift.
pub const MODIFIER_NAMES: [&str; 3] = ["Ctrl+Alt", "Alt+Shift", "Ctrl+Shift"];
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
    let strokes = [key(VK_CONTROL, false), key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)];
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

const SECRET: &str = "Tingzi/RecognitionServiceKey";

pub fn secret_read() -> Option<String> {
    let target = HSTRING::from(SECRET);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW fills `found` with a buffer Windows owns, which is read and then freed with CredFree.
    unsafe {
        CredReadW(&target, CRED_TYPE_GENERIC, 0, &mut found).ok()?;
        let blob = std::slice::from_raw_parts((*found).CredentialBlob, (*found).CredentialBlobSize as usize).to_vec();
        CredFree(found as *const _);
        String::from_utf8(blob).ok().filter(|key| !key.is_empty())
    }
}

/// Stores `key` for this Windows user, replacing any earlier one.
pub fn secret_store(key: &str) -> bool {
    let mut target = wide(SECRET);
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

pub fn secret_clear() {
    // SAFETY: the target name is a valid string for the length of the call.
    let _ = unsafe { CredDeleteW(&HSTRING::from(SECRET), CRED_TYPE_GENERIC, 0) };
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

/// The folder the local recogniser lives in when no `voice-root.txt` names one. Windows has none.
pub fn voice_root() -> Option<PathBuf> {
    None
}

/// The Whisper program inside the local recogniser's folder.
pub fn whisper_program(root: &Path) -> Option<PathBuf> {
    Some(root.join("whisper-v1.9.2").join("Release").join("whisper-cli.exe")).filter(|program| program.is_file())
}

/// Keeps a console program from flashing a window of its own.
pub fn quiet(command: &mut Command) -> &mut Command {
    const NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(NO_WINDOW)
}

/// Nothing to prepare on Windows.
pub fn prepare(_app: &mut tauri::App) {}

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
    fn this_test_program_is_not_codex() {
        assert!(!is_codex(std::process::id()));
        assert!(program(std::process::id()).unwrap().to_lowercase().ends_with(".exe"));
    }
}
