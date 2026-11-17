//! The settings window: what it shows, and what saving changes.

use crate::apps::{self, AppRule};
use crate::dialogs::{self, Kind};
use crate::replace::{self, Replacement};
use crate::settings::Settings;
use crate::words::{self, Entry};
use crate::{audio, paths, recognise};
use std::ffi::OsStr;
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const ENGINE_KEY: &str = r"Software\Tingzi\Engine";
const LOOK_KEY: &str = r"Software\Tingzi\Appearance";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NO_WINDOW: u32 = 0x0800_0000;

/// Zhuyin choices the text service reads from the registry when an application loads it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Zhuyin {
    /// 0 is the standard layout, 1 is ETen.
    layout: u32,
    /// True settles long words first; false compares whole sentences.
    staged: bool,
    candidate_size: u32,
    /// 0 turns the large composition line off.
    line_size: u32,
}

impl Zhuyin {
    fn load() -> Self {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let number = |key: &str, name: &str| user.open_subkey(key).and_then(|key| key.get_value::<u32, _>(name)).ok();
        Self {
            layout: number(ENGINE_KEY, "KeyboardLayout").filter(|layout| *layout == 1).unwrap_or(0),
            staged: number(ENGINE_KEY, "StagedSegmentation") == Some(1),
            candidate_size: number(LOOK_KEY, "CandidateDIP").filter(|size| [20, 24, 32].contains(size)).unwrap_or(24),
            line_size: number(LOOK_KEY, "CompositionDIP").filter(|size| [0, 36, 44, 52].contains(size)).unwrap_or(44),
        }
    }

    fn valid(&self) -> bool {
        self.layout <= 1 && [20, 24, 32].contains(&self.candidate_size) && [0, 36, 44, 52].contains(&self.line_size)
    }

    fn save(&self) -> std::io::Result<()> {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (engine, _) = user.create_subkey(ENGINE_KEY)?;
        engine.set_value("KeyboardLayout", &self.layout)?;
        engine.set_value("StagedSegmentation", &u32::from(self.staged))?;
        let (look, _) = user.create_subkey(LOOK_KEY)?;
        look.set_value("CandidateDIP", &self.candidate_size)?;
        look.set_value("CompositionDIP", &self.line_size)
    }
}

fn starts_at_login() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).and_then(|run| run.get_value::<String, _>("Tingzi")).is_ok()
}

fn set_login_start(enabled: bool) -> std::io::Result<()> {
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    if enabled {
        let program = std::env::current_exe()?;
        run.set_value("Tingzi", &format!("\"{}\"", program.display()))
    } else {
        run.delete_value("Tingzi").or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) })
    }
}

/// An upgrade installs the program in a new folder; keeps the login entry pointing at the installed copy that is running.
pub fn follow_upgrade() {
    let installed = std::env::current_exe().is_ok_and(|program| program.file_name().is_some_and(|name| name == "TingziFloat.exe"));
    if installed && starts_at_login() {
        let _ = set_login_start(true);
    }
}

/// The colours and sizes every window follows.
#[derive(Clone, Serialize)]
pub struct Look {
    shortcut: String,
    text_size: u32,
    /// 0 follows Windows, 1 is light, 2 is dark.
    appearance: u32,
    /// The Windows accent colour and a readable colour on top of it, when the user chose to follow it.
    accent: Option<(String, String)>,
}

pub fn look(prefs: &Settings, shortcut: String) -> Look {
    let accent = prefs.system_accent.then(windows_accent).flatten();
    Look { shortcut, text_size: prefs.text_size, appearance: prefs.appearance, accent }
}

fn windows_accent() -> Option<(String, String)> {
    let stored: u32 = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\DWM").ok()?.get_value("AccentColor").ok()?;
    // Windows stores the colour as alpha, blue, green, red.
    Some(accent_pair(stored & 0xff, (stored >> 8) & 0xff, (stored >> 16) & 0xff))
}

/// The accent as a CSS colour, with dark text on light accents and white on dark ones.
fn accent_pair(red: u32, green: u32, blue: u32) -> (String, String) {
    let brightness = (0.299 * f64::from(red) + 0.587 * f64::from(green) + 0.114 * f64::from(blue)) / 255.0;
    (format!("#{red:02x}{green:02x}{blue:02x}"), if brightness > 0.55 { "#181b3c" } else { "#ffffff" }.to_string())
}

/// Everything the settings window shows when it opens.
#[derive(Serialize)]
pub struct Panel {
    prefs: Settings,
    zhuyin: Zhuyin,
    startup: bool,
    microphones: Vec<String>,
    /// False when another program holds the recording shortcut.
    shortcut_bound: bool,
    local_ready: bool,
    version: &'static str,
    has_keyboard: bool,
    has_pad: bool,
    has_learning: bool,
    has_word_list: bool,
    look: Look,
}

#[tauri::command(async)]
pub fn load_settings(app: AppHandle) -> Panel {
    let prefs = Settings::load();
    let keys = app.state::<crate::Keys>();
    Panel {
        zhuyin: Zhuyin::load(),
        startup: starts_at_login(),
        microphones: audio::microphones(),
        shortcut_bound: keys.bound(),
        local_ready: recognise::local_parts().is_some(),
        version: env!("CARGO_PKG_VERSION"),
        has_keyboard: paths::tool("TingziKeyboard.exe").is_some(),
        has_pad: paths::tool("TingziPad.exe").is_some(),
        has_learning: paths::tool("TingziLearning.exe").is_some(),
        has_word_list: paths::word_list().is_some(),
        look: look(&prefs, keys.label()),
        prefs,
    }
}

/// Applies and stores the choices. On failure nothing is stored and the reason is returned for the page to show.
#[tauri::command]
pub fn save_settings(app: AppHandle, prefs: Settings, zhuyin: Zhuyin, startup: bool) -> Result<(), String> {
    if let Some(problem) = prefs.problem() {
        return Err(problem.into());
    }
    if !zhuyin.valid() {
        return Err("設定內容不完整，請重新選擇。".into());
    }
    let before = Settings::load();
    // The earlier voice window finds a microphone by its device id; keep it only while the name still matches.
    let prefs = Settings { microphone_id: before.microphone_id.clone().filter(|_| before.microphone_name == prefs.microphone_name), ..prefs };
    let keys = app.state::<crate::Keys>();
    if !keys.rebind(&app, &prefs) {
        return Err("此快捷鍵已被占用，請選其他組合。".into());
    }
    if prefs.store().is_err() {
        keys.rebind(&app, &before);
        return Err("無法儲存設定。".into());
    }
    // These follow the saved file; a failure here leaves the main choices in place.
    let _ = zhuyin.save();
    if startup != starts_at_login() {
        let _ = set_login_start(startup);
    }
    let _ = app.emit("look", look(&prefs, keys.label()));
    Ok(())
}

pub fn open(app: &AppHandle) {
    open_at(app, None);
}

/// Opens the settings window, on the page with this name when one is given (voice, zhuyin, look or about).
pub fn open_at(app: &AppHandle, page: Option<&'static str>) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(page) = page {
            let _ = app.emit_to("settings", "open-page", page);
        }
        return;
    }
    // Creating a window from an event handler can stall on Windows, so it happens on its own thread.
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App(page.map_or_else(|| "settings.html".to_string(), |page| format!("settings.html#{page}")).into()))
            .title("聽字・注音與語音輸入法 · 設定")
            .inner_size(860.0, 660.0)
            .min_inner_size(660.0, 460.0)
            .center()
            .build();
    });
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    open(&app);
}

#[tauri::command]
pub fn close_settings(app: AppHandle, test: State<'_, MicTest>) {
    test.0.lock().unwrap().take();
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.close();
    }
}

/// The microphone test in progress, with a number that tells one test from the next.
#[derive(Default)]
pub struct MicTest(Mutex<Option<(u64, audio::Recorder)>>);

/// Starts or stops listening to a microphone so the page can show its level. Stops by itself after ten seconds.
#[tauri::command]
pub fn test_microphone(app: AppHandle, test: State<'_, MicTest>, name: Option<String>, on: bool) -> Result<(), String> {
    let mut running = test.0.lock().unwrap();
    let previous = running.take().map_or(0, |(number, _)| number);
    if !on {
        return Ok(());
    }
    let meter = app.clone();
    let recorder = audio::Recorder::start(name, move |peak| {
        let _ = meter.emit_to("settings", "mic-level", peak);
    })
    .map_err(|_| "無法開啟這支麥克風，請確認 Windows 的麥克風權限。".to_string())?;
    let number = previous + 1;
    *running = Some((number, recorder));
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        let test = app.state::<MicTest>();
        let mut running = test.0.lock().unwrap();
        if running.as_ref().is_some_and(|(current, _)| *current == number) {
            running.take();
            let _ = app.emit_to("settings", "mic-test-ended", ());
        }
    });
    Ok(())
}

/// Runs the learning tool and returns whether it succeeded, with what it printed.
fn learning_tool(arguments: &[&std::ffi::OsStr]) -> Option<(bool, String)> {
    let tool = paths::tool("TingziLearning.exe")?;
    let output = Command::new(tool).args(arguments).stdin(Stdio::null()).stderr(Stdio::null()).creation_flags(NO_WINDOW).output().ok()?;
    Some((output.status.success(), String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

/// Whether local learning is on; None when the tool is missing or its answer is not understood.
#[tauri::command(async)]
pub fn learning_state() -> Option<bool> {
    match learning_tool(&["status".as_ref()])? {
        (true, answer) if answer == "Local learning: on" => Some(true),
        (true, answer) if answer == "Local learning: off" => Some(false),
        _ => None,
    }
}

#[tauri::command(async)]
pub fn set_learning(on: bool) -> bool {
    learning_tool(&[if on { "on" } else { "off" }.as_ref()]).is_some_and(|(done, _)| done)
}

/// A word the Zhuyin engine learned from the user's typing. `reading` has its syllables joined with hyphens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Learned {
    reading: String,
    text: String,
    /// How many times it was typed; a word seen once is only kept provisionally.
    hits: u32,
}

/// Reads the learning tool's word list, `reading TAB text TAB hits` per line. Lines it cannot read are skipped.
fn parse_learned(output: &str) -> Vec<Learned> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [reading, text, hits] = fields[..] else { return None };
            let hits = hits.parse::<u32>().ok().filter(|hits| *hits >= 1)?;
            (!reading.is_empty() && !text.is_empty()).then(|| Learned { reading: reading.to_string(), text: text.to_string(), hits })
        })
        .collect()
}

/// The words local learning has picked up. None when the tool is missing or fails.
#[tauri::command(async)]
pub fn learned_words() -> Option<Vec<Learned>> {
    match learning_tool(&["words".as_ref()])? {
        (true, output) => Some(parse_learned(&output)),
        _ => None,
    }
}

/// Removes one learned word. False when the tool is missing or did not remove it.
#[tauri::command(async)]
pub fn forget_learned_word(reading: String, text: String) -> bool {
    learning_tool(&["forget-word".as_ref(), OsStr::new(&reading), OsStr::new(&text)]).is_some_and(|(done, _)| done)
}

fn owner(app: &AppHandle) -> isize {
    app.get_webview_window("settings").and_then(|window| window.hwnd().ok()).map_or(0, |handle| handle.0 as isize)
}

/// Imports, exports or clears the learning data. None means the user closed the dialog.
#[tauri::command(async)]
pub fn learning_data(app: AppHandle, action: String) -> Option<bool> {
    let owner = owner(&app);
    let done = match action.as_str() {
        "import" => {
            let file = dialogs::pick(owner, Kind::Table, None)?;
            learning_tool(&["import".as_ref(), file.as_os_str()])
        }
        "export" => {
            let file = dialogs::pick(owner, Kind::Table, Some("tingzi-learning.tsv"))?;
            // The tool writes only to a new file, and the dialog has already asked about replacing this one.
            let _ = std::fs::remove_file(&file);
            learning_tool(&["export".as_ref(), file.as_os_str()])
        }
        "clear" => {
            if !dialogs::confirm(owner, "會刪除所有記住的改字習慣，無法復原。要繼續嗎？") {
                return None;
            }
            learning_tool(&["clear".as_ref(), "--confirm".as_ref()])
        }
        _ => None,
    };
    Some(done.is_some_and(|(done, _)| done))
}

#[tauri::command(async)]
pub fn words_load(templates: bool) -> Vec<Entry> {
    words::load(&words::own_file(templates), templates)
}

#[tauri::command(async)]
pub fn words_save(templates: bool, entries: Vec<Entry>) -> bool {
    words::save(&words::own_file(templates), &entries, templates).is_ok()
}

/// The readings of each character in `word`, most common first. None when the word list is missing;
/// an empty list for a character means the word list does not know it.
#[tauri::command(async)]
pub fn word_readings(word: String) -> Option<Vec<Vec<String>>> {
    let lexicon = words::lexicon()?;
    Some(word.chars().map(|character| lexicon.readings(character)).collect())
}

/// Reads words or templates from a file the user picks. None means the user closed the dialog.
#[tauri::command(async)]
pub fn words_import(app: AppHandle, templates: bool) -> Option<Vec<Entry>> {
    dialogs::pick(owner(&app), Kind::Text, None).map(|file| words::load(&file, templates))
}

/// Writes words or templates to a file the user names. None means the user closed the dialog.
#[tauri::command(async)]
pub fn words_export(app: AppHandle, templates: bool, entries: Vec<Entry>) -> Option<bool> {
    let name = if templates { "tingzi-templates.txt" } else { "tingzi-words.txt" };
    dialogs::pick(owner(&app), Kind::Text, Some(name)).map(|file| words::save(&file, &entries, templates).is_ok())
}

#[tauri::command(async)]
pub fn replacements_load() -> Vec<Replacement> {
    replace::load(&replace::own_file())
}

#[tauri::command(async)]
pub fn replacements_save(rules: Vec<Replacement>) -> bool {
    replace::save(&replace::own_file(), &rules).is_ok()
}

/// Reads replacement rules from a file the user picks. None means the user closed the dialog.
#[tauri::command(async)]
pub fn replacements_import(app: AppHandle) -> Option<Vec<Replacement>> {
    dialogs::pick(owner(&app), Kind::Text, None).map(|file| replace::load(&file))
}

/// Writes replacement rules to a file the user names. None means the user closed the dialog.
#[tauri::command(async)]
pub fn replacements_export(app: AppHandle, rules: Vec<Replacement>) -> Option<bool> {
    dialogs::pick(owner(&app), Kind::Text, Some("tingzi-replacements.txt")).map(|file| replace::save(&file, &rules).is_ok())
}

#[tauri::command(async)]
pub fn app_rules_load() -> Vec<AppRule> {
    apps::load(&apps::own_file())
}

#[tauri::command(async)]
pub fn app_rules_save(rules: Vec<AppRule>) -> bool {
    apps::save(&apps::own_file(), &rules).is_ok()
}

/// The programs with a window on screen, offered as names when the user adds a rule.
#[tauri::command(async)]
pub fn running_programs() -> Vec<String> {
    apps::running()
}

/// Opens one of the fixed places the settings page links to. Returns false when it could not be opened.
#[tauri::command(async)]
pub fn open_target(which: String) -> bool {
    let tool = |name: &str, argument: Option<&str>| {
        paths::tool(name).is_some_and(|tool| Command::new(tool).args(argument).creation_flags(NO_WINDOW).spawn().is_ok())
    };
    match which.as_str() {
        "keyboard" => tool("TingziKeyboard.exe", Some("switch")),
        "pad" => tool("TingziPad.exe", None),
        "language" => dialogs::open("ms-settings:regionlanguage"),
        "home" => dialogs::open("https://github.com/skydreamer0/tingzi"),
        "issues" => dialogs::open("https://github.com/skydreamer0/tingzi/issues"),
        "folder" => std::fs::create_dir_all(paths::data()).is_ok() && dialogs::open(&paths::data().to_string_lossy()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_text_stays_readable() {
        assert_eq!(accent_pair(0, 120, 215), ("#0078d7".to_string(), "#ffffff".to_string()));
        assert_eq!(accent_pair(255, 220, 0), ("#ffdc00".to_string(), "#181b3c".to_string()));
    }

    #[test]
    fn learned_words_are_read_and_bad_lines_skipped() {
        let output = "ㄊㄧㄥ-ㄗˋ\t聽字\t3\r\nㄋㄧˇ\t你\t1\n只有一欄\nㄅㄚ\t八\t0\nㄅㄚ\t八\tmany\n\t空\t2\nㄎㄨㄥ\t\t2\nㄧ\t一\t2\t多一欄\n";
        assert_eq!(
            parse_learned(output),
            vec![
                Learned { reading: "ㄊㄧㄥ-ㄗˋ".into(), text: "聽字".into(), hits: 3 },
                Learned { reading: "ㄋㄧˇ".into(), text: "你".into(), hits: 1 },
            ]
        );
        assert!(parse_learned("").is_empty());
    }

    #[test]
    fn zhuyin_choices_are_checked() {
        let fine = Zhuyin { layout: 1, staged: true, candidate_size: 32, line_size: 0 };
        assert!(fine.valid());
        assert!(!Zhuyin { candidate_size: 25, ..fine }.valid());
        assert!(!Zhuyin { layout: 2, ..fine }.valid());
    }
}
