//! Recognition through a transcription service that speaks the OpenAI API, with the user's own key.
//!
//! OpenAI, Groq and self-hosted servers such as whisper.cpp's own server or faster-whisper all accept
//! `POST {address}/audio/transcriptions`. A server on this computer may use plain HTTP and no key.

use reqwest::blocking::{multipart, Client};
use std::sync::Mutex;
use std::time::Duration;

/// OpenAI's limit for one upload; other services allow at least as much.
const LARGEST_UPLOAD: usize = 25 * 1024 * 1024;

/// A service address the user typed, checked before any audio is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// The address with no trailing slash.
    pub base: String,
    pub host: String,
    /// Whether the service runs on this computer, so audio does not leave it.
    pub on_this_computer: bool,
}

impl Service {
    pub fn parse(address: &str) -> Result<Self, String> {
        let base = address.trim().trim_end_matches('/').to_string();
        let parsed = reqwest::Url::parse(&base).map_err(|_| "辨識服務的網址格式不正確".to_string())?;
        let host = parsed.host_str().ok_or("辨識服務的網址格式不正確")?.trim_matches(['[', ']']).to_ascii_lowercase();
        let on_this_computer = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1");
        match parsed.scheme() {
            "https" => {}
            // Plain HTTP would send the recording and the key in the clear, except to this computer.
            "http" if on_this_computer => {}
            _ => return Err("辨識服務的網址須以 https:// 開頭；只有本機服務可以用 http://".into()),
        }
        if !parsed.username().is_empty() || parsed.password().is_some() || parsed.query().is_some() {
            return Err("辨識服務的網址不能包含帳號、密碼或查詢參數".into());
        }
        Ok(Self { base, host, on_this_computer })
    }

    fn endpoint(&self) -> String {
        format!("{}/audio/transcriptions", self.base)
    }
}

/// Whether the service can be tried, and why not, in words for the settings page and the window.
pub fn problem(address: &str, model: &str, has_key: bool) -> Option<String> {
    let service = match Service::parse(address) {
        Ok(service) => service,
        Err(reason) => return Some(reason),
    };
    if model.trim().is_empty() {
        return Some("請填入辨識服務的模型名稱".into());
    }
    if !has_key && !service.on_this_computer {
        return Some("請先填入辨識服務的金鑰".into());
    }
    None
}

/// Sends WAV bytes and returns the recognised text. `hint` lists words the speaker is likely to say.
/// Blocks; call it off the UI thread.
pub fn transcribe(address: &str, model: &str, wav: Vec<u8>, hint: &str) -> Result<String, String> {
    let key = crate::secret::read();
    if let Some(reason) = problem(address, model, key.is_some()) {
        return Err(reason);
    }
    if wav.len() <= 44 {
        return Err("沒有錄到聲音".into());
    }
    if wav.len() > LARGEST_UPLOAD {
        return Err("錄音超過 25 MB，請分段錄".into());
    }
    let service = Service::parse(address)?;
    let part = multipart::Part::bytes(wav).file_name("audio.wav").mime_str("audio/wav").map_err(|_| "無法準備錄音")?;
    let mut form = multipart::Form::new()
        .part("file", part)
        .text("model", model.trim().to_string())
        .text("language", "zh")
        .text("response_format", "json");
    if !hint.is_empty() {
        form = form.text("prompt", hint.to_string());
    }
    let mut request = client(&service)?.post(service.endpoint()).multipart(form);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request.send().map_err(|failure| explain_failure(&failure))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(explain_status(status));
    }
    let body: serde_json::Value = response.json().map_err(|_| "線上辨識失敗：服務回傳的內容無法讀取".to_string())?;
    let text = body.get("text").and_then(|text| text.as_str()).ok_or("線上未回傳文字")?;
    Ok(text.trim().to_string())
}

/// Opens the connection to the service ahead of time, so the handshake overlaps with the user
/// speaking. Sends no recording and no key; a failure is left for the real request to report.
pub fn warm(address: &str) {
    let Ok(service) = Service::parse(address) else { return };
    if service.on_this_computer {
        return;
    }
    if let Ok(client) = client(&service) {
        let _ = client.head(&service.base).timeout(Duration::from_secs(10)).send();
    }
}

/// One client per service and proxy, so the connection stays open between dictations.
fn client(service: &Service) -> Result<Client, String> {
    type Kept = (String, Option<String>, Client);
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let proxy = if service.on_this_computer { None } else { crate::recognise::proxy_for(&service.host) };
    let mut kept = KEPT.lock().map_err(|_| "線上辨識無法使用".to_string())?;
    if let Some((base, routed, ready)) = kept.as_ref() {
        if *base == service.base && *routed == proxy {
            return Ok(ready.clone());
        }
    }
    let mut builder = Client::builder().timeout(Duration::from_secs(90)).connect_timeout(Duration::from_secs(15));
    builder = match &proxy {
        Some(address) => builder.proxy(reqwest::Proxy::all(address).map_err(|_| "線上辨識無法使用：請檢查代理設定")?),
        None => builder.no_proxy(),
    };
    let fresh = builder.build().map_err(|_| "線上辨識無法使用：請檢查代理設定")?;
    *kept = Some((service.base.clone(), proxy, fresh.clone()));
    Ok(fresh)
}

/// Says what went wrong without echoing anything the server sent.
fn explain_status(status: u16) -> String {
    match status {
        400 => "線上辨識失敗：服務不接受這次請求，請檢查模型名稱".into(),
        401 => "線上辨識失敗：金鑰無效或已過期，請重新填入".into(),
        403 => "線上辨識失敗：這把金鑰沒有使用這個模型的權限".into(),
        404 => "線上辨識失敗：找不到服務，請檢查網址與模型名稱".into(),
        413 => "錄音太大，請分段錄".into(),
        429 => "線上辨識失敗：用量已達上限或請求太頻繁，請稍後再試".into(),
        500.. => "線上辨識失敗：服務暫時無法辨識，請稍後再試".into(),
        _ => "線上辨識失敗：服務拒絕這次請求".into(),
    }
}

fn explain_failure(failure: &reqwest::Error) -> String {
    if failure.is_timeout() {
        "線上辨識逾時，請檢查網路".into()
    } else if failure.is_connect() {
        "線上辨識失敗：連不上服務，請檢查網址、網路或代理設定".into()
    } else {
        "線上辨識失敗：請檢查網路及服務狀態".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_must_be_safe_to_send_audio_to() {
        let openai = Service::parse("https://api.openai.com/v1/").unwrap();
        assert_eq!((openai.base.as_str(), openai.host.as_str(), openai.on_this_computer), ("https://api.openai.com/v1", "api.openai.com", false));
        assert_eq!(openai.endpoint(), "https://api.openai.com/v1/audio/transcriptions");
        assert!(Service::parse("http://127.0.0.1:8080/v1").unwrap().on_this_computer);
        assert!(Service::parse("http://[::1]:8000/v1").unwrap().on_this_computer);
        assert!(Service::parse("http://example.com/v1").is_err());
        assert!(Service::parse("ftp://example.com").is_err());
        assert!(Service::parse("https://user:secret@example.com/v1").is_err());
        assert!(Service::parse("https://example.com/v1?key=1").is_err());
        assert!(Service::parse("不是網址").is_err());
    }

    #[test]
    fn only_a_service_on_this_computer_may_go_without_a_key() {
        assert!(problem("https://api.openai.com/v1", "whisper-1", false).unwrap().contains("金鑰"));
        assert_eq!(problem("https://api.openai.com/v1", "whisper-1", true), None);
        assert_eq!(problem("http://localhost:8080/v1", "whisper-1", false), None);
        assert!(problem("https://api.openai.com/v1", " ", true).unwrap().contains("模型"));
    }

    #[test]
    fn statuses_get_advice() {
        assert!(explain_status(401).contains("金鑰"));
        assert!(explain_status(404).contains("網址"));
        assert!(explain_status(429).contains("稍後"));
        assert!(explain_status(502).contains("暫時"));
    }
}
