//! macOS: AppKit for the frontmost program and the pasteboard, Quartz events for keys,
//! the keychain for the service key, and a launch agent for starting at login.
//!
//! macOS has no Tingzi Zhuyin input method, so the choices the Windows text service reads are
//! accepted and dropped here.

use crate::deliver::Spot;
use crate::dialogs::Kind;
use core_foundation::base::{CFRelease, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationOptions, NSApplicationActivationPolicy, NSPasteboard, NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::Manager;

/// The system's name, for messages that send the user to its settings.
pub const SYSTEM: &str = "macOS";
/// What the settings page is told, so it can word itself for this system.
pub const PLATFORM: &str = "macos";
/// How the installer for this system is named on the Releases page: its start and end. None when there is none yet, so the page is opened instead.
pub const UPDATE_INSTALLER: Option<(&str, &str)> = None;
/// Where the service key is kept, as the user knows it.
pub const KEY_STORE: &str = "鑰匙圈";
/// The modifier keys as the user reads them, in the order a shortcut is written: Control, Option, Shift and Command.
pub const MODIFIER_KEYS: [&str; 4] = ["Control", "Option", "Shift", "Command"];
/// The system page that lists keyboards and input sources.
pub const LANGUAGE_SETTINGS: &str = "x-apple.systempreferences:com.apple.Keyboard-Settings.extension";

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> u8;
}

/// The keys and buttons held now, whichever program they were pressed in.
const COMBINED_STATE: i32 = 0;
const SHIFT: u64 = 0x0002_0000;
const CONTROL: u64 = 0x0004_0000;
const OPTION: u64 = 0x0008_0000;
const COMMAND: u64 = 0x0010_0000;

// ---- Text

/// Character-level script conversion from the system's own tables.
pub fn convert_script(text: &str, simplified: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    autoreleasepool(|_| {
        let transform = NSString::from_str(if simplified { "Hant-Hans" } else { "Hans-Hant" });
        #[allow(unused_unsafe)]
        let mapped = unsafe { NSString::from_str(text).stringByApplyingTransform_reverse(&transform, false) };
        mapped.map_or_else(|| text.to_string(), |mapped| mapped.to_string())
    })
}

// ---- Where the keyboard is

/// The process of the program in front, when there is one.
fn frontmost() -> Option<u32> {
    autoreleasepool(|_| {
        #[allow(unused_unsafe)]
        let program = unsafe { NSWorkspace::sharedWorkspace().frontmostApplication() }?;
        #[allow(unused_unsafe)]
        let process = unsafe { program.processIdentifier() };
        u32::try_from(process).ok()
    })
}

/// The program in front. macOS does not show another program's windows or fields without the
/// accessibility permission, so the program stands for its window and no control is named.
pub fn spot() -> Spot {
    let process = frontmost().unwrap_or(0);
    Spot { window: process as isize, process, focus: 0 }
}

/// macOS has no text box that can be written to from outside; text always goes through the pasteboard.
pub fn write_to_text_box(_spot: &Spot, _text: &str) -> bool {
    false
}

/// Hands the keyboard back to the program recording started from, and waits until it is in front.
pub fn bring_forward(spot: &Spot) {
    autoreleasepool(|_| {
        #[allow(unused_unsafe)]
        let program = unsafe { NSRunningApplication::runningApplicationWithProcessIdentifier(spot.process as i32) };
        if let Some(program) = program {
            #[allow(unused_unsafe)]
            let _ = unsafe { program.activateWithOptions(NSApplicationActivationOptions::empty()) };
        }
    });
    for _ in 0..20 {
        if frontmost() == Some(spot.process) {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // The program in front still has to give the keyboard to its own window.
    std::thread::sleep(Duration::from_millis(80));
}

pub fn modifier_held() -> bool {
    // SAFETY: reads the current key state; no pointers involved.
    unsafe { CGEventSourceFlagsState(COMBINED_STATE) & (SHIFT | CONTROL | OPTION | COMMAND) != 0 }
}

/// Whether the main mouse button is down, as it is for the length of a drag.
pub fn mouse_held() -> bool {
    // SAFETY: reads the current button state; no pointers involved.
    unsafe { CGEventSourceButtonState(COMBINED_STATE, 0) }
}

/// Whether macOS lets this app press keys in other programs. The first time it does not, macOS
/// shows its own request, which leads to Privacy & Security > Accessibility.
fn may_press_keys() -> bool {
    static ASKED: AtomicBool = AtomicBool::new(false);
    // SAFETY: the dictionary lives until the call returns, and the key is a constant the system owns.
    unsafe {
        if AXIsProcessTrusted() != 0 {
            return true;
        }
        if !ASKED.swap(true, Ordering::SeqCst) {
            let prompt = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let options = CFDictionary::from_CFType_pairs(&[(prompt, CFBoolean::true_value())]);
            return AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0;
        }
    }
    false
}

/// Presses Command+V. False when macOS has not been told this app may press keys.
pub fn press_paste() -> bool {
    if !may_press_keys() {
        return false;
    }
    /// The key in V's place on a US keyboard; programs read it together with Command.
    const V: u16 = 9;
    const WHOLE_SYSTEM: u32 = 0;
    // SAFETY: each event is checked, used and released here; a null source means the default one.
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), V, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), V, false);
        let made = !down.is_null() && !up.is_null();
        if made {
            CGEventSetFlags(down, COMMAND);
            CGEventSetFlags(up, COMMAND);
            CGEventPost(WHOLE_SYSTEM, down);
            CGEventPost(WHOLE_SYSTEM, up);
        }
        for event in [down, up] {
            if !event.is_null() {
                CFRelease(event as *const c_void);
            }
        }
        made
    }
}

/// A number that changes whenever any program writes to the pasteboard.
pub fn clipboard_serial() -> u64 {
    autoreleasepool(|_| {
        #[allow(unused_unsafe)]
        let count = unsafe { NSPasteboard::generalPasteboard().changeCount() };
        u64::try_from(count).unwrap_or(0)
    })
}

// ---- Programs

/// The name a program goes by: its application bundle, such as Safari.app, or the file itself
/// when it is not in one. Helpers live in bundles inside the program's own, so the outermost counts.
fn program_name(path: &Path) -> Option<String> {
    let bundle = path.iter().map(|part| part.to_string_lossy()).find(|part| part.ends_with(".app"));
    bundle.map(|name| name.into_owned()).or_else(|| path.file_name().map(|file| file.to_string_lossy().into_owned()))
}

/// The name of the program a process runs, such as Safari.app. None when macOS will not say.
pub fn program(process: u32) -> Option<String> {
    if process == 0 {
        return None;
    }
    let mut path = vec![0u8; 4096];
    // SAFETY: the buffer and its length travel together.
    let length = unsafe { libc::proc_pidpath(process as i32, path.as_mut_ptr().cast(), path.len() as u32) };
    if length <= 0 {
        return None;
    }
    program_name(Path::new(std::ffi::OsStr::from_bytes(&path[..length as usize])))
}

/// The processes of the programs in the Dock, which are the ones that can have a window on screen.
pub fn processes_on_screen() -> Vec<u32> {
    autoreleasepool(|_| {
        #[allow(unused_unsafe)]
        let programs = unsafe { NSWorkspace::sharedWorkspace().runningApplications() };
        let mut processes = Vec::new();
        for index in 0..programs.count() {
            let program = programs.objectAtIndex(index);
            #[allow(unused_unsafe)]
            let (policy, process) = unsafe { (program.activationPolicy(), program.processIdentifier()) };
            if policy == NSApplicationActivationPolicy::Regular {
                processes.extend(u32::try_from(process).ok());
            }
        }
        processes
    })
}

/// None: on macOS the Codex recogniser reads the installed version itself.
pub fn codex_version() -> Option<String> {
    None
}

/// Whether the keyboard is in the Codex desktop app. Telling its message box from its other
/// fields needs the accessibility permission, so any field in Codex counts here.
pub fn in_codex_message_box(process: u32) -> bool {
    program(process).is_some_and(|name| name.eq_ignore_ascii_case("Codex.app"))
}

// ---- Dialogs

/// The product's name on this system, where it has no Zhuyin input method.
pub const TITLE: &str = "聽字・語音輸入";

/// Lets the user pick a file to open, or name one to save when `save_as` is given.
/// Blocks until the dialog closes; call it off the UI thread.
pub fn pick(_owner: isize, kind: Kind, save_as: Option<&str>) -> Option<PathBuf> {
    let (label, extension) = match kind {
        Kind::Wav => ("WAV 音檔", "wav"),
        Kind::Text => ("文字檔", "txt"),
        Kind::Table => ("Tab 分隔文字", "tsv"),
    };
    let dialog = rfd::FileDialog::new();
    match save_as {
        Some(name) => dialog.add_filter(label, &[extension]).set_file_name(name).save_file(),
        None if matches!(kind, Kind::Wav) => dialog.add_filter(label, &[extension]).pick_file(),
        // Word lists and tables may carry any extension, as they may on Windows.
        None => dialog.pick_file(),
    }
}

/// Asks a yes-or-no question; Cancel is the default so Return never confirms by accident.
pub fn confirm(_owner: isize, question: &str) -> bool {
    const GO_ON: &str = "繼續";
    let answer = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(TITLE)
        .set_description(question)
        .set_buttons(rfd::MessageButtons::OkCancelCustom("取消".into(), GO_ON.into()))
        .show();
    answer == rfd::MessageDialogResult::Custom(GO_ON.into())
}

/// Opens a folder, a web address or a System Settings page with whatever macOS uses for it.
pub fn open(target: &str) -> bool {
    Command::new("open").arg(target).status().is_ok_and(|status| status.success())
}

/// Dialogs on macOS find their own place, so no window is named.
pub fn owner(_window: Option<tauri::WebviewWindow>) -> isize {
    0
}

/// The window's level set by Tauri is not cleared by other programs on macOS, so there is nothing to restore.
pub fn keep_on_top(_window: &tauri::WebviewWindow) -> bool {
    false
}

// ---- One running copy

/// The socket the running copy listens on, named after the single-instance name.
static SOCKET: OnceLock<PathBuf> = OnceLock::new();
static LISTENER: Mutex<Option<UnixListener>> = Mutex::new(None);

/// Claims the single-instance name. False means another copy is already running.
pub fn first_instance(name: &str) -> bool {
    let socket = SOCKET.get_or_init(|| data().join(format!("{name}.sock")));
    if UnixStream::connect(socket).is_ok() {
        return false;
    }
    // Nobody answered, so any file there was left by a copy that did not close properly.
    let _ = std::fs::create_dir_all(data());
    let _ = std::fs::remove_file(socket);
    if let (Ok(listener), Ok(mut kept)) = (UnixListener::bind(socket), LISTENER.lock()) {
        *kept = Some(listener);
    }
    true
}

/// Sends a named signal to the copy that is already running. False when it could not be reached.
pub fn signal(name: &str) -> bool {
    let Some(socket) = SOCKET.get() else { return false };
    UnixStream::connect(socket).and_then(|mut running| running.write_all(name.as_bytes())).is_ok()
}

/// Listens for the named signals for the life of the app, and calls `raised` with a signal's place in `names`.
pub fn listen(names: Vec<String>, raised: impl Fn(usize) + Send + 'static) {
    let Some(listener) = LISTENER.lock().ok().and_then(|mut kept| kept.take()) else { return };
    std::thread::spawn(move || {
        for arrived in listener.incoming() {
            let Ok(mut arrived) = arrived else { continue };
            let _ = arrived.set_read_timeout(Some(Duration::from_secs(2)));
            let mut asked = String::new();
            // A second launch that only checks whether a copy is running sends nothing.
            if arrived.read_to_string(&mut asked).is_err() {
                continue;
            }
            if let Some(index) = names.iter().position(|name| name == asked.trim()) {
                raised(index);
            }
        }
    });
}

// ---- The service key

/// Every key is kept under this service, with its `name`, such as `RecognitionServiceKey`, as the account.
const KEYCHAIN_SERVICE: &str = "Tingzi";

pub fn secret_read(name: &str) -> Option<String> {
    let stored = security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, name).ok()?;
    String::from_utf8(stored).ok().filter(|key| !key.is_empty())
}

/// Stores `key` under `name` in this user's login keychain, replacing any earlier one.
pub fn secret_store(name: &str, key: &str) -> bool {
    security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, name, key.as_bytes()).is_ok()
}

pub fn secret_clear(name: &str) {
    let _ = security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, name);
}

// ---- Choices kept by the system

/// The Zhuyin text service is Windows only, so nothing is stored for it and every choice reads as unset.
pub fn zhuyin_choices() -> [Option<u32>; 4] {
    [None; 4]
}

pub fn store_zhuyin_choices(_layout: u32, _staged: u32, _candidate_size: u32, _line_size: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn share_with_text_service(_simplified: bool, _appearance: u32) -> std::io::Result<()> {
    Ok(())
}

/// The launch agent that starts the app when the user logs in.
fn agent_file() -> PathBuf {
    home().join("Library").join("LaunchAgents").join("app.tingzi.voice.plist")
}

fn agent(program: &Path) -> String {
    let program = program.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>app.tingzi.voice</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    )
}

pub fn starts_at_login() -> bool {
    agent_file().is_file()
}

pub fn set_login_start(enabled: bool) -> std::io::Result<()> {
    let file = agent_file();
    if !enabled {
        return std::fs::remove_file(file).or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) });
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, agent(&std::env::current_exe()?))
}

/// The accent colour chosen in System Settings > Appearance, as red, green and blue.
pub fn accent() -> Option<(u32, u32, u32)> {
    let answer = Command::new("defaults").args(["read", "-g", "AppleAccentColor"]).output().ok()?;
    // macOS stores a number for each colour, and nothing at all for blue, the one it starts with.
    Some(match String::from_utf8_lossy(&answer.stdout).trim().parse::<i32>().unwrap_or(4) {
        -1 => (140, 140, 140),
        0 => (255, 82, 87),
        1 => (247, 130, 27),
        2 => (255, 198, 0),
        3 => (98, 186, 70),
        5 => (165, 80, 167),
        6 => (247, 79, 158),
        _ => (0, 122, 255),
    })
}

/// Reads `scutil --proxy`: whether an HTTPS proxy is on, its address, and the hosts that go direct,
/// joined with semicolons as Windows writes them.
fn read_proxy(listing: &str) -> Option<(bool, String, String)> {
    let mut enabled = false;
    let (mut server, mut port) = (None, None);
    let mut direct: Vec<&str> = Vec::new();
    for line in listing.lines() {
        let Some((name, value)) = line.split_once(" : ") else { continue };
        let (name, value) = (name.trim(), value.trim());
        match name {
            "HTTPSEnable" => enabled = value == "1",
            "HTTPSProxy" => server = Some(value),
            "HTTPSPort" => port = Some(value),
            // The hosts that go direct are numbered lines in a block of their own.
            _ if !name.is_empty() && name.bytes().all(|digit| digit.is_ascii_digit()) => direct.push(value),
            _ => {}
        }
    }
    let server = server?;
    Some((enabled, port.map_or_else(|| server.to_string(), |port| format!("{server}:{port}")), direct.join(";")))
}

/// The proxy set in System Settings > Network for HTTPS traffic to `host`.
pub fn system_proxy(host: &str) -> Option<String> {
    let answer = Command::new("scutil").arg("--proxy").output().ok()?;
    let (enabled, server, direct) = read_proxy(&String::from_utf8_lossy(&answer.stdout))?;
    crate::recognise::https_proxy(enabled, &server, &direct, host)
}

// ---- Folders and programs

/// The user's home folder.
pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// The user's settings, words and window position.
pub fn data() -> PathBuf {
    home().join("Library").join("Application Support").join("Tingzi")
}

/// The folder that holds the word list: the application's Resources, or the user's data folder
/// for a build that is run outside an application bundle.
pub fn install() -> Option<PathBuf> {
    let bundled = std::env::current_exe().ok().and_then(|program| Some(program.parent()?.parent()?.join("Resources")));
    [bundled, Some(data())].into_iter().flatten().find(|folder| folder.join("phrases.txt").is_file())
}

/// The Whisper program: one placed in the local recogniser's folder, or the one Homebrew installs.
/// The same program works with and without the graphics card.
pub fn whisper_program(root: &Path, _gpu: bool) -> Option<PathBuf> {
    [root.join("whisper-cli"), PathBuf::from("/opt/homebrew/bin/whisper-cli"), PathBuf::from("/usr/local/bin/whisper-cli")]
        .into_iter()
        .find(|program| program.is_file())
}

/// Nothing to look for: no separate build is downloaded on macOS.
pub fn gpu_driver() -> bool {
    false
}

/// The archive tool that ships with macOS.
pub fn tar() -> PathBuf {
    PathBuf::from("/usr/bin/tar")
}

/// Programs started from here never open a window of their own on macOS.
pub fn quiet(command: &mut Command) -> &mut Command {
    command
}

/// Makes the app a menu bar program: no Dock icon, and the keycap in view on every desktop.
pub fn prepare(app: &mut tauri::App) {
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    if let Some(window) = app.get_webview_window("capsule") {
        let _ = window.set_visible_on_all_workspaces(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_goes_by_its_outermost_bundle() {
        let name = |path: &str| program_name(Path::new(path));
        assert_eq!(name("/Applications/Safari.app/Contents/MacOS/Safari").as_deref(), Some("Safari.app"));
        let helper = "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper";
        assert_eq!(name(helper).as_deref(), Some("Visual Studio Code.app"));
        assert_eq!(name("/opt/homebrew/bin/whisper-cli").as_deref(), Some("whisper-cli"));
        assert!(program(0).is_none());
    }

    #[test]
    fn the_network_proxy_is_read() {
        let listing = "<dictionary> {\n  ExceptionsList : <array> {\n    0 : *.local\n    1 : 169.254/16\n  }\n  HTTPEnable : 0\n  HTTPSEnable : 1\n  HTTPSPort : 8080\n  HTTPSProxy : 127.0.0.1\n}\n";
        assert_eq!(read_proxy(listing), Some((true, "127.0.0.1:8080".to_string(), "*.local;169.254/16".to_string())));
        assert_eq!(read_proxy("<dictionary> {\n  HTTPEnable : 0\n}\n"), None);
    }

    #[test]
    fn the_login_entry_names_the_program() {
        let entry = agent(Path::new("/Applications/Tingzi & Co.app/Contents/MacOS/Tingzi"));
        assert!(entry.contains("<string>/Applications/Tingzi &amp; Co.app/Contents/MacOS/Tingzi</string>"));
    }
}
