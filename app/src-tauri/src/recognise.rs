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

/// The host recordings are sent to, for matching the proxy bypass list.
const SERVICE_HOST: &str = "chatgpt.com";

/// One client for the whole session keeps the connection to the service open between dictations.
/// Codex rewrites its login file when the token refreshes, and the user may change the proxy,
/// so the client is rebuilt when either changes.
fn client() -> Result<CodexAsrClient, String> {
    type Kept = (Option<SystemTime>, Option<String>, CodexAsrClient);
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let file = auth_file();
    let changed = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
    let proxy = proxy();
    let mut kept = KEPT.lock().map_err(|_| "線上辨識無法使用".to_string())?;
    if let Some((seen, routed, ready)) = kept.as_ref() {
        if *seen == changed && *routed == proxy {
            return Ok(ready.clone());
        }
    }
    let auth = CodexAuth::from_auth_file(&file).map_err(explain)?;
    let fresh = CodexAsrClient::builder(auth)
        .proxy(proxy.clone())
        .timeout(Some(Duration::from_secs(90)))
        .connect_timeout(Some(Duration::from_secs(15)))
        .build()
        .map_err(explain)?;
    *kept = Some((changed, proxy, fresh.clone()));
    Ok(fresh)
}

/// The proxy to reach the service through: one named in the environment first, as `codex-asr` reads it,
/// then the one set in Windows' proxy settings, which `codex-asr` does not read on Windows.
fn proxy() -> Option<String> {
    codex_asr::resolve_proxy(None).or_else(system_proxy)
}

fn system_proxy() -> Option<String> {
    let user = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let settings = user.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings").ok()?;
    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    let server: String = settings.get_value("ProxyServer").ok()?;
    let bypass: String = settings.get_value("ProxyOverride").unwrap_or_default();
    https_proxy(enabled != 0, &server, &bypass, SERVICE_HOST)
}

/// Reads Windows' manual proxy setting for HTTPS traffic to `host`.
/// `server` is either one `host:port` for every protocol or a list like `http=a:80;https=b:443`.
/// Automatic configuration scripts and SOCKS proxies are not followed.
fn https_proxy(enabled: bool, server: &str, bypass: &str, host: &str) -> Option<String> {
    if !enabled || bypassed(bypass, host) {
        return None;
    }
    let server = server.trim();
    let chosen = if server.contains('=') {
        server.split(';').find_map(|entry| {
            let (scheme, address) = entry.split_once('=')?;
            scheme.trim().eq_ignore_ascii_case("https").then(|| address.trim())
        })?
    } else {
        server
    };
    if chosen.is_empty() {
        return None;
    }
    Some(if chosen.contains("://") { chosen.to_string() } else { format!("http://{chosen}") })
}

/// Whether Windows' bypass list sends `host` direct. Entries may start with `*`, like `*.example.com`.
fn bypassed(bypass: &str, host: &str) -> bool {
    bypass.split(';').map(|entry| entry.trim().to_ascii_lowercase()).any(|entry| match entry.strip_prefix('*') {
        _ if entry.is_empty() || entry == "<local>" => false,
        Some(suffix) => host.ends_with(&suffix) || suffix.strip_prefix('.') == Some(host),
        None => entry == host,
    })
}

/// Turns a service error into something the user can act on, without echoing server text.
fn explain(error: CodexAsrError) -> String {
    match error {
        CodexAsrError::ReadFile { .. } | CodexAsrError::MissingAccessToken { .. } => "請先在這台電腦登入 Codex".into(),
        CodexAsrError::ParseAuth { path, .. } if path.as_os_str() == "<transcribe response>" => {
            "線上辨識失敗：服務回傳的內容無法讀取".into()
        }
        CodexAsrError::ParseAuth { .. } => "Codex 的登入檔無法讀取，請重新登入 Codex".into(),
        CodexAsrError::UnsupportedAuthMode { .. } => "線上辨識需要以 ChatGPT 帳號登入 Codex，目前不是這種登入方式".into(),
        CodexAsrError::BuildClient(_) => "線上辨識無法使用：請檢查代理設定".into(),
        CodexAsrError::Http { status, .. } => match status {
            // Only Codex refreshes the login, and only while it runs.
            401 => "線上辨識失敗：登入已過期，請開啟 Codex 一次讓登入更新".into(),
            403 => "線上辨識失敗：帳號或服務拒絕請求".into(),
            413 => "錄音太大，請分段錄".into(),
            429 => "線上辨識失敗：服務要求稍後再試".into(),
            500.. => "線上辨識失敗：服務暫時無法辨識，請稍後再試".into(),
            _ => "線上辨識失敗：服務拒絕這次請求，可能需要更新聽字".into(),
        },
        CodexAsrError::Request(failure) if failure.is_timeout() => "線上辨識逾時，請檢查網路".into(),
        CodexAsrError::Request(failure) if failure.is_connect() => "線上辨識失敗：連不上服務，請檢查網路或代理設定".into(),
        CodexAsrError::MissingText => "線上未回傳文字".into(),
        _ => "線上辨識失敗：請檢查網路及服務狀態".into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_proxy_for_every_protocol_is_used() {
        assert_eq!(https_proxy(true, "127.0.0.1:7890", "", SERVICE_HOST).as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(https_proxy(true, " http://proxy:8080 ", "", SERVICE_HOST).as_deref(), Some("http://proxy:8080"));
    }

    #[test]
    fn the_https_entry_is_picked_from_a_list() {
        let server = "http=web:80;https=secure:443;ftp=files:21";
        assert_eq!(https_proxy(true, server, "", SERVICE_HOST).as_deref(), Some("http://secure:443"));
        assert_eq!(https_proxy(true, "http=web:80;socks=sock:1080", "", SERVICE_HOST), None);
    }

    #[test]
    fn a_disabled_or_empty_proxy_is_not_used() {
        assert_eq!(https_proxy(false, "127.0.0.1:7890", "", SERVICE_HOST), None);
        assert_eq!(https_proxy(true, "  ", "", SERVICE_HOST), None);
        assert_eq!(https_proxy(true, "https=", "", SERVICE_HOST), None);
    }

    #[test]
    fn the_bypass_list_sends_the_service_direct() {
        assert_eq!(https_proxy(true, "p:1", "localhost;*.chatgpt.com;<local>", SERVICE_HOST), None);
        assert_eq!(https_proxy(true, "p:1", "CHATGPT.COM", SERVICE_HOST), None);
        assert_eq!(https_proxy(true, "p:1", "*.local;<local>;10.*", SERVICE_HOST).as_deref(), Some("http://p:1"));
        assert_eq!(https_proxy(true, "p:1", "notchatgpt.com", SERVICE_HOST).as_deref(), Some("http://p:1"));
    }

    #[test]
    fn service_answers_get_their_own_advice() {
        let http = |status| explain(CodexAsrError::Http { status, body: "server text".into() });
        assert!(http(401).contains("開啟 Codex"));
        assert!(http(429).contains("稍後再試"));
        assert!(http(503).contains("暫時無法辨識"));
        assert!(http(426).contains("更新聽字"));
        assert!(!http(400).contains("server text"));
    }

    #[test]
    fn login_problems_are_told_apart() {
        assert!(explain(CodexAsrError::UnsupportedAuthMode { mode: "apikey".into() }).contains("ChatGPT 帳號"));
        assert!(explain(CodexAsrError::MissingAccessToken { path: auth_file() }).contains("請先在這台電腦登入"));
        let broken = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        assert!(explain(CodexAsrError::ParseAuth { path: auth_file(), source: broken }).contains("登入檔"));
        let broken = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let response = CodexAsrError::ParseAuth { path: PathBuf::from("<transcribe response>"), source: broken };
        assert!(explain(response).contains("回傳的內容"));
    }
}
