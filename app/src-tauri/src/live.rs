//! The text of one recording whose sentences are typed where the user was while they are still speaking.
//!
//! Each recognised piece is finished by itself and joined to the ones before it. What has been typed
//! is remembered, so a piece is typed once, and whatever could not be typed waits for the end of the
//! recording. Nothing here touches the keyboard or the clipboard.

use crate::apps::{self, AppRule};
use crate::streaming_asr::join;

#[derive(Default)]
pub struct Typing {
    rule: Option<AppRule>,
    pieces: Vec<String>,
    typed: String,
    typed_pieces: usize,
    /// The sample position up to which the recording has been recognised.
    heard_to: usize,
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

    pub fn revise(&mut self, index: usize, piece: &str) -> bool {
        if index < self.typed_pieces { return false; }
        self.pieces[index] = piece.trim().to_owned();
        true
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
        self.typed_pieces = self.pieces.len();
    }

    /// The user moved away from where recording started; the rest is kept for the end.
    pub fn leave(&mut self) {
        self.away = true;
    }

    /// The recording is over, or was cancelled: nothing more is typed sentence by sentence.
    pub fn end(&mut self) {
        self.ended = true;
    }

    pub fn ended(&self) -> bool {
        self.ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn only_sentences_still_waiting_can_be_revised() {
        let mut typing = Typing::default();
        typing.add("第一句", 10);
        typing.note_typed("第一句");
        typing.add("未定", 20);
        assert!(!typing.revise(0, "晚到"));
        assert!(typing.revise(1, "第二句"));
        assert_eq!(typing.next().as_deref(), Some("第二句"));
        assert_eq!(typing.text(), "第一句第二句");
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
