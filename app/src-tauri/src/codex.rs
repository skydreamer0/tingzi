//! Recognises the Codex desktop app's message box, for the Ctrl+Alt+V draft shortcut.

/// Whether the keyboard is in Codex's message box rather than, say, its search field.
pub fn in_message_box(process: u32) -> bool {
    crate::sys::in_codex_message_box(process)
}

/// The message box is a ProseMirror editor; ordinary search boxes are not.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_composer(class: &str, automation_id: &str) -> bool {
    class.split_whitespace().any(|name| name == "ProseMirror") || automation_id == "prompt-textarea"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_message_box_counts() {
        assert!(is_composer("ProseMirror ProseMirror-focused", ""));
        assert!(is_composer("", "prompt-textarea"));
        assert!(!is_composer("search-input", "search"));
    }
}
