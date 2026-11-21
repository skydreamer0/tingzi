use rustfft::{num_complex::Complex32, FftPlanner};
use std::f32::consts::PI;

// 32 ms at 16 kHz, with 50% overlap so the squared sine windows add up to one.
const FRAME: usize = 512;
const HOP: usize = FRAME / 2;
const NOISE_FACTOR: f32 = 2.0;
const MIN_POWER_GAIN: f32 = 0.04;

pub(super) fn suppress(samples: &[f32]) -> Vec<f32> {
    if samples.len() < FRAME {
        return samples.to_vec();
    }

    let mut frames: Vec<_> = samples
        .windows(FRAME)
        .step_by(HOP)
        .enumerate()
        .map(|(frame, chunk)| (frame * HOP, super::levels(chunk).1))
        .collect();
    let quiet_count = frames.len().div_ceil(5);
    frames.select_nth_unstable_by(quiet_count - 1, |a, b| a.1.total_cmp(&b.1));

    let mut planner = FftPlanner::new();
    let forward = planner.plan_fft_forward(FRAME);
    let inverse = planner.plan_fft_inverse(FRAME);
    let mut scratch = vec![
        Complex32::default();
        forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len())
    ];
    let mut spectrum = [Complex32::default(); FRAME];
    let window: [f32; FRAME] =
        std::array::from_fn(|i| (PI * (i as f32 + 0.5) / FRAME as f32).sin());

    let mut noise_power = [0.0; HOP + 1];
    for &(start, _) in &frames[..quiet_count] {
        window_samples(&mut spectrum, &window, samples, start as isize);
        forward.process_with_scratch(&mut spectrum, &mut scratch);
        for (noise, bin) in noise_power.iter_mut().zip(&spectrum) {
            *noise += bin.norm_sqr() / quiet_count as f32;
        }
    }

    let mut output = vec![0.0; samples.len()];
    for center in (0..samples.len() + HOP).step_by(HOP) {
        let start = center as isize - HOP as isize;
        window_samples(&mut spectrum, &window, samples, start);
        forward.process_with_scratch(&mut spectrum, &mut scratch);
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let noise = noise_power[i.min(FRAME - i)];
            let power = bin.norm_sqr();
            let remaining = 1.0 - NOISE_FACTOR * noise / power.max(f32::MIN_POSITIVE);
            *bin *= remaining.max(MIN_POWER_GAIN).sqrt();
        }
        inverse.process_with_scratch(&mut spectrum, &mut scratch);
        for (i, (&bin, &weight)) in spectrum.iter().zip(&window).enumerate() {
            if let Some(sample) = output.get_mut((start + i as isize) as usize) {
                *sample += bin.re * weight / FRAME as f32;
            }
        }
    }
    output
}

fn window_samples(
    spectrum: &mut [Complex32; FRAME],
    window: &[f32; FRAME],
    samples: &[f32],
    start: isize,
) {
    for (i, (bin, weight)) in spectrum.iter_mut().zip(window).enumerate() {
        let value = samples
            .get((start + i as isize) as usize)
            .copied()
            .unwrap_or(0.0);
        let value = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        *bin = Complex32::new(value * weight, 0.0);
    }
}
