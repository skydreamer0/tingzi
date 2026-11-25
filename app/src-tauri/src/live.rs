//! The text of one recording whose sentences are typed where the user was while they are still speaking.
//!
//! Each recognised piece is finished by itself and joined to the ones before it. What has been typed
//! is remembered, so a piece is typed once, and whatever could not be typed waits for the end of the
//! recording. Nothing here touches the keyboard or the clipboard.

use std::time::Duration;

use crate::apps::{self, AppRule};
use crate::streaming_asr::join;

/// The editable suffix after the last confirmed character sent to the target.
#[derive(Default)]
pub struct Tail {
    shown: String,
    draft: String,
    confirmed: Option<String>,
    /// Final for the draft already on screen, if a held shortcut delayed its correction.
    visible_final: Option<String>,
    pending: bool,
    final_only: bool,
    retracting: bool,
    pub updates: usize,
    pub deleted: usize,
    pub first_key: Option<Duration>,
}

pub enum KeyState { Ready, Held, Gone }

pub trait Keys {
    fn state(&mut self) -> KeyState;
    fn erase(&mut self, count: usize) -> bool;
    fn write(&mut self, text: &str) -> bool;
}

fn difference<'a>(old: &str, new: &'a str) -> (usize, &'a str) {
    let prefix: usize = old.chars().zip(new.chars()).take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8()).sum();
    (old[prefix..].chars().count(), &new[prefix..])
}

#[derive(Default)]
pub struct Typing {
    rule: Option<AppRule>,
    pieces: Vec<String>,
    typed: String,
    /// The sample position up to which the recording has been recognised.
    heard_to: usize,
    pub tail: Tail,
    away: bool,
    ended: bool,
}

impl Typing {
    pub fn new(rule: Option<AppRule>) -> Self {
        Self { rule, ..Self::default() }
    }

    /// Adds the finished text of the next piece, which ends at sample `end` of the recording.
    pub fn add(&mut self, piece: &str, end: usize) {
        self.pieces.push(piece.trim().to_owned());
        self.heard_to = end;
    }

    pub fn started(&self) -> bool {
        self.pieces.iter().any(|piece| !piece.is_empty())
    }

    pub fn heard_to(&self) -> usize {
        self.heard_to
    }

    /// Everything recognised so far, as it is delivered. A closing 「。」 the program's rule drops is
    /// left out, and comes back in front of the next piece when there is one.
    pub fn text(&self) -> String {
        apps::finish(&join(&self.pieces), self.rule.as_ref())
    }

    #[cfg(test)]
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// What is recognised but not in the user's window.
    pub fn waiting(&self) -> String {
        self.text().strip_prefix(self.typed.as_str()).unwrap_or_default().to_owned()
    }

    /// What to type now; None when there is nothing, the user has moved away, or the recording is over.
    pub fn next(&self) -> Option<String> {
        Some(self.waiting()).filter(|waiting| !waiting.is_empty() && !self.away && !self.ended)
    }

    pub fn note_typed(&mut self, text: &str) {
        self.typed.push_str(text);
    }

    /// The user moved away from where recording started; the rest is kept for the end.
    pub fn leave(&mut self) {
        self.away = true;
    }

    /// The recording is over, or was cancelled: nothing more is typed sentence by sentence.
    pub fn end(&mut self) {
        self.ended = true;
    }

    pub fn visible_prefix(&self, final_text: &str) -> String {
        let mut prefix = self.typed.clone();
        if let Some(rest) = final_text.strip_prefix(&prefix) {
            if rest.starts_with(&self.tail.shown) { prefix.push_str(&self.tail.shown); }
        }
        prefix
    }

    pub fn cancel_draft(&mut self) {
        // Correct an already visible utterance, but do not start typing later queued finals.
        self.tail.confirmed = self.tail.visible_final.take();
        self.retract();
        self.end();
    }

    pub fn away(&self) -> bool { self.away }

    pub fn segment(&mut self, piece: &str) {
        self.tail.retracting = false;
        let confirmed = self.text();
        // A preview can insert a pause comma before text already on screen.
        // Wait for the sentence final before rewriting the target for that punctuation.
        // ASCII commas (e.g. 1,200) and the final transcript are unchanged.
        self.pieces.push(piece.trim().replace('，', ""));
        let display = self.text();
        self.pieces.pop();
        self.tail.draft = display.strip_prefix(&confirmed).unwrap_or_default().to_owned();
        if self.tail.confirmed.is_none() && difference(&self.tail.shown, &self.tail.draft).0 > 20 {
            self.tail.final_only = true;
        }
        self.tail.pending = true;
    }

    /// A later utterance started before this one was final. Leave its visible text
    /// in place, but discard the queued revision so a timer cannot type stale text.
    pub fn freeze_segment(&mut self) {
        self.tail.draft.clear();
        self.tail.pending = self.tail.confirmed.is_some();
    }

    pub fn stream_final(&mut self, piece: &str, end: usize) {
        self.tail.retracting = false;
        self.add(piece, end);
        let waiting = self.waiting();
        if self.tail.confirmed.is_none() && !self.tail.shown.is_empty() {
            self.tail.visible_final = Some(waiting.clone());
        }
        self.tail.confirmed = Some(waiting);
        self.tail.draft.clear();
        self.tail.final_only = false;
        self.tail.pending = true;
    }

    /// Removing a draft uses the same guarded key path as correcting it.
    pub fn retract(&mut self) {
        self.tail.retracting = true;
        self.tail.draft.clear();
        self.tail.final_only = false;
        self.tail.pending = !self.tail.shown.is_empty() || self.tail.confirmed.is_some();
    }

    /// False means a held modifier requires a timed retry.
    pub fn flush(&mut self, now: Duration, keys: &mut impl Keys) -> bool {
        if self.away || !self.tail.pending { return true; }
        let tail = &mut self.tail;
        let final_pending = tail.confirmed.is_some();
        if !final_pending && tail.final_only { return true; }

        let mut desired = tail.confirmed.clone().unwrap_or_default();
        let confirmed_bytes = desired.len();
        desired.push_str(&tail.draft);
        let (erase, write) = difference(&tail.shown, &desired);
        if !final_pending && erase > 20 && !tail.retracting {
            tail.final_only = true;
            return true;
        }
        if erase > 0 {
            match keys.state() {
                KeyState::Gone => { self.leave(); return true; }
                KeyState::Held => return false,
                KeyState::Ready => {}
            }
            if !keys.erase(erase) { self.leave(); return true; }
            tail.deleted += erase;
            tail.shown.truncate(desired.len() - write.len());
        }
        if !write.is_empty() {
            match keys.state() {
                KeyState::Gone => { self.leave(); return true; }
                KeyState::Held => return false,
                KeyState::Ready => {}
            }
            if !keys.write(write) { self.leave(); return true; }
            tail.first_key.get_or_insert(now);
        }
        if !tail.retracting && (erase > 0 || !write.is_empty()) && (!final_pending || !tail.draft.is_empty()) {
            tail.updates += 1;
        }
        tail.shown = desired[confirmed_bytes..].to_owned();
        tail.pending = false;
        tail.visible_final = None;
        if let Some(confirmed) = tail.confirmed.take() {
            self.note_typed(&confirmed);
        }
        true
    }

    pub fn ended(&self) -> bool {
        self.ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Keyboard {
        held: bool,
        gone: bool,
        strokes: Vec<String>,
    }

    impl Keys for Keyboard {
        fn state(&mut self) -> KeyState {
            if self.gone { KeyState::Gone } else if self.held { KeyState::Held } else { KeyState::Ready }
        }
        fn erase(&mut self, count: usize) -> bool {
            self.strokes.push(format!("back {count}"));
            true
        }
        fn write(&mut self, text: &str) -> bool {
            self.strokes.push(text.into());
            true
        }
    }

    #[test]
    fn differences_count_characters_and_return_the_new_suffix() {
        for (old, new, erase, add) in [
            ("你😀AB", "你😀CD", 2, "CD"), ("👋好", "world", 2, "world"),
            ("A", "A中", 0, "中"), ("abc好", "ab", 2, ""), ("同", "同", 0, ""),
        ] {
            assert_eq!(difference(old, new), (erase, add));
        }
    }

    #[test]
    fn segments_update_immediately_and_large_deletions_wait_until_final() {
        let mut t = Typing::default();
        let mut k = Keyboard::default();
        let ms = Duration::from_millis;
        t.segment("A");
        t.flush(ms(50), &mut k);
        t.segment("AB");
        t.flush(ms(249), &mut k);
        let long = "A".repeat(22);
        t.segment(&long);
        t.flush(ms(300), &mut k);
        t.segment("B");
        t.flush(ms(550), &mut k);
        t.segment("Able");
        t.flush(ms(800), &mut k);
        t.stream_final("ABC", 100);
        t.flush(ms(810), &mut k);
        assert_eq!(k.strokes, ["A", "B", "back 1", &"A".repeat(21), "back 21", "BC"]);
        assert_eq!((t.typed(), t.heard_to()), ("ABC", 100));
        assert_eq!((t.tail.updates, t.tail.deleted, t.tail.first_key), (3, 22, Some(ms(50))));
    }

    #[test]
    fn release_applies_the_latest_draft_and_finalized_prefix_together() {
        let mut t = Typing::default();
        let mut k = Keyboard { held: true, ..Default::default() };
        t.segment("old");
        t.flush(Duration::ZERO, &mut k);
        t.stream_final("use", 5);
        t.flush(Duration::ZERO, &mut k);
        t.segment("one");
        t.segment("two");
        assert!(k.strokes.is_empty());
        k.held = false;
        t.flush(Duration::ZERO, &mut k);
        assert_eq!(k.strokes, ["use two"]);
        assert_eq!(t.typed(), "use");
        t.stream_final("too", 10);
        t.flush(Duration::ZERO, &mut k);
        assert_eq!(k.strokes, ["use two", "back 2", "oo"]);
        assert_eq!(t.typed(), "use too");
    }

    #[test]
    fn exactly_twenty_backspaces_are_allowed_and_unchanged_text_sends_no_keys() {
        let mut t = Typing::default();
        let mut k = Keyboard::default();
        let long = format!("A{}", "字".repeat(20));
        t.segment(&long);
        t.flush(Duration::ZERO, &mut k);
        t.segment("A");
        t.flush(Duration::ZERO, &mut k);
        t.segment("A");
        t.flush(Duration::ZERO, &mut k);
        t.stream_final("A", 1);
        t.flush(Duration::from_millis(510), &mut k);
        assert_eq!(k.strokes, [long.as_str(), "back 20"]);
        assert_eq!(t.tail.updates, 2);
    }

    #[test]
    fn late_pause_commas_do_not_retype_a_fast_preview() {
        let mut t = Typing::default();
        let mut k = Keyboard::default();
        t.segment("第一段ASR");
        t.flush(Duration::ZERO, &mut k);
        t.segment("第一段，ASR");
        t.flush(Duration::ZERO, &mut k);
        t.segment("第一段，ASR是1.25和1,200");
        t.flush(Duration::ZERO, &mut k);
        assert_eq!(k.strokes, ["第一段ASR", "是1.25和1,200"]);
        assert_eq!(t.tail.deleted, 0);

        // A final is an ordered barrier, even if the next preview is already queued.
        t.stream_final("第一段，ASR是1.25和1,200。", 100);
        t.segment("下一段");
        t.flush(Duration::ZERO, &mut k);
        assert_eq!(t.typed(), "第一段，ASR是1.25和1,200。");
        t.stream_final("下一段", 200);
        t.flush(Duration::ZERO, &mut k);
        let mut field = String::new();
        for stroke in &k.strokes {
            if let Some(count) = stroke.strip_prefix("back ") {
                for _ in 0..count.parse::<usize>().unwrap() {
                    assert!(field.pop().is_some());
                }
            } else {
                field.push_str(stroke);
            }
        }
        assert_eq!(field, "第一段，ASR是1.25和1,200。下一段");
        assert_eq!(t.typed(), field);
    }

    #[test]
    fn starting_a_later_utterance_discards_the_unsent_revision() {
        let mut t = Typing::default();
        let mut k = Keyboard::default();
        t.segment("早");
        t.flush(Duration::ZERO, &mut k);
        t.segment("早點來");
        t.freeze_segment();
        t.flush(Duration::from_millis(300), &mut k);
        t.stream_final("早安", 1);
        t.flush(Duration::from_millis(310), &mut k);
        assert_eq!(k.strokes, ["早", "安"]);
    }

    #[test]
    fn rollback_only_erases_the_provisional_suffix_when_still_at_target() {
        for (cancel, gone) in [(false, false), (true, false), (true, true)] {
            let mut t = Typing::new(Some(AppRule { drop_final_stop: true, ..Default::default() }));
            let mut k = Keyboard::default();
            t.stream_final("確認。", 100);
            t.flush(Duration::ZERO, &mut k);
            t.segment("中😀");
            t.flush(Duration::ZERO, &mut k);
            k.gone = gone;
            if cancel { t.cancel_draft(); } else { t.retract(); }
            t.flush(Duration::ZERO, &mut k);
            assert_eq!(t.typed(), "確認");
            assert_eq!(t.text(), "確認");
            if gone {
                k.gone = false;
                t.flush(Duration::from_secs(1), &mut k);
                assert_eq!(k.strokes, ["確認", "。中😀"]);
                assert!(t.away());
            } else {
                assert_eq!(k.strokes, ["確認", "。中😀", "back 3"]);
                assert_eq!(t.tail.deleted, 3);
            }
        }
    }

    #[test]
    fn each_piece_is_typed_once_and_joined_like_the_whole_text() {
        let mut typing = Typing::default();
        assert!(!typing.started());
        assert_eq!(typing.next(), None);
        typing.add(" 明天開會。 ", 100);
        assert_eq!(typing.next().as_deref(), Some("明天開會。"));
        typing.note_typed("明天開會。");
        assert_eq!(typing.next(), None);
        typing.add("use ABC", 200);
        typing.add("123 再見", 300);
        assert_eq!(typing.next().as_deref(), Some("use ABC 123 再見"));
        typing.note_typed("use ABC 123 再見");
        assert_eq!(typing.text(), "明天開會。use ABC 123 再見");
        assert_eq!((typing.typed(), typing.waiting().as_str(), typing.heard_to()), ("明天開會。use ABC 123 再見", "", 300));
    }

    #[test]
    fn a_piece_that_could_not_be_typed_waits_with_the_next_one() {
        let mut typing = Typing::default();
        typing.add("第一句，", 10);
        // The shortcut was still held, so nothing was typed.
        typing.add("第二句。", 20);
        assert_eq!(typing.next().as_deref(), Some("第一句，第二句。"));
    }

    #[test]
    fn cancel_after_a_held_final_keeps_the_visible_sentence_only() {
        let mut t = Typing::default();
        let mut k = Keyboard::default();
        t.segment("台杯");
        t.flush(Duration::ZERO, &mut k);
        k.held = true;
        t.stream_final("台北", 20);
        t.stream_final("第二句", 30);
        t.cancel_draft();
        t.flush(Duration::ZERO, &mut k);
        k.held = false;
        t.flush(Duration::ZERO, &mut k);
        assert_eq!(k.strokes, ["台杯", "back 1", "北"]);
    }

    #[test]
    fn nothing_is_typed_after_the_user_leaves_or_the_recording_ends() {
        let mut typing = Typing::default();
        typing.add("留在浮窗", 10);
        typing.leave();
        assert_eq!(typing.next(), None);
        assert_eq!(typing.waiting(), "留在浮窗");
        let mut typing = Typing::default();
        typing.add("已經停止", 10);
        typing.end();
        assert!(typing.ended());
        assert_eq!(typing.next(), None);
        assert_eq!(typing.waiting(), "已經停止");
    }

    #[test]
    fn a_dropped_closing_stop_returns_before_the_next_piece() {
        let rule = AppRule { drop_final_stop: true, ..AppRule::default() };
        let mut typing = Typing::new(Some(rule));
        typing.add("你好。", 10);
        assert_eq!(typing.next().as_deref(), Some("你好"));
        typing.note_typed("你好");
        typing.add("再見。", 20);
        assert_eq!(typing.next().as_deref(), Some("。再見"));
        typing.note_typed("。再見");
        assert_eq!(typing.text(), "你好。再見");
        assert_eq!(typing.waiting(), "");
    }
}
