//! Microphone capture: records the chosen input and hands back 16 kHz mono WAV bytes.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

mod denoise;
mod segments;
use segments::Segmenter;

const TARGET_RATE: u32 = 16_000;
const DAMAGED: &str = "錄音資料損毀";
const INTERRUPTED: &str = "錄音中斷";
pub const MICROPHONE_LOST: &str = "麥克風在錄音中斷線";
const MICROPHONE_LOST_EMPTY: &str = "麥克風在錄音中斷線，沒有可辨識的內容";
/// Recording stops by itself after ten minutes.
pub const LONGEST_SECONDS: usize = 600;

pub struct Recorder {
    stop: Sender<()>,
    worker: JoinHandle<Result<FinishedCapture, String>>,
    info: CaptureInfo,
    failed: Arc<AtomicBool>,
    /// Set when the chosen microphone could not be used and the Windows default records instead.
    substitute: Option<String>,
}

pub struct CaptureInfo {
    pub name: String,
    pub rate: u32,
    pub channels: u16,
    pub format: cpal::SampleFormat,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CaptureLevels {
    pub captured_ms: usize,
    pub kept_ms: usize,
    pub peak_dbfs: i32,
    pub rms_dbfs: i32,
    pub gain_db: i32,
    pub speech_detected: bool,
}

pub struct FinishedCapture {
    pub levels: CaptureLevels,
    pub failed: bool,
    pub noise_suppression: bool,
    pub wav: Result<Vec<u8>, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Segment {
    pub recording_id: u64,
    pub sequence: usize,
    pub start: usize,
    pub end: usize,
}

pub enum SegmentEvent {
    Ready(Segment),
    Incomplete,
}

#[derive(Clone)]
pub struct SegmentAudio {
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    noise_suppression: bool,
}

impl SegmentAudio {
    #[cfg(test)]
    pub fn synthetic(samples: Vec<f32>, rate: u32, noise_suppression: bool) -> Self {
        Self { samples: Arc::new(Mutex::new(samples)), rate, noise_suppression }
    }

    pub fn len(&self) -> usize {
        self.samples.lock().unwrap().len()
    }

    pub fn stream_rate(&self) -> usize {
        self.rate as usize
    }

    /// Stream and fallback boundaries both use native mono sample positions.
    pub fn pcm(&self, range: std::ops::Range<usize>) -> Vec<i16> {
        let samples = self.samples.lock().unwrap()[range].to_vec();
        samples.into_iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16).collect()
    }

    pub fn resume_local(&self, from: usize, sequence: usize, listener: SegmentListener,
                        stopped: impl Fn() -> bool, cancelled: impl Fn() -> bool) {
        let (id, notify) = listener;
        let audio = self.clone();
        let mut reader = SegmentReader::new(self.rate, id, Box::new(move |mut event| {
            if let SegmentEvent::Ready(piece) = &mut event {
                piece.start += from;
                piece.end += from;
                piece.sequence += sequence;
            }
            notify(event, &audio);
        }));
        reader.cursor = from;
        while !cancelled() {
            let done = stopped();
            let end = self.len();
            if done {
                let _ = reader.finish(&self.samples, end);
                break;
            }
            let Ok(next) = reader.read(&self.samples, end) else { break };
            reader = next;
            thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn encode(&self, range: std::ops::Range<usize>) -> Result<Vec<u8>, String> {
        let samples = self.samples.lock().map_err(|_| DAMAGED.to_string())?
            .get(range).ok_or_else(|| DAMAGED.to_string())?.to_vec();
        let rms = sound(&samples, self.rate).map(|(start, end)| levels(&samples[start..end]).1);
        Ok(encode(&samples, self.rate, rms, self.noise_suppression).0)
    }

    /// How long this many samples of the recording last.
    pub fn millis(&self, samples: usize) -> usize {
        samples * 1000 / (self.rate as usize).max(1)
    }

    /// The recording from sample `start` to its end, trimmed like a whole recording; None when too
    /// little sound is left there to be worth recognising. Call it after the recording has stopped.
    pub fn encode_from(&self, start: usize) -> Result<Option<Vec<u8>>, String> {
        let samples = self.samples.lock().map_err(|_| DAMAGED.to_string())?
            .get(start..).ok_or_else(|| DAMAGED.to_string())?.to_vec();
        let Some((first, last)) = sound(&samples, self.rate) else { return Ok(None) };
        if shorter_than_word(last - first, self.rate) {
            return Ok(None);
        }
        let rms = levels(&samples[first..last]).1;
        let kept = trim_with_sound(&samples, self.rate, Some((first, last)));
        Ok(Some(encode(kept, self.rate, Some(rms), self.noise_suppression).0))
    }
}

type SegmentNotice = Box<dyn Fn(SegmentEvent) + Send>;
pub type SegmentListener = (u64, Box<dyn Fn(SegmentEvent, &SegmentAudio) + Send>);

pub enum SentenceSource {
    Local(SegmentListener),
    Stream(Sender<u32>, Box<dyn FnOnce(SegmentAudio, mpsc::Receiver<()>, Instant) + Send>),
}

impl Recorder {
    /// Starts recording from the microphone with this name, or the Windows default when None,
    /// and fails when that microphone cannot be used: the microphone test checks this one only.
    /// `level` receives the peak of each buffer, from 0 to 1.
    pub fn start(microphone: Option<String>, level: impl Fn(f32) + Send + 'static) -> Result<Self, String> {
        Self::start_capture(move || open(microphone.as_deref(), level, None).map(|opened| (opened, None)), false, None, || {})
    }

    /// Starts a dictation. When the chosen microphone is unplugged or cannot be opened, the Windows
    /// default records instead and `substitute` says so; the choice itself is left as it is, so the
    /// next dictation goes back to that microphone once it is there again.
    pub fn start_with_segments(
        microphone: Option<String>,
        noise_suppression: bool,
        level: impl Fn(f32) + Send + Sync + 'static,
        segments: Option<SentenceSource>,
        on_failure: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let rate = match &segments {
            Some(SentenceSource::Stream(rate, _)) => Some(rate.clone()),
            _ => None,
        };
        Self::start_capture(move || open_or_default(microphone.as_deref(), level, rate.as_ref()), noise_suppression, segments, on_failure)
    }

    /// What the user is told when the chosen microphone gave way to the default.
    pub fn substitute(&self) -> Option<&str> {
        self.substitute.as_deref()
    }

    pub fn info(&self) -> &CaptureInfo {
        &self.info
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    fn start_capture<Stream>(
        open: impl FnOnce() -> Result<(Opened<Stream>, Option<String>), String> + Send + 'static,
        noise_suppression: bool,
        segments: Option<SentenceSource>,
        on_failure: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready, started) = mpsc::channel();
        let worker = thread::spawn(move || -> Result<FinishedCapture, String> {
            // The stream is not Send, so the thread that opens it also owns and closes it.
            let (Opened { stream, samples, info, failed, began }, substitute) = match open() {
                Ok(parts) => parts,
                Err(reason) => {
                    let _ = ready.send(Err(reason.clone()));
                    return Err(reason);
                }
            };
            let rate = info.rate;
            let _ = ready.send(Ok((info, substitute, failed.clone())));
            let audio = SegmentAudio { samples: samples.clone(), rate, noise_suppression };
            let mut remote = None;
            let mut reader = match segments {
                Some(SentenceSource::Local((recording_id, notify))) => Some(
                    SegmentReader::new(rate, recording_id, Box::new(move |event| notify(event, &audio)))
                ),
                Some(SentenceSource::Stream(_, process)) => {
                    let (finish, ending) = mpsc::channel();
                    remote = Some((finish, thread::spawn(move || process(audio, ending, began))));
                    None
                }
                None => None,
            };
            while !failed.load(Ordering::Relaxed) {
                match stopped.recv_timeout(Duration::from_millis(20)) {
                    Ok(()) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Err(INTERRUPTED.into()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if let Some(current) = reader.take() {
                    let end = samples.lock().map_err(|_| DAMAGED.to_string())?.len();
                    reader = Some(current.read(&samples, end)?);
                }
            }
            drop(stream);
            let failed = failed.load(Ordering::Relaxed);
            if failed {
                on_failure();
            }
            let end = samples.lock().map_err(|_| DAMAGED.to_string())?.len();
            if let Some(reader) = reader {
                reader.finish(&samples, end)?;
            }
            if let Some((finish, worker)) = remote {
                let _ = finish.send(());
                drop(finish);
                worker.join().map_err(|_| INTERRUPTED.to_string())?;
            }
            let captured = samples.lock().map_err(|_| DAMAGED.to_string())?;
            Ok(encode_recording(&captured[..end], rate, failed, noise_suppression))
        });
        match started.recv() {
            Ok(Ok((info, substitute, failed))) => Ok(Self { stop, worker, info, substitute, failed }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => Err("無法啟動錄音".into()),
        }
    }

    /// Stops recording and returns the WAV and measurements, even when the clip is too short.
    pub fn finish(self) -> Result<FinishedCapture, String> {
        let _ = self.stop.send(());
        self.worker.join().map_err(|_| INTERRUPTED.to_string())?
    }
}

struct SegmentReader {
    segmenter: Segmenter,
    recording_id: u64,
    sequence: usize,
    cursor: usize,
    notify: SegmentNotice,
}

impl SegmentReader {
    fn new(rate: u32, recording_id: u64, notify: SegmentNotice) -> Self {
        Self { segmenter: Segmenter::new(rate), recording_id, sequence: 0, cursor: 0, notify }
    }

    fn read(mut self, samples: &Mutex<Vec<f32>>, end: usize) -> Result<Self, String> {
        while self.cursor < end {
            let next = (self.cursor + 8192).min(end);
            let batch = samples.lock().map_err(|_| DAMAGED.to_string())?[self.cursor..next].to_vec();
            self.cursor = next;
            let had_gap = self.segmenter.incomplete();
            let (segmenter, segments) = self.segmenter.push(&batch);
            self.segmenter = segmenter;
            if !had_gap && self.segmenter.incomplete() {
                (self.notify)(SegmentEvent::Incomplete);
            }
            for range in segments {
                self.send(range);
            }
        }
        Ok(self)
    }

    fn send(&mut self, range: std::ops::Range<usize>) {
        self.sequence += 1;
        (self.notify)(SegmentEvent::Ready(Segment {
            recording_id: self.recording_id,
            sequence: self.sequence,
            start: range.start,
            end: range.end,
        }));
    }

    fn finish(self, samples: &Mutex<Vec<f32>>, end: usize) -> Result<(), String> {
        let Self { segmenter, recording_id, sequence, notify, .. } = self.read(samples, end)?;
        let had_gap = segmenter.incomplete();
        let (tail, incomplete) = segmenter.finish();
        if incomplete && !had_gap {
            notify(SegmentEvent::Incomplete);
        }
        if let Some(range) = tail {
            notify(SegmentEvent::Ready(Segment {
                recording_id, sequence: sequence + 1, start: range.start, end: range.end,
            }));
        }
        Ok(())
    }
}

fn encode_recording(samples: &[f32], rate: u32, failed: bool, noise_suppression: bool) -> FinishedCapture {
    let speech = sound(samples, rate);
    let measured = speech.map_or(samples, |(start, end)| &samples[start..end]);
    let (peak, rms) = levels(measured);
    // A finite floor keeps silent and empty recordings numeric in the report.
    let dbfs = |value: f64| (20.0 * value.max(1e-6).log10()).round() as i32;
    let millis = |len: usize| (len as u64 * 1000 / u64::from(rate)) as usize;
    let too_short = shorter_than_word(measured.len(), rate);
    let reason = if failed && (speech.is_none() || too_short) {
        Some(MICROPHONE_LOST_EMPTY)
    } else if too_short {
        Some(TOO_SHORT)
    } else {
        None
    };
    let kept = if reason.is_some() { &[][..] } else { trim_with_sound(samples, rate, speech) };
    let (wav, gain_db) = match reason {
        Some(reason) => (Err(reason.into()), 0),
        None => {
            let (wav, gain_db) = encode(kept, rate, speech.map(|_| rms), noise_suppression);
            (Ok(wav), gain_db)
        }
    };
    FinishedCapture {
        levels: CaptureLevels {
            captured_ms: millis(samples.len()),
            kept_ms: millis(kept.len()),
            peak_dbfs: dbfs(peak),
            rms_dbfs: dbfs(rms),
            gain_db,
            speech_detected: speech.is_some(),
        },
        failed,
        noise_suppression,
        wav,
    }
}

struct Opened<Stream = cpal::Stream> {
    began: Instant,
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    info: CaptureInfo,
    failed: Arc<AtomicBool>,
}

/// The names of the microphones Windows can record from right now.
pub fn microphones() -> Vec<String> {
    cpal::default_host().input_devices().map(|devices| devices.filter_map(|device| device.name().ok()).collect()).unwrap_or_default()
}

fn open(microphone: Option<&str>, level: impl Fn(f32) + Send + 'static, rate: Option<&Sender<u32>>) -> Result<Opened, String> {
    let host = cpal::default_host();
    let device = match microphone {
        Some(name) => find(&host, name).ok_or_else(|| missing(name))?,
        None => host.default_input_device().ok_or("找不到麥克風")?,
    };
    open_device(&device, level, rate)
}

/// Opens the chosen microphone, or the system default when the chosen one is not there or will not open.
fn open_or_default(microphone: Option<&str>, level: impl Fn(f32) + Send + Sync + 'static, rate: Option<&Sender<u32>>) -> Result<(Opened, Option<String>), String> {
    let Some(name) = microphone else {
        return open(None, level, rate).map(|opened| (opened, None));
    };
    let level = Arc::new(level);
    let host = cpal::default_host();
    let shared = level.clone();
    let default = host.default_input_device();
    chosen_or_default(
        name,
        || find(&host, name).ok_or_else(|| missing(name)).and_then(|device| open_device(&device, move |peak| (*shared)(peak), rate)),
        default.as_ref().and_then(|device| device.name().ok()),
        || default.as_ref().ok_or_else(String::new).and_then(|device| open_device(device, move |peak| (*level)(peak), rate)),
    )
}

/// Tries the chosen microphone, then the default one unless it is the same microphone. A failure of the
/// default is reported as the chosen microphone's, since that is the one the user asked for.
fn chosen_or_default<T>(
    chosen: &str,
    open_chosen: impl FnOnce() -> Result<T, String>,
    default: Option<String>,
    open_default: impl FnOnce() -> Result<T, String>,
) -> Result<(T, Option<String>), String> {
    let reason = match open_chosen() {
        Ok(opened) => return Ok((opened, None)),
        Err(reason) => reason,
    };
    let Some(used) = default.filter(|used| used != chosen) else { return Err(reason) };
    let opened = open_default().map_err(|_| reason)?;
    Ok((opened, Some(substitute_note(chosen, &used))))
}

fn find(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    host.input_devices().ok()?.find(|device| device.name().is_ok_and(|found| found == name))
}

fn missing(name: &str) -> String {
    format!("{name}未連接，請開語音設定")
}

fn substitute_note(wanted: &str, used: &str) -> String {
    format!("{wanted}目前不能用，改用 {} 預設的{used}", crate::sys::SYSTEM)
}

pub fn low_rate_note(rate: u32) -> Option<String> {
    (rate <= 16_000).then(|| {
        let khz = f64::from(rate) / 1000.0;
        format!("這支麥克風目前是通話音質（{khz} kHz），辨識可能較不準。可改用電腦或 USB 麥克風")
    })
}

fn open_device(device: &cpal::Device, level: impl Fn(f32) + Send + 'static, rate: Option<&Sender<u32>>) -> Result<Opened, String> {
    let config = device.default_input_config().map_err(|_| "無法讀取麥克風格式")?;
    let info = CaptureInfo {
        name: device.name().unwrap_or_else(|_| "未知".into()),
        rate: config.sample_rate().0,
        channels: config.channels(),
        format: config.sample_format(),
    };
    if let Some(rate) = rate { let _ = rate.send(info.rate); }
    let channels = usize::from(info.channels.max(1));
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = samples.clone();
    let limit = info.rate as usize * LONGEST_SECONDS;
    let failed = Arc::new(AtomicBool::new(false));
    let failure = failed.clone();
    // Mixes each frame down to mono and reports how loud the buffer was.
    let keep = move |frames: &mut dyn Iterator<Item = f32>| {
        let Ok(mut held) = sink.lock() else { return };
        if failure.load(Ordering::Relaxed) {
            return;
        }
        let peak = capture_mono(&mut held, frames, channels, limit);
        drop(held);
        level(peak.min(1.0));
    };
    let failure = failed.clone();
    let mark_failed = move |_: cpal::StreamError| failure.store(true, Ordering::Relaxed);
    let stream = match info.format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| keep(&mut data.iter().copied()),
            mark_failed,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| keep(&mut data.iter().map(|&s| f32::from(s) / 32768.0)),
            mark_failed,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &cpal::InputCallbackInfo| keep(&mut data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0)),
            mark_failed,
            None,
        ),
        _ => return Err("不支援這支麥克風的取樣格式".into()),
    }
    .map_err(|_| format!("無法開啟麥克風，請確認 {} 的麥克風權限", crate::sys::SYSTEM))?;
    let began = Instant::now();
    stream.play().map_err(|_| "無法開始錄音")?;
    Ok(Opened { stream, samples, info, failed, began })
}

fn capture_mono(samples: &mut Vec<f32>, frames: impl Iterator<Item = f32>, channels: usize, limit: usize) -> f32 {
    let mut peak = 0f32;
    let mut sum = 0f32;
    let mut count = 0usize;
    for sample in frames {
        sum += sample;
        count += 1;
        if count == channels {
            let mono = sum / channels as f32;
            peak = peak.max(mono.abs());
            if samples.len() < limit {
                samples.push(mono);
            }
            sum = 0.0;
            count = 0;
        }
    }
    peak
}

/// Where the sound starts and ends, in samples, to the nearest 20 ms. None when nothing stands out.
fn sound(samples: &[f32], rate: u32) -> Option<(usize, usize)> {
    let window = sound_window(rate);
    let loudest = samples.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
    // Speech stands well above the room; the floor keeps a silent recording from being cut to nothing.
    let threshold = sound_threshold(loudest);
    let speaking = |chunk: &[f32]| chunk.iter().any(|sample| sample.abs() > threshold);
    let chunks = samples.chunks(window);
    let (first, last) = (chunks.clone().position(speaking)?, chunks.clone().rposition(speaking)?);
    Some((first * window, ((last + 1) * window).min(samples.len())))
}

fn sound_window(rate: u32) -> usize {
    (rate as usize / 50).max(1)
}

fn sound_threshold(loudest: f32) -> f32 {
    (loudest * 0.08).max(0.004)
}

const TOO_SHORT: &str = "錄音太短，請再說一次";

/// Sound shorter than this is a key press, a click or a cough, not a word. A single spoken
/// syllable such as 「好」 runs longer, so it still goes through.
const SHORTEST_SOUND_MS: usize = 200;

/// Whether there is too little here to be worth recognising: a brush of the shortcut, or one brief
/// noise in an otherwise quiet recording. Recognisers tend to make words up from such clips.
/// A longer recording in which nothing stands out is not judged here; the recogniser answers with no text.
fn too_short(samples: &[f32], rate: u32, speech: Option<(usize, usize)>) -> bool {
    let length = speech.map_or(samples.len(), |(start, end)| end - start);
    shorter_than_word(length, rate)
}

fn shorter_than_word(length: usize, rate: u32) -> bool {
    length < (u64::from(rate) * SHORTEST_SOUND_MS as u64 / 1000) as usize
}

/// Drops the silence before and after speech, keeping a quarter second on each side.
/// Less audio means a smaller upload and a faster answer; a recording with no clear speech is left whole.
#[cfg(test)]
fn trim(samples: &[f32], rate: u32) -> &[f32] {
    trim_with_sound(samples, rate, sound(samples, rate))
}

fn trim_with_sound(samples: &[f32], rate: u32, speech: Option<(usize, usize)>) -> &[f32] {
    let Some((first, last)) = speech else { return samples };
    let margin = rate as usize / 4;
    let start = first.saturating_sub(margin);
    let end = (last + margin).min(samples.len());
    if end - start < rate as usize * 3 / 10 {
        return samples;
    }
    &samples[start..end]
}

fn levels(samples: &[f32]) -> (f64, f64) {
    let (peak, squares) = samples.iter().fold((0.0f64, 0.0), |(peak, squares), &sample| {
        let value = f64::from(sample);
        (peak.max(value.abs()), squares + value * value)
    });
    (peak, (squares / samples.len().max(1) as f64).sqrt())
}

fn encode(samples: &[f32], rate: u32, speech_rms: Option<f64>, noise_suppression: bool) -> (Vec<u8>, i32) {
    if noise_suppression {
        let cleaned = denoise::suppress(&resample(samples, rate));
        encode_with_gain(&cleaned, TARGET_RATE, speech_rms)
    } else {
        encode_with_gain(samples, rate, speech_rms)
    }
}

fn encode_with_gain(samples: &[f32], rate: u32, speech_rms: Option<f64>) -> (Vec<u8>, i32) {
    let Some(rms) = speech_rms else { return (wav(&resample(samples, rate)), 0) };
    let gain = (0.1 / rms).clamp(1.0, 10.0);
    let mut gained = if gain > 1.0 {
        Cow::Owned(samples.iter().map(|&sample| (f64::from(sample) * gain) as f32).collect())
    } else {
        Cow::Borrowed(samples)
    };
    if exceeds_peak_limit(&gained) {
        limit_peaks(gained.to_mut());
    }
    let mut output = resample(&gained, rate);
    // The anti-aliasing filter can overshoot its input peaks.
    if rate > TARGET_RATE && exceeds_peak_limit(&output) {
        limit_peaks(&mut output);
    }
    (wav(&output), (20.0 * gain.log10()).round() as i32)
}

const PEAK_LIMIT: f32 = 0.891_250_9; // -1 dBFS

fn exceeds_peak_limit(samples: &[f32]) -> bool {
    samples.iter().any(|sample| sample.abs() > PEAK_LIMIT)
}

fn limit_peaks(samples: &mut [f32]) {
    const KNEE: f32 = 0.8;
    let headroom = PEAK_LIMIT - KNEE;
    for sample in samples {
        let excess = sample.abs() - KNEE;
        if excess > 0.0 {
            *sample = (PEAK_LIMIT - headroom / (1.0 + excess / headroom)).copysign(*sample);
        }
    }
}

fn resample(samples: &[f32], rate: u32) -> Vec<f32> {
    if rate == TARGET_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let step = f64::from(rate) / f64::from(TARGET_RATE);
    let length = (samples.len() as u64 * u64::from(TARGET_RATE) / u64::from(rate)) as usize;
    if rate > TARGET_RATE {
        return downsample(samples, rate, step, length);
    }
    (0..length)
        .map(|i| {
            let at = i as f64 * step;
            let left = at as usize;
            let right = (left + 1).min(samples.len() - 1);
            let mix = (at - left as f64) as f32;
            samples[left] * (1.0 - mix) + samples[right] * mix
        })
        .collect()
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn downsample(samples: &[f32], rate: u32, step: f64, length: usize) -> Vec<f32> {
    use std::f64::consts::PI;

    if length == 0 {
        return Vec::new();
    }
    let cutoff = 7_400.0 / f64::from(rate);
    let radius = ((12.0 * step).ceil() as usize).min(512);
    let taps = 2 * radius + 1;
    // Use exact phases for common device rates, with bounded tables for unusual WAV rates.
    let phases = (TARGET_RATE / gcd(rate, TARGET_RATE)).min(256);
    let mut kernels = Vec::with_capacity(phases as usize * taps);
    for phase in 0..phases {
        let frac = f64::from(phase) / f64::from(phases);
        let start = kernels.len();
        let mut sum = 0.0;
        for tap in 0..taps {
            let offset = tap as f64 - radius as f64 - frac;
            let sinc_at = 2.0 * PI * cutoff * offset;
            let sinc = if sinc_at == 0.0 { 1.0 } else { sinc_at.sin() / sinc_at };
            let angle = PI * offset / radius as f64;
            let window = if offset.abs() <= radius as f64 {
                0.42 + 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos()
            } else {
                0.0
            };
            let weight = 2.0 * cutoff * sinc * window;
            sum += weight;
            kernels.push(weight as f32);
        }
        for weight in &mut kernels[start..] {
            *weight /= sum as f32;
        }
    }

    (0..length).map(|i| {
        let at = i as u64 * u64::from(rate);
        let center = (at / u64::from(TARGET_RATE)) as usize;
        let remainder = (at % u64::from(TARGET_RATE)) as u32;
        let phase = (remainder * phases / TARGET_RATE) as usize;
        let kernel = &kernels[phase * taps..][..taps];
        if center >= radius && center + radius < samples.len() {
            let frame = &samples[center - radius..][..taps];
            frame.iter().zip(kernel).map(|(sample, weight)| sample * weight).sum()
        } else {
            kernel.iter().enumerate().map(|(tap, weight)| {
                let index = (center + tap).saturating_sub(radius).min(samples.len() - 1);
                samples[index] * weight
            }).sum()
        }
    }).collect()
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&TARGET_RATE.to_le_bytes());
    out.extend_from_slice(&(TARGET_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Turns any PCM or float WAV file into the 16 kHz mono form the recognisers expect.
pub fn normalise(file: &[u8], noise_suppression: bool) -> Result<Vec<u8>, String> {
    let unreadable = || "無法讀取這個 WAV 音檔".to_string();
    if file.len() < 12 || &file[0..4] != b"RIFF" || &file[8..12] != b"WAVE" {
        return Err(unreadable());
    }
    let (mut format, mut data) = (None, None);
    let mut at = 12;
    while at + 8 <= file.len() {
        let size = u32::from_le_bytes(file[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &file[at + 8..(at + 8 + size).min(file.len())];
        match &file[at..at + 4] {
            b"fmt " if body.len() >= 16 => format = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + size + (size & 1);
    }
    let (format, data) = (format.ok_or_else(unreadable)?, data.ok_or_else(unreadable)?);
    let word = |at: usize| u16::from_le_bytes([format[at], format[at + 1]]);
    let mut kind = word(0);
    let channels = usize::from(word(2).max(1));
    let rate = u32::from_le_bytes(format[4..8].try_into().unwrap());
    let bits = word(14);
    if kind == 0xFFFE && format.len() >= 26 {
        kind = word(24); // extensible files name the real format in a sub-type
    }
    let width = usize::from(bits / 8);
    let sample = |bytes: &[u8]| -> Option<f32> {
        Some(match (kind, bits) {
            (1, 8) => (f32::from(bytes[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            _ => return None,
        })
    };
    if rate == 0 || width == 0 || sample(&[0; 4]).is_none() {
        return Err("不支援這個 WAV 的格式，請轉成 PCM".into());
    }
    let mono: Vec<f32> = data
        .chunks_exact(width * channels)
        .map(|frame| frame.chunks_exact(width).filter_map(&sample).sum::<f32>() / channels as f32)
        .collect();
    if mono.is_empty() {
        return Err("這個音檔沒有聲音".into());
    }
    let speech = sound(&mono, rate);
    if too_short(&mono, rate, speech) {
        return Err(TOO_SHORT.into());
    }
    let rms = speech.map(|(start, end)| levels(&mono[start..end]).1);
    Ok(encode(&mono, rate, rms, noise_suppression).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_input<Stream>(stream: Stream, samples: Arc<Mutex<Vec<f32>>>, rate: u32) -> Opened<Stream> {
        Opened {
            began: Instant::now(),
            stream,
            samples,
            info: CaptureInfo { name: "測試麥克風".into(), rate, channels: 2, format: cpal::SampleFormat::I16 },
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn reports_input_format_and_measures_speech_without_trim_margins() {
        let rate = 16_000;
        let mut samples = vec![0.001; rate as usize * 2];
        samples.extend(std::iter::repeat_n(0.5, rate as usize / 2));
        samples.extend(std::iter::repeat_n(-0.25, rate as usize / 2));
        samples.resize(rate as usize * 6, 0.001);
        let recorder = Recorder::start_capture(
            move || Ok((test_input((), Arc::new(Mutex::new(samples)), rate), None)),
            false,
            None,
            || {},
        ).unwrap();
        let info = recorder.info();
        assert_eq!((&*info.name, info.rate, info.channels, info.format), ("測試麥克風", 16_000, 2, cpal::SampleFormat::I16));
        let capture = recorder.finish().unwrap();
        assert_eq!(capture.levels, CaptureLevels {
            captured_ms: 6000, kept_ms: 1500, peak_dbfs: -6, rms_dbfs: -8, gain_db: 0, speech_detected: true,
        });
        assert_eq!(capture.wav.unwrap().len(), 44 + 24_000 * 2);
    }

    #[test]
    fn a_disconnect_keeps_speech_but_reports_an_error_for_silence_or_a_click() {
        let speech = vec![0.25; 4000];
        let saved = encode_recording(&speech, 16_000, true, false);
        assert!(saved.failed);
        assert_eq!(saved.wav, Ok(wav(&speech)));
        assert_eq!(saved.levels.kept_ms, 250);

        let mut click = vec![0.0; 16_000];
        click[8_000] = 0.5;
        for unusable in [&[][..], &vec![0.0; 16_000], &click] {
            let failed = encode_recording(unusable, 16_000, true, false);
            assert_eq!(failed.wav, Err("麥克風在錄音中斷線，沒有可辨識的內容".into()));
            assert_eq!(failed.levels.kept_ms, 0);
        }
        let silent = encode_recording(&[0.001; 16_000], 16_000, false, false);
        assert_eq!(silent.levels, CaptureLevels {
            captured_ms: 1000, kept_ms: 1000, peak_dbfs: -60, rms_dbfs: -60, gain_db: 0, speech_detected: false,
        });
        assert!(silent.wav.is_ok());
    }

    fn pcm_samples(wav: &[u8]) -> Vec<f32> {
        wav[44..].chunks_exact(2).map(|sample| {
            f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32767.0
        }).collect()
    }

    #[test]
    fn quiet_speech_uses_up_to_twenty_db_of_gain() {
        for (input, gain_db, expected_rms) in [
            (0.008, 20, 0.08),
            (0.01, 20, 0.1),
            (10f32.powf(-26.0 / 20.0), 6, 0.1),
        ] {
            let speech: Vec<_> = [input, -input].into_iter().cycle().take(16_000).collect();
            let capture = encode_recording(&speech, 16_000, false, false);
            assert!(capture.levels.speech_detected);
            assert_eq!(capture.levels.gain_db, gain_db);
            let (_, rms) = levels(&pcm_samples(&capture.wav.unwrap()));
            assert!((rms - expected_rms).abs() < 0.000_04, "RMS was {rms}");
        }
    }

    #[test]
    fn loud_speech_and_silence_only_keep_their_levels() {
        for (sample, speech_detected) in [(0.5, true), (0.0, false), (0.001, false)] {
            let samples = vec![sample; 16_000];
            let capture = encode_recording(&samples, 16_000, false, false);
            assert_eq!(capture.levels.speech_detected, speech_detected);
            assert_eq!(capture.levels.gain_db, 0);
            assert_eq!(capture.wav.unwrap(), wav(&samples));
        }
    }

    #[test]
    fn a_loud_transient_is_limited_when_quiet_speech_gets_gain() {
        for rate in [16_000, 48_000] {
            let mut samples = vec![0.08; rate as usize];
            samples[rate as usize / 2..rate as usize / 2 + 24].fill(-0.95);
            let capture = encode_recording(&samples, rate, false, false);
            assert!(capture.levels.gain_db > 0);
            let output = pcm_samples(&capture.wav.unwrap());
            let (peak, _) = levels(&output);
            assert!((0.8..=10f64.powf(-1.0 / 20.0)).contains(&peak), "peak was {peak}");
            assert!((output[100] - 0.1).abs() < 0.01);
        }
    }

    #[test]
    fn segments_and_wav_files_use_their_own_speech_level() {
        let quiet = vec![0.02; 16_000];
        let mut samples = vec![0.5; 16_000];
        samples.extend_from_slice(&quiet);
        let audio = SegmentAudio { samples: Arc::new(Mutex::new(samples)), rate: 16_000, noise_suppression: false };
        let expected = wav(&[0.1; 16_000]);
        assert_eq!(audio.encode(16_000..32_000).unwrap(), expected);
        assert_eq!(audio.encode_from(16_000).unwrap().unwrap(), expected);
        assert_eq!(normalise(&wav(&quiet), false).unwrap(), expected);
    }

    #[test]
    fn disabled_noise_suppression_encodes_the_same_wav_as_gain_alone() {
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000] {
            let samples: Vec<_> = (0..rate + 137)
                .map(|i| 0.06 * (i as f32 * 0.71).sin())
                .collect();
            let rms = Some(levels(&samples).1);
            assert_eq!(encode(&samples, rate, rms, false), encode_with_gain(&samples, rate, rms));
        }
    }

    #[test]
    fn noise_suppression_reduces_background_energy_without_changing_duration() {
        let mut seed = 23u32;
        for rate in [8_000, 16_000, 44_100, 48_000, 96_000] {
            let samples: Vec<_> = (0..rate + 137).map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = ((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 0.006;
                let t = i as f32 / rate as f32;
                let voice = if (0.3..0.7).contains(&t) {
                    0.2 * (std::f32::consts::TAU * 330.0 * t).sin()
                } else {
                    0.0
                };
                voice + noise
            }).collect();
            let audio = SegmentAudio { samples: Arc::new(Mutex::new(samples.clone())), rate, noise_suppression: false };
            let enabled = SegmentAudio { noise_suppression: true, ..audio.clone() };
            let range = 0..samples.len();
            let before = audio.encode(range.clone()).unwrap();
            let after = enabled.encode(range).unwrap();
            assert_eq!(after.len(), before.len(), "{rate} Hz");
            let before_pcm = pcm_samples(&before);
            let after_pcm = pcm_samples(&after);
            for noise_only in [1600..4000, 12_000..14_400] {
                let energy = |pcm: &[f32]| pcm[noise_only.clone()].iter().map(|sample| sample * sample).sum::<f32>();
                assert!(energy(&after_pcm) < energy(&before_pcm) / 4.0, "{rate} Hz, {noise_only:?}: {} vs {}", energy(&after_pcm), energy(&before_pcm));
            }
            assert!(levels(&after_pcm[6400..9600]).1 > 0.01, "{rate} Hz: lost the test tone");

            let raw_capture = encode_recording(&samples, rate, false, false);
            let clean_capture = encode_recording(&samples, rate, false, true);
            assert!(!raw_capture.noise_suppression && clean_capture.noise_suppression);
            assert_eq!(clean_capture.levels, raw_capture.levels);
            let raw_wav = raw_capture.wav.unwrap();
            let clean_wav = clean_capture.wav.unwrap();
            assert_eq!(clean_wav.len(), raw_wav.len());
            assert_ne!(clean_wav, raw_wav);
            assert_eq!(enabled.encode_from(0).unwrap().unwrap(), clean_wav);

            let import = wav(&resample(&samples, rate));
            let original = normalise(&import, false).unwrap();
            let cleaned = normalise(&import, true).unwrap();
            assert_eq!(cleaned.len(), original.len());
            assert_ne!(cleaned, original);
        }
    }

    #[test]
    fn noise_suppression_preserves_samples_at_both_ends() {
        let mut samples = vec![0.0; 16_137];
        let last = samples.len() - 1;
        samples[0] = 0.5;
        samples[last] = -0.5;
        let (original, _) = encode(&samples, TARGET_RATE, None, false);
        let (cleaned, _) = encode(&samples, TARGET_RATE, None, true);
        let original = pcm_samples(&original);
        let cleaned = pcm_samples(&cleaned);
        assert_eq!(cleaned.len(), original.len());
        for (actual, expected) in cleaned.iter().zip(&original) {
            assert!((actual - expected).abs() <= 1.0 / 32767.0);
        }
    }

    #[test]
    fn low_rate_microphone_note_reports_the_actual_rate() {
        for (rate, khz) in [(8_000, "8"), (11_025, "11.025"), (16_000, "16")] {
            assert_eq!(low_rate_note(rate), Some(format!(
                "這支麥克風目前是通話音質（{khz} kHz），辨識可能較不準。可改用電腦或 USB 麥克風"
            )));
        }
        for rate in [16_001, 44_100, 48_000] {
            assert_eq!(low_rate_note(rate), None);
        }
    }

    #[test]
    fn stream_failure_closes_capture_before_requesting_recognition() {
        struct Stream(Sender<()>);
        impl Drop for Stream {
            fn drop(&mut self) {
                self.0.send(()).unwrap();
            }
        }
        let (closed, stream_closed) = mpsc::channel();
        let (notify, notified) = mpsc::channel();
        let speech = vec![0.25; 4000];
        let opened_samples = Arc::new(Mutex::new(speech.clone()));
        let recorder = Recorder::start_capture(
            move || Ok((test_input(Stream(closed), opened_samples, 16_000), None)),
            false,
            None,
            move || {
                stream_closed.try_recv().unwrap();
                notify.send(()).unwrap();
            },
        ).unwrap();
        recorder.failed.store(true, Ordering::Relaxed);
        notified.recv_timeout(Duration::from_secs(2)).unwrap();
        let capture = recorder.finish().unwrap();
        assert!(capture.failed);
        assert_eq!(capture.wav, Ok(wav(&speech)));
    }

    #[test]
    fn a_missing_microphone_gives_way_to_the_default_and_says_which() {
        let opened = chosen_or_default("Bose 耳麥", || Err::<u8, _>(missing("Bose 耳麥")), Some("Realtek 麥克風".into()), || Ok(2));
        assert_eq!(opened, Ok((2, Some(substitute_note("Bose 耳麥", "Realtek 麥克風")))));
        assert!(substitute_note("Bose 耳麥", "Realtek 麥克風").contains("Realtek 麥克風"));
    }

    #[test]
    fn the_chosen_microphone_is_used_whenever_it_opens() {
        let opened = chosen_or_default("Bose 耳麥", || Ok(1u8), Some("Realtek 麥克風".into()), || panic!("the default was opened"));
        assert_eq!(opened, Ok((1, None)));
    }

    #[test]
    fn without_another_microphone_the_chosen_ones_problem_is_reported() {
        let none = chosen_or_default("Bose 耳麥", || Err::<u8, _>(missing("Bose 耳麥")), None, || Ok(2));
        assert_eq!(none, Err(missing("Bose 耳麥")));
        let same = chosen_or_default("Bose 耳麥", || Err::<u8, _>("無法開始錄音".into()), Some("Bose 耳麥".into()), || Ok(2));
        assert_eq!(same, Err("無法開始錄音".to_string()));
        let broken = chosen_or_default("Bose 耳麥", || Err::<u8, _>(missing("Bose 耳麥")), Some("Realtek 麥克風".into()), || Err("無法開始錄音".into()));
        assert_eq!(broken, Err(missing("Bose 耳麥")));
    }

    #[test]
    fn positions_arrive_during_capture_and_at_finish_without_changing_the_wav() {
        struct TailOnStop(Arc<Mutex<Vec<f32>>>, Vec<f32>);
        impl Drop for TailOnStop {
            fn drop(&mut self) {
                self.0.lock().unwrap().extend(&self.1);
            }
        }

        let rate = 44_100;
        let frame = rate as usize / 100;
        let mut input = vec![0.0; 30 * frame];
        input.extend(std::iter::repeat_n(-0.5, 20 * frame));
        input.resize(110 * frame, 0.0);
        let open = || {
            let samples = Arc::new(Mutex::new(input.clone()));
            move || Ok((test_input(TailOnStop(samples.clone(), vec![0.25; 21 * frame]), samples, rate), None))
        };
        let (send, received) = mpsc::channel();
        let listener: SegmentListener = (19, Box::new(move |event, audio| {
            let SegmentEvent::Ready(segment) = event else { panic!("unexpected gap") };
            let wav = audio.encode(segment.start..segment.end).unwrap();
            send.send((segment, wav)).unwrap();
        }));
        let recorder = Recorder::start_capture(open(), false, Some(SentenceSource::Local(listener)), || {}).unwrap();
        let first = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first.0, Segment { recording_id: 19, sequence: 1, start: 5 * frame, end: 75 * frame });
        assert_eq!(first.1, wav(&resample(&input[5 * frame..75 * frame], rate)));
        assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Empty));

        let segmented = recorder.finish().unwrap().wav;
        assert_eq!(received.try_recv().unwrap().0, Segment { recording_id: 19, sequence: 2, start: 85 * frame, end: 131 * frame });
        assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Disconnected));
        let full = Recorder::start_capture(open(), false, None, || {}).unwrap().finish().unwrap().wav;
        input.extend(std::iter::repeat_n(0.25, 21 * frame));
        assert_eq!(full, Ok(wav(&resample(trim(&input, rate), rate))));
        assert_eq!(segmented, full);

        let brief = move || Ok((test_input((), Arc::new(Mutex::new(vec![0.5; 18 * frame])), rate), None));
        let listener: SegmentListener = (20, Box::new(|event, _| {
            assert!(matches!(event, SegmentEvent::Incomplete));
        }));
        for option in [None, Some(SentenceSource::Local(listener))] {
            let result = Recorder::start_capture(brief, false, option, || {}).unwrap().finish().unwrap().wav;
            assert_eq!(result, Err(TOO_SHORT.to_string()));
        }
    }

    #[test]
    fn the_last_segment_stops_at_the_capture_limit() {
        let rate = 1_000u32;
        let limit = rate as usize * LONGEST_SECONDS;
        let mut input = Vec::new();
        capture_mono(&mut input, std::iter::repeat_n(0.0, limit - 250), 1, limit);
        assert_eq!(capture_mono(&mut input, std::iter::repeat_n(-0.5, 400), 1, limit), 0.5);
        assert_eq!(input.len(), 600_000);
        let (send, received) = mpsc::channel();
        let read = SegmentReader::new(rate, 42, Box::new(move |event| {
            let SegmentEvent::Ready(segment) = event else { panic!("unexpected gap") };
            send.send(segment).unwrap();
        }));
        read.finish(&Mutex::new(input), limit).unwrap();
        assert_eq!(received.try_recv().unwrap(), Segment { recording_id: 42, sequence: 1, start: 599_490, end: 600_000 });
        assert_eq!(received.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    }

    #[test]
    fn silence_around_speech_is_dropped() {
        let rate = 16_000;
        let mut recording = vec![0.001f32; rate as usize * 2];
        recording.extend(std::iter::repeat(0.5).take(rate as usize));
        recording.extend(std::iter::repeat(0.001).take(rate as usize * 3));
        // One second of speech keeps a quarter second on each side.
        assert_eq!(trim(&recording, rate).len(), rate as usize * 3 / 2);
        let quiet = vec![0.0f32; rate as usize];
        assert_eq!(trim(&quiet, rate).len(), quiet.len());
        assert!(trim(&[], rate).is_empty());
    }

    #[test]
    fn a_brief_noise_is_not_sent_but_a_single_word_is() {
        let rate = 16_000usize;
        let too_short = |samples: &[f32]| super::too_short(samples, rate as u32, sound(samples, rate as u32));
        let quiet_then = |milliseconds: usize| {
            let mut recording = vec![0.001f32; rate];
            recording.extend(std::iter::repeat_n(0.5, rate * milliseconds / 1000));
            recording.extend(std::iter::repeat_n(0.001, rate));
            recording
        };
        // A click or a key press.
        assert!(too_short(&quiet_then(60)));
        // One syllable.
        assert!(!too_short(&quiet_then(260)));
        // The shortcut brushed, with nothing said.
        assert!(too_short(&vec![0.001; rate / 10]));
        assert!(too_short(&[]));
        // A long quiet recording is left for the recogniser to answer.
        assert!(!too_short(&vec![0.0; rate * 2]));
    }

    #[test]
    fn other_wav_formats_become_16k_mono() {
        // One second of 8 kHz stereo 16-bit silence.
        let frames = 8_000u32;
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + frames * 4).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        for value in [1u16, 2] {
            file.extend_from_slice(&value.to_le_bytes());
        }
        file.extend_from_slice(&8_000u32.to_le_bytes());
        file.extend_from_slice(&32_000u32.to_le_bytes());
        for value in [4u16, 16] {
            file.extend_from_slice(&value.to_le_bytes());
        }
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(frames * 4).to_le_bytes());
        file.extend(std::iter::repeat(0u8).take(frames as usize * 4));
        let out = normalise(&file, false).unwrap();
        assert_eq!(out.len(), 44 + 16_000 * 2);
        assert_eq!(u32::from_le_bytes(out[24..28].try_into().unwrap()), 16_000);
        assert!(normalise(b"not a wave file", false).is_err());
    }

    #[test]
    fn resampling_keeps_duration() {
        let second: Vec<f32> = (0..48_000).map(|i| (i as f32 / 48_000.0).sin()).collect();
        assert_eq!(resample(&second, 48_000).len(), 16_000);
        assert_eq!(resample(&second[..44_100], 44_100).len(), 16_000);
        assert_eq!(resample(&second, 16_000).len(), 48_000);
    }

    fn tone(rate: u32, frequency: u32) -> Vec<f32> {
        (0..rate).map(|i| {
            let phase = (i * frequency) % rate;
            (std::f32::consts::TAU * phase as f32 / rate as f32).sin() * 0.5
        }).collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn downsampling_keeps_a_speech_band_tone() {
        for rate in [44_100, 48_000] {
            let input = tone(rate, 1000);
            let output = resample(&input, rate);
            let change = 20.0 * (rms(&output) / rms(&input)).log10();
            assert!(change.abs() < 1.0, "{rate} Hz input: {change} dB");
        }
    }

    #[test]
    fn downsampling_rejects_a_tone_above_the_output_nyquist_frequency() {
        let input = tone(48_000, 12_000);
        let output = resample(&input, 48_000);
        let change = 20.0 * (rms(&output) / rms(&input)).log10();
        assert!(change < -30.0, "{change} dB");
    }

    #[test]
    fn resampling_at_the_target_rate_preserves_each_sample() {
        let input = [0.125, -0.75, 0.0, 0.25];
        assert_eq!(resample(&input, 16_000), input);
    }

    #[test]
    fn wav_header_describes_the_samples() {
        let bytes = wav(&[0.0, 0.5, -0.5, 1.0]);
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 8);
        assert_eq!(i16::from_le_bytes(bytes[50..52].try_into().unwrap()), 32767);
    }
}
