//! Speech recognition: with the Whisper program on this computer, through Codex built into the app,
//! or through a transcription service the user chose and holds a key for.

use crate::settings::{Engine, Settings};
use codex_asr::{CodexAsrClient, CodexAsrError, CodexAuth};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const NOT_READY: &str = "本機語音模型尚未就緒，請到設定的「語音」頁下載模型";

/// Why `engine` cannot be used with these choices right now; None when it can be tried.
pub fn problem(engine: Engine, prefs: &Settings) -> Option<String> {
    match engine {
        Engine::Local => local_parts(prefs.local_model.as_deref(), prefs.gpu).is_none().then(|| NOT_READY.into()),
        Engine::Codex => None,
        Engine::Api => crate::api::problem(&prefs.api_url, &prefs.api_model, crate::secret::exists()),
    }
}

/// What the window says while `engine` works; a service on this computer counts as local.
pub fn working_label(engine: Engine, prefs: &Settings) -> &'static str {
    match engine {
        Engine::Local => "本機辨識中・再點一下取消",
        Engine::Codex => "Codex 線上辨識中・再點一下取消",
        Engine::Api if destination(engine, prefs).is_none() => "本機服務辨識中・再點一下取消",
        Engine::Api => "線上辨識中・再點一下取消",
    }
}

/// Where a recording goes, in words for asking first; None when it stays on this computer.
pub fn destination(engine: Engine, prefs: &Settings) -> Option<String> {
    match engine {
        Engine::Local => None,
        Engine::Codex => Some("經 Codex 傳到 ChatGPT".into()),
        Engine::Api => match crate::api::Service::parse(&prefs.api_url) {
            Ok(service) if service.on_this_computer => None,
            Ok(service) => Some(format!("傳到 {}", service.host)),
            Err(_) => Some("傳到線上辨識服務".into()),
        },
    }
}

/// Gets ready for a recording that has just started, off the UI thread. Only the user's own service
/// is connected to ahead of time: the Codex client keeps its connection to itself and offers nothing
/// to open it with short of sending audio.
pub fn warm(engine: Engine, prefs: &Settings) {
    if engine == Engine::Api && crate::api::problem(&prefs.api_url, &prefs.api_model, crate::secret::exists()).is_none() {
        crate::api::warm(&prefs.api_url);
    }
}

// A cancelled Codex upload keeps its slot until the blocking client returns.
static CODEX_REQUEST: Mutex<()> = Mutex::new(());

/// Recognises 16 kHz mono WAV bytes with `engine`. Blocks; call it off the UI thread.
pub fn run(engine: Engine, prefs: &Settings, wav: Vec<u8>, cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    match engine {
        Engine::Local => local(prefs.local_model.as_deref(), prefs.gpu, &wav, cancelled),
        Engine::Codex => {
            if cancelled() {
                return Err("已取消".into());
            }
            let _request = CODEX_REQUEST.lock().map_err(|_| "線上辨識無法使用".to_string())?;
            // The previous upload may have occupied the slot while the user cancelled this one.
            if cancelled() {
                return Err("已取消".into());
            }
            online(wav)
        }
        Engine::Api => {
            let hint = hint();
            crate::api::transcribe(&prefs.api_url, &prefs.api_model, wav, &hint).map(|heard| drop_echo(heard, &hint))
        }
    }
}

/// Roughly what fits in Whisper's prompt, which keeps only the last 224 tokens.
const LONGEST_HINT: usize = 150;

/// The user's own words and the spellings their replacement rules write, as a prompt that makes
/// a recogniser more likely to write them. Being in Traditional characters, it also nudges
/// Whisper-based services towards Traditional output.
pub fn hint() -> String {
    let own = crate::words::load(&crate::words::own_file(false), false).into_iter().map(|entry| entry.word);
    let wanted = crate::replace::load(&crate::replace::own_file()).into_iter().map(|rule| rule.wanted);
    hint_from(own.chain(wanted))
}

fn hint_from(words: impl Iterator<Item = String>) -> String {
    let mut chosen: Vec<String> = Vec::new();
    let mut length = 0;
    for word in words {
        let word = word.split_whitespace().collect::<Vec<_>>().join(" ");
        let size = word.chars().count();
        // Single characters say little and would crowd out names.
        if size < 2 || chosen.contains(&word) {
            continue;
        }
        if length + size + 1 > LONGEST_HINT {
            break;
        }
        length += size + 1;
        chosen.push(word);
    }
    chosen.join("、")
}

/// Whisper sometimes says the prompt back when a recording is nearly silent.
fn drop_echo(heard: String, hint: &str) -> String {
    let bare = |text: &str| text.chars().filter(|c| !c.is_whitespace() && !matches!(c, '、' | '，' | '。' | ',' | '.')).collect::<String>();
    if hint.contains('、') && bare(&heard) == bare(hint) {
        return String::new();
    }
    heard
}

/// The upload limit the service accepts for one request.
const LARGEST_UPLOAD: usize = 50 * 1024 * 1024;

fn auth_file() -> PathBuf {
    let home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| crate::sys::home().join(".codex"));
    home.join("auth.json")
}

pub const CODEX_LANGUAGE: Option<&str> = Some("zh");

/// Sends WAV bytes to Codex and returns the recognised text. Blocks; call it off the UI thread.
pub fn online(wav: Vec<u8>) -> Result<String, String> {
    if wav.len() <= 44 {
        return Err("沒有錄到聲音".into());
    }
    if wav.len() > LARGEST_UPLOAD {
        return Err("錄音超過 50 MiB，請分段錄".into());
    }
    let heard = client()?.transcribe_bytes(wav, "audio.wav", "audio/wav", CODEX_LANGUAGE.map(str::to_owned)).map_err(explain)?;
    Ok(heard.text.trim().to_string())
}

/// The host Codex recordings are sent to, for matching the proxy bypass list.
const SERVICE_HOST: &str = "chatgpt.com";

/// One client for the whole session keeps the connection to the service open between dictations.
/// Codex rewrites its login file when the token refreshes, the user may change the proxy, and
/// Codex updates itself, so the client is rebuilt when any of them changes.
///
/// The client names itself with the Codex version installed here rather than the one the
/// recogniser was built with, which the service may one day turn away as too old.
fn client() -> Result<CodexAsrClient, String> {
    type Kept = (Option<SystemTime>, Option<String>, Option<String>, CodexAsrClient);
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let file = auth_file();
    let changed = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
    let proxy = proxy_for(SERVICE_HOST);
    let named = crate::codex::user_agent();
    let mut kept = KEPT.lock().map_err(|_| "線上辨識無法使用".to_string())?;
    if let Some((seen, routed, known, ready)) = kept.as_ref() {
        if *seen == changed && *routed == proxy && *known == named {
            return Ok(ready.clone());
        }
    }
    let auth = CodexAuth::from_auth_file(&file).map_err(explain)?;
    let mut builder = CodexAsrClient::builder(auth);
    if let Some(name) = &named {
        builder = builder.user_agent(name.clone());
    }
    let fresh = builder
        .proxy(proxy.clone())
        .timeout(Some(Duration::from_secs(90)))
        .connect_timeout(Some(Duration::from_secs(15)))
        .build()
        .map_err(explain)?;
    *kept = Some((changed, proxy, named, fresh.clone()));
    Ok(fresh)
}

/// The proxy to reach `host` through: one named in the environment first, as `codex-asr` reads it,
/// then the one set in the system's proxy settings, which `codex-asr` does not read.
pub fn proxy_for(host: &str) -> Option<String> {
    codex_asr::resolve_proxy(None).or_else(|| crate::sys::system_proxy(host))
}

/// Reads a manual proxy setting, as Windows writes it, for HTTPS traffic to `host`.
/// `server` is either one `host:port` for every protocol or a list like `http=a:80;https=b:443`.
/// Automatic configuration scripts and SOCKS proxies are not followed.
pub fn https_proxy(enabled: bool, server: &str, bypass: &str, host: &str) -> Option<String> {
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

/// The model the local recogniser was first set up with, used unless the user picks another.
const FIRST_MODEL: &str = "ggml-base.bin";

/// The app's own folder for the local recogniser. Programs the app downloads go here, and models
/// go in its `downloads` folder.
pub fn own_root() -> PathBuf {
    crate::paths::data().join("whisper")
}

/// The folders the local recogniser may be in: the app's own first, then the one named in
/// `voice-root.txt`, written when the local recogniser was set up by hand.
fn roots() -> Vec<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("voice-root.txt")));
    let installed = crate::paths::install().map(|folder| folder.join("TingziFloat").join("voice-root.txt"));
    let named = [beside, installed].into_iter().flatten().find_map(|file| std::fs::read_to_string(file).ok());
    let named = named.map(|named| PathBuf::from(named.trim())).filter(|root| root.is_absolute());
    std::iter::once(own_root()).chain(named).collect()
}

/// The Whisper model files on this computer, by file name.
pub fn local_models() -> Vec<String> {
    let mut names: Vec<String> = roots()
        .iter()
        .flat_map(|root| std::fs::read_dir(root.join("downloads")).into_iter().flatten().flatten())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_model_name(name))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `ggml-<name>.bin`, with nothing in the name that could reach outside the folder.
pub fn is_model_name(name: &str) -> bool {
    let Some(middle) = name.strip_prefix("ggml-").and_then(|rest| rest.strip_suffix(".bin")) else { return false };
    !middle.is_empty() && !middle.contains("..") && middle.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn model_file(name: &str) -> Option<PathBuf> {
    roots().into_iter().map(|root| root.join("downloads").join(name)).find(|model| model.is_file())
}

/// The folders in the app's own folder that hold the Whisper programs it downloads.
const OWN_PROGRAMS: [&str; 2] = ["engine-cpu", "engine-gpu"];

/// Gives a program the app downloaded into `root` the runtime it may lack, copied from `carried`.
/// A program set up by hand, in `whisper-v1.9.2` or a folder named by `voice-root.txt`, keeps its own files.
fn repair_runtime(program: &Path, root: &Path, carried: &Path) {
    let Some(home) = program.parent() else { return };
    if OWN_PROGRAMS.iter().any(|folder| home == root.join(folder).join("Release").as_path()) {
        crate::sys::copy_runtime(carried, home);
    }
}

/// The Whisper program on this computer: the one that uses the graphics card when `gpu`, else the plain one.
pub fn local_program(gpu: bool) -> Option<PathBuf> {
    roots().iter().find_map(|root| crate::sys::whisper_program(root, gpu))
}

/// What one local recognition runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parts {
    pub program: PathBuf,
    pub model: PathBuf,
    /// Whether the graphics card does the work.
    pub gpu: bool,
}

/// The Whisper program and model on this computer, when both are in place.
/// `chosen` names a model file; a missing or unset choice falls back to the first model set up,
/// then to any model there is. `gpu` is honoured only when a program that can use the card is in place.
pub fn local_parts(chosen: Option<&str>, gpu: bool) -> Option<Parts> {
    let model = chosen
        .filter(|name| is_model_name(name))
        .and_then(model_file)
        .or_else(|| model_file(FIRST_MODEL))
        .or_else(|| local_models().first().and_then(|name| model_file(name)))?;
    if let Some(program) = local_program(true).filter(|_| gpu) {
        return Some(Parts { program, model, gpu: true });
    }
    Some(Parts { program: local_program(false)?, model, gpu: false })
}

/// Recognises 16 kHz mono WAV bytes on this computer; nothing is uploaded.
/// Stops the program early when `cancelled` turns true. When the graphics card fails, for example
/// after a driver change, the same recording is recognised without it.
///
/// The user's words are not passed as a prompt here: whisper-cli reads its arguments in the
/// Windows code page, so Chinese in `--prompt` arrives garbled and has no effect.
pub fn local(chosen: Option<&str>, gpu: bool, wav: &[u8], cancelled: &dyn Fn() -> bool) -> Result<String, String> {
    let parts = local_parts(chosen, gpu).ok_or(NOT_READY)?;
    let outcome = match whisper(&parts, wav, cancelled) {
        Err(_) if parts.gpu && !cancelled() => match local_parts(chosen, false) {
            Some(plain) => whisper(&plain, wav, cancelled),
            None => Err("顯示卡加速無法使用，請在設定關閉顯示卡加速".into()),
        },
        outcome => outcome,
    };
    outcome.map(|(heard, _)| heard)
}

/// One second of silence, for trying a program and a model without a microphone.
fn silence() -> Vec<u8> {
    let bytes = 32_000u32;
    let mut wav = Vec::with_capacity(44 + bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel.
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    // Two bytes a sample, sixteen bits.
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.resize(44 + bytes as usize, 0);
    wav
}

/// Runs `parts` once on silence. Ok tells whether the graphics card really did the work:
/// the program falls back to the processor by itself when it finds no card it can use.
pub fn trial(parts: &Parts, cancelled: &dyn Fn() -> bool) -> Result<bool, String> {
    whisper(parts, &silence(), cancelled).map(|(_, log)| parts.gpu && used_gpu(&log))
}

/// Whether the program's log says a graphics card was set up.
fn used_gpu(log: &str) -> bool {
    log.lines().any(|line| line.contains("whisper_backend_init_gpu: using"))
}

/// Runs the program in a folder of its own and returns the text with what the program logged.
fn whisper(parts: &Parts, wav: &[u8], cancelled: &dyn Fn() -> bool) -> Result<(String, String), String> {
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|since| since.as_nanos()).unwrap_or(0);
    let folder = std::env::temp_dir().join("TingziVoice").join(format!("local-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|_| "無法建立暫存資料夾")?;
    let outcome = run_whisper(parts, &folder, wav, cancelled);
    let _ = std::fs::remove_dir_all(&folder);
    outcome
}

fn run_whisper(parts: &Parts, folder: &Path, wav: &[u8], cancelled: &dyn Fn() -> bool) -> Result<(String, String), String> {
    let input = folder.join("input.wav");
    let result = folder.join("result");
    let log = folder.join("log.txt");
    std::fs::write(&input, wav).map_err(|_| "無法寫入暫存音檔")?;
    let record = std::fs::File::create(&log).map_err(|_| "無法寫入暫存檔")?;
    // Half the logical processors, between four and eight, measured fastest for short clips.
    let threads = (std::thread::available_parallelism().map_or(4, |count| count.get()) / 2).clamp(4, 8);
    // A program downloaded by an earlier version may still lack the runtime it needs.
    if let Some(carried) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.to_path_buf())) {
        repair_runtime(&parts.program, &own_root(), &carried);
    }
    let mut command = Command::new(&parts.program);
    command.args(["-l", "zh", "-otxt", "-np", "-nt", "-t", &threads.to_string()]);
    if !parts.gpu {
        command.arg("-ng");
    }
    command
        .arg("-m")
        .arg(&parts.model)
        .arg("-f")
        .arg(&input)
        .arg("-of")
        .arg(&result)
        .current_dir(parts.program.parent().unwrap_or(folder))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(record);
    let mut child = crate::sys::quiet(&mut command).spawn().map_err(|_| "無法啟動本機語音辨識")?;
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
    let logged = std::fs::read(&log).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
    text.map(|heard| (heard.trim().to_string(), logged)).ok_or_else(|| "本機語音辨識失敗，請檢查音檔或模型".to_string())
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
    fn the_hint_keeps_names_and_fits_the_prompt() {
        let words = ["李曉明", "好", "GitHub", "李曉明", "Visual  Studio Code"].map(String::from);
        assert_eq!(hint_from(words.into_iter()), "李曉明、GitHub、Visual Studio Code");
        let many = (0..100).map(|n| format!("詞語{n:03}"));
        assert!(hint_from(many).chars().count() <= LONGEST_HINT);
        assert_eq!(hint_from(std::iter::empty()), "");
    }

    #[test]
    fn a_prompt_said_back_is_not_text() {
        assert_eq!(drop_echo("李曉明、GitHub。".into(), "李曉明、GitHub"), "");
        assert_eq!(drop_echo("請李曉明回電".into(), "李曉明、GitHub"), "請李曉明回電");
        // A single word the user really said is kept.
        assert_eq!(drop_echo("李曉明".into(), "李曉明"), "李曉明");
    }

    #[test]
    fn model_names_stay_in_their_folder() {
        assert!(is_model_name("ggml-base.bin"));
        assert!(is_model_name("ggml-large-v3-turbo-q5_0.bin"));
        assert!(!is_model_name("ggml-.bin"));
        assert!(!is_model_name("ggml-../x.bin"));
        assert!(!is_model_name("ggml-a/x.bin"));
        assert!(!is_model_name("model.bin"));
    }

    #[test]
    fn the_trial_recording_is_one_second_of_wav() {
        let wav = silence();
        assert_eq!((wav.len(), &wav[..4], &wav[8..12]), (44 + 32_000, &b"RIFF"[..], &b"WAVE"[..]));
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 32_000);
        assert!(crate::audio::normalise(&wav, false).is_ok());
    }

    #[test]
    fn the_log_tells_whether_the_card_was_used() {
        assert!(used_gpu("whisper_init: x\nwhisper_backend_init_gpu: using CUDA0 backend\n"));
        assert!(!used_gpu("whisper_backend_init_gpu: no GPU found\n"));
        assert!(!used_gpu(""));
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

    #[cfg(windows)]
    #[test]
    fn the_runtime_goes_only_beside_the_apps_own_programs() {
        let base = std::env::temp_dir().join("TingziVoice").join(format!("runtime-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (carried, root) = (base.join("app"), base.join("whisper"));
        let names = ["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll", "vcomp140.dll"];
        std::fs::create_dir_all(&carried).unwrap();
        for name in names {
            std::fs::write(carried.join(name), "carried").unwrap();
        }
        let own = [root.join("engine-cpu").join("Release"), root.join("engine-gpu").join("Release")];
        // Set up by hand: the folder the app also looks in, and one named by voice-root.txt.
        let external = [root.join("whisper-v1.9.2").join("Release"), base.join("by-hand").join("engine-cpu").join("Release")];
        for home in own.iter().chain(&external) {
            std::fs::create_dir_all(home).unwrap();
            std::fs::write(home.join("whisper-cli.exe"), "program").unwrap();
        }
        for home in &external {
            std::fs::write(home.join("vcomp140.dll"), "the engine's own build").unwrap();
        }
        for home in own.iter().chain(&external) {
            repair_runtime(&home.join("whisper-cli.exe"), &root, &carried);
        }
        for home in &own {
            for name in names {
                assert_eq!(std::fs::read_to_string(home.join(name)).unwrap(), "carried", "{}", home.display());
            }
        }
        for home in &external {
            assert_eq!(std::fs::read_to_string(home.join("vcomp140.dll")).unwrap(), "the engine's own build", "{}", home.display());
            assert!(names[..3].iter().all(|name| !home.join(name).exists()), "{}", home.display());
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
