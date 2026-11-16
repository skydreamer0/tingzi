//! Tingzi voice window: press the shortcut, speak, and the text lands where you were typing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod deliver;
mod recognise;
mod settings;

use serde::Serialize;
use settings::Settings;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// What the window shows. `state` is idle, recording, working, done or error.
#[derive(Clone, Serialize)]
struct Voice {
    state: &'static str,
    text: String,
}

#[derive(Default)]
struct Session {
    recorder: Mutex<Option<audio::Recorder>>,
    /// The window that had the keyboard when recording started.
    target: Mutex<isize>,
    working: Mutex<bool>,
}

fn show(app: &AppHandle, state: &'static str, text: impl Into<String>) {
    let _ = app.emit("voice", Voice { state, text: text.into() });
}

/// Starts recording, or stops and recognises what was said.
fn toggle(app: &AppHandle) {
    let session = app.state::<Session>();
    if *session.working.lock().unwrap() {
        return;
    }
    let recording = session.recorder.lock().unwrap().take();
    match recording {
        None => {
            *session.target.lock().unwrap() = deliver::foreground();
            let meter = app.clone();
            match audio::Recorder::start(move |peak| {
                let _ = meter.emit("level", peak);
            }) {
                Ok(recorder) => {
                    *session.recorder.lock().unwrap() = Some(recorder);
                    show(app, "recording", "錄音中，再按一次停止");
                }
                Err(reason) => show(app, "error", reason),
            }
        }
        Some(recorder) => {
            *session.working.lock().unwrap() = true;
            show(app, "working", "辨識中…");
            let target = *session.target.lock().unwrap();
            let app = app.clone();
            // Recognition waits on the network, so it runs off the UI thread.
            std::thread::spawn(move || {
                let outcome = recorder.finish().and_then(recognise::transcribe).and_then(|heard| finish(&heard, target));
                match outcome {
                    Ok((text, note)) if text.is_empty() => show(&app, "error", note),
                    Ok((text, _)) => show(&app, "done", text),
                    Err(reason) => show(&app, "error", reason),
                }
                *app.state::<Session>().working.lock().unwrap() = false;
            });
        }
    }
}

/// Applies the script choice and delivers the text. Returns the text and a note for the empty case.
fn finish(heard: &str, target: isize) -> Result<(String, String), String> {
    let prefs = Settings::load();
    // The service may answer in Simplified characters; settle on Traditional before the user's choice.
    let text = deliver::convert_script(&deliver::convert_script(heard, false), prefs.simplified);
    if text.is_empty() {
        return Ok((text, "沒有辨識到文字".into()));
    }
    if prefs.inserts() {
        deliver::insert(&text, target)?;
    } else if prefs.copies() {
        arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text.clone())).map_err(|_| "無法寫入剪貼簿".to_string())?;
    }
    Ok((text, String::new()))
}

#[tauri::command]
fn toggle_recording(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
fn shortcut_label(label: State<'_, ShortcutLabel>) -> String {
    label.0.clone()
}

struct ShortcutLabel(String);

/// The recording shortcut from the saved preferences; None when the user turned it off.
fn shortcut(prefs: &Settings) -> Option<(Shortcut, String)> {
    const LETTERS: [Code; 26] = [
        Code::KeyA, Code::KeyB, Code::KeyC, Code::KeyD, Code::KeyE, Code::KeyF, Code::KeyG, Code::KeyH, Code::KeyI,
        Code::KeyJ, Code::KeyK, Code::KeyL, Code::KeyM, Code::KeyN, Code::KeyO, Code::KeyP, Code::KeyQ, Code::KeyR,
        Code::KeyS, Code::KeyT, Code::KeyU, Code::KeyV, Code::KeyW, Code::KeyX, Code::KeyY, Code::KeyZ,
    ];
    // A development override keeps this app from fighting the installed voice window for the same keys.
    let key = std::env::var("TINGZI_RECORD_KEY").ok().and_then(|value| value.bytes().next()).map(|b| u32::from(b.to_ascii_uppercase())).unwrap_or(prefs.record_key);
    let index = key.checked_sub(u32::from(b'A')).filter(|i| *i < 26)? as usize;
    let (modifiers, name) = match prefs.modifiers {
        5 => (Modifiers::ALT | Modifiers::SHIFT, "Alt+Shift"),
        6 => (Modifiers::CONTROL | Modifiers::SHIFT, "Ctrl+Shift"),
        _ => (Modifiers::CONTROL | Modifiers::ALT, "Ctrl+Alt"),
    };
    Some((Shortcut::new(Some(modifiers), LETTERS[index]), format!("{name}+{}", (b'A' + index as u8) as char)))
}

fn main() {
    // `Tingzi --transcribe a.wav [b.wav ...]` checks the built-in recognition without the window.
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() >= 3 && arguments[1] == "--transcribe" {
        for file in &arguments[2..] {
            let started = std::time::Instant::now();
            let outcome = std::fs::read(file).map_err(|_| "無法讀取音檔".to_string()).and_then(recognise::transcribe);
            match outcome {
                Ok(text) => println!("{}", deliver::convert_script(&text, false)),
                Err(reason) => {
                    eprintln!("{reason}");
                    std::process::exit(1);
                }
            }
            eprintln!("{} ms", started.elapsed().as_millis());
        }
        return;
    }

    let prefs = Settings::load();
    let chosen = shortcut(&prefs);
    let label = chosen.as_ref().map(|(_, name)| name.clone()).unwrap_or_else(|| "未設定快捷鍵".into());
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle(app);
                    }
                })
                .build(),
        )
        .manage(Session::default())
        .manage(ShortcutLabel(label))
        .invoke_handler(tauri::generate_handler![toggle_recording, shortcut_label])
        .setup(move |app| {
            if let Some((keys, _)) = chosen {
                // Another program may already hold the shortcut; the mic button still works.
                if app.global_shortcut().register(keys).is_err() {
                    show(app.handle(), "error", "錄音快捷鍵被其他程式占用，請點麥克風");
                }
            }
            let quit = MenuItem::with_id(app, "quit", "結束聽字", true, None::<&str>)?;
            let reveal = MenuItem::with_id(app, "show", "顯示浮窗", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&reveal, &quit])?;
            let mut tray = TrayIconBuilder::new().tooltip("聽字・注音與語音輸入法").menu(&menu).on_menu_event(|app, event| match event.id().as_ref() {
                "quit" => app.exit(0),
                "show" => {
                    if let Some(window) = app.get_webview_window("capsule") {
                        let _ = window.show();
                    }
                }
                _ => {}
            });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the voice window could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_follows_saved_preferences() {
        std::env::remove_var("TINGZI_RECORD_KEY");
        let (_, name) = shortcut(&Settings { modifiers: 6, record_key: u32::from(b'T'), ..Settings::default() }).unwrap();
        assert_eq!(name, "Ctrl+Shift+T");
        assert!(shortcut(&Settings { record_key: 0, ..Settings::default() }).is_none());
    }
}
