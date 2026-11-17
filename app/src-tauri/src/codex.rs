//! Recognises the Codex desktop app's message box, for the Ctrl+Alt+V draft shortcut.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, UIA_EditControlTypeId};

const CODEX_PACKAGE: &str = "OpenAI.Codex_2p2nqsd0c76g0";

/// Whether `process` is the Codex desktop app, by the package Windows installed it from.
fn is_codex(process: u32) -> bool {
    // SAFETY: the handle is closed before returning and the buffer length is passed with the buffer.
    unsafe {
        let Ok(opened) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process) else { return false };
        let mut name = [0u16; 128];
        let mut length = name.len() as u32;
        let found = GetPackageFamilyName(opened, &mut length, PWSTR(name.as_mut_ptr())) == ERROR_SUCCESS;
        let _ = CloseHandle(opened);
        let end = name.iter().position(|&unit| unit == 0).unwrap_or(0);
        found && String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case(CODEX_PACKAGE)
    }
}

/// Whether the keyboard is in Codex's message box rather than, say, its search field.
pub fn in_message_box(process: u32) -> bool {
    if !is_codex(process) {
        return false;
    }
    // SAFETY: COM is initialised on this thread before use; every call returns an owned object or an error.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(automation) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { return false };
        let Ok(focused) = automation.GetFocusedElement() else { return false };
        let editable = focused.CurrentControlType().is_ok_and(|kind| kind == UIA_EditControlTypeId)
            && focused.CurrentIsEnabled().is_ok_and(|enabled| enabled.as_bool())
            && focused.CurrentIsPassword().is_ok_and(|password| !password.as_bool());
        let class = focused.CurrentClassName().map(|name| name.to_string()).unwrap_or_default();
        let id = focused.CurrentAutomationId().map(|name| name.to_string()).unwrap_or_default();
        editable && is_composer(&class, &id)
    }
}

/// The message box is a ProseMirror editor; ordinary search boxes are not.
fn is_composer(class: &str, automation_id: &str) -> bool {
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
        assert!(!is_codex(std::process::id()));
    }
}
