//! Microphone capture: records the chosen input and hands back 16 kHz mono WAV bytes.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const TARGET_RATE: u32 = 16_000;
/// Recording stops by itself after ten minutes.
pub const LONGEST_SECONDS: usize = 600;

pub struct Recorder {
    stop: Sender<()>,
    worker: JoinHandle<Result<Vec<u8>, String>>,
}

impl Recorder {
    /// Starts recording from the microphone with this name, or the Windows default when None.
    /// `level` receives the peak of each buffer, from 0 to 1.
    pub fn start(microphone: Option<String>, level: impl Fn(f32) + Send + 'static) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready, started) = mpsc::channel::<Result<(), String>>();
        let worker = thread::spawn(move || -> Result<Vec<u8>, String> {
            // The stream is not Send, so the thread that opens it also owns and closes it.
            let opened = open(microphone.as_deref(), level);
            let (stream, samples, rate) = match opened {
                Ok(parts) => parts,
                Err(reason) => {
                    let _ = ready.send(Err(reason.clone()));
                    return Err(reason);
                }
            };
            let _ = ready.send(Ok(()));
            let _ = stopped.recv();
            drop(stream);
            let captured = samples.lock().map_err(|_| "錄音資料損毀".to_string())?;
            if too_short(&captured, rate) {
                return Err(TOO_SHORT.into());
            }
            Ok(wav(&resample(trim(&captured, rate), rate)))
        });
        match started.recv() {
            Ok(Ok(())) => Ok(Self { stop, worker }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => Err("無法啟動錄音".into()),
        }
    }

    /// Stops recording and returns the WAV file contents.
    pub fn finish(self) -> Result<Vec<u8>, String> {
        let _ = self.stop.send(());
        self.worker.join().map_err(|_| "錄音中斷".to_string())?
    }
}

type Opened = (cpal::Stream, Arc<Mutex<Vec<f32>>>, u32);

/// The names of the microphones Windows can record from right now.
pub fn microphones() -> Vec<String> {
    cpal::default_host().input_devices().map(|devices| devices.filter_map(|device| device.name().ok()).collect()).unwrap_or_default()
}

fn open(microphone: Option<&str>, level: impl Fn(f32) + Send + 'static) -> Result<Opened, String> {
    let host = cpal::default_host();
    let device = match microphone {
        Some(name) => host
            .input_devices()
            .ok()
            .and_then(|mut devices| devices.find(|device| device.name().is_ok_and(|found| found == name)))
            .ok_or_else(|| format!("{name}未連接，請開語音設定"))?,
        None => host.default_input_device().ok_or("找不到麥克風")?,
    };
    let config = device.default_input_config().map_err(|_| "無法讀取麥克風格式")?;
    let channels = usize::from(config.channels().max(1));
    let rate = config.sample_rate().0;
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = samples.clone();
    let limit = rate as usize * LONGEST_SECONDS;
    // Mixes each frame down to mono and reports how loud the buffer was.
    let keep = move |frames: &mut dyn Iterator<Item = f32>| {
        let mut peak = 0f32;
        if let Ok(mut held) = sink.lock() {
            let mut sum = 0f32;
            let mut count = 0usize;
            for sample in frames {
                sum += sample;
                count += 1;
                if count == channels {
                    let mono = sum / channels as f32;
                    peak = peak.max(mono.abs());
                    if held.len() < limit {
                        held.push(mono);
                    }
                    sum = 0.0;
                    count = 0;
                }
            }
        }
        level(peak.min(1.0));
    };
    let failed = |_: cpal::StreamError| {};
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| keep(&mut data.iter().copied()),
            failed,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| keep(&mut data.iter().map(|&s| f32::from(s) / 32768.0)),
            failed,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &cpal::InputCallbackInfo| keep(&mut data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0)),
            failed,
            None,
        ),
        _ => return Err("不支援這支麥克風的取樣格式".into()),
    }
    .map_err(|_| format!("無法開啟麥克風，請確認 {} 的麥克風權限", crate::sys::SYSTEM))?;
    stream.play().map_err(|_| "無法開始錄音")?;
    Ok((stream, samples, rate))
}

/// Where the sound starts and ends, in samples, to the nearest 20 ms. None when nothing stands out.
fn sound(samples: &[f32], rate: u32) -> Option<(usize, usize)> {
    let window = (rate as usize / 50).max(1); // 20 ms
    let loudest = samples.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
    // Speech stands well above the room; the floor keeps a silent recording from being cut to nothing.
    let threshold = (loudest * 0.08).max(0.004);
    let speaking = |chunk: &[f32]| chunk.iter().any(|sample| sample.abs() > threshold);
    let chunks = samples.chunks(window);
    let (first, last) = (chunks.clone().position(speaking)?, chunks.clone().rposition(speaking)?);
    Some((first * window, ((last + 1) * window).min(samples.len())))
}

const TOO_SHORT: &str = "錄音太短，請再說一次";

/// Sound shorter than this is a key press, a click or a cough, not a word. A single spoken
/// syllable such as 「好」 runs longer, so it still goes through.
const SHORTEST_SOUND_MS: usize = 200;

/// Whether there is too little here to be worth recognising: a brush of the shortcut, or one brief
/// noise in an otherwise quiet recording. Recognisers tend to make words up from such clips.
/// A longer recording in which nothing stands out is not judged here; the recogniser answers with no text.
fn too_short(samples: &[f32], rate: u32) -> bool {
    let shortest = rate as usize * SHORTEST_SOUND_MS / 1000;
    match sound(samples, rate) {
        Some((start, end)) => end - start < shortest,
        None => samples.len() < shortest,
    }
}

/// Drops the silence before and after speech, keeping a quarter second on each side.
/// Less audio means a smaller upload and a faster answer; a recording with no clear speech is left whole.
fn trim(samples: &[f32], rate: u32) -> &[f32] {
    let Some((first, last)) = sound(samples, rate) else { return samples };
    let margin = rate as usize / 4;
    let start = first.saturating_sub(margin);
    let end = (last + margin).min(samples.len());
    if end - start < rate as usize * 3 / 10 {
        return samples;
    }
    &samples[start..end]
}

/// Linear resampling is enough for speech going to a recogniser.
fn resample(samples: &[f32], rate: u32) -> Vec<f32> {
    if rate == TARGET_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let step = f64::from(rate) / f64::from(TARGET_RATE);
    let length = (samples.len() as f64 / step) as usize;
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
pub fn normalise(file: &[u8]) -> Result<Vec<u8>, String> {
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
    if too_short(&mono, rate) {
        return Err(TOO_SHORT.into());
    }
    Ok(wav(&resample(&mono, rate)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let quiet_then = |milliseconds: usize| {
            let mut recording = vec![0.001f32; rate];
            recording.extend(std::iter::repeat_n(0.5, rate * milliseconds / 1000));
            recording.extend(std::iter::repeat_n(0.001, rate));
            recording
        };
        // A click or a key press.
        assert!(too_short(&quiet_then(60), rate as u32));
        // One syllable.
        assert!(!too_short(&quiet_then(260), rate as u32));
        // The shortcut brushed, with nothing said.
        assert!(too_short(&vec![0.001; rate / 10], rate as u32));
        assert!(too_short(&[], rate as u32));
        // A long quiet recording is left for the recogniser to answer.
        assert!(!too_short(&vec![0.0; rate * 2], rate as u32));
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
        let out = normalise(&file).unwrap();
        assert_eq!(out.len(), 44 + 16_000 * 2);
        assert_eq!(u32::from_le_bytes(out[24..28].try_into().unwrap()), 16_000);
        assert!(normalise(b"not a wave file").is_err());
    }

    #[test]
    fn resampling_keeps_duration() {
        let second: Vec<f32> = (0..48_000).map(|i| (i as f32 / 48_000.0).sin()).collect();
        assert_eq!(resample(&second, 48_000).len(), 16_000);
        assert_eq!(resample(&second, 16_000).len(), 48_000);
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
