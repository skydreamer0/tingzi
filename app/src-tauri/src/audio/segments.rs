use super::{sound_threshold, sound_window, SHORTEST_SOUND_MS};
use std::collections::VecDeque;
use std::ops::Range;

#[derive(Clone, Copy)]
struct Window {
    start: usize,
    end: usize,
    peak: f32,
    voiced: bool,
}

struct Phrase {
    start: usize,
    voice: Range<usize>,
}

/// Sample positions refer to the original mono input. End positions are exclusive.
pub(super) struct Segmenter {
    rate: usize,
    window_size: usize,
    end: usize,
    window_start: usize,
    peak: f32,
    recent_peaks: VecDeque<(usize, f32)>,
    windows: VecDeque<Window>,
    earlier_voice_end: usize,
    previous_end: usize,
    phrase: Option<Phrase>,
}

impl Segmenter {
    pub(super) fn new(rate: u32) -> Self {
        Self {
            rate: rate as usize,
            window_size: sound_window(rate),
            end: 0,
            window_start: 0,
            peak: 0.0,
            recent_peaks: VecDeque::new(),
            windows: VecDeque::new(),
            earlier_voice_end: 0,
            previous_end: 0,
            phrase: None,
        }
    }

    pub(super) fn push(mut self, samples: &[f32]) -> (Self, Vec<Range<usize>>) {
        let mut segments = Vec::new();
        for &sample in samples {
            let peak = 0f32.max(sample.abs());
            self.end += 1;
            self.peak = self.peak.max(peak);
            // The deque's front is the loudest sample in the last second, even at a partial window.
            let recent = self.end.saturating_sub(self.rate);
            if self
                .recent_peaks
                .front()
                .is_some_and(|&(at, _)| at < recent)
            {
                self.recent_peaks.pop_front();
            }
            while self
                .recent_peaks
                .back()
                .is_some_and(|&(_, old)| old <= peak)
            {
                self.recent_peaks.pop_back();
            }
            self.recent_peaks.push_back((self.end - 1, peak));

            let full = self.end - self.window_start == self.window_size;
            if full {
                let window = self.close_window();
                while self.windows.front().is_some_and(|old| old.end <= recent) {
                    let old = self.windows.pop_front().unwrap();
                    if old.voiced {
                        self.earlier_voice_end = old.end;
                    }
                }
                self.windows.push_back(window);
            }
            if let Some(segment) = self
                .cut_at_limit()
                .or_else(|| full.then(|| self.cut_at_pause()).flatten())
            {
                segments.push(segment);
            }
        }
        (self, segments)
    }

    fn close_window(&mut self) -> Window {
        let peak = std::mem::take(&mut self.peak);
        let loudest = self.recent_peaks.front().map_or(0.0, |&(_, peak)| peak);
        let window = Window {
            start: std::mem::replace(&mut self.window_start, self.end),
            end: self.end,
            peak,
            voiced: peak > sound_threshold(loudest),
        };
        if window.voiced {
            let first = window.start.max(self.previous_end);
            let phrase = self.phrase.get_or_insert_with(|| Phrase {
                start: first.saturating_sub(self.rate / 4).max(self.previous_end),
                voice: first..window.end,
            });
            phrase.voice.end = window.end;
        }
        window
    }

    fn long_enough(&self, voice: &Range<usize>) -> bool {
        voice.len() >= self.rate * SHORTEST_SOUND_MS / 1000
    }

    fn cut_at_pause(&mut self) -> Option<Range<usize>> {
        let phrase = self.phrase.as_ref()?;
        if self.end - phrase.voice.end < self.rate * 600 / 1000 || !self.long_enough(&phrase.voice)
        {
            return None;
        }
        let end = phrase.voice.end + self.rate / 4;
        self.previous_end = end;
        Some(self.phrase.take().unwrap().start..end)
    }

    fn cut_at_limit(&mut self) -> Option<Range<usize>> {
        let phrase = self.phrase.as_ref()?;
        if self.end < phrase.start + self.rate * 15 {
            return None;
        }
        let phrase = self.phrase.take().unwrap();
        if !self.long_enough(&phrase.voice) {
            self.previous_end = self.end;
            return None;
        }
        let cut = self
            .windows
            .iter()
            .rev()
            .filter(|window| window.start >= self.end - self.rate)
            .min_by(|a, b| a.peak.total_cmp(&b.peak))
            .map_or(self.end, |window| window.end);
        let last = if phrase.voice.end <= cut {
            phrase.voice.end
        } else {
            self.windows
                .iter()
                .rev()
                .find(|window| window.voiced && window.end <= cut)
                .map_or(self.earlier_voice_end, |window| window.end)
                .max(phrase.voice.start)
        };
        let voice = phrase.voice.start..last;
        self.previous_end = cut;
        if let Some(first) = self
            .windows
            .iter()
            .find(|window| window.voiced && window.start >= cut)
        {
            self.phrase = Some(Phrase {
                start: cut,
                voice: first.start..phrase.voice.end,
            });
        }
        self.long_enough(&voice).then_some(phrase.start..cut)
    }

    pub(super) fn finish(mut self) -> Option<Range<usize>> {
        if self.end > self.window_start {
            self.close_window();
        }
        let phrase = self.phrase.as_ref()?;
        let end = (phrase.voice.end + self.rate / 4).min(self.end);
        self.long_enough(&phrase.voice).then_some(phrase.start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(rate: u32, block: usize, runs: &[(usize, f32)]) -> Vec<Range<usize>> {
        let input: Vec<_> = runs
            .iter()
            .flat_map(|&(ms, value)| std::iter::repeat_n(value, rate as usize * ms / 1000))
            .collect();
        let mut segmenter = Segmenter::new(rate);
        let mut segments = Vec::new();
        for chunk in input.chunks(block) {
            let (next, ready) = segmenter.push(chunk);
            segmenter = next;
            segments.extend(ready);
        }
        segments.extend(segmenter.finish());
        segments
    }

    fn advance(s: Segmenter, samples: &[f32], expected: &[Range<usize>]) -> Segmenter {
        let (s, segments) = s.push(samples);
        assert_eq!(segments, expected);
        s
    }

    #[test]
    fn voice_must_exceed_the_absolute_and_relative_thresholds() {
        assert!(cut(1000, 157, &[(200, -0.004), (600, 0.0)]).is_empty());
        assert_eq!(cut(1000, 157, &[(200, -0.004001), (600, 0.0)]), [0..450]);
        let s = advance(Segmenter::new(1000), &[0.5; 200], &[]);
        let s = advance(s, &[0.04; 600], &[0..450]);
        let s = advance(s, &[0.040001; 200], &[]);
        assert_eq!(s.finish(), Some(550..1000));
    }

    #[test]
    fn a_pause_and_minimum_voice_are_measured_in_samples() {
        let runs = [(320, 0.0), (200, 0.5), (580, 0.0), (200, 0.5), (600, 0.0)];
        for rate in [16_000, 44_100, 48_000] {
            for block in [137, 4096] {
                let at = |ms| (u64::from(rate) * ms / 1000) as usize;
                assert_eq!(cut(rate, block, &runs), [at(70)..at(1550)]);
                assert_eq!(
                    cut(
                        rate,
                        block,
                        &[(320, 0.0), (200, 0.5), (600, 0.0), (200, 0.5)]
                    ),
                    [at(70)..at(770), at(870)..at(1320)]
                );
            }
        }
        assert!(cut(1000, 31, &[(180, 0.5), (600, 0.0)]).is_empty());
        assert_eq!(
            cut(1000, 31, &[(180, 0.5), (600, 0.0), (200, 0.5)]),
            [0..980]
        );
        let s = advance(Segmenter::new(1000), &[0.5; 200], &[]);
        let s = advance(s, &[0.0; 599], &[]);
        advance(s, &[0.0], &[0..450]);
    }

    #[test]
    fn silence_and_expired_loud_peaks_do_not_hold_open_a_sentence() {
        assert!(cut(1000, 157, &[(40_000, 0.001)]).is_empty());
        assert_eq!(
            cut(
                1000,
                157,
                &[(200, 0.9), (1_000, 0.0), (200, 0.006), (600, 0.0)]
            ),
            [0..450, 950..1650]
        );
        assert_eq!(
            cut(1000, 157, &[(180, 0.5), (15_000, 0.0), (200, 0.5)]),
            [15000..15380]
        );
    }

    #[test]
    fn forced_cuts_use_the_quietest_complete_window_and_keep_the_suffix() {
        let s = advance(Segmenter::new(1000), &vec![0.5; 14_000], &[]);
        let s = advance(s, &[0.1; 40], &[]);
        let s = advance(s, &[0.6; 240], &[]);
        let s = advance(s, &[0.1; 20], &[]);
        let s = advance(s, &[0.5; 699], &[]);
        let s = advance(s, &[0.5], &[0..14300]);
        assert_eq!(s.finish(), Some(14300..15000));
        assert_eq!(
            cut(1000, 4096, &[(30_200, 0.5)]),
            [0..15000, 15000..30000, 30000..30200]
        );
    }

    #[test]
    fn forced_cut_ties_choose_the_latest_window_and_assign_the_suffix_once() {
        let mut input = vec![0.5; 15_000];
        input[14_000..14_020].fill(0.1);
        input[14_500..14_520].fill(0.1);

        for block in [137, 4096] {
            let mut segmenter = Segmenter::new(1000);
            let mut segments = Vec::new();
            for chunk in input.chunks(block) {
                let (next, ready) = segmenter.push(chunk);
                segmenter = next;
                segments.extend(ready);
            }
            assert_eq!(segments, [0..14_520]);
            assert_eq!(segmenter.finish(), Some(14_520..15_000));
        }
    }

    #[test]
    fn finishing_classifies_a_partial_window_and_ignores_a_silent_tail() {
        assert_eq!(cut(1000, 13, &[(190, 0.5)]), []);
        assert_eq!(cut(1000, 13, &[(190, 0.5), (9, 0.0)]), []);
        assert_eq!(cut(1000, 13, &[(190, 0.5), (15, 0.0)]), [0..205]);
        assert_eq!(cut(1000, 13, &[(180, 0.5), (25, -0.5)]), [0..205]);
        assert_eq!(cut(1000, 13, &[(200, 0.5), (2000, 0.0)]), [0..450]);
    }
}
