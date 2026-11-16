//! Speech recognition through Codex, built into the app rather than run as a separate program.

use codex_asr::{CodexAsrClient, CodexAsrError, CodexAuth};
use std::path::PathBuf;
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

/// Sends WAV bytes and returns the recognised text. Blocks; call it off the UI thread.
pub fn transcribe(wav: Vec<u8>) -> Result<String, String> {
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
