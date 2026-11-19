use serde::Serialize;

#[derive(Clone, Default, Serialize)]
pub struct Preview {
    recording_id: Option<u64>,
    version: u64,
    pub segments: usize,
    pub sentences: Vec<String>,
    pub pending: usize,
}

impl Preview {
    pub fn begin(&mut self, recording_id: u64) {
        *self = Self {
            recording_id: Some(recording_id),
            version: self.version + 1,
            ..Self::default()
        };
    }

    pub fn clear(&mut self) -> bool {
        if self.recording_id.is_none() {
            return false;
        }
        *self = Self {
            version: self.version + 1,
            ..Self::default()
        };
        true
    }

    pub fn update(&mut self, recording_id: u64, change: impl FnOnce(&mut Self) -> bool) -> bool {
        if self.recording_id != Some(recording_id) || !change(self) {
            return false;
        }
        self.version += 1;
        true
    }

    pub fn visible(&self) -> bool {
        self.pending > 0 || !self.sentences.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};
    use std::sync::mpsc;

    #[test]
    fn delayed_sentences_update_only_the_current_recordings_preview() {
        let (send, replies) = mpsc::channel();
        let (next, requested) = mpsc::channel();
        let fake = std::thread::spawn(move || {
            for sentence in ["明天開會。", "記得帶資料。"] {
                let recording_id = requested.recv().unwrap();
                send.send((recording_id, sentence)).unwrap();
            }
        });
        let mut preview = Preview::default();
        preview.begin(7);
        assert!(preview.update(7, |view| {
            view.segments = 2;
            view.pending = 2;
            true
        }));
        assert!(preview.visible());
        assert_eq!(
            to_value(&preview).unwrap(),
            json!({
                "recording_id": 7, "version": 2, "segments": 2, "sentences": [], "pending": 2
            })
        );
        for (pending, sentences) in [
            (1, vec!["明天開會。"]),
            (0, vec!["明天開會。", "記得帶資料。"]),
        ] {
            next.send(7).unwrap();
            let (recording_id, sentence) = replies.recv().unwrap();
            assert!(preview.update(recording_id, |view| {
                view.pending -= 1;
                view.sentences.push(sentence.into());
                true
            }));
            assert_eq!(preview.pending, pending);
            assert_eq!(preview.sentences, sentences);
        }
        fake.join().unwrap();
        assert!(preview.clear());
        assert!(!preview.visible());
        preview.begin(9);
        assert!(!preview.update(7, |_| panic!("old reply was accepted")));
        assert_eq!(
            to_value(preview).unwrap(),
            json!({
                "recording_id": 9, "version": 6, "segments": 0, "sentences": [], "pending": 0
            })
        );
    }

    #[test]
    fn duplicate_and_out_of_order_segment_events_do_not_advance_the_preview() {
        let mut preview = Preview::default();
        preview.begin(12);
        let accept = |preview: &mut Preview, recording_id, sequence| {
            preview.update(recording_id, |view| {
                if sequence != view.segments + 1 {
                    return false;
                }
                view.segments = sequence;
                true
            })
        };

        assert!(accept(&mut preview, 12, 1));
        let version = preview.version;
        assert!(!accept(&mut preview, 12, 1));
        assert!(!accept(&mut preview, 12, 3));
        assert!(!accept(&mut preview, 11, 2));
        assert_eq!((preview.segments, preview.version), (1, version));
        assert!(accept(&mut preview, 12, 2));
        assert_eq!(preview.segments, 2);
    }
}
