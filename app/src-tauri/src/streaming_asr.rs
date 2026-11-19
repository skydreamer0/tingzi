//! One recording's request queue and final transcript, independent of the recogniser and UI.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex};

const MAX_QUEUED: usize = 3;

#[derive(Default, PartialEq, Eq)]
enum Status {
    #[default]
    Active,
    Fallback,
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Transcript {
    Sentences(String),
    FullRecording,
    Cancelled,
}

enum Admission {
    Ignored,
    Changed,
    Start(Range<usize>),
}

#[derive(Default)]
pub struct Queue {
    status: Status,
    accepted: usize,
    running: bool,
    waiting: VecDeque<Range<usize>>,
    sentences: Vec<String>,
}

impl Queue {
    pub fn accepted(&self) -> usize {
        self.accepted
    }

    pub fn pending(&self) -> usize {
        usize::from(self.running) + self.waiting.len()
    }

    pub fn sentences(&self) -> &[String] {
        &self.sentences
    }

    pub fn fell_back(&self) -> bool {
        self.status == Status::Fallback
    }

    fn push(&mut self, sequence: usize, range: Range<usize>) -> Admission {
        if self.status != Status::Active || sequence <= self.accepted {
            return Admission::Ignored;
        }
        if sequence != self.accepted + 1 || self.waiting.len() == MAX_QUEUED {
            self.fallback();
            return Admission::Changed;
        }
        self.accepted = sequence;
        if self.running {
            self.waiting.push_back(range);
            Admission::Changed
        } else {
            self.running = true;
            Admission::Start(range)
        }
    }

    fn complete(&mut self, heard: Result<String, String>) -> Option<Range<usize>> {
        if self.status != Status::Active {
            return None;
        }
        match heard {
            Ok(text) if !text.trim().is_empty() => self.sentences.push(text.trim().to_owned()),
            _ => {
                self.fallback();
                return None;
            }
        }
        let next = self.waiting.pop_front();
        self.running = next.is_some();
        next
    }

    fn close(&mut self, status: Status) {
        self.status = status;
        self.running = false;
        self.waiting.clear();
        self.sentences.clear();
    }

    fn fallback(&mut self) -> bool {
        if self.status != Status::Active {
            return false;
        }
        self.close(Status::Fallback);
        true
    }

    /// Called after capture has stopped producing segments.
    fn transcript(&self) -> Option<Transcript> {
        match self.status {
            Status::Cancelled => Some(Transcript::Cancelled),
            Status::Fallback => Some(Transcript::FullRecording),
            Status::Active if self.running => None,
            Status::Active
                if self.sentences.is_empty() || self.sentences.len() != self.accepted =>
            {
                Some(Transcript::FullRecording)
            }
            Status::Active => Some(Transcript::Sentences(join(&self.sentences))),
        }
    }
}

fn join(sentences: &[String]) -> String {
    let mut text = String::new();
    for sentence in sentences {
        if text
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && sentence
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            text.push(' ');
        }
        text.push_str(sentence);
    }
    text
}

#[derive(Default)]
pub struct Requests {
    queue: Mutex<Queue>,
    changed: Condvar,
}

impl Requests {
    pub fn submit(
        self: &Arc<Self>,
        sequence: usize,
        range: Range<usize>,
        recognise: impl Fn(Range<usize>, &dyn Fn() -> bool) -> Result<String, String> + Send + 'static,
        publish: impl Fn(&Queue) + Send + 'static,
    ) {
        let mut range = {
            let mut queue = self.queue.lock().unwrap();
            match queue.push(sequence, range) {
                Admission::Ignored => return,
                Admission::Changed => {
                    publish(&queue);
                    self.changed.notify_all();
                    return;
                }
                Admission::Start(range) => {
                    publish(&queue);
                    range
                }
            }
        };
        let requests = self.clone();
        std::thread::spawn(move || {
            let cancelled = || requests.queue.lock().unwrap().status != Status::Active;
            loop {
                if cancelled() {
                    return;
                }
                let heard = recognise(range, &cancelled);
                let mut queue = requests.queue.lock().unwrap();
                if queue.status != Status::Active {
                    return;
                }
                let next = queue.complete(heard);
                publish(&queue);
                requests.changed.notify_all();
                match next {
                    Some(next) => range = next,
                    None => return,
                }
            }
        });
    }

    pub fn fallback(&self, publish: impl FnOnce(&Queue)) {
        let mut queue = self.queue.lock().unwrap();
        if queue.fallback() {
            publish(&queue);
            self.changed.notify_all();
        }
    }

    pub fn cancel(&self) {
        self.queue.lock().unwrap().close(Status::Cancelled);
        self.changed.notify_all();
    }

    pub fn finish(&self) -> Transcript {
        let mut queue = self.queue.lock().unwrap();
        loop {
            if let Some(transcript) = queue.transcript() {
                return transcript;
            }
            queue = self.changed.wait(queue).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn ordered_transcript_preserves_punctuation_and_repeated_words() {
        let mut queue = Queue::default();
        assert_eq!(queue.transcript(), Some(Transcript::FullRecording));
        for sequence in 1..=4 {
            queue.push(sequence, sequence..sequence + 1);
        }
        assert_eq!(queue.pending(), 4);
        assert!(matches!(queue.push(2, 0..1), Admission::Ignored));
        assert_eq!(queue.transcript(), None);
        for (index, text) in [" 你好\n", "你好，ABC ", " 123 ", "。再見 "]
            .into_iter()
            .enumerate()
        {
            let next = queue.complete(Ok(text.into()));
            let sequence = index + 2;
            assert_eq!(next, (sequence <= 4).then_some(sequence..sequence + 1));
            assert_eq!(queue.pending(), 3 - index);
        }
        assert_eq!(
            queue.transcript(),
            Some(Transcript::Sentences("你好你好，ABC 123。再見".into()))
        );
    }

    #[test]
    fn errors_empty_responses_overflow_and_missing_segments_use_the_full_recording() {
        for result in [Err("失敗"), Err("逾時"), Err("429"), Ok(" \n ")] {
            let mut queue = Queue::default();
            queue.push(1, 0..1);
            queue.complete(Ok("第一句".into()));
            queue.push(2, 1..2);
            queue.push(3, 2..3);
            assert_eq!(
                queue.complete(result.map(str::to_owned).map_err(str::to_owned)),
                None
            );
            assert_eq!(queue.pending(), 0);
            assert!(queue.sentences().is_empty());
            assert_eq!(queue.transcript(), Some(Transcript::FullRecording));
        }
        let mut queue = Queue::default();
        for sequence in 1..=5 {
            queue.push(sequence, 0..1);
        }
        assert!(queue.fell_back());
        assert_eq!(queue.complete(Ok("晚到".into())), None);
        assert!(queue.sentences().is_empty());
        assert_eq!(queue.transcript(), Some(Transcript::FullRecording));
        let mut queue = Queue::default();
        queue.push(2, 0..1);
        assert_eq!(queue.transcript(), Some(Transcript::FullRecording));
    }

    #[test]
    fn injected_recogniser_drains_in_order_and_publishes_pending_counts() {
        let requests = Arc::new(Requests::default());
        let (release, first) = mpsc::channel();
        let first = Mutex::new(first);
        let (publish, views) = mpsc::channel();
        let enqueue = publish.clone();
        requests.submit(
            1,
            10..20,
            move |range, _| {
                if range.start == 10 {
                    first.lock().unwrap().recv().unwrap();
                }
                Ok(range.start.to_string())
            },
            move |queue| {
                publish
                    .send((queue.pending(), queue.sentences().to_vec()))
                    .unwrap();
            },
        );
        requests.submit(
            2,
            20..30,
            |_, _| panic!("uses the running worker"),
            move |queue| {
                enqueue
                    .send((queue.pending(), queue.sentences().to_vec()))
                    .unwrap();
            },
        );
        release.send(()).unwrap();
        assert_eq!(requests.finish(), Transcript::Sentences("10 20".into()));
        let views: Vec<_> = views.try_iter().collect();
        assert_eq!(
            views,
            [
                (1, vec![]),
                (2, vec![]),
                (1, vec!["10".into()]),
                (0, vec!["10".into(), "20".into()])
            ]
        );
    }

    #[test]
    fn successful_tail_and_silent_tail_finish_with_one_ordered_transcript() {
        let requests = Arc::new(Requests::default());
        requests.submit(1, 0..1, |_, _| Ok("中文".into()), |_| {});
        requests.submit(2, 1..2, |_, _| Ok("尾句".into()), |_| {});
        assert_eq!(requests.finish(), Transcript::Sentences("中文尾句".into()));

        // Silence creates no segment, so the preceding sentence remains the transcript.
        let requests = Arc::new(Requests::default());
        requests.submit(1, 0..1, |_, _| Ok("中文".into()), |_| {});
        assert_eq!(requests.finish(), Transcript::Sentences("中文".into()));
    }

    #[test]
    fn empty_tail_timeout_and_rate_limit_fall_back_once_and_clear_preview() {
        for result in [Ok(" \n ".into()), Err("timeout".into()), Err("429".into())] {
            let requests = Arc::new(Requests::default());
            let (published, states) = mpsc::channel();
            requests.submit(1, 0..1, |_, _| Ok("先前句子".into()), |_| {});
            requests.submit(2, 1..2, move |_, _| result, move |queue| {
                published
                    .send((queue.fell_back(), queue.sentences().to_vec()))
                    .unwrap();
            });
            assert_eq!(requests.finish(), Transcript::FullRecording);
            let (fallback, sentences) = states.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(fallback);
            assert!(sentences.is_empty());
        }
    }

    #[test]
    fn fallback_drops_queued_requests_without_starting_them() {
        let requests = Arc::new(Requests::default());
        let (release, unblock) = mpsc::channel();
        let unblock = Mutex::new(unblock);
        let (started, wait_started) = mpsc::channel();
        requests.submit(
            1,
            0..1,
            move |_, _| {
                started.send(()).unwrap();
                unblock.lock().unwrap().recv().unwrap();
                Ok("late".into())
            },
            |_| {},
        );
        wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
        let (queued, ran_queued) = mpsc::channel();
        requests.submit(
            2,
            1..2,
            move |_, _| {
                queued.send(()).unwrap();
                Ok("should not run".into())
            },
            |_| {},
        );
        requests.fallback(|_| {});
        assert_eq!(requests.finish(), Transcript::FullRecording);
        assert_eq!(ran_queued.try_recv(), Err(mpsc::TryRecvError::Empty));
        release.send(()).unwrap();
    }

    #[test]
    fn cancelling_releases_the_waiter_and_ignores_a_late_response() {
        let requests = Arc::new(Requests::default());
        let (release, late) = mpsc::channel();
        let late = Mutex::new(late);
        let (started, in_flight) = mpsc::channel();
        let (publish, views) = mpsc::channel();
        requests.submit(
            1,
            0..1,
            move |_, cancelled| {
                started.send(()).unwrap();
                late.lock().unwrap().recv().unwrap();
                assert!(cancelled());
                Ok("晚到".into())
            },
            move |queue| publish.send(queue.pending()).unwrap(),
        );
        in_flight.recv_timeout(Duration::from_secs(5)).unwrap();
        requests.submit(2, 1..2, |_, _| panic!("already running"), |_| {});
        requests.cancel();
        assert_eq!(requests.finish(), Transcript::Cancelled);
        release.send(()).unwrap();
        assert_eq!(views.recv().unwrap(), 1);
        assert_eq!(
            views.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
    }
}
