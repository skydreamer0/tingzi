//! Codex desktop dictation. The socket and sample cursor belong to one recording worker.
use crate::{audio::SegmentAudio, report};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{mpsc, Condvar, Mutex};
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
    dirty: bool,
    final_end: usize,
    segment_revision: Option<u64>,
    segment_text: String,
    previewed_revision: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
enum Preview<'a> {
    Segment(&'a str),
    Freeze,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Update {
    Segment(String),
    Freeze,
    Final(usize, String, usize),
    Finish,
}

/// Session publishes drafts only for the first unfinished utterance. A final or
/// freeze supersedes its queued draft, and each final remains an ordered barrier.
#[derive(Default)]
pub struct Updates {
    items: Mutex<VecDeque<Update>>,
    ready: Condvar,
}

impl Updates {
    pub fn push(&self, update: Update) {
        let mut items = self.items.lock().unwrap();
        if !matches!(update, Update::Finish)
            && matches!(items.back(), Some(Update::Segment(_) | Update::Freeze))
        {
            items.pop_back();
        }
        items.push_back(update);
        self.ready.notify_one();
    }

    /// None is a modifier retry. Idle workers wait indefinitely.
    pub fn next(&self, retry: bool) -> Option<Update> {
        let items = self.items.lock().unwrap();
        let mut items = if retry {
            self.ready.wait_timeout_while(items, Duration::from_millis(20), |q| q.is_empty()).unwrap().0
        } else {
            self.ready.wait_while(items, |q| q.is_empty()).unwrap()
        };
        items.pop_front()
    }
}

struct Session {
    rate: usize,
    live: bool,
    stop_request: Option<String>,
    started: bool,
    frozen: bool,
    rotate: bool,
    closed: bool,
    pieces: Vec<Piece>,
}

impl Session {
    fn new(rate: usize, live: bool) -> Self {
        Self {
            rate,
            live,
            started: false,
            frozen: false,
            rotate: false,
            closed: false,
            stop_request: None,
            pieces: Vec::new(),
        }
    }

    fn piece(&mut self, id: &str) -> &mut Piece {
        if let Some(index) = self.pieces.iter().position(|piece| piece.id == id) {
            return &mut self.pieces[index];
        }
        self.pieces.push(Piece { id: id.into(), ..Piece::default() });
        self.pieces.last_mut().unwrap()
    }

    fn transcript_piece(&mut self, id: &str) -> &mut Piece {
        // A stop can precede the first identified transcript. A new speech.started,
        // on the other hand, belongs to a new utterance and must not claim that stop.
        if !self.pieces.iter().any(|piece| piece.id == id) {
            if let Some(piece) = self.pieces.iter_mut().find(|piece| piece.id.is_empty()) {
                piece.id = id.into();
            }
        }
        self.piece(id)
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
                if let Some(id) = id {
                    self.piece(id);
                }
            }
            // Segment text is a complete preview; deltas never supply preview text.
            Some("transcript.segment") => {
                if let Some(id) = id {
                    let piece = self.transcript_piece(id);
                    if let (Some(revision), Some(text)) =
                        (e["revision"].as_u64(), e["text"].as_str())
                    {
                        if piece.revision.is_none()
                            && piece.segment_revision.is_none_or(|old| revision > old)
                        {
                            piece.segment_revision = Some(revision);
                            piece.segment_text = text.trim().to_owned();
                        }
                    }
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
                let live = self.live;
                let p = self.transcript_piece(id);
                if live && p.revision.is_some() {
                    return Ok(None);
                }
                if p.revision.is_some_and(|old| old >= revision) {
                    return Ok(None);
                }
                if p.revision.is_none() {
                    p.final_end = sent;
                }
                p.revision = Some(revision);
                p.text = text.trim().to_owned();
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

    fn publish(&mut self, mut emit: impl FnMut(usize, &str, usize)) {
        for (index, piece) in self.pieces.iter_mut().enumerate() {
            if piece.revision.is_none() {
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

    fn preview(&mut self) -> Option<Preview<'_>> {
        let prefix = self
            .pieces
            .iter()
            .take_while(|p| p.revision.is_some())
            .count();
        if prefix == self.pieces.len() {
            return None;
        }
        if prefix + 1 != self.pieces.len() {
            // A newer utterance supersedes any pending draft for the earlier gap.
            return (!std::mem::replace(&mut self.frozen, true)).then_some(Preview::Freeze);
        }
        self.frozen = false;
        let piece = &mut self.pieces[prefix];
        let Some(revision) = piece.segment_revision else {
            return None;
        };
        if piece.previewed_revision == Some(revision) {
            return None;
        }
        piece.previewed_revision = Some(revision);
        Some(Preview::Segment(&piece.segment_text))
    }

    fn rotating(&self, sent: usize) -> bool {
        self.rotate || sent >= 285 * self.rate
    }
}

/// Interim text counts for one event type: how many, first and last time, last length.
#[derive(Default)]
struct Interim {
    count: usize,
    first_ms: u128,
    last_ms: u128,
    chars: usize,
}

/// What one session received, for the log only: event times and lengths, never text, audio or tokens.
#[derive(Default)]
struct Trace {
    session: usize,
    max_queue: Duration,
    delta: Interim,
    segment: Interim,
}

impl Trace {
    /// One log line for a speech or final event; interim events are only counted.
    fn record(&mut self, e: &Value, ms: u128) -> Option<String> {
        let id = e["utterance_id"].as_str().unwrap_or("-");
        let chars = e["text"].as_str().map_or(0, |t| t.trim().chars().count());
        let interim = match e["type"].as_str()? {
            "speech.started" => return Some(format!("串流事件 #{}：{ms} ms speech.started {id}", self.session)),
            "speech.stopped" => return Some(format!("串流事件 #{}：{ms} ms speech.stopped {id}", self.session)),
            "transcript.final" => {
                let revision = e["revision"].as_u64().map_or("-".to_owned(), |r| r.to_string());
                return Some(format!(
                    "串流事件 #{}：{ms} ms transcript.final {id} 修訂 {revision}，{chars} 字",
                    self.session
                ));
            }
            "transcript.delta" => &mut self.delta,
            "transcript.segment" => &mut self.segment,
            _ => return None,
        };
        if interim.count == 0 {
            interim.first_ms = ms;
        }
        interim.count += 1;
        interim.last_ms = ms;
        interim.chars = chars;
        None
    }

    fn summary(&self) -> String {
        let part = |name: &str, i: &Interim| {
            if i.count == 0 {
                format!("{name} 0 次")
            } else {
                format!("{name} {} 次（{}–{} ms，最後 {} 字）", i.count, i.first_ms, i.last_ms, i.chars)
            }
        };
        format!(
            "串流事件 #{}：{}，{}",
            self.session,
            part("transcript.delta", &self.delta),
            part("transcript.segment", &self.segment)
        )
    }
}

pub(crate) struct Connection {
    socket: Socket,
    handshake: Instant,
    requested: Instant,
}

fn begin(socket: Socket, handshake: Instant, rate: usize) -> Result<Connection> {
    let mut socket = socket;
    send(&mut socket, start(rate, crate::recognise::CODEX_LANGUAGE))?;
    Ok(Connection { socket, handshake, requested: Instant::now() })
}

fn open_session(rate: usize) -> Result<Connection> {
    let socket = connect()?;
    begin(socket, Instant::now(), rate)
}

/// One recording attempt owns the handshake even if microphone setup fails.
pub struct Preconnection {
    pub rate: mpsc::Sender<u32>,
    worker: std::thread::JoinHandle<Result<(usize, Connection)>>,
    pub began: Instant,
}

impl Preconnection {
    pub fn finish(self, actual: usize) -> Result<Connection> {
        let (rate, connection) = self.worker.join().map_err(|_| "串流連線中斷")??;
        if rate == actual { return Ok(connection); }
        // The selected input failed to start and its substitute uses another format.
        drop(connection);
        open_session(actual)
    }
}

pub fn preconnect() -> Preconnection {
    let began = Instant::now();
    let (rate, rates) = mpsc::channel::<u32>();
    let worker = std::thread::spawn(move || {
        let socket = connect()?;
        let handshake = Instant::now();
        let rate = rates.recv().map_err(|_| "錄音已取消")? as usize;
        Ok((rate, begin(socket, handshake, rate)?))
    });
    Preconnection { rate, worker, began }
}

/// Logs the interim counts when a session ends, however it ends.
struct TraceLog(Trace);

impl Drop for TraceLog {
    fn drop(&mut self) {
        report::note(self.0.summary());
        report::note(format!("串流事件 #{}：音訊排隊最長 {} ms", self.0.session, self.0.max_queue.as_millis()));
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
    updates: &Updates,
    recording: Instant,
    capture: Instant,
    rotations: &mut usize,
    mut open: impl FnMut() -> Result<Connection>,
) -> std::result::Result<(), String> {
    let rate = audio.stream_rate();
    let mut total = 0;
    let mut first = true;
    let mut session_number = 0;
    loop {
        if cancelled() {
            return Ok(());
        }
        let done = stopped();
        if !first && total == audio.len() {
            if done {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        let Connection { mut socket, handshake, requested: began } = open()?;
        if cancelled() {
            return Ok(());
        }
        if !first {
            *rotations += 1;
        }
        first = false;
        session_number += 1;
        let mut trace = TraceLog(Trace { session: session_number, ..Trace::default() });
        report::note(format!("串流事件 #{session_number}：握手完成 {} ms，session.start sample_rate_hz {rate}，串流不降噪",
            handshake.duration_since(recording).as_millis()));
        let mut state = Session::new(rate, live);
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
                if let Some(line) = trace.0.record(&event, recording.elapsed().as_millis()) {
                    report::note(line);
                }
                if !state.started && event["type"] == "session.started" {
                    report::note(format!("串流事件 #{session_number}：session.started {} ms", recording.elapsed().as_millis()));
                }
                if let Some(latency) = state.receive(&event, sent, began.elapsed())? {
                    report::note(format!("串流辨識：停頓至定稿 {} ms", latency.as_millis()));
                }
            }
            state.publish(|i, text, end| {
                let boundary = total + end;
                updates.push(Update::Final(base + i, text.into(), boundary));
                progress.count = progress.count.max(base + i + 1);
                progress.end = progress.end.max(boundary);
            });
            if live {
                if let Some(change) = state.preview() {
                    updates.push(match change {
                        Preview::Segment(text) => Update::Segment(text.into()),
                        Preview::Freeze => Update::Freeze,
                    });
                }
            }
            if state.closed {
                if closing.is_none() {
                    return Err("串流提前關閉".into());
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
            // Snapshot once so a fast producer cannot starve incoming socket events.
            let mut remaining = audio.len() - (total + sent);
            let limit = 285 * rate;
            let mut record_delay = |end| {
                let captured = Duration::from_secs_f64((total + end) as f64 / rate as f64);
                trace.0.max_queue = trace.0.max_queue.max(capture.elapsed().saturating_sub(captured));
            };
            while state.stop_request.is_none() && !state.rotating(sent) {
                let size = packet_size(sent).min(limit - sent);
                if remaining < size { break; }
                if cancelled() { return Ok(()); }
                append(&mut socket, &audio.pcm(total + sent..total + sent + size))?;
                sent += size;
                remaining -= size;
                record_delay(sent);
            }
            let end_session = state.stop_request.is_some() || state.rotating(sent);
            if end_session || done {
                let tail = if end_session { 0 } else { remaining };
                let mut last = audio.pcm(total + sent..total + sent + tail);
                last.resize(tail + rate / 4, 0);
                let mut offset = 0;
                while offset < last.len() {
                    let end = (offset + packet_size(sent + offset)).min(last.len());
                    append(&mut socket, &last[offset..end])?;
                    if offset < tail { record_delay(sent + end.min(tail)); }
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
    updates: &Updates,
    recording: Instant,
    capture: Instant,
    first: impl FnOnce(usize) -> Result<Connection>,
) -> Option<Progress> {
    let rate = audio.stream_rate();
    let mut first = Some(first);
    let mut recovery = None;
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
                updates,
                recording,
                capture,
                &mut rotations,
                || match first.take() { Some(open) => open(rate), None => open_session(rate) },
            )
        },
        |progress, reason| {
            if !cancelled() {
                report::note(format!("串流退回本機切句與逐句辨識：{reason}"));
                recovery = Some(progress);
            }
        },
    );
    report::note(format!("串流辨識結束，換了 {rotations} 次連線"));
    recovery
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::SegmentEvent,
        live::Typing,
        streaming_asr::{Requests, Transcript},
    };
    use std::cell::RefCell;
    use std::net::TcpListener;
    use std::sync::Arc;
    use tungstenite::protocol::Role;

    const RATE: usize = 44_100;

    #[test]
    fn the_trace_logs_event_times_and_lengths_without_text() {
        let mut trace = Trace { session: 1, ..Trace::default() };
        assert_eq!(
            trace.record(&json!({"type":"speech.started","utterance_id":"u1"}), 120),
            Some("串流事件 #1：120 ms speech.started u1".into())
        );
        assert_eq!(
            trace.record(&json!({"type":"speech.stopped","utterance_id":"u1"}), 2400),
            Some("串流事件 #1：2400 ms speech.stopped u1".into())
        );
        for (ms, text) in [(900, "你"), (1500, "你好嗎")] {
            assert_eq!(trace.record(&json!({"type":"transcript.segment","utterance_id":"u1","text":text}), ms), None);
        }
        let line = trace
            .record(&json!({"type":"transcript.final","utterance_id":"u1","revision":2,"text":" 你好嗎？ "}), 3000)
            .unwrap();
        assert_eq!(line, "串流事件 #1：3000 ms transcript.final u1 修訂 2，4 字");
        assert!(!line.contains("你好"));
        assert_eq!(trace.record(&json!({"type":"audio.frame_processed"}), 3100), None);
        assert_eq!(
            trace.summary(),
            "串流事件 #1：transcript.delta 0 次，transcript.segment 2 次（900–1500 ms，最後 3 字）"
        );
    }

    #[test]
    fn streaming_reads_the_same_sign_in_as_per_sentence_uploads() {
        let source = include_str!("dictation_stream.rs");
        assert!(source.contains(concat!("recognise::", "codex_auth()")));
        assert!(!source.contains(concat!("from_codex", "_home")));
        assert!(!source.contains(concat!("default_auth", "_file")));
    }

    #[test]
    fn live_finals_publish_immediately_and_ignore_later_revisions() {
        let mut session = Session::new(RATE, true);
        for (revision, text) in [(1, "first"), (2, "revised")] {
            session
                .receive(
                    &json!({"type":"transcript.final","utterance_id":"a",
                "revision":revision,"text":text}),
                    100,
                    Duration::ZERO,
                )
                .unwrap();
        }
        let mut output = Vec::new();
        session.publish(|_, text, _| {
            output.push(text.to_owned())
        });
        assert_eq!(output, ["first"]);
        session.publish(|_, _, _| panic!("duplicate final"));
    }

    #[test]
    fn previews_use_highest_complete_revision_once_at_latest_contiguous_piece() {
        let mut session = Session::new(RATE, true);
        let segment = |id, revision, text| {
            json!({"type":"transcript.segment",
            "utterance_id":id,"revision":revision,"text":text})
        };
        session
            .receive(
                &json!({"type":"speech.started","utterance_id":"a"}),
                0,
                Duration::ZERO,
            )
            .unwrap();
        session
            .receive(&segment("b", 3, "full preview"), 0, Duration::ZERO)
            .unwrap();
        session
            .receive(&segment("b", 2, "stale"), 0, Duration::ZERO)
            .unwrap();
        assert_eq!(session.preview(), Some(Preview::Freeze));
        session
            .receive(
                &json!({"type":"transcript.final","utterance_id":"a",
            "revision":1,"text":"final"}),
                0,
                Duration::ZERO,
            )
            .unwrap();
        assert_eq!(session.preview(), Some(Preview::Segment("full preview")));
        assert_eq!(session.preview(), None);
        session
            .receive(&segment("b", 3, "duplicate"), 0, Duration::ZERO)
            .unwrap();
        assert_eq!(session.preview(), None);
        session
            .receive(&segment("b", 4, "new complete preview"), 0, Duration::ZERO)
            .unwrap();
        assert_eq!(session.preview(), Some(Preview::Segment("new complete preview")));
        session
            .receive(
                &json!({"type":"speech.started","utterance_id":"c"}),
                0,
                Duration::ZERO,
            )
            .unwrap();
        session
            .receive(&segment("b", 5, "old utterance"), 0, Duration::ZERO)
            .unwrap();
        assert_eq!(session.preview(), Some(Preview::Freeze));
    }

    #[test]
    fn a_new_utterance_freezes_pending_drafts_until_the_preceding_final_arrives() {
        let mut session = Session::new(RATE, true);
        assert_eq!(session.preview(), None);
        session.receive(&json!({"type":"transcript.segment","utterance_id":"a",
            "revision":1,"text":"old draft"}), 100, Duration::ZERO).unwrap();
        assert_eq!(session.preview(), Some(Preview::Segment("old draft")));
        assert_eq!(session.preview(), None);
        session.receive(&json!({"type":"speech.started","utterance_id":"b"}), 150, Duration::ZERO).unwrap();
        assert_eq!(session.preview(), Some(Preview::Freeze));
        assert_eq!(session.preview(), None);
        session.receive(&json!({"type":"transcript.segment","utterance_id":"b",
            "revision":2,"text":"latest draft"}), 200, Duration::ZERO).unwrap();
        assert_eq!(session.preview(), None);
        session.receive(&json!({"type":"transcript.final","utterance_id":"a",
            "revision":1,"text":"old final"}), 250, Duration::ZERO).unwrap();
        session.publish(|_, text, _| assert_eq!(text, "old final"));
        assert_eq!(session.preview(), Some(Preview::Segment("latest draft")));
        assert_eq!(session.preview(), None);
        session.receive(&json!({"type":"transcript.final","utterance_id":"b",
            "revision":1,"text":"latest final"}), 300, Duration::ZERO).unwrap();
        session.publish(|_, text, _| assert_eq!(text, "latest final"));
        assert_eq!(session.preview(), None);
    }

    #[test]
    fn identified_segment_reuses_anonymous_stop_and_preserves_recovery_boundary() {
        let mut session = Session::new(RATE, true);
        session.receive(&json!({"type":"speech.stopped"}), 100, Duration::ZERO).unwrap();
        session.receive(&json!({"type":"transcript.segment","utterance_id":"a",
            "revision":1,"text":"preview"}), 150, Duration::ZERO).unwrap();
        assert_eq!(session.pieces.len(), 1);
        assert_eq!(session.preview(), Some(Preview::Segment("preview")));
        session.receive(&json!({"type":"transcript.final","utterance_id":"a",
            "revision":1,"text":"final"}), 200, Duration::ZERO).unwrap();
        let mut emitted = Vec::new();
        session.publish(|index, text, end| emitted.push((index, text.to_owned(), end)));
        assert_eq!(emitted, [(0, "final".to_owned(), 100)]);
        assert_eq!(session.preview(), None);
    }

    #[test]
    fn nonlive_finals_keep_highest_revision() {
        let mut session = Session::new(RATE, false);
        for (revision, text) in [(1, "first"), (3, "highest"), (2, "stale")] {
            session
                .receive(
                    &json!({"type":"transcript.final","utterance_id":"a",
                "revision":revision,"text":text}),
                    100,
                    Duration::ZERO,
                )
                .unwrap();
        }
        session.publish(|_, text, _| {
            assert_eq!(text, "highest")
        });
    }

    #[test]
    fn session_failures_and_rotation() {
        let mut s = Session::new(RATE, false);
        let mut receive = |e: Value| s.receive(&e, 0, Duration::ZERO);
        assert!(receive(json!({"type":"transcript.failed"})).is_err());
        assert!(receive(json!({"type":"session.error","fatal":false})).is_ok());
        assert!(receive(json!({"type":"session.error","fatal":true})).is_err());
        receive(json!({"type":"session.stop_requested"})).unwrap();
        assert!(!s.rotating(1));
        assert_eq!(s.stop_request.as_deref(), Some("stop_requested"));
        let mut s = Session::new(RATE, false);
        let stop = json!({"type":"speech.stopped"});
        s.receive(&stop, 240 * RATE - 1, Duration::ZERO).unwrap();
        assert!(!s.rotating(240 * RATE));
        s.receive(&stop, 240 * RATE, Duration::ZERO).unwrap();
        assert!(s.rotating(240 * RATE));
        let s = Session::new(RATE, false);
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
        let mut s = Session::new(RATE, false);
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
            if ending == Ending::Disconnect {
                // Drop after reading the client's backlog, before the closing acknowledgement.
                return;
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
    fn queued_revisions_coalesce_without_overwriting_finals() {
        let updates = Updates::default();
        updates.push(Update::Segment("old".into()));
        updates.push(Update::Segment("latest".into()));
        assert_eq!(updates.next(true), Some(Update::Segment("latest".into())));
        for index in 0..2 {
            updates.push(Update::Segment("pending".into()));
            updates.push(Update::Final(index, index.to_string(), index + 1));
        }
        updates.push(Update::Segment("next".into()));
        for index in 0..2 {
            assert_eq!(updates.next(true), Some(Update::Final(index, index.to_string(), index + 1)));
        }
        assert_eq!(updates.next(true), Some(Update::Segment("next".into())));
    }

    #[test]
    fn buffered_native_samples_arrive_in_order_before_typing_consumes_any_updates() {
        let samples: Vec<_> = (0..5333).map(|i| (i as f32 - 2500.0) / 8192.0).collect();
        let expected: Vec<_> = samples.iter().map(|x| (x * 32767.0).round() as i16).collect();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.set_read_timeout(Some(Duration::from_millis(10))).unwrap();
        let peer = listener.accept().unwrap().0;
        let (buffered, captured) = mpsc::channel();
        let server = std::thread::spawn(move || {
            peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ws = WebSocket::from_raw_socket(peer, Role::Server, None);
            let config: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(config["config"]["sample_rate_hz"], RATE);
            captured.recv_timeout(Duration::from_secs(5)).unwrap();
            ws.send(Message::text(r#"{"type":"session.started"}"#)).unwrap();
            let mut received = Vec::new();
            loop {
                let packet: Value = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
                if packet["type"] == "session.close" { break; }
                let bytes = STANDARD.decode(packet["audio"].as_str().unwrap()).unwrap();
                let size = packet_size(received.len()).min(expected.len() + RATE / 4 - received.len());
                assert_eq!(bytes.len(), size * 2);
                received.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
            }
            assert_eq!(&received[..expected.len()], expected);
            assert_eq!(&received[expected.len()..], vec![0; RATE / 4]);
            for event in [
                json!({"type":"transcript.segment","utterance_id":"a","revision":1,"text":"draft"}),
                json!({"type":"transcript.final","utterance_id":"a","revision":2,"text":"final"}),
                json!({"type":"session.updated","session":{"status":"closed"}})
            ] { ws.send(Message::text(event.to_string())).unwrap(); }
        });
        let socket = WebSocket::from_raw_socket(MaybeTlsStream::Plain(Route::Plain(client)), Role::Client, None);
        let mut connection = Some(begin(socket, Instant::now(), RATE).unwrap());
        let audio = SegmentAudio::synthetic(samples, RATE as u32, true);
        buffered.send(()).unwrap();
        let updates = Updates::default();
        let mut progress = Progress { count: 0, end: 0 };
        sessions(&audio, true, &|| true, &|| false, &mut progress, &updates,
            Instant::now(), Instant::now(), &mut 0, || Ok(connection.take().unwrap())).unwrap();
        server.join().unwrap();
        assert_eq!(updates.next(true), Some(Update::Final(0, "final".into(), 5333)));
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
    fn noise_setting_preserves_native_stream_rate_and_recovery_boundary() {
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
        let updates = Updates::default();
        let mut rotations = 0;
        with_fallback(
            |progress| {
                sessions(
                    &audio,
                    true,
                    &|| ending != Ending::StopRequest,
                    &|| false,
                    progress,
                    &updates,
                    Instant::now(),
                    Instant::now(),
                    &mut rotations,
                    || {
                        assert!(server.borrow().is_none(), "unexpected second session");
                        if !interrupted {
                            return Err("連線失敗");
                        }
                        let expected_rate = RATE;
                        let (socket, worker) = peer_after_first(ending, expected_rate);
                        assert!(server.replace(Some(worker)).is_none());
                        begin(socket, Instant::now(), expected_rate)
                    },
                )
            },
            |progress, reason| {
                if interrupted {
                    let Some(Update::Final(index, text, end)) = updates.next(true) else { panic!("missing final") };
                    assert_eq!((index, end), (0, RATE / 2));
                    let mut t = typing.lock().unwrap();
                    t.add(&text, end);
                    let next = t.next().unwrap();
                    t.note_typed(&next);
                    requests.replace_sentences(&[text], |_| {});
                }
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
