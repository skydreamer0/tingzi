//! The Claude voice stream protocol as Claude Code 2.1.323 speaks it: the request, the frames the client
//! sends and the frames the server answers with. Nothing here opens a connection or reads a sign-in.
#![allow(dead_code)]

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde_json::Value;
use std::fmt;
use std::time::Duration;
use tungstenite::{client::IntoClientRequest, handshake::client::Request, http::HeaderValue, Message};

/// A sign-in token. It can be handed to the request and nowhere else: it never prints.
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timings {
    /// Between KeepAlive frames while the socket is open.
    pub keep_alive: Duration,
    /// After CloseStream with no server frame at all.
    pub no_data: Duration,
    /// After CloseStream, whatever arrives.
    pub safety: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            keep_alive: Duration::from_millis(8000),
            no_data: Duration::from_millis(1500),
            safety: Duration::from_millis(5000),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceConfig {
    pub language: String,
    pub keyterms: Vec<String>,
    pub timings: Timings,
}

const ENDPOINT: &str = "wss://api.anthropic.com/api/ws/speech_to_text/voice_stream";
const KEYTERMS_LIMIT: usize = 1024;
/// Only unreserved characters stay as they are, so a language value cannot add query parameters.
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');

/// The parameters and their order follow Claude Code 2.1.323. It sends no stt_provider, so neither does this.
pub fn request_url(config: &VoiceConfig) -> String {
    let language = utf8_percent_encode(&config.language, QUERY_VALUE);
    format!(
        "{ENDPOINT}?encoding=linear16&sample_rate=16000&channels=1&endpointing_ms=300&utterance_end_ms=1000\
         &language={language}&use_conversation_engine=true"
    )
}

/// Each term keeps printable ASCII only, with commas as spaces and runs of spaces collapsed. Empty and repeated
/// terms are dropped, and terms stop before the joined list would pass 1024 bytes. None when nothing is left.
pub fn keyterms(terms: &[String]) -> Option<String> {
    let mut joined = String::new();
    let mut kept: Vec<String> = Vec::new();
    for term in terms {
        let ascii: String = term.chars().map(|c| if c == ',' { ' ' } else { c }).filter(|c| (' '..='~').contains(c)).collect();
        let term = ascii.split_whitespace().collect::<Vec<_>>().join(" ");
        if term.is_empty() || kept.contains(&term) {
            continue;
        }
        let separator = usize::from(!joined.is_empty());
        if joined.len() + separator + term.len() > KEYTERMS_LIMIT {
            break;
        }
        if separator == 1 {
            joined.push(',');
        }
        joined.push_str(&term);
        kept.push(term);
    }
    (!joined.is_empty()).then_some(joined)
}

pub fn request(config: &VoiceConfig, token: &Secret, user_agent: &str) -> Result<Request, &'static str> {
    let mut request = request_url(config).into_client_request().map_err(|_| "語音服務網址無效")?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.0)).map_err(|_| "登入資料無效")?;
    authorization.set_sensitive(true);
    let headers = request.headers_mut();
    headers.insert("authorization", authorization);
    let mut values = vec![
        ("user-agent", user_agent.to_owned()),
        ("x-app", "cli".to_owned()),
        ("anthropic-client-platform", "claude_code_cli".to_owned()),
    ];
    if let Some(terms) = keyterms(&config.keyterms) {
        values.push(("x-config-keyterms", terms));
    }
    for (name, value) in values {
        headers.insert(name, HeaderValue::from_str(&value).map_err(|_| "語音服務標頭無效")?);
    }
    Ok(request)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClientFrame {
    /// PCM16LE, 16 kHz, mono.
    Audio(Vec<u8>),
    KeepAlive,
    CloseStream,
}

impl ClientFrame {
    pub fn message(self) -> Message {
        match self {
            Self::Audio(pcm) => Message::Binary(pcm.into()),
            Self::KeepAlive => Message::text(r#"{"type":"KeepAlive"}"#),
            Self::CloseStream => Message::text(r#"{"type":"CloseStream"}"#),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerFrame {
    /// The whole provisional text of the current utterance. It replaces the held text and is never final.
    /// Empty text must not clear what is held.
    Interim(String),
    /// Ends the utterance: the held text becomes final.
    Endpoint(Option<String>),
    TranscriptError { description: Option<String>, error_code: Option<String> },
    Error { message: Option<String>, keys: Vec<String> },
    Unknown(Option<String>),
}

impl ServerFrame {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let frame: Value = serde_json::from_str(text).map_err(|_| "語音服務回傳無法解析的資料")?;
        let data = frame["data"].as_str().unwrap_or_default();
        Ok(match frame["type"].as_str() {
            Some("TranscriptInterim" | "TranscriptText") => Self::Interim(data.to_owned()),
            Some("TranscriptEndpoint") => Self::Endpoint((!data.is_empty()).then(|| data.to_owned())),
            Some("TranscriptError") => Self::TranscriptError {
                description: scalar(&frame["description"]),
                error_code: scalar(&frame["error_code"]),
            },
            Some("error") => {
                let mut keys: Vec<String> = frame.as_object().map_or_else(Vec::new, |map| map.keys().cloned().collect());
                keys.sort();
                Self::Error { message: scalar(&frame["message"]), keys }
            }
            other => Self::Unknown(other.map(str::to_owned)),
        })
    }

    /// What an error frame reports, None for the other frames.
    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::TranscriptError { description, error_code } => Some(
                description.clone().or_else(|| error_code.clone()).unwrap_or_else(|| "unknown transcription error".into()),
            ),
            Self::Error { message, keys } => Some(
                message.clone().unwrap_or_else(|| format!("error frame without a message (keys: {})", keys.join(", "))),
            ),
            _ => None,
        }
    }
}

/// A string or number field as text; the client falls through to the next field only when one is absent.
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "made-up-token-0123456789";

    fn config(language: &str, keyterms: &[&str]) -> VoiceConfig {
        VoiceConfig {
            language: language.into(),
            keyterms: keyterms.iter().map(|t| t.to_string()).collect(),
            timings: Timings::default(),
        }
    }

    #[test]
    fn the_url_carries_the_contract_parameters_in_order_and_no_provider() {
        let url = request_url(&config("zh", &[]));
        assert_eq!(
            url,
            "wss://api.anthropic.com/api/ws/speech_to_text/voice_stream?encoding=linear16&sample_rate=16000\
             &channels=1&endpointing_ms=300&utterance_end_ms=1000&language=zh&use_conversation_engine=true"
        );
        assert!(!url.contains("stt_provider"));
        assert!(request_url(&config("zh&stt_provider=x", &[])).contains("&language=zh%26stt_provider%3Dx&"));
    }

    #[test]
    fn default_timings_follow_the_client_and_can_be_changed() {
        let timings = Timings::default();
        assert_eq!(
            (timings.keep_alive, timings.no_data, timings.safety),
            (Duration::from_millis(8000), Duration::from_millis(1500), Duration::from_millis(5000))
        );
        let custom = VoiceConfig { timings: Timings { no_data: Duration::from_millis(20), ..timings }, ..config("zh", &[]) };
        assert_eq!(custom.timings.no_data, Duration::from_millis(20));
    }

    #[test]
    fn the_request_sends_the_sign_in_and_the_identifying_headers() {
        let sent = request(&config("zh", &["Tingzi", "GitHub Actions"]), &Secret::new(TOKEN), "Tingzi/0.10.0").unwrap();
        assert_eq!(sent.uri().to_string(), request_url(&config("zh", &[])));
        let header = |name: &str| sent.headers().get(name).map(|v| v.to_str().unwrap().to_owned());
        assert_eq!(header("authorization"), Some(format!("Bearer {TOKEN}")));
        assert_eq!(header("user-agent").as_deref(), Some("Tingzi/0.10.0"));
        assert_eq!(header("x-app").as_deref(), Some("cli"));
        assert_eq!(header("anthropic-client-platform").as_deref(), Some("claude_code_cli"));
        assert_eq!(header("x-config-keyterms").as_deref(), Some("Tingzi,GitHub Actions"));
        assert!(sent.headers().get("authorization").unwrap().is_sensitive());
        assert!(!format!("{:?}", sent.headers()).contains(TOKEN));
        assert!(request(&config("zh", &[]), &Secret::new(TOKEN), "bad\nagent").is_err());
    }

    #[test]
    fn the_token_never_prints() {
        let secret = Secret::new(TOKEN);
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(!format!("{:#?}", (&secret, 1)).contains(TOKEN));
    }

    #[test]
    fn keyterms_keep_printable_ascii_once_and_stop_before_1024_bytes() {
        let terms = |list: &[&str]| keyterms(&list.iter().map(|t| t.to_string()).collect::<Vec<_>>());
        assert_eq!(terms(&["聽字", "注音輸入", "  "]), None);
        assert_eq!(terms(&[]), None);
        assert_eq!(terms(&["聽字 Tingzi", "a,b", "Tingzi", "x\ty  z", "café"]), Some("Tingzi,a b,xy z,caf".into()));
        let long: Vec<String> = (0..200).map(|i| format!("term{i:04}")).collect();
        let joined = keyterms(&long).unwrap();
        assert!(joined.len() <= 1024 && joined.len() > 1000, "{}", joined.len());
        assert!(long.iter().any(|t| joined.ends_with(t.as_str())));
        let sent = request(&config("zh", &["聽字"]), &Secret::new(TOKEN), "Tingzi").unwrap();
        assert!(sent.headers().get("x-config-keyterms").is_none());
    }

    #[test]
    fn client_frames_are_binary_audio_and_exact_json_controls() {
        assert_eq!(ClientFrame::Audio(vec![1, 0, 255, 127]).message(), Message::Binary(vec![1, 0, 255, 127].into()));
        assert_eq!(ClientFrame::KeepAlive.message(), Message::text(r#"{"type":"KeepAlive"}"#));
        assert_eq!(ClientFrame::CloseStream.message(), Message::text(r#"{"type":"CloseStream"}"#));
    }

    #[test]
    fn interim_and_text_frames_are_both_provisional_and_endpoint_is_final() {
        let parse = |text: &str| ServerFrame::parse(text).unwrap();
        assert_eq!(parse(r#"{"type":"TranscriptInterim","data":"你好"}"#), ServerFrame::Interim("你好".into()));
        assert_eq!(parse(r#"{"type":"TranscriptText","data":"你好嗎"}"#), ServerFrame::Interim("你好嗎".into()));
        assert_eq!(parse(r#"{"type":"TranscriptInterim","data":""}"#), ServerFrame::Interim(String::new()));
        assert_eq!(parse(r#"{"type":"TranscriptText"}"#), ServerFrame::Interim(String::new()));
        assert_eq!(parse(r#"{"type":"TranscriptEndpoint"}"#), ServerFrame::Endpoint(None));
        assert_eq!(parse(r#"{"type":"TranscriptEndpoint","data":""}"#), ServerFrame::Endpoint(None));
        assert_eq!(parse(r#"{"type":"TranscriptEndpoint","data":"好"}"#), ServerFrame::Endpoint(Some("好".into())));
    }

    #[test]
    fn error_frames_keep_their_fields_and_report_like_the_client() {
        let parse = |text: &str| ServerFrame::parse(text).unwrap();
        let both = parse(r#"{"type":"TranscriptError","description":"bad audio","error_code":"E1"}"#);
        assert_eq!(both, ServerFrame::TranscriptError { description: Some("bad audio".into()), error_code: Some("E1".into()) });
        assert_eq!(both.error_message().as_deref(), Some("bad audio"));
        assert_eq!(parse(r#"{"type":"TranscriptError","error_code":"E1"}"#).error_message().as_deref(), Some("E1"));
        assert_eq!(parse(r#"{"type":"TranscriptError","error_code":502}"#).error_message().as_deref(), Some("502"));
        assert_eq!(parse(r#"{"type":"TranscriptError"}"#).error_message().as_deref(), Some("unknown transcription error"));
        let error = parse(r#"{"type":"error","message":"rate limited"}"#);
        assert_eq!(error, ServerFrame::Error { message: Some("rate limited".into()), keys: vec!["message".into(), "type".into()] });
        assert_eq!(error.error_message().as_deref(), Some("rate limited"));
        assert_eq!(
            parse(r#"{"type":"error","code":429}"#).error_message().as_deref(),
            Some("error frame without a message (keys: code, type)")
        );
        assert_eq!(parse(r#"{"type":"TranscriptInterim","data":"x"}"#).error_message(), None);
    }

    #[test]
    fn unknown_and_malformed_frames_never_panic() {
        assert_eq!(ServerFrame::parse(r#"{"type":"Metadata","data":"x"}"#), Ok(ServerFrame::Unknown(Some("Metadata".into()))));
        assert_eq!(ServerFrame::parse(r#"{"data":"x"}"#), Ok(ServerFrame::Unknown(None)));
        assert_eq!(ServerFrame::parse("[1,2]"), Ok(ServerFrame::Unknown(None)));
        assert_eq!(ServerFrame::parse(r#"{"type":"TranscriptInterim","data":5}"#), Ok(ServerFrame::Interim(String::new())));
        for bad in ["", "{", "not json", r#"{"type":"TranscriptInterim","data":"x""#] {
            assert!(ServerFrame::parse(bad).is_err(), "{bad:?}");
        }
    }
}
