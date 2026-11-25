//! Codex desktop dictation. The socket and sample cursor belong to one recording worker.
use crate::{audio::SegmentAudio, report};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use tungstenite::{
    client::IntoClientRequest, stream::MaybeTlsStream, Error, HandshakeError, Message, WebSocket,
};

const HOST: &str = "chatgpt.com";
const URL: &str =
    "wss://chatgpt.com/backend-api/dictation/stream?dictation_surface=composer&client_type=desktop";
const TIMEOUT: Duration = Duration::from_secs(15);
type Result<T> = std::result::Result<T, &'static str>;
type Route = MaybeTlsStream<TcpStream>;
type Socket = WebSocket<MaybeTlsStream<Route>>;

fn blocked(status: u16, mitigated: bool) -> bool {
    status == 403 || mitigated
}

fn connect_tcp(host: &str, port: u16) -> Result<TcpStream> {
    let until = Instant::now() + TIMEOUT;
    for address in (host, port)
        .to_socket_addrs()
        .map_err(|_| "無法解析伺服器位址")?
    {
        if let Ok(stream) =
            TcpStream::connect_timeout(&address, until.saturating_duration_since(Instant::now()))
        {
            stream
                .set_read_timeout(Some(TIMEOUT))
                .map_err(|_| "連線設定失敗")?;
            stream
                .set_write_timeout(Some(TIMEOUT))
                .map_err(|_| "連線設定失敗")?;
            return Ok(stream);
        }
    }
    Err("連線失敗或逾時")
}

fn transport() -> Result<(Route, TcpStream)> {
    let control = |tcp: &TcpStream| tcp.try_clone().map_err(|_| "連線設定失敗");
    let Some(route) = crate::recognise::proxy_for(HOST) else {
        let tcp = connect_tcp(HOST, 443)?;
        let control = control(&tcp)?;
        return Ok((Route::Plain(tcp), control));
    };
    let proxy = reqwest::Url::parse(&route).map_err(|_| "代理位址無效")?;
    if !matches!(proxy.scheme(), "http" | "https") {
        return Err("串流不支援此代理類型");
    }
    let host = proxy
        .host_str()
        .ok_or("代理位址無效")?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let tcp = connect_tcp(host, proxy.port_or_known_default().ok_or("代理連接埠無效")?)?;
    let control = control(&tcp)?;
    let mut stream = if proxy.scheme() == "https" {
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|_| "代理主機名稱無效")?;
        let tls =
            rustls::ClientConnection::new(config.into(), name).map_err(|_| "代理 TLS 連線失敗")?;
        Route::Rustls(rustls::StreamOwned::new(tls, tcp))
    } else {
        Route::Plain(tcp)
    };
    let mut request = format!("CONNECT {HOST}:443 HTTP/1.1\r\nHost: {HOST}:443\r\n");
    if !proxy.username().is_empty() {
        let mut user = percent_encoding::percent_decode_str(proxy.username()).collect::<Vec<_>>();
        user.push(b':');
        user.extend(percent_encoding::percent_decode_str(
            proxy.password().unwrap_or(""),
        ));
        let credentials = STANDARD.encode(user);
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "代理連線失敗")?;
    let mut head = BufReader::new((&mut stream).take(16384));
    let mut line = String::new();
    head.read_line(&mut line).map_err(|_| "代理握手失敗")?;
    if line.split_whitespace().nth(1) != Some("200") {
        return Err("代理拒絕連線");
    }
    loop {
        line.clear();
        if head.read_line(&mut line).map_err(|_| "代理握手失敗")? == 0 {
            return Err("代理握手中斷");
        }
        if line == "\r\n" {
            break;
        }
    }
    Ok((stream, control))
}

fn connect() -> Result<Socket> {
    let auth = crate::recognise::codex_auth().map_err(|reason| {
        report::note(format!("串流無法讀取 Codex 登入：{reason}"));
        "無法讀取 Codex 登入"
    })?;
    let mut request = URL.into_client_request().map_err(|_| "串流網址無效")?;
    let headers = request.headers_mut();
    let agent = crate::codex::user_agent().unwrap_or_else(|| {
        format!(
            "Codex Desktop/26.429.30905 ({}; {})",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    });
    for (name, value) in [
        ("authorization", format!("Bearer {}", auth.access_token)),
        (
            "chatgpt-account-id",
            auth.account_id.ok_or("Codex 登入缺少帳號識別碼")?,
        ),
        ("originator", codex_asr::DEFAULT_ORIGINATOR.to_owned()),
        ("user-agent", agent),
        (
            "accept-language",
            crate::sys::preferred_language().unwrap_or_else(|| "zh-TW".into()),
        ),
        (
            "sec-websocket-protocol",
            "chatgpt-dictation, codex-desktop".into(),
        ),
    ] {
        headers.insert(name, value.parse().map_err(|_| "串流標頭無效")?);
    }
    let (stream, control) = transport()?;
    let (socket, response) = tungstenite::client_tls(request, stream).map_err(|err| match err {
        HandshakeError::Failure(Error::Http(r))
            if blocked(
                r.status().as_u16(),
                r.headers().contains_key("cf-mitigated"),
            ) =>
        {
            "串流連線被擋"
        }
        _ => "串流握手失敗",
    })?;
    if blocked(
        response.status().as_u16(),
        response.headers().contains_key("cf-mitigated"),
    ) {
        return Err("串流連線被擋");
    }
    control
        .set_read_timeout(Some(Duration::from_millis(10)))
        .map_err(|_| "連線設定失敗")?;
    Ok(socket)
}

fn start(rate: usize, language: Option<&str>) -> Value {
    let mut config = json!({"input_audio_format":"pcm16","sample_rate_hz":rate,"num_channels":1,
        "max_buffer_size_bytes":4194304,"max_utterance_duration_ms":30000,"session_ttl_ms":300000,
        "provider_mode":"streaming_sse","transcript_delivery_mode":"segment",
        "vad":{"type":"server_vad","threshold":0.5,"prefix_padding_ms":300,"silence_duration_ms":500}});
    if let Some(language) = language {
        config["language"] = json!(language);
    }
    json!({"type":"session.start","client_capabilities":["server_stop_requested"],"config":config})
}

fn send(socket: &mut Socket, message: Value) -> Result<()> {
    socket
        .send(Message::Text(message.to_string().into()))
        .map_err(|_| "串流傳送中斷")
}

fn packet_size(sent: usize) -> usize {
    if sent == 0 {
        128
    } else {
        2048
    }
}

fn append(socket: &mut Socket, pcm: &[i16]) -> Result<()> {
    let bytes: Vec<_> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    send(
        socket,
        json!({"type":"audio.append","audio":STANDARD.encode(bytes)}),
    )
}

#[derive(Default)]
struct Piece {
    id: String,
    stopped: Option<(Duration, usize)>,
    revision: Option<u64>,
    text: String,
    ready: Duration,
    dirty: bool,
    final_end: usize,
}

struct Session {
    rate: usize,
    stop_request: Option<String>,
    started: bool,
    rotate: bool,
    closed: bool,
    pieces: Vec<Piece>,
}

impl Session {
    fn new(rate: usize) -> Self {
        Self {
            rate,
            started: false,
            rotate: false,
            closed: false,
            stop_request: None,
            pieces: Vec::new(),
        }
    }

    fn piece(&mut self, id: &str) -> &mut Piece {
        let index = self
            .pieces
            .iter()
            .position(|piece| piece.id == id)
            .unwrap_or_else(|| {
                self.pieces.push(Piece {
                    id: id.into(),
                    ..Piece::default()
                });
                self.pieces.len() - 1
            });
        &mut self.pieces[index]
    }

    /// Returns the stop-to-final latency for a new revision, when its speech boundary is known.
    fn receive(&mut self, e: &Value, sent: usize, now: Duration) -> Result<Option<Duration>> {
        let id = e["utterance_id"].as_str();
        match e["type"].as_str() {
            Some("session.started") => self.started = true,
            Some("session.stop_requested") => {
                self.stop_request = Some(match e["reason"].as_str() {
                    Some(reason) => {
                        format!("stop_requested {}", reason.lines().next().unwrap_or(""))
                    }
                    None => "stop_requested".into(),
                });
            }
            Some("session.updated") if e["session"]["status"] == "closed" => self.closed = true,
            Some("speech.started") => {
                for piece in &mut self.pieces {
                    piece.ready = now;
                }
                if let Some(id) = id {
                    self.piece(id);
                }
            }
            // Track the utterance for recovery at close. Only a final supplies text.
            Some("transcript.segment") => {
                if let Some(id) = id {
                    self.piece(id);
                }
            }
            Some("speech.stopped") => {
                self.rotate |= sent >= 240 * self.rate;
                let end = e["audio_end_ms"]
                    .as_u64()
                    .map(|ms| {
                        (ms.saturating_mul(self.rate as u64) / 1000).min(sent as u64) as usize
                    })
                    .unwrap_or(sent);
                if let Some(id) = id {
                    self.piece(id).stopped = Some((now, end));
                } else {
                    self.unassigned_stop(now, end);
                }
            }
            Some("transcript.final") => {
                let (Some(id), Some(revision), Some(text)) =
                    (id, e["revision"].as_u64(), e["text"].as_str())
                else {
                    return Ok(None);
                };
                // Some servers identify the utterance only when returning its transcript.
                if !self.pieces.iter().any(|p| p.id == id) {
                    if let Some(p) = self.pieces.iter_mut().find(|p| p.id.is_empty()) {
                        p.id = id.into();
                    }
                }
                let p = self.piece(id);
                if p.revision.is_some_and(|old| old >= revision) {
                    return Ok(None);
                }
                if p.revision.is_none() {
                    p.final_end = sent;
                }
                p.revision = Some(revision);
                p.text = text.trim().to_owned();
                p.ready = now + Duration::from_secs(1);
                p.dirty = true;
                return Ok(p.stopped.map(|(stop, _)| now.saturating_sub(stop)));
            }
            Some("transcript.failed") => return Err("一句語音辨識失敗"),
            Some("session.error") if e["fatal"] == true || e["error"]["fatal"] == true => {
                return Err("串流服務回報致命錯誤")
            }
            _ => {}
        }
        Ok(None)
    }

    fn unassigned_stop(&mut self, now: Duration, end: usize) {
        if let Some(p) = self.pieces.iter_mut().find(|p| p.stopped.is_none()) {
            p.stopped = Some((now, end));
        } else {
            self.pieces.push(Piece {
                stopped: Some((now, end)),
                ..Piece::default()
            });
        }
    }

    fn publish(&mut self, now: Duration, live: bool, mut emit: impl FnMut(usize, &str, usize)) {
        for (index, piece) in self.pieces.iter_mut().enumerate() {
            if piece.revision.is_none() || (live && now < piece.ready) {
                break;
            }
            if piece.dirty {
                emit(
                    index,
                    &piece.text,
                    piece.stopped.map_or(piece.final_end, |(_, end)| end),
                );
                piece.dirty = false;
            }
        }
    }

    fn rotating(&self, sent: usize) -> bool {
        self.rotate || sent >= 285 * self.rate
    }
}

fn parse_event(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

fn read(socket: &mut Socket) -> Result<Option<Value>> {
    match socket.read() {
        Ok(Message::Text(text)) => Ok(parse_event(&text)),
        Ok(Message::Close(_)) => Err("串流提前關閉"),
        Ok(_) => Ok(None),
        Err(Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            Ok(None)
        }
        Err(_) => Err("串流連線中斷"),
    }
}

pub struct Progress {
    pub count: usize,
    pub end: usize,
}

fn sessions(
    audio: &SegmentAudio,
    live: bool,
    stopped: &dyn Fn() -> bool,
    cancelled: &dyn Fn() -> bool,
    progress: &mut Progress,
    emit: &mut impl FnMut(usize, &str, usize),
    rotations: &mut usize,
    mut open: impl FnMut() -> Result<Socket>,
) -> std::result::Result<(), String> {
    let rate = audio.stream_rate();
    let mut total = 0;
    let mut first = true;
    loop {
        if cancelled() {
            return Ok(());
        }
        let done = stopped();
        if total == audio.stream_len() {
            if done {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let mut socket = open()?;
        if cancelled() {
            return Ok(());
        }
        if !first {
            *rotations += 1;
        }
        first = false;
        send(&mut socket, start(rate, crate::recognise::CODEX_LANGUAGE))?;
        let began = Instant::now();
        let mut state = Session::new(rate);
        let mut sent = 0;
        let base = progress.count;
        let mut closing = None;
        loop {
            if cancelled() {
                return Ok(());
            }
            let event = if state.closed {
                None
            } else {
                read(&mut socket)?
            };
            if let Some(event) = event {
                if let Some(latency) = state.receive(&event, sent, began.elapsed())? {
                    report::note(format!("串流辨識：停頓至定稿 {} ms", latency.as_millis()));
                }
            }
            state.publish(
                began.elapsed(),
                live && !(state.closed && stopped()),
                |i, text, end| {
                    let boundary = audio.source_position(total + end);
                    emit(base + i, text, boundary);
                    progress.count = progress.count.max(base + i + 1);
                    progress.end = progress.end.max(boundary);
                },
            );
            if state.closed {
                if closing.is_none() {
                    return Err("串流提前關閉".into());
                }
                // publish only releases a contiguous prefix. Any finals beyond a gap
                // belong to the local suffix and must never reach the typing queue.
                if state
                    .pieces
                    .iter()
                    .take_while(|p| p.revision.is_some())
                    .any(|p| p.dirty)
                {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                if let Some(reason) = state.stop_request {
                    return Err(reason);
                }
                if state.pieces.iter().any(|p| p.revision.is_none()) {
                    return Err("收尾時有句子缺少定稿".into());
                }
                break;
            }
            if let Some(deadline) = closing {
                if Instant::now() >= deadline {
                    return Err("串流收尾逾時".into());
                }
                continue;
            }
            if !state.started {
                if began.elapsed() >= TIMEOUT {
                    return Err("等待串流啟動逾時".into());
                }
                continue;
            }
            let done = stopped();
            let cursor = total + sent;
            let available = audio.stream_len() - cursor;
            let size = packet_size(sent).min(285 * rate - sent);
            let end_session = state.stop_request.is_some() || state.rotating(sent);
            if !end_session && available >= size {
                append(&mut socket, &audio.pcm(cursor..cursor + size))?;
                sent += size;
            } else if end_session || done {
                let tail = if end_session { 0 } else { available };
                let mut last = audio.pcm(cursor..cursor + tail);
                last.resize(tail + rate / 4, 0);
                let mut offset = 0;
                while offset < last.len() {
                    let end = (offset + packet_size(sent + offset)).min(last.len());
                    append(&mut socket, &last[offset..end])?;
                    offset = end;
                }
                sent += tail;
                send(&mut socket, json!({"type":"session.close"}))?;
                closing = Some(
                    Instant::now()
                        + TIMEOUT.max(Duration::from_secs_f64(
                            (sent + rate / 4) as f64 / rate as f64,
                        )),
                );
            }
        }
        total += sent;
    }
}

fn with_fallback(
    stream: impl FnOnce(&mut Progress) -> std::result::Result<(), String>,
    fallback: impl FnOnce(Progress, String),
) {
    let mut progress = Progress { count: 0, end: 0 };
    if let Err(reason) = stream(&mut progress) {
        fallback(progress, reason);
    }
}

pub fn run(
    audio: &SegmentAudio,
    live: bool,
    stopped: impl Fn() -> bool,
    cancelled: impl Fn() -> bool,
    mut emit: impl FnMut(usize, &str, usize),
    fallback: impl FnOnce(Progress),
) {
    let mut rotations = 0;
    report::note("使用 Codex 串流辨識");
    with_fallback(
        |progress| {
            sessions(
                audio,
                live,
                &stopped,
                &cancelled,
                progress,
                &mut emit,
                &mut rotations,
                connect,
            )
        },
        |progress, reason| {
            if !cancelled() {
                report::note(format!("串流退回本機切句與逐句辨識：{reason}"));
                fallback(progress);
            }
        },
    );
    report::note(format!("串流辨識結束，換了 {rotations} 次連線"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::SegmentEvent,
        live::Typing,
        streaming_asr::{join, Requests, Transcript},
    };
    use std::cell::RefCell;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use tungstenite::protocol::Role;

    const RATE: usize = 44_100;

    #[test]
    fn streaming_reads_the_same_sign_in_as_per_sentence_uploads() {
        let source = include_str!("dictation_stream.rs");
        assert!(source.contains(concat!("recognise::", "codex_auth()")));
        assert!(!source.contains(concat!("from_codex", "_home")));
        assert!(!source.contains(concat!("default_auth", "_file")));
    }

    #[test]
    fn revisions_keep_order_and_live_typing_waits_for_a_boundary() {
        let mut session = Session::new(RATE);
        let event = |kind| json!({"type":kind});
        let final_text = |id, revision, text| json!({"type":"transcript.final","utterance_id":id,"revision":revision,"text":text});
        let ms = Duration::from_millis;
        session
            .receive(&event("session.started"), 0, ms(0))
            .unwrap();
        assert!(session.started);
        session
            .receive(&event("speech.stopped"), 100, ms(10))
            .unwrap();
        assert_eq!(
            session.receive(&final_text("a", 1, "舊稿"), 200, ms(30)),
            Ok(Some(ms(20)))
        );
        session
            .receive(&final_text("a", 3, "第一句"), 200, ms(60))
            .unwrap();
        session
            .receive(&final_text("a", 2, "晚到的舊稿"), 200, ms(90))
            .unwrap();
        session.publish(ms(100), true, |_, _, _| panic!("too early"));
        session
            .receive(&event("speech.started"), 200, ms(100))
            .unwrap();
        let mut pieces = Vec::new();
        session.publish(ms(100), true, |i, t, end| {
            assert_eq!((i, end), (0, 100));
            pieces.push(t.to_owned());
        });
        session
            .receive(&final_text("b", 1, "第二句"), 400, ms(110))
            .unwrap();
        session.publish(ms(1109), true, |_, _, _| panic!("too early"));
        session.publish(ms(1110), true, |i, t, _| {
            assert_eq!(i, 1);
            pieces.push(t.to_owned());
        });
        session
            .receive(
                &json!({"type":"session.updated","session":{"status":"closed"}}),
                400,
                ms(1300),
            )
            .unwrap();
        assert!(session.closed);
        assert_eq!(join(&pieces), "第一句第二句");
    }

    #[test]
    fn session_failures_and_rotation() {
        let mut s = Session::new(RATE);
        let mut receive = |e: Value| s.receive(&e, 0, Duration::ZERO);
        assert!(receive(json!({"type":"transcript.failed"})).is_err());
        assert!(receive(json!({"type":"session.error","fatal":false})).is_ok());
        assert!(receive(json!({"type":"session.error","fatal":true})).is_err());
        receive(json!({"type":"session.stop_requested"})).unwrap();
        assert!(!s.rotating(1));
        assert_eq!(s.stop_request.as_deref(), Some("stop_requested"));
        let mut s = Session::new(RATE);
        let stop = json!({"type":"speech.stopped"});
        s.receive(&stop, 240 * RATE - 1, Duration::ZERO).unwrap();
        assert!(!s.rotating(240 * RATE));
        s.receive(&stop, 240 * RATE, Duration::ZERO).unwrap();
        assert!(s.rotating(240 * RATE));
        let s = Session::new(RATE);
        assert!(!s.rotating(285 * RATE - 1));
        assert!(s.rotating(285 * RATE));
        assert!(blocked(403, false));
        assert!(blocked(503, true));
        assert!(!blocked(401, false));
        assert!(start(RATE, None)["config"].get("language").is_none());
    }

    #[test]
    fn malformed_events_do_not_create_or_revise_a_sentence() {
        assert!(parse_event("{").is_none());
        let mut s = Session::new(RATE);
        for event in [
            json!({"type":"transcript.final","revision":1,"text":"x"}),
            json!({"type":"transcript.final","utterance_id":"a","text":"x"}),
            json!({"type":"transcript.final","utterance_id":"a","revision":7}),
            json!({"type":"transcript.delta","utterance_id":"a","text":"draft"}),
            json!({"type":"asset.failed"}),
            json!({}),
        ] {
            assert_eq!(s.receive(&event, 10, Duration::ZERO), Ok(None));
        }
        assert!(s.pieces.is_empty());
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Ending {
        ConnectFailure,
        Disconnect,
        StopRequest,
        MissingFinal,
    }

    fn peer_after_first(ending: Ending, stream_rate: usize) -> (Socket, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(10)))
            .unwrap();
        let (peer, _) = listener.accept().unwrap();
        let server = std::thread::spawn(move || {
            peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = WebSocket::from_raw_socket(peer, Role::Server, None);
            let start: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(start["type"], "session.start");
            assert_eq!(start["config"]["sample_rate_hz"], stream_rate);
            ws.send(Message::text(r#"{"type":"session.started"}"#))
                .unwrap();
            let mut sent = 0;
            while sent < stream_rate / 2 {
                let packet: Value =
                    serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
                let bytes = STANDARD.decode(packet["audio"].as_str().unwrap()).unwrap();
                assert_eq!(bytes.len() / 2, if sent == 0 { 128 } else { 2048 });
                sent += bytes.len() / 2;
            }
            let stop = json!({"type":"speech.stopped","utterance_id":"a","audio_end_ms":500});
            let first =
                json!({"type":"transcript.final","utterance_id":"a","revision":1,"text":"甲"});
            ws.send(Message::text(stop.to_string())).unwrap();
            if ending == Ending::StopRequest {
                ws.send(Message::text(
                    r#"{"type":"session.stop_requested","reason":"limit"}"#,
                ))
                .unwrap();
            } else {
                ws.send(Message::text(first.to_string())).unwrap();
                let next = if ending == Ending::MissingFinal {
                    r#"{"type":"transcript.segment","utterance_id":"b","text":"draft"}"#
                } else {
                    r#"{"type":"speech.started","utterance_id":"b"}"#
                };
                ws.send(Message::text(next)).unwrap();
            }
            if ending == Ending::Disconnect {
                // Each received event allows the client to send another packet.
                for _ in 0..3 {
                    let _ = ws.read();
                }
                ws.close(None).unwrap();
                return;
            }
            let mut audio_bytes = Vec::new();
            loop {
                let msg = ws.read().unwrap();
                let e: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
                if e["type"] == "session.close" {
                    break;
                }
                assert_eq!(e["type"], "audio.append");
                audio_bytes.extend(STANDARD.decode(e["audio"].as_str().unwrap()).unwrap());
            }
            assert!(audio_bytes.ends_with(&vec![0; stream_rate / 4 * 2]));
            if ending == Ending::MissingFinal {
                assert_eq!(sent + audio_bytes.len() / 2, 2 * stream_rate + stream_rate / 4);
            }
            if ending == Ending::StopRequest {
                // The last final may arrive after the client has sent session.close.
                ws.send(Message::text(first.to_string())).unwrap();
            } else {
                // A final beyond unfinished b must not be typed before local recovery.
                ws.send(Message::text(r#"{"type":"transcript.final","utterance_id":"c","revision":1,"text":"unexpected"}"#)).unwrap();
            }
            ws.send(Message::text(
                r#"{"type":"session.updated","session":{"status":"closed"}}"#,
            ))
            .unwrap();
        });
        (
            WebSocket::from_raw_socket(
                MaybeTlsStream::Plain(Route::Plain(client)),
                Role::Client,
                None,
            ),
            server,
        )
    }

    #[test]
    fn server_stop_drains_finals_then_resumes_local_without_a_second_session() {
        assert_local_recovery(Ending::StopRequest, false);
    }

    #[test]
    fn incomplete_utterance_recovers_suffix_without_typing_later_finals_twice() {
        assert_local_recovery(Ending::MissingFinal, false);
    }

    #[test]
    fn denoised_stream_sends_16_khz_and_recovers_at_the_native_boundary() {
        assert_local_recovery(Ending::MissingFinal, true);
    }

    #[test]
    fn connection_failure_and_disconnect_resume_local_requests_without_duplicate_typing() {
        assert_local_recovery(Ending::ConnectFailure, false);
        assert_local_recovery(Ending::Disconnect, false);
    }

    fn assert_local_recovery(ending: Ending, noise_suppression: bool) {
        let interrupted = ending != Ending::ConnectFailure;
        let mut samples = vec![0.4; RATE / 2];
        samples.resize(RATE * 3 / 2, 0.0);
        samples.resize(RATE * 2, 0.4);
        let audio = SegmentAudio::synthetic(samples, RATE as u32, noise_suppression);
        let requests = Arc::new(Requests::default());
        let typing = Arc::new(Mutex::new(Typing::default()));
        let server = RefCell::new(None);
        let mut rotations = 0;
        with_fallback(
            |progress| {
                sessions(
                    &audio,
                    true,
                    &|| ending != Ending::StopRequest,
                    &|| false,
                    progress,
                    &mut |index, text, end| {
                        assert_eq!(index, 0);
                        assert_eq!(end, RATE / 2);
                        let mut t = typing.lock().unwrap();
                        t.add(text, end);
                        let next = t.next().unwrap();
                        t.note_typed(&next);
                        requests.replace_sentences(&[text.to_owned()], |_| {});
                    },
                    &mut rotations,
                    || {
                        assert!(server.borrow().is_none(), "unexpected second session");
                        if !interrupted {
                            return Err("連線失敗");
                        }
                        let expected_rate = if noise_suppression { 16_000 } else { RATE };
                        let (socket, worker) = peer_after_first(ending, expected_rate);
                        assert!(server.replace(Some(worker)).is_none());
                        Ok(socket)
                    },
                )
            },
            |progress, reason| {
                if ending == Ending::StopRequest {
                    assert_eq!(reason, "stop_requested limit");
                }
                assert_eq!(progress.count, usize::from(interrupted));
                assert_eq!(progress.end, if interrupted { RATE / 2 } else { 0 });
                let local = requests.clone();
                let typed = typing.clone();
                audio.resume_local(
                    progress.end,
                    progress.count,
                    (
                        1,
                        Box::new(move |event, _| {
                            let SegmentEvent::Ready(s) = event else {
                                panic!("unexpected gap")
                            };
                            let typed = typed.clone();
                            local.submit(
                                s.sequence,
                                s.start..s.end,
                                move |range, _| {
                                    let word = if range.start < RATE / 2 { "甲" } else { "乙" };
                                    let mut t = typed.lock().unwrap();
                                    t.add(word, range.end);
                                    let next = t.next().unwrap();
                                    t.note_typed(&next);
                                    Ok(word.into())
                                },
                                |_| {},
                            );
                        }),
                    ),
                    || true,
                    || false,
                );
            },
        );
        if let Some(worker) = server.into_inner() {
            worker.join().unwrap();
        }
        assert_eq!(requests.finish(), Transcript::Sentences("甲乙".into()));
        assert_eq!(typing.lock().unwrap().typed(), "甲乙");
        assert_eq!(rotations, 0);
    }
}
