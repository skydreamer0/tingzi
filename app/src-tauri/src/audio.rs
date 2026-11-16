//! Microphone capture: records the default input and hands back 16 kHz mono WAV bytes.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const TARGET_RATE: u32 = 16_000;
/// Five minutes at the device rate is more than one dictation ever needs.
const LONGEST_SECONDS: usize = 300;

pub struct Recorder {
    stop: Sender<()>,
    worker: JoinHandle<Result<Vec<u8>, String>>,
}

impl Recorder {
    /// Starts recording. `level` receives the peak of each buffer, from 0 to 1.
    pub fn start(level: impl Fn(f32) + Send + 'static) -> Result<Self, String> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (ready, started) = mpsc::channel::<Result<(), String>>();
        let worker = thread::spawn(move || -> Result<Vec<u8>, String> {
            // The stream is not Send, so the thread that opens it also owns and closes it.
            let opened = open(level);
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
            Ok(wav(&resample(&captured, rate)))
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

fn open(level: impl Fn(f32) + Send + 'static) -> Result<Opened, String> {
    let device = cpal::default_host().default_input_device().ok_or("找不到麥克風")?;
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
    .map_err(|_| "無法開啟麥克風，請確認 Windows 的麥克風權限")?;
    stream.play().map_err(|_| "無法開始錄音")?;
    Ok((stream, samples, rate))
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

#[cfg(test)]
mod tests {
    use super::*;

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
