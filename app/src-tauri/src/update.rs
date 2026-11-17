//! Looks for a newer version on the project's GitHub Releases page, at most once a day, and
//! installs it only when the user asks. With the check turned off, nothing here touches the network
//! unless the user presses the button to check now.

use crate::{fetch, paths, settings::Settings};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const LATEST: &str = "https://api.github.com/repos/skydreamer0/tingzi/releases/latest";
/// Every address taken from the answer must start with one of these, so a changed or forged answer cannot send the user elsewhere.
const PAGES: &str = "https://github.com/skydreamer0/tingzi/releases/";
const DOWNLOADS: &str = "https://github.com/skydreamer0/tingzi/releases/download/";
const DAY: u64 = 24 * 60 * 60;
/// How long after start the automatic check waits, so it does not compete with the first dictation.
const SETTLE: Duration = Duration::from_secs(20);
/// The most characters of release notes kept; the page links to the rest.
const LONGEST_NOTES: usize = 4000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// The address of the `.sha256` file published beside the installer.
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    /// None when the release has no installer for this system; the page is opened instead.
    pub installer: Option<Asset>,
}

/// What the last check found, kept so the settings page can show it without asking again.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    /// Seconds since 1970 when GitHub was last asked.
    checked: u64,
    latest: Option<Release>,
}

#[derive(Serialize)]
pub struct UpdateState {
    current: &'static str,
    /// Seconds since 1970 of the last check; 0 when there has not been one.
    checked: u64,
    /// The newer release, if the last check found one.
    offered: Option<Release>,
    installing: bool,
}

static INSTALLING: AtomicBool = AtomicBool::new(false);

/// The numbers of a version such as `v0.8.1`; None for anything else, including pre-releases.
fn parts(version: &str) -> Option<Vec<u64>> {
    let numbers = version.trim().trim_start_matches(['v', 'V']);
    let parts = numbers.split('.').map(|part| part.parse::<u64>().ok()).collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

/// Whether `offered` is a later version than `current`; 0.8 and 0.8.0 are the same.
pub fn newer(current: &str, offered: &str) -> bool {
    let (Some(mut current), Some(mut offered)) = (parts(current), parts(offered)) else {
        return false;
    };
    let length = current.len().max(offered.len());
    current.resize(length, 0);
    offered.resize(length, 0);
    offered > current
}

/// Whether a day has passed since `checked`. A clock set back counts as due, so a wrong date cannot stop the checks for good.
fn due(checked: u64, now: u64) -> bool {
    now < checked || now - checked >= DAY
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// The release described by GitHub's answer, with the installer for this system when there is one and its checksum is published beside it.
fn read_release(answer: &serde_json::Value, installer: Option<(&str, &str)>) -> Option<Release> {
    let text = |value: &serde_json::Value, key: &str| value.get(key).and_then(serde_json::Value::as_str).map(str::to_owned);
    let version = text(answer, "tag_name")?;
    parts(&version)?;
    let page = text(answer, "html_url").filter(|page| page.starts_with(PAGES))?;
    let notes = text(answer, "body").unwrap_or_default().chars().take(LONGEST_NOTES).collect::<String>().trim().to_owned();
    let assets = answer.get("assets").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    let address = |asset: &serde_json::Value| text(asset, "browser_download_url").filter(|url| url.starts_with(DOWNLOADS));
    let installer = installer.and_then(|(starts, ends)| {
        let asset = assets.iter().find(|asset| text(*asset, "name").is_some_and(|name| name.starts_with(starts) && name.ends_with(ends)))?;
        let name = text(asset, "name")?;
        let checksum = assets.iter().find(|other| text(*other, "name").as_deref() == Some(format!("{name}.sha256").as_str())).and_then(address)?;
        let size = asset.get("size").and_then(serde_json::Value::as_u64).filter(|size| *size > 0)?;
        Some(Asset { url: address(asset)?, name, size, checksum })
    });
    Some(Release { version, notes, page, installer })
}

fn record_file() -> PathBuf {
    paths::data().join("update-check.json")
}

fn load() -> Record {
    std::fs::read_to_string(record_file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn store(record: &Record) {
    let _ = std::fs::create_dir_all(paths::data());
    if let Ok(text) = serde_json::to_string(record) {
        let _ = std::fs::write(record_file(), text);
    }
}

fn user_agent() -> String {
    format!("Tingzi/{}", env!("CARGO_PKG_VERSION"))
}

/// Asks GitHub for the latest release.
fn ask() -> Result<Release, String> {
    let failed = "無法檢查更新，請確認網路連線。";
    let answer = fetch::client(LATEST)?
        .get(LATEST)
        .header("User-Agent", user_agent())
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|_| failed)?;
    if !answer.status().is_success() {
        return Err(failed.into());
    }
    let answer = answer.json::<serde_json::Value>().map_err(|_| failed)?;
    read_release(&answer, crate::sys::UPDATE_INSTALLER).ok_or_else(|| "更新資訊的格式不對，請到專案首頁查看。".into())
}

/// Asks now, keeps the answer, and returns what the page shows.
fn check_now() -> Result<UpdateState, String> {
    let latest = ask()?;
    store(&Record { checked: now(), latest: Some(latest) });
    Ok(state())
}

fn state() -> UpdateState {
    let record = load();
    let current = env!("CARGO_PKG_VERSION");
    UpdateState {
        current,
        checked: record.checked,
        offered: record.latest.filter(|latest| newer(current, &latest.version)),
        installing: INSTALLING.load(Ordering::Relaxed),
    }
}

/// How often the running app looks at whether a day has passed since the last check.
const LOOK_AGAIN: Duration = Duration::from_secs(60 * 60);
/// How long to wait before asking again while the user is recording or a dictation is being recognised.
const WHILE_BUSY: Duration = Duration::from_secs(30);

/// Automatic updates: at start and then once an hour, a check is made if a day has passed since the
/// last one. A newer version's installer is downloaded and checked in the background; then the user
/// is asked, once per start of the app, whether to install it now. Nothing happens while the check is off.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE);
        let mut asked: Option<String> = None;
        loop {
            // The user may have turned the check off since the app started.
            if Settings::load().check_updates {
                look(&app, &mut asked);
            }
            std::thread::sleep(LOOK_AGAIN);
        }
    });
}

fn look(app: &AppHandle, asked: &mut Option<String>) {
    let earlier = load().latest.map(|release| release.version);
    let state = if due(load().checked, now()) { check_now().unwrap_or_else(|_| state()) } else { state() };
    let Some(offered) = state.offered.clone() else { return };
    if earlier.as_deref() != Some(&offered.version) {
        let _ = app.emit("update", &state);
    }
    if asked.as_deref() == Some(&offered.version) {
        return;
    }
    let Some(installer) = offered.installer.as_ref() else {
        // Without an installer for this system the user can only be told; once per version.
        if earlier.as_deref() != Some(&offered.version) {
            let _ = app.emit("notice", format!("聽字有新版 {}，可到設定的「關於」頁下載", short(&offered.version)));
        }
        *asked = Some(offered.version.clone());
        return;
    };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return;
    }
    let ready = fetch_installer(app, installer);
    INSTALLING.store(false, Ordering::SeqCst);
    let Ok(file) = ready else { return };
    // Never interrupt a dictation.
    while crate::busy(app) {
        std::thread::sleep(WHILE_BUSY);
    }
    *asked = Some(offered.version.clone());
    let question = format!(
        "聽字新版 {} 已經下載好了。\n\n要現在安裝嗎？安裝時語音浮窗會暫時關閉，裝好後自動開啟；設定、詞表、學習資料與模型都會保留。\n\n按「取消」的話，下次開啟聽字時會再問一次，也可以到設定的「關於」頁安裝。",
        short(&offered.version)
    );
    if crate::dialogs::confirm(0, &question) && !run_installer(&file, true) {
        let _ = app.emit("notice", "無法啟動安裝程式，請到設定的「關於」頁再試一次");
    }
}

fn short(version: &str) -> &str {
    version.trim_start_matches(['v', 'V'])
}

#[tauri::command]
pub fn update_state() -> UpdateState {
    state()
}

/// The page's button: asks even when the automatic check is off, because the user pressed it.
#[tauri::command(async)]
pub fn update_check() -> Result<UpdateState, String> {
    check_now()
}

/// The installer's expected SHA-256, the first word of the `.sha256` file.
fn read_checksum(text: &str) -> Option<String> {
    let word = text.split_whitespace().next()?.to_ascii_lowercase();
    (word.len() == 64 && word.chars().all(|letter| letter.is_ascii_hexdigit())).then_some(word)
}

fn hash_of(file: &Path) -> Option<String> {
    let mut source = std::fs::File::open(file).ok()?;
    let mut hash = Sha256::new();
    std::io::copy(&mut source, &mut hash).ok()?;
    Some(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The offered installer on this computer, checked against its published SHA-256: one downloaded
/// earlier is used again if it still matches, otherwise it is downloaded, carrying on from a part
/// left by an interrupted try.
fn fetch_installer(app: &AppHandle, installer: &Asset) -> Result<PathBuf, String> {
    let client = fetch::client(&installer.url)?;
    let checksum = client
        .get(&installer.checksum)
        .header("User-Agent", user_agent())
        .send()
        .ok()
        .filter(|answer| answer.status().is_success())
        .and_then(|answer| answer.text().ok())
        .and_then(|text| read_checksum(&text))
        .ok_or("無法取得新版的檢查碼，請稍後再試。")?;
    let folder = paths::data().join("updates");
    std::fs::create_dir_all(&folder).map_err(|_| "無法建立下載資料夾，請確認磁碟空間。")?;
    let file = folder.join(&installer.name);
    let part = folder.join(format!("{}.part", installer.name));
    // Earlier versions' installers are not needed again; a part of this one is kept so the download carries on.
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten().filter(|entry| entry.path() != file && entry.path() != part) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    if hash_of(&file).as_deref() == Some(checksum.as_str()) {
        return Ok(file);
    }
    let tell = |stage: &'static str, have: u64| {
        let _ = app.emit("update-progress", (stage, have, installer.size));
    };
    fetch::download(&client, &installer.url, installer.size, &checksum, &part, fetch::CHUNK, &|| false, &tell)?;
    std::fs::rename(&part, &file).map_err(|_| "無法寫入下載的檔案，請確認磁碟空間。")?;
    Ok(file)
}

/// Starts the installer. `quiet` shows only its progress, for an update the user agreed to in one
/// question; otherwise its usual pages. Windows asks for administrator permission either way.
fn run_installer(file: &Path, quiet: bool) -> bool {
    let mut command = Command::new(file);
    if quiet {
        command.args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
    }
    // Should the system refuse to start it directly, opening it the usual way still asks for permission and shows its pages.
    command.spawn().is_ok() || crate::dialogs::open(&file.to_string_lossy())
}

/// The page's button: downloads the offered installer (or uses the one already downloaded), checks
/// it and starts it; the installer closes this window and opens the new one. Without an installer
/// for this system, opens the release page. Returns what the page tells the user.
#[tauri::command(async)]
pub fn update_install(app: AppHandle) -> Result<String, String> {
    let offered = state().offered.ok_or("目前已是最新版。")?;
    let Some(installer) = offered.installer else {
        return if crate::dialogs::open(&offered.page) {
            Ok("已開啟下載頁，請依頁面說明安裝新版。".into())
        } else {
            Err("無法開啟下載頁。".into())
        };
    };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("新版正在下載，請稍候。".into());
    }
    let ready = fetch_installer(&app, &installer);
    INSTALLING.store(false, Ordering::SeqCst);
    if !run_installer(&ready?, false) {
        return Err("無法啟動安裝程式。".into());
    }
    Ok("安裝程式已開啟。依畫面完成安裝後，語音浮窗會自動換成新版。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.7.7", "v0.7.8"));
        assert!(newer("0.7.7", "v0.8"));
        assert!(newer("0.7.9", "v0.7.10"));
        assert!(newer("0.9.9", "v1.0.0"));
        assert!(!newer("0.7.7", "v0.7.7"));
        assert!(!newer("0.8.0", "v0.8"));
        assert!(!newer("0.8.0", "v0.7.10"));
        assert!(!newer("0.7.7", "v0.8.0-rc1"));
        assert!(!newer("0.7.7", "latest"));
        assert!(!newer("0.7.7", ""));
    }

    #[test]
    fn checks_at_most_once_a_day() {
        assert!(due(0, 1_800_000_000));
        assert!(!due(1_800_000_000, 1_800_000_000 + DAY - 1));
        assert!(due(1_800_000_000, 1_800_000_000 + DAY));
        assert!(due(1_800_000_000, 1_700_000_000));
    }

    fn answer(assets: serde_json::Value) -> serde_json::Value {
        json!({
            "tag_name": "v0.8.0",
            "html_url": "https://github.com/skydreamer0/tingzi/releases/tag/v0.8.0",
            "body": "## 這一版改了什麼\n- 檢查更新\n",
            "assets": assets,
        })
    }

    fn asset(name: &str, size: u64) -> serde_json::Value {
        json!({ "name": name, "size": size, "browser_download_url": format!("{DOWNLOADS}v0.8.0/{name}") })
    }

    const WINDOWS: Option<(&str, &str)> = Some(("Tingzi-Setup-", "-x64.exe"));

    #[test]
    fn finds_the_installer_and_its_checksum() {
        let release = read_release(
            &answer(json!([
                asset("Tingzi-0.8.0-windows-x64.zip", 9),
                asset("Tingzi-Setup-0.8.0-x64.exe", 1234),
                asset("Tingzi-Setup-0.8.0-x64.exe.sha256", 90),
            ])),
            WINDOWS,
        )
        .unwrap();
        assert_eq!(release.version, "v0.8.0");
        assert_eq!(release.notes, "## 這一版改了什麼\n- 檢查更新");
        let installer = release.installer.unwrap();
        assert_eq!(installer.name, "Tingzi-Setup-0.8.0-x64.exe");
        assert_eq!(installer.size, 1234);
        assert!(installer.checksum.ends_with("/Tingzi-Setup-0.8.0-x64.exe.sha256"));
    }

    #[test]
    fn an_installer_without_a_checksum_or_from_elsewhere_is_not_offered() {
        let unchecked = read_release(&answer(json!([asset("Tingzi-Setup-0.8.0-x64.exe", 1234)])), WINDOWS).unwrap();
        assert_eq!(unchecked.installer, None);
        let mut elsewhere = asset("Tingzi-Setup-0.8.0-x64.exe", 1234);
        elsewhere["browser_download_url"] = json!("https://example.com/Tingzi-Setup-0.8.0-x64.exe");
        let moved = read_release(&answer(json!([elsewhere, asset("Tingzi-Setup-0.8.0-x64.exe.sha256", 90)])), WINDOWS).unwrap();
        assert_eq!(moved.installer, None);
        // macOS has no installer yet, so the page is offered.
        let mac = read_release(&answer(json!([asset("Tingzi-Setup-0.8.0-x64.exe", 1234), asset("Tingzi-Setup-0.8.0-x64.exe.sha256", 90)])), None).unwrap();
        assert_eq!(mac.installer, None);
    }

    #[test]
    fn an_answer_pointing_elsewhere_is_ignored() {
        let mut moved = answer(json!([]));
        moved["html_url"] = json!("https://example.com/tingzi");
        assert_eq!(read_release(&moved, WINDOWS), None);
        let mut unnamed = answer(json!([]));
        unnamed["tag_name"] = json!("nightly");
        assert_eq!(read_release(&unnamed, WINDOWS), None);
    }

    #[test]
    fn reads_the_checksum_file() {
        let sha = "ab".repeat(32);
        assert_eq!(read_checksum(&format!("{sha}  Tingzi-Setup-0.8.0-x64.exe\n")), Some(sha.clone()));
        assert_eq!(read_checksum(&sha.to_uppercase()), Some(sha));
        assert_eq!(read_checksum("not a checksum"), None);
        assert_eq!(read_checksum(""), None);
    }
}
