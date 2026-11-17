//! One running copy of the app, and how a second launch or the Zhuyin text service talks to it.
//!
//! The names match the earlier voice window, so the text service and the Start menu shortcuts
//! work with either program.

use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForMultipleObjects, EVENT_MODIFY_STATE, INFINITE};

/// Something asked of the running app from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Show,
    Settings,
    Edit,
    /// Recognise this WAV file, or let the user pick one when None.
    OpenWav(Option<PathBuf>),
    /// The Zhuyin input method switched between Chinese and English.
    Chinese,
    English,
    /// Letters and digits switched between full-width and half-width.
    FullWidth,
    HalfWidth,
}

const SIGNALS: [(&str, fn() -> Request); 8] = [
    ("TingziFloatShow020", || Request::Show),
    ("TingziVoiceSettings", || Request::Settings),
    ("TingziEditPreview", || Request::Edit),
    ("TingziOpenWav", || Request::OpenWav(waiting_file())),
    ("TingziChinese", || Request::Chinese),
    ("TingziEnglish", || Request::English),
    ("TingziFullWidth", || Request::FullWidth),
    ("TingziHalfWidth", || Request::HalfWidth),
];

/// A development build started with TINGZI_RECORD_KEY uses its own names, so it can run beside the installed app.
fn name(base: &str) -> Vec<u16> {
    let suffix = if std::env::var_os("TINGZI_RECORD_KEY").is_some() { "Dev" } else { "" };
    format!("Local\\{base}{suffix}").encode_utf16().chain(std::iter::once(0)).collect()
}

fn request_file() -> PathBuf {
    crate::paths::data().join("open-wav-request.txt")
}

fn waiting_file() -> Option<PathBuf> {
    let asked = std::fs::read_to_string(request_file()).ok();
    let _ = std::fs::remove_file(request_file());
    asked.map(|path| path.trim().to_string()).filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// What the command line asks for. None means the arguments are not understood.
pub fn parse(arguments: &[String]) -> Option<Option<Request>> {
    match arguments {
        [] => Some(None),
        [flag] if flag == "--show" => Some(Some(Request::Show)),
        [flag] if flag == "--settings" => Some(Some(Request::Settings)),
        [flag] if flag == "--edit" => Some(Some(Request::Edit)),
        [flag] if flag == "--open-wav" => Some(Some(Request::OpenWav(None))),
        [flag, file] if flag == "--open-wav" => Some(Some(Request::OpenWav(Some(std::path::absolute(file).ok()?)))),
        _ => None,
    }
}

/// Claims the single-instance name. False means another copy is already running.
pub fn first() -> bool {
    let name = name("TingziFloat020");
    // SAFETY: the name is null-terminated; the handle is kept for the life of the process on purpose.
    unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())).is_ok() && GetLastError() != ERROR_ALREADY_EXISTS }
}

/// Passes a request to the copy that is already running. False when it could not be reached.
pub fn forward(request: &Request) -> bool {
    if let Request::OpenWav(file) = request {
        let path = file.as_ref().map(|file| file.to_string_lossy().into_owned()).unwrap_or_default();
        if std::fs::create_dir_all(crate::paths::data()).and_then(|()| std::fs::write(request_file(), path)).is_err() {
            return false;
        }
    }
    let Some((signal, _)) = SIGNALS.iter().find(|(_, make)| std::mem::discriminant(&make()) == std::mem::discriminant(request)) else { return false };
    let name = name(signal);
    // SAFETY: the name is null-terminated and the handle is only used to raise the signal.
    unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())).is_ok_and(|event| SetEvent(event).is_ok()) }
}

/// Listens for requests for the life of the app and hands each one to `handle`.
pub fn listen(handle: impl Fn(Request) + Send + 'static) {
    // Handles are plain numbers here so the list can move to the listening thread.
    let events: Vec<isize> = SIGNALS
        .iter()
        .filter_map(|(signal, _)| {
            let name = name(signal);
            // SAFETY: the name is null-terminated; auto-reset events start unsignalled.
            unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())).ok() }.map(|event| event.0 as isize)
        })
        .collect();
    if events.len() != SIGNALS.len() {
        return;
    }
    std::thread::spawn(move || {
        let events: Vec<HANDLE> = events.into_iter().map(|event| HANDLE(event as *mut core::ffi::c_void)).collect();
        loop {
            // SAFETY: the handles stay open for the life of the process.
            let raised = unsafe { WaitForMultipleObjects(&events, false, INFINITE) };
            let index = raised.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
            match SIGNALS.get(index) {
                Some((_, make)) => handle(make()),
                None => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn command_line_is_understood() {
        assert_eq!(parse(&[]), Some(None));
        assert_eq!(parse(&arguments(&["--settings"])), Some(Some(Request::Settings)));
        assert_eq!(parse(&arguments(&["--show"])), Some(Some(Request::Show)));
        assert_eq!(parse(&arguments(&["--open-wav"])), Some(Some(Request::OpenWav(None))));
        assert!(matches!(parse(&arguments(&["--open-wav", "a.wav"])), Some(Some(Request::OpenWav(Some(file)))) if file.is_absolute()));
        assert_eq!(parse(&arguments(&["--unknown"])), None);
        assert_eq!(parse(&arguments(&["--show", "extra"])), None);
    }
}
