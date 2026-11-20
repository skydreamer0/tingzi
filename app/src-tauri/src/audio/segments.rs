use super::{sound_threshold, sound_window, SHORTEST_SOUND_MS};
use std::collections::VecDeque;
use std::ops::Range;

/// A pause this long ends a sentence.
const PAUSE_MS: usize = 600;
/// Once a sentence has run this long, a breath ends it, so text keeps coming to someone who speaks on.
const LONG_SENTENCE_MS: usize = 3000;
const BREATH_MS: usize = 300;
/// A sentence spoken without even a breath is cut when it has run this long.
const LONGEST_SENTENCE_SECONDS: usize = 6;

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
    incomplete: bool,
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
            incomplete: false,
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
        let long = phrase.voice.end - phrase.start >= self.rate * LONG_SENTENCE_MS / 1000;
        let pause = if long { BREATH_MS } else { PAUSE_MS };
        if self.end - phrase.voice.end < self.rate * pause / 1000 || !self.long_enough(&phrase.voice)
        {
            return None;
        }
        let end = phrase.voice.end + self.rate / 4;
        self.previous_end = end;
        Some(self.phrase.take().unwrap().start..end)
    }

    fn cut_at_limit(&mut self) -> Option<Range<usize>> {
        let phrase = self.phrase.as_ref()?;
        if self.end < phrase.start + self.rate * LONGEST_SENTENCE_SECONDS {
            return None;
        }
        let phrase = self.phrase.take().unwrap();
        if !self.long_enough(&phrase.voice) {
            self.incomplete = true;
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
        let long_enough = self.long_enough(&voice);
        self.incomplete |= !long_enough;
        long_enough.then_some(phrase.start..cut)
    }

    pub(super) fn incomplete(&self) -> bool {
        self.incomplete
    }

    pub(super) fn finish(mut self) -> (Option<Range<usize>>, bool) {
        if self.end > self.window_start {
            self.close_window();
        }
        let Some(phrase) = self.phrase.as_ref() else {
            return (None, self.incomplete);
        };
        let end = (phrase.voice.end + self.rate / 4).min(self.end);
        let long_enough = self.long_enough(&phrase.voice);
        (long_enough.then_some(phrase.start..end), self.incomplete || !long_enough)
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
        segments.extend(segmenter.finish().0);
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
        assert_eq!(s.finish(), (Some(550..1000), false));
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
    fn a_breath_ends_a_sentence_that_has_run_long() {
        let breath = [(3_000, 0.5), (300, 0.0), (500, 0.5), (600, 0.0)];
        assert_eq!(cut(1000, 157, &breath), [0..3250, 3250..4050]);
        // Before three seconds the same breath is part of the sentence, which then ends at the next one.
        let early = [(2_400, 0.5), (300, 0.0), (500, 0.5), (300, 0.0), (400, 0.5), (600, 0.0)];
        assert_eq!(cut(1000, 157, &early), [0..3450, 3450..4150]);
        assert_eq!(cut(1000, 157, &[(3_000, 0.5), (280, 0.0), (500, 0.5)]), [0..3780]);
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
            cut(1000, 157, &[(180, 0.5), (6_000, 0.0), (200, 0.5)]),
            [6000..6380]
        );
    }

    #[test]
    fn short_unsubmitted_speech_requires_the_full_recording() {
        let sentence = advance(Segmenter::new(1000), &[0.5; 200], &[]);
        let silence = advance(sentence, &[0.0; 600], &[0..450]);
        assert_eq!(silence.finish(), (None, false));

        let sentence = advance(Segmenter::new(1000), &[0.5; 200], &[]);
        let silence = advance(sentence, &[0.0; 600], &[0..450]);
        let short_tail = advance(silence, &[0.5; 180], &[]);
        assert_eq!(short_tail.finish(), (None, true));

        let short_start = advance(Segmenter::new(1000), &[0.5; 180], &[]);
        let gap = advance(short_start, &[0.0; 6_000], &[]);
        assert!(gap.incomplete());
        let tail = advance(gap, &[0.5; 200], &[]);
        assert_eq!(tail.finish(), (Some(6000..6380), true));
    }

    #[test]
    fn forced_cuts_use_the_quietest_complete_window_and_keep_the_suffix() {
        let s = advance(Segmenter::new(1000), &vec![0.5; 5_000], &[]);
        let s = advance(s, &[0.1; 40], &[]);
        let s = advance(s, &[0.6; 240], &[]);
        let s = advance(s, &[0.1; 20], &[]);
        let s = advance(s, &[0.5; 699], &[]);
        let s = advance(s, &[0.5], &[0..5300]);
        assert_eq!(s.finish(), (Some(5300..6000), false));
        assert_eq!(
            cut(1000, 4096, &[(12_200, 0.5)]),
            [0..6000, 6000..12000, 12000..12200]
        );
    }

    #[test]
    fn forced_cut_ties_choose_the_latest_window_and_assign_the_suffix_once() {
        let mut input = vec![0.5; 6_000];
        input[5_000..5_020].fill(0.1);
        input[5_500..5_520].fill(0.1);

        for block in [137, 4096] {
            let mut segmenter = Segmenter::new(1000);
            let mut segments = Vec::new();
            for chunk in input.chunks(block) {
                let (next, ready) = segmenter.push(chunk);
                segmenter = next;
                segments.extend(ready);
            }
            assert_eq!(segments, [0..5_520]);
            assert_eq!(segmenter.finish(), (Some(5_520..6_000), false));
        }
    }

    #[test]
    fn finishing_classifies_a_partial_window_and_ignores_a_silent_tail() {
        assert!(cut(1000, 13, &[(190, 0.5)]).is_empty());
        assert!(cut(1000, 13, &[(190, 0.5), (9, 0.0)]).is_empty());
        assert_eq!(cut(1000, 13, &[(190, 0.5), (15, 0.0)]), [0..205]);
        assert_eq!(cut(1000, 13, &[(180, 0.5), (25, -0.5)]), [0..205]);
        assert_eq!(cut(1000, 13, &[(200, 0.5), (2000, 0.0)]), [0..450]);
    }
}
