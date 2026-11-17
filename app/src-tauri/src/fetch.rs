//! Getting the local recogniser's parts from the settings page: Whisper models, and on Windows the
//! Whisper program itself, with or without graphics card support.
//!
//! Every file is named here with its size and SHA-256, so what lands on the computer is exactly what
//! was tried when this list was written. Nothing is downloaded until the user has seen where it
//! comes from and agreed, and a part that fails its check or its trial run is not kept.

use crate::recognise::{self, Parts};
use crate::settings::Settings;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// Where a downloaded file ends up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// A model, under this file name in the `downloads` folder.
    Model(&'static str),
    /// A zip of the Whisper program, unpacked into this folder.
    Program(&'static str),
}

struct Item {
    id: &'static str,
    title: &'static str,
    note: &'static str,
    /// Who publishes the file, as the user reads it before agreeing.
    source: &'static str,
    licence: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
    place: Place,
}

const WHISPER_MODELS: &str = "Hugging Face・ggerganov/whisper.cpp";

const MODELS: &[Item] = &[
    Item {
        id: "small",
        title: "Whisper small",
        note: "一般筆電只用 CPU 也跑得動，不確定就先選這個。",
        source: WHISPER_MODELS,
        licence: "MIT（OpenAI Whisper）",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin",
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        place: Place::Model("ggml-small.bin"),
    },
    Item {
        id: "breeze",
        title: "Breeze ASR 25（台灣華語）",
        note: "聯發科以台灣口音和中英夾雜的語料調整過的模型，直接寫出繁體。檔案大，建議搭配顯示卡加速。",
        source: "Hugging Face・alan314159/Breeze-ASR-25-whispercpp（由 MediaTek-Research/Breeze-ASR-25 轉檔）",
        licence: "Apache-2.0",
        url: "https://huggingface.co/alan314159/Breeze-ASR-25-whispercpp/resolve/c7f120183c8e8ad932f315e04e3d0359d839702d/ggml-model-q8_0.bin",
        size: 1_656_129_708,
        sha256: "669eb226a0e23b42465a6d2f60ce1902fbd534e19faab59511730420eb25e90d",
        place: Place::Model("ggml-breeze-asr-25-q8_0.bin"),
    },
    Item {
        id: "turbo",
        title: "Whisper large-v3-turbo",
        note: "比 small 準。建議搭配顯示卡加速；只用 CPU 時每句要等比較久。",
        source: WHISPER_MODELS,
        licence: "MIT（OpenAI Whisper）",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
        size: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        place: Place::Model("ggml-large-v3-turbo-q5_0.bin"),
    },
    Item {
        id: "base",
        title: "Whisper base",
        note: "最小最快，適合舊電腦；同音字和專有名詞比較常錯。",
        source: WHISPER_MODELS,
        licence: "MIT（OpenAI Whisper）",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        place: Place::Model("ggml-base.bin"),
    },
];

const CPU_FOLDER: &str = "engine-cpu";
const GPU_FOLDER: &str = "engine-gpu";
const GPU_ID: &str = "gpu";

/// The Whisper program for x64 Windows, from whisper.cpp's own releases. Other systems bring their own.
const PROGRAMS: &[Item] = &[
    Item {
        id: "cpu",
        title: "辨識程式 whisper.cpp",
        note: "",
        source: "GitHub・ggml-org/whisper.cpp（b5454）",
        licence: "MIT",
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip",
        size: 8_928_640,
        sha256: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb",
        place: Place::Program(CPU_FOLDER),
    },
    Item {
        id: GPU_ID,
        title: "顯示卡加速（NVIDIA）",
        note: "",
        source: "GitHub・ggml-org/whisper.cpp（b5454，CUDA 12.4）",
        licence: "MIT，內含 NVIDIA CUDA 執行元件（依 NVIDIA 授權條款）",
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-win-cuda-12.4.0-x64.zip",
        size: 684_913_404,
        sha256: "afef0b881c500958921c3f5523b50e59ee2ec9b6f5cbd25b324c51ed308a957a",
        place: Place::Program(GPU_FOLDER),
    },
];

fn item(id: &str) -> Option<&'static Item> {
    let programs = if cfg!(windows) { PROGRAMS } else { &[] };
    MODELS.iter().chain(programs).find(|item| item.id == id)
}

/// A size as people read it: megabytes up to a thousand, then gigabytes with one decimal.
fn human(bytes: u64) -> String {
    let megabytes = bytes as f64 / 1_000_000.0;
    if megabytes < 1000.0 {
        format!("{megabytes:.0} MB")
    } else {
        format!("{:.1} GB", megabytes / 1000.0)
    }
}

/// The download in progress: which item, and the flag that stops it.
#[derive(Default)]
pub struct Fetching(Mutex<Option<(String, Arc<AtomicBool>)>>);

#[derive(Serialize)]
pub struct ModelRow {
    id: &'static str,
    title: &'static str,
    note: &'static str,
    file: &'static str,
    size: String,
    installed: bool,
    /// Whether the file is in the app's own folder, so the page may offer to delete it.
    removable: bool,
}

/// What the settings page shows about the local recogniser.
#[derive(Serialize)]
pub struct LocalState {
    /// The models that can be downloaded, the suggested one first.
    models: Vec<ModelRow>,
    /// Every model file on this computer, including ones the user placed by hand.
    files: Vec<String>,
    /// Whether a program and a model are both in place.
    ready: bool,
    /// Whether local recognition can use the graphics card right now.
    gpu_ready: bool,
    /// The size of the graphics card build, when this computer could use it and does not have it yet.
    gpu_offer: Option<String>,
    /// The item being downloaded now.
    busy: Option<String>,
}

#[tauri::command(async)]
pub fn local_state(fetching: State<'_, Fetching>) -> LocalState {
    let files = recognise::local_models();
    let prefs = Settings::load();
    let gpu_ready = recognise::local_program(true).is_some();
    let offered = !gpu_ready && crate::sys::gpu_driver();
    LocalState {
        models: MODELS
            .iter()
            .filter_map(|item| match item.place {
                Place::Model(file) => {
                    let removable = own_model(file).is_some();
                    Some(ModelRow { id: item.id, title: item.title, note: item.note, file, size: human(item.size), installed: files.iter().any(|name| name == file), removable })
                }
                Place::Program(_) => None,
            })
            .collect(),
        ready: recognise::local_parts(prefs.local_model.as_deref(), prefs.gpu).is_some(),
        gpu_ready,
        gpu_offer: item(GPU_ID).filter(|_| offered).map(|item| human(item.size)),
        busy: fetching.0.lock().unwrap().as_ref().map(|(id, _)| id.clone()),
        files,
    }
}

/// One step of a download, for the page's progress line.
#[derive(Clone, Serialize)]
struct Progress {
    id: &'static str,
    /// "download", "verify", "unpack", "trial", then one of "done", "cancelled" or "failed".
    stage: &'static str,
    title: &'static str,
    done: u64,
    total: u64,
    /// Why it failed, in words for the user.
    note: String,
}

const CANCELLED: &str = "已取消";

/// Asks first, then downloads `id` in the background and reports on the `local-fetch` event.
/// Ok(false) means the user said no. A model brings the Whisper program with it when there is none yet.
#[tauri::command(async)]
pub fn local_fetch(app: AppHandle, fetching: State<'_, Fetching>, id: String) -> Result<bool, String> {
    let wanted = item(&id).ok_or("找不到這個項目。")?;
    if fetching.0.lock().unwrap().is_some() {
        return Err("另一個下載還在進行，請等它完成。".into());
    }
    let mut plan = Vec::new();
    match wanted.place {
        Place::Model(file) if recognise::local_models().iter().any(|name| name == file) => return Err("這個模型已經下載。".into()),
        Place::Model(_) if recognise::local_program(false).is_none() => {
            plan.push(item("cpu").ok_or("找不到 whisper-cli，請先用 Homebrew 安裝 whisper-cpp。")?);
        }
        Place::Program(GPU_FOLDER) if recognise::local_parts(Settings::load().local_model.as_deref(), false).is_none() => {
            return Err("請先下載一個模型，再下載顯示卡加速。".into());
        }
        _ => {}
    }
    plan.push(wanted);
    let mut question = format!("要下載「{}」嗎？\n\n來源：{}\n大小：{}\n授權：{}", wanted.title, wanted.source, human(wanted.size), wanted.licence);
    if let [program, _] = plan[..] {
        question += &format!("\n\n第一次使用還會下載{}（{}，{}，{}）。", program.title, human(program.size), program.source, program.licence);
    }
    question += "\n\n下載後會核對檔案並試跑一次，通過才會使用。";
    if !crate::sys::confirm(crate::sys::owner(app.get_webview_window("settings")), &question) {
        return Ok(false);
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut running = fetching.0.lock().unwrap();
        if running.is_some() {
            return Err("另一個下載還在進行，請等它完成。".into());
        }
        *running = Some((id, stop.clone()));
    }
    std::thread::spawn(move || {
        let tell = |progress: Progress| {
            let _ = app.emit_to("settings", "local-fetch", progress);
        };
        let outcome = plan.iter().try_for_each(|item| get(item, &recognise::own_root(), &|| stop.load(Ordering::Relaxed), &tell));
        app.state::<Fetching>().0.lock().unwrap().take();
        let (stage, note) = match outcome {
            Ok(()) => ("done", String::new()),
            Err(reason) if reason == CANCELLED => ("cancelled", String::new()),
            Err(reason) => ("failed", reason),
        };
        tell(Progress { id: wanted.id, stage, title: wanted.title, done: 0, total: 0, note });
    });
    Ok(true)
}

/// A model file the app downloaded into its own folder. Models the user placed elsewhere are theirs to remove.
fn own_model(file: &str) -> Option<std::path::PathBuf> {
    let path = recognise::own_root().join("downloads").join(file);
    (recognise::is_model_name(file) && path.is_file()).then_some(path)
}

/// The model to use instead once `removed` is gone: the saved choice when it stays, else the first one left.
fn next_model(saved: Option<&str>, removed: &str, left: &[String]) -> Option<String> {
    saved.filter(|name| *name != removed && left.iter().any(|kept| kept.as_str() == *name)).or_else(|| left.first().map(String::as_str)).map(str::to_string)
}

/// Asks, then deletes a model the app downloaded. When it was the one in use, another model on this computer
/// takes its place; with none left, local recognition needs a new download. Ok(false) means the user said no.
#[tauri::command(async)]
pub fn local_delete(app: AppHandle, fetching: State<'_, Fetching>, file: String) -> Result<bool, String> {
    let path = own_model(&file).ok_or("這個模型不在聽字的資料夾裡，請到它所在的資料夾自己刪除。")?;
    if fetching.0.lock().unwrap().is_some() {
        return Err("下載進行中，請等它完成再刪除。".into());
    }
    let prefs = Settings::load();
    let in_use = recognise::local_parts(prefs.local_model.as_deref(), false).is_some_and(|parts| parts.model == path);
    let left: Vec<String> = recognise::local_models().into_iter().filter(|name| *name != file).collect();
    let title = MODELS.iter().find(|item| matches!(item.place, Place::Model(name) if name == file)).map_or(file.as_str(), |item| item.title);
    let size = std::fs::metadata(&path).map(|meta| human(meta.len())).unwrap_or_default();
    let mut question = format!("要刪除「{title}」嗎？會釋放 {size} 的空間。");
    if in_use {
        question += &match left.first() {
            Some(other) => format!("\n\n這是目前使用的模型，刪除後改用 {other}。"),
            None => "\n\n這是目前使用的模型，也是這台電腦上唯一的模型。刪除後要再下載一個，本機辨識才能使用。".to_string(),
        };
    }
    if !crate::sys::confirm(crate::sys::owner(app.get_webview_window("settings")), &question) {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|_| "無法刪除這個模型，可能正在辨識中，請稍後再試。".to_string())?;
    if in_use || prefs.local_model.as_deref() == Some(file.as_str()) {
        let _ = Settings { local_model: next_model(prefs.local_model.as_deref(), &file, &left), ..prefs }.store();
    }
    Ok(true)
}

/// Stops the download in progress. What has arrived is kept, so asking again carries on from there.
#[tauri::command]
pub fn local_fetch_cancel(fetching: State<'_, Fetching>) {
    if let Some((_, stop)) = fetching.0.lock().unwrap().as_ref() {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Downloads one item into `root`, checks it, tries it, and only then puts it where the recogniser looks.
fn get(item: &'static Item, root: &Path, cancelled: &dyn Fn() -> bool, tell: &dyn Fn(Progress)) -> Result<(), String> {
    let step = |stage, done| tell(Progress { id: item.id, stage, title: item.title, done, total: item.size, note: String::new() });
    let models = root.join("downloads");
    std::fs::create_dir_all(&models).map_err(|_| "無法建立存放模型的資料夾。")?;
    let part = match item.place {
        Place::Model(file) => models.join(format!("{file}.part")),
        Place::Program(folder) => root.join(format!("{folder}.zip.part")),
    };
    let client = client(item.url)?;
    download(&client, item.url, item.size, item.sha256, &part, CHUNK, cancelled, &|stage, done| step(stage, done))?;
    match item.place {
        Place::Model(file) => {
            let model = models.join(file);
            std::fs::rename(&part, &model).map_err(|_| "無法存放下載的模型。")?;
            step("trial", item.size);
            let program = recognise::local_program(false).ok_or("找不到辨識程式，請再下載一次。")?;
            let tried = recognise::trial(&Parts { program, model: model.clone(), gpu: false }, cancelled);
            if let Err(reason) = tried {
                let _ = std::fs::remove_file(&model);
                return Err(if reason == CANCELLED { reason } else { "這個模型在這台電腦上試跑失敗，沒有保留。".into() });
            }
            Ok(())
        }
        Place::Program(folder) => {
            step("unpack", item.size);
            let fresh = root.join(format!("{folder}.new"));
            let outcome = unpack(&part, &fresh).and_then(|program| {
                if folder != GPU_FOLDER {
                    return Ok(());
                }
                step("trial", item.size);
                let model = recognise::local_parts(Settings::load().local_model.as_deref(), false).ok_or("請先下載一個模型，再下載顯示卡加速。")?.model;
                match recognise::trial(&Parts { program, model, gpu: true }, cancelled) {
                    Ok(true) => Ok(()),
                    Err(reason) if reason == CANCELLED => Err(reason),
                    _ => Err("這台電腦的顯示卡試跑沒有成功，沒有保留；本機辨識仍使用 CPU。請更新 NVIDIA 驅動程式後再試。".into()),
                }
            });
            let placed = outcome.and_then(|()| {
                let home = root.join(folder);
                let _ = std::fs::remove_dir_all(&home);
                std::fs::rename(&fresh, &home).map_err(|_| "無法存放下載的程式。".to_string())
            });
            let _ = std::fs::remove_dir_all(&fresh);
            // A cancelled trial keeps the checked archive, so the next try does not download it again.
            if !matches!(&placed, Err(reason) if reason == CANCELLED) {
                let _ = std::fs::remove_file(&part);
            }
            placed
        }
    }
}

/// Unpacks a zip of the Whisper program into an empty `folder` and returns the program inside it.
fn unpack(zip: &Path, folder: &Path) -> Result<PathBuf, String> {
    let failed = "下載的程式無法解開。";
    let _ = std::fs::remove_dir_all(folder);
    std::fs::create_dir_all(folder).map_err(|_| failed)?;
    let mut command = Command::new(crate::sys::tar());
    command.arg("-xf").arg(zip).arg("-C").arg(folder).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let done = crate::sys::quiet(&mut command).status().is_ok_and(|status| status.success());
    Some(folder.join("Release").join("whisper-cli.exe")).filter(|program| done && program.is_file()).ok_or_else(|| failed.to_string())
}

/// How much is asked for in one request. Each request has its own time limit, so a stalled
/// connection is noticed and the download carries on from where it stopped.
pub(crate) const CHUNK: u64 = 4 * 1024 * 1024;

pub(crate) fn client(url: &str) -> Result<reqwest::blocking::Client, String> {
    let host = url.split('/').nth(2).unwrap_or_default();
    let mut builder = reqwest::blocking::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(180));
    if let Some(proxy) = recognise::proxy_for(host) {
        builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| "無法下載：請檢查代理設定。")?);
    }
    builder.build().map_err(|_| "無法下載：請檢查代理設定。".into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Downloads `url` into `part`, carrying on from what an earlier try left there, and checks the
/// whole file against `sha256`. A file that fails the check is removed; an interrupted one is kept.
/// `tell` gets "download" with the bytes so far, then "verify".
#[allow(clippy::too_many_arguments)]
pub(crate) fn download(
    client: &reqwest::blocking::Client,
    url: &str,
    size: u64,
    sha256: &str,
    part: &Path,
    chunk: u64,
    cancelled: &dyn Fn() -> bool,
    tell: &dyn Fn(&'static str, u64),
) -> Result<(), String> {
    let cannot_write = "無法寫入下載的檔案，請確認磁碟空間。";
    let mut hash = Sha256::new();
    let mut have = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    // What an earlier try left is hashed again rather than trusted.
    if let Ok(mut earlier) = std::fs::File::open(part) {
        if earlier.metadata().map_or(0, |meta| meta.len()) <= size {
            loop {
                match earlier.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        hash.update(&buffer[..count]);
                        have += count as u64;
                    }
                    Err(_) => return Err(cannot_write.into()),
                }
                if cancelled() {
                    return Err(CANCELLED.into());
                }
            }
        }
    }
    let mut file = if have > 0 {
        std::fs::OpenOptions::new().append(true).open(part)
    } else {
        std::fs::File::create(part)
    }
    .map_err(|_| cannot_write)?;
    let mut failures = 0;
    let mut told = Instant::now();
    tell("download", have);
    while have < size {
        if cancelled() {
            return Err(CANCELLED.into());
        }
        let last = (have + chunk).min(size) - 1;
        let before = have;
        let arrived = (|| -> Result<(), bool> {
            let mut answer = client.get(url).header("Range", format!("bytes={have}-{last}")).send().map_err(|_| false)?;
            // 200 means the server sent the whole file instead of the part asked for.
            if answer.status().as_u16() != 206 {
                return Err(false);
            }
            while have <= last {
                let room = ((last + 1 - have) as usize).min(buffer.len());
                let count = answer.read(&mut buffer[..room]).map_err(|_| false)?;
                if count == 0 {
                    break;
                }
                file.write_all(&buffer[..count]).map_err(|_| true)?;
                hash.update(&buffer[..count]);
                have += count as u64;
                if told.elapsed() >= Duration::from_millis(200) {
                    told = Instant::now();
                    tell("download", have);
                }
                if cancelled() {
                    break;
                }
            }
            Ok(())
        })();
        match arrived {
            Err(true) => return Err(cannot_write.into()),
            _ if have > before => failures = 0,
            _ => {
                failures += 1;
                if failures >= 3 {
                    return Err("下載中斷，請檢查網路。已下載的部分會保留，再按一次「下載」會接續。".into());
                }
                std::thread::sleep(Duration::from_millis(800));
            }
        }
    }
    file.flush().map_err(|_| cannot_write)?;
    drop(file);
    tell("verify", size);
    if hex(&hash.finalize()) != sha256 {
        let _ = std::fs::remove_file(part);
        return Err("下載的檔案和預期的不一樣，已刪除，請再試一次。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn every_item_is_pinned_and_lands_in_its_own_place() {
        let mut ids = Vec::new();
        for item in MODELS.iter().chain(PROGRAMS) {
            assert!(item.url.starts_with("https://"), "{}", item.id);
            assert!(item.sha256.len() == 64 && item.sha256.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{}", item.id);
            assert!(item.size > 1_000_000, "{}", item.id);
            match item.place {
                Place::Model(file) => assert!(recognise::is_model_name(file), "{file}"),
                Place::Program(folder) => assert!(folder == CPU_FOLDER || folder == GPU_FOLDER),
            }
            assert!(!ids.contains(&item.id));
            ids.push(item.id);
        }
        // A model address names a revision, so a later change upstream cannot break the download.
        assert!(MODELS.iter().all(|item| item.url.contains("/resolve/") && !item.url.contains("/resolve/main/")));
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human(8_928_640), "9 MB");
        assert_eq!(human(487_601_967), "488 MB");
        assert_eq!(human(1_656_129_708), "1.7 GB");
    }

    /// Serves `body` over HTTP on this computer, honouring byte ranges, and counts the requests.
    /// With `whole`, it ignores ranges and always sends everything.
    fn serve(body: Vec<u8>, whole: bool) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/file", listener.local_addr().unwrap());
        let asked = Arc::new(AtomicUsize::new(0));
        let count = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut lines = BufReader::new(stream.try_clone().unwrap());
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if lines.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (first, last) = value.trim().split_once('-').unwrap();
                        range = Some((first.parse::<usize>().unwrap(), last.parse::<usize>().unwrap()));
                    }
                }
                count.fetch_add(1, Ordering::Relaxed);
                let mut stream = stream;
                let (status, slice) = match range.filter(|_| !whole) {
                    Some((first, last)) => ("206 Partial Content", &body[first..=last.min(body.len() - 1)]),
                    None => ("200 OK", &body[..]),
                };
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len());
                let _ = stream.write_all(slice);
            }
        });
        (address, asked)
    }

    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join("TingziVoice").join(format!("fetch-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder.join("file.part")
    }

    fn plain() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(10)).build().unwrap()
    }

    fn body() -> Vec<u8> {
        (0..10_000u32).map(|n| (n * 7 % 251) as u8).collect()
    }

    #[test]
    fn a_file_arrives_in_parts_and_is_checked() {
        let body = body();
        let sum = hex(&Sha256::digest(&body));
        let (address, asked) = serve(body.clone(), false);
        let part = scratch("parts");
        let stages = Mutex::new(Vec::new());
        download(&plain(), &address, body.len() as u64, &sum, &part, 3000, &|| false, &|stage, _| stages.lock().unwrap().push(stage)).unwrap();
        assert_eq!(std::fs::read(&part).unwrap(), body);
        assert_eq!(asked.load(Ordering::Relaxed), 4);
        assert_eq!(stages.lock().unwrap().last(), Some(&"verify"));
    }

    #[test]
    fn an_interrupted_download_carries_on_from_what_it_has() {
        let body = body();
        let sum = hex(&Sha256::digest(&body));
        let (address, asked) = serve(body.clone(), false);
        let part = scratch("resume");
        std::fs::write(&part, &body[..7000]).unwrap();
        download(&plain(), &address, body.len() as u64, &sum, &part, 3000, &|| false, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(&part).unwrap(), body);
        assert_eq!(asked.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_file_that_fails_its_check_is_removed() {
        let body = body();
        let (address, _) = serve(body.clone(), false);
        let part = scratch("wrong");
        let wrong = "0".repeat(64);
        let outcome = download(&plain(), &address, body.len() as u64, &wrong, &part, 3000, &|| false, &|_, _| {});
        assert!(outcome.is_err_and(|reason| reason.contains("不一樣")));
        assert!(!part.exists());
    }

    #[test]
    fn stopping_keeps_what_has_arrived() {
        let body = body();
        let sum = hex(&Sha256::digest(&body));
        let (address, _) = serve(body.clone(), false);
        let part = scratch("stop");
        let seen = AtomicUsize::new(0);
        let stop = || seen.fetch_add(1, Ordering::Relaxed) >= 2;
        assert_eq!(download(&plain(), &address, body.len() as u64, &sum, &part, 3000, &stop, &|_, _| {}), Err(CANCELLED.to_string()));
        let kept = std::fs::read(&part).unwrap();
        assert!(!kept.is_empty() && kept.len() < body.len() && body.starts_with(&kept));
    }

    #[test]
    fn a_server_that_ignores_ranges_is_refused() {
        let body = body();
        let sum = hex(&Sha256::digest(&body));
        let (address, asked) = serve(body.clone(), true);
        let part = scratch("whole");
        let outcome = download(&plain(), &address, body.len() as u64, &sum, &part, 3000, &|| false, &|_, _| {});
        assert!(outcome.is_err_and(|reason| reason.contains("下載中斷")));
        assert_eq!(asked.load(Ordering::Relaxed), 3);
    }

    /// Downloads the real Windows program from GitHub and unpacks it. Needs the network, so it only
    /// runs when asked for: `cargo test -- --ignored the_real_program`.
    #[test]
    #[ignore]
    fn the_real_program_downloads_and_unpacks() {
        let cpu = item("cpu").unwrap();
        let part = scratch("real");
        download(&client(cpu.url).unwrap(), cpu.url, cpu.size, cpu.sha256, &part, CHUNK, &|| false, &|_, _| {}).unwrap();
        let folder = part.with_file_name("unpacked");
        let program = unpack(&part, &folder).unwrap();
        assert!(program.is_file());
    }

    #[test]
    fn deleting_a_model_moves_to_one_that_is_left() {
        let left = vec!["ggml-base.bin".to_string(), "ggml-small.bin".to_string()];
        assert_eq!(next_model(Some("ggml-small.bin"), "ggml-turbo.bin", &left).as_deref(), Some("ggml-small.bin"));
        assert_eq!(next_model(Some("ggml-small.bin"), "ggml-small.bin", &left).as_deref(), Some("ggml-base.bin"));
        assert_eq!(next_model(None, "ggml-small.bin", &left).as_deref(), Some("ggml-base.bin"));
        assert_eq!(next_model(Some("ggml-small.bin"), "ggml-small.bin", &[]), None);
    }
}
