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
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    fn CGEventGetFlags(event: *mut c_void) -> u64;
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementCopyAttributeValue(element: *mut c_void, attribute: CFStringRef, value: *mut *const c_void) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *mut c_void, timeout: f32) -> i32;
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
    /// The key in V's place on a US keyboard; programs read it together with Command.
    const V: u16 = 9;
    press_with_command(V)
}

/// Presses Command+C, so the program in front copies what is selected. False when the keys could not be sent.
pub fn press_copy() -> bool {
    /// The key in C's place on a US keyboard.
    const C: u16 = 8;
    press_with_command(C)
}

fn press_with_command(letter: u16) -> bool {
    if !may_press_keys() {
        return false;
    }
    const WHOLE_SYSTEM: u32 = 0;
    // SAFETY: each event is checked, used and released here; a null source means the default one.
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), letter, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), letter, false);
        let made = !down.is_null() && !up.is_null();
        if made {
            CGEventSetIntegerValueField(down, 42, INPUT_TAG);
            CGEventSetIntegerValueField(up, 42, INPUT_TAG);
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

pub fn open_codex_desktop() -> Option<bool> {
    // The ChatGPT desktop app retains Codex's bundle identity:
    // https://help.openai.com/en/articles/20001535-managing-chatgpt-desktop-browser-policies-with-mdm
    const BUNDLE_ID: &str = "com.openai.codex";
    let installed = autoreleasepool(|_| {
        NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(BUNDLE_ID))
            .is_some()
    });
    if !installed {
        return None;
    }
    Some(Command::new("/usr/bin/open")
        .args(["-b", BUNDLE_ID])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success()))
}

pub fn codex_cli() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|folder| folder.join("codex"))
        .find(|path| path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0))
        .and_then(|path| std::path::absolute(path).ok())
}

pub fn open_codex_cli(cli: &Path) -> bool {
    // Terminal opens a .command file in a new window without Apple Events permission.
    let stamp = std::time::UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos();
    let path = std::env::temp_dir().join(format!("tingzi-codex-login-{}-{stamp}.command", std::process::id()));
    let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o700).open(&path) else {
        return false;
    };
    let written = (|| -> std::io::Result<()> {
        file.write_all(b"#!/bin/sh\n/bin/rm -f -- \"$0\"\n")?;
        for name in ["PATH", "CODEX_HOME"] {
            if let Some(value) = std::env::var_os(name) {
                write!(file, "export {name}=")?;
                write_shell_word(&mut file, &value)?;
                writeln!(file)?;
            }
        }
        file.write_all(b"exec ")?;
        write_shell_word(&mut file, cli.as_os_str())?;
        file.write_all(b" login\n")
    })();
    drop(file);
    let opened = written.is_ok() && Command::new("/usr/bin/open")
        .args(["-a", "Terminal"])
        .arg(&path)
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !opened {
        let _ = std::fs::remove_file(path);
    }
    opened
}

fn write_shell_word(file: &mut std::fs::File, word: &std::ffi::OsStr) -> std::io::Result<()> {
    file.write_all(b"'")?;
    for byte in word.as_bytes() {
        file.write_all(if *byte == b'\'' { b"'\\''" } else { std::slice::from_ref(byte) })?;
    }
    file.write_all(b"'")
}

/// Whether the keyboard is in the Codex desktop app. Telling its message box from its other
/// fields needs the accessibility permission, so any field in Codex counts here.
pub fn in_codex_message_box(process: u32) -> bool {
    program(process).is_some_and(|name| name.eq_ignore_ascii_case("Codex.app"))
}

/// The macOS release, such as `macOS 15.1`.
pub fn system_version() -> String {
    let answer = Command::new("sw_vers").arg("-productVersion").output();
    let version = answer.map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string()).unwrap_or_default();
    if version.is_empty() { "macOS".into() } else { format!("macOS {version}") }
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

/// whisper-cli on macOS needs nothing placed beside it.
pub fn whisper_runtime(_folder: &Path) {}

/// whisper-cli on macOS needs nothing placed beside it.
pub fn copy_runtime(_carried: &Path, _folder: &Path) {}

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

/// macOS has no Zhuyin input method, so there are no fixes typed with it.
pub fn typed_fixes() -> Option<Vec<crate::learn::Fix>> {
    None
}

pub fn note_dictation(_text: &str) {}

pub fn forget_dictations() {}

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

pub fn preferred_language() -> Option<String> {
    sys_locale::get_locale()
}


const INPUT_TAG: i64 = 0x5449_4e47_5a49;
static INPUT_WATCHING: AtomicBool = AtomicBool::new(false);
static INPUT_ACTIVITY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, text: *const u16);
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enabled: bool);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(loop_: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
}

unsafe extern "C" fn watch_input(
    _proxy: *mut c_void,
    kind: u32,
    event: *mut c_void,
    _user: *mut c_void,
) -> *mut c_void {
    // A disabled tap means activity was missed; invalidate the current insertion conservatively.
    if kind >= 0xffff_fffe {
        INPUT_WATCHING.store(false, Ordering::SeqCst);
        INPUT_ACTIVITY.fetch_add(1, Ordering::SeqCst);
    } else if !event.is_null()
        && CGEventGetIntegerValueField(event, 42) != INPUT_TAG
        && CGEventGetIntegerValueField(event, 41) != i64::from(std::process::id())
        && !stream_shortcut_event(kind, event)
    {
        INPUT_ACTIVITY.fetch_add(1, Ordering::SeqCst);
    }
    event
}

/// Event counters retain quick taps and arrow presses; modifier-only flags changes are excluded.
/// If macOS denies event observation, every read changes so insertion stops conservatively.
pub fn input_activity() -> u64 {
    static READY: OnceLock<bool> = OnceLock::new();
    let ready = READY.get_or_init(|| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("input-activity".into())
            .spawn(move || unsafe {
                // Key up clears the shortcut latch. Modifier-only changes do not count as activity.
                let mask = (1u64 << 11) | (1u64 << 10) | (1u64 << 1) | (1u64 << 3) | (1u64 << 25);
                let tap = CGEventTapCreate(0, 0, 1, mask, watch_input, std::ptr::null_mut());
                if tap.is_null() {
                    let _ = send.send(false);
                    return;
                }
                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                if source.is_null() {
                    CFRelease(tap);
                    let _ = send.send(false);
                    return;
                }
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                INPUT_WATCHING.store(true, Ordering::SeqCst);
                let _ = send.send(true);
                CFRunLoopRun();
                INPUT_WATCHING.store(false, Ordering::SeqCst);
                CFRelease(source);
                CFRelease(tap);
            })
            .is_err()
        {
            return false;
        }
        receive.recv().unwrap_or(false)
    });
    if !ready || !INPUT_WATCHING.load(Ordering::SeqCst) {
        INPUT_ACTIVITY.fetch_add(1, Ordering::SeqCst);
    }
    INPUT_ACTIVITY.load(Ordering::SeqCst)
}

fn send_plain_key(code: u16, text: Option<&[u16]>) -> bool {
    if !may_press_keys() {
        return false;
    }
    // SAFETY: events and the UTF-16 slice remain alive through posting and are released once.
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), code, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), code, false);
        let made = !down.is_null() && !up.is_null();
        if made {
            for event in [down, up] {
                CGEventSetFlags(event, 0);
                CGEventSetIntegerValueField(event, 42, INPUT_TAG);
                if let Some(text) = text {
                    CGEventKeyboardSetUnicodeString(event, text.len(), text.as_ptr());
                }
                CGEventPost(0, event);
            }
        }
        for event in [down, up] {
            if !event.is_null() {
                CFRelease(event);
            }
        }
        made
    }
}

/// Sends Unicode keyboard input without changing the clipboard.
pub fn type_text(text: &str) -> Result<(), bool> {
    // Keep surrogate pairs together and avoid the event's bounded Unicode payload.
    for (index, letter) in text.chars().enumerate() {
        let mut units = [0u16; 2];
        if !send_plain_key(0, Some(letter.encode_utf16(&mut units))) { return Err(index != 0); }
    }
    Ok(())
}

/// Deletes `count` preceding characters using the macOS Delete (Backspace) key.
pub fn backspace(count: usize) -> bool {
    (0..count).all(|_| send_plain_key(51, None))
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFHash(value: *const c_void) -> usize;
}

pub fn stream_field_empty() -> Option<bool> { None }

/// Identifies the focused Accessibility element and its window inside the frontmost app.
/// Missing permission, unsupported attributes and unresponsive apps cannot establish a safe
/// insertion target, so they return None rather than treating the whole app as one field.
pub fn stream_focus() -> Option<u64> {
    let process = frontmost()?;
    // SAFETY: all copied Accessibility objects are released on every path. The hash uses
    // CF equality semantics, not the address of a newly created Accessibility proxy.
    unsafe {
        if AXIsProcessTrusted() == 0 {
            return None;
        }
        let app = AXUIElementCreateApplication(i32::try_from(process).ok()?);
        if app.is_null() {
            return None;
        }
        let result = (|| {
            // Bound each cross-process request; a busy target must not stall capture delivery.
            if AXUIElementSetMessagingTimeout(app, 0.2) != 0 {
                return None;
            }
            let focused = CFString::new("AXFocusedUIElement");
            let window = CFString::new("AXFocusedWindow");
            let mut field_value = std::ptr::null();
            let field_status =
                AXUIElementCopyAttributeValue(app, focused.as_concrete_TypeRef(), &mut field_value);
            if field_status != 0 || field_value.is_null() {
                if !field_value.is_null() {
                    CFRelease(field_value);
                }
                return None;
            }
            let mut window_value = std::ptr::null();
            let window_status =
                AXUIElementCopyAttributeValue(app, window.as_concrete_TypeRef(), &mut window_value);
            let fingerprint = if window_status == 0 && !window_value.is_null() {
                Some(
                    (CFHash(field_value) as u64).rotate_left(23)
                        ^ (CFHash(window_value) as u64).rotate_left(7)
                        ^ u64::from(process),
                )
            } else {
                None
            };
            CFRelease(field_value);
            if !window_value.is_null() {
                CFRelease(window_value);
            }
            fingerprint
        })();
        CFRelease(app);
        // Reject a switch that happened while Accessibility was servicing the requests.
        if frontmost() == Some(process) {
            result
        } else {
            None
        }
    }
}

static STREAM_SHORTCUT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Stores the recording shortcut as a physical macOS key code plus exact modifier flags.
pub fn set_stream_shortcut(shortcut: crate::hotkey::Hotkey) {
    let letters: [u32; 26] = [
        0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16,
        6,
    ];
    let digits: [u32; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    let functions: [u32; 20] = [
        122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
    ];
    let code = match shortcut.key {
        0x41..=0x5A => Some(letters[(shortcut.key - 0x41) as usize]),
        0x30..=0x39 => Some(digits[(shortcut.key - 0x30) as usize]),
        0x70..=0x83 => Some(functions[(shortcut.key - 0x70) as usize]),
        _ => None,
    };
    // Add one so physical key code zero (A) remains distinct from disabled.
    let packed = code.map_or(0, |code| {
        (u64::from(shortcut.modifiers) << 32) | u64::from(code + 1)
    });
    STREAM_SHORTCUT.store(packed, Ordering::SeqCst);
}

unsafe fn stream_shortcut_event(kind: u32, event: *mut c_void) -> bool {
    static HELD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if !matches!(kind, 10 | 11) { return false; }
    let key = CGEventGetIntegerValueField(event, 9) as u32 + 1;
    !crate::hotkey::manual_key(&HELD, key, kind == 10, kind == 10 && is_stream_shortcut(event))
}

unsafe fn is_stream_shortcut(event: *mut c_void) -> bool {
    use crate::hotkey;
    let shortcut = STREAM_SHORTCUT.load(Ordering::SeqCst);
    if shortcut as u32 == 0
        || CGEventGetIntegerValueField(event, 9) + 1 != i64::from(shortcut as u32)
    {
        return false;
    }
    let flags = CGEventGetFlags(event);
    let modifiers = if flags & OPTION != 0 { hotkey::ALT } else { 0 }
        | if flags & CONTROL != 0 {
            hotkey::CONTROL
        } else {
            0
        }
        | if flags & SHIFT != 0 { hotkey::SHIFT } else { 0 }
        | if flags & COMMAND != 0 {
            hotkey::SUPER
        } else {
            0
        };
    modifiers == (shortcut >> 32) as u32
}
