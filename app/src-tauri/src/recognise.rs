//! Speech recognition: through Codex, built into the app, or with the Whisper program on this computer.

use codex_asr::{CodexAsrClient, CodexAsrError, CodexAuth};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// The upload limit the service accepts for one request.
const LARGEST_UPLOAD: usize = 50 * 1024 * 1024;

fn auth_file() -> PathBuf {
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join(".codex")
    });
    home.join("auth.json")
}

/// Sends WAV bytes to Codex and returns the recognised text. Blocks; call it off the UI thread.
pub fn online(wav: Vec<u8>) -> Result<String, String> {
    if wav.len() <= 44 {
        return Err("沒有錄到聲音".into());
    }
    if wav.len() > LARGEST_UPLOAD {
        return Err("錄音超過 50 MiB，請分段錄".into());
    }
    let heard = client()?.transcribe_bytes(wav, "audio.wav", "audio/wav", Some("zh".into())).map_err(explain)?;
    Ok(heard.text.trim().to_string())
}

/// One client for the whole session keeps the connection to the service open between dictations.
/// Codex rewrites its login file when the token refreshes, so the client is rebuilt when that file changes.
fn client() -> Result<CodexAsrClient, String> {
    static KEPT: Mutex<Option<(Option<SystemTime>, CodexAsrClient)>> = Mutex::new(None);
    let file = auth_file();
    let changed = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
    let mut kept = KEPT.lock().map_err(|_| "線上辨識無法使用".to_string())?;
    if let Some((seen, ready)) = kept.as_ref() {
        if *seen == changed {
            return Ok(ready.clone());
        }
    }
    let auth = CodexAuth::from_auth_file(&file).map_err(|_| "請先在這台電腦登入 Codex".to_string())?;
    let fresh = CodexAsrClient::builder(auth)
        .timeout(Some(Duration::from_secs(90)))
        .connect_timeout(Some(Duration::from_secs(15)))
        .build()
        .map_err(explain)?;
    *kept = Some((changed, fresh.clone()));
    Ok(fresh)
}

/// Turns a service error into something the user can act on, without echoing server text.
fn explain(error: CodexAsrError) -> String {
    let detail = error.to_string();
    if detail.contains("401") || detail.contains("403") {
        "線上辨識失敗：帳號或服務拒絕請求，請重新登入 Codex".into()
    } else if detail.contains("429") {
        "線上辨識失敗：服務要求稍後再試".into()
    } else if detail.to_ascii_lowercase().contains("timed out") {
        "線上辨識逾時，請檢查網路".into()
    } else {
        "線上辨識失敗：請檢查網路及服務狀態".into()
    }
}

/// The Whisper program and model on this computer, when both are in place.
/// Their folder is named in `voice-root.txt`, written when the local recogniser was set up.
pub fn local_parts() -> Option<(PathBuf, PathBuf)> {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("voice-root.txt")));
    let installed = crate::paths::install().map(|folder| folder.join("TingziFloat").join("voice-root.txt"));
    let named = [beside, installed].into_iter().flatten().find_map(|file| std::fs::read_to_string(file).ok())?;
    let root = PathBuf::from(named.trim());
    let program = root.join("whisper-v1.9.2").join("Release").join("whisper-cli.exe");
    let model = root.join("downloads").join("ggml-base.bin");
    (root.is_absolute() && program.is_file() && model.is_file()).then_some((program, model))
}

/// Recognises 16 kHz mono WAV bytes on this computer; nothing is uploaded.
/// Stops the program early when `cancelled` turns true.
pub fn local(wav: &[u8], cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    let (program, model) = local_parts().ok_or("本機語音模型尚未就緒，請先完成本機語音模型安裝")?;
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|since| since.as_nanos()).unwrap_or(0);
    let folder = std::env::temp_dir().join("TingziVoice").join(format!("local-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|_| "無法建立暫存資料夾")?;
    let outcome = run_whisper(&program, &model, &folder, wav, cancelled);
    let _ = std::fs::remove_dir_all(&folder);
    outcome
}

fn run_whisper(program: &Path, model: &Path, folder: &Path, wav: &[u8], cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    let input = folder.join("input.wav");
    let result = folder.join("result");
    std::fs::write(&input, wav).map_err(|_| "無法寫入暫存音檔")?;
    // Half the logical processors, between four and eight, measured fastest for short clips.
    let threads = (std::thread::available_parallelism().map_or(4, |count| count.get()) / 2).clamp(4, 8);
    const NO_WINDOW: u32 = 0x0800_0000;
    let mut child = Command::new(program)
        .args(["-l", "zh", "-otxt", "-np", "-nt", "-ng", "-t", &threads.to_string(), "-m"])
        .arg(model)
        .arg("-f")
        .arg(&input)
        .arg("-of")
        .arg(&result)
        .current_dir(program.parent().unwrap_or(folder))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(NO_WINDOW)
        .spawn()
        .map_err(|_| "無法啟動本機語音辨識")?;
    let finished = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if cancelled() => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("已取消".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => break false,
        }
    };
    let text = std::fs::read_to_string(result.with_extension("txt")).ok().filter(|_| finished);
    text.map(|heard| heard.trim().to_string()).ok_or_else(|| "本機語音辨識失敗，請檢查音檔或模型".to_string())
}
