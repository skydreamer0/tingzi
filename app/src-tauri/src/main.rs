//! Tingzi voice window: press the shortcut, speak, and the text lands where you were typing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod apps;
mod audio;
mod codex;
mod deliver;
mod dialogs;
mod fetch;
mod history;
mod hotkey;
mod instance;
mod layout;
mod learn;
mod panel;
mod paths;
mod recognise;
mod refine;
mod replace;
mod secret;
mod settings;
mod sys;
mod tidy;
mod update;
mod words;

use deliver::{Desktop, Mode, Outcome, Spot};
use instance::Request;
use layout::Layout;
use serde::Serialize;
use settings::{Engine, Settings};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// What the window shows. `state` is idle, recording, working, done, retry or error.
#[derive(Clone, Serialize)]
struct Voice {
    state: &'static str,
    message: String,
    /// The recognised text, kept so the user can edit, copy or paste it.
    text: String,
}

#[derive(Default)]
struct Session {
    recorder: Mutex<Option<audio::Recorder>>,
    /// Goes up whenever work starts or is cancelled; a result from an older run is dropped.
    run: AtomicU64,
    working: AtomicBool,
    state: Mutex<&'static str>,
    message: Mutex<String>,
    preview: Mutex<String>,
    /// A recording kept after online recognition failed, so this computer can recognise it instead.
    kept: Mutex<Option<Vec<u8>>>,
    draft_shortcut: AtomicBool,
    /// Whether the Zhuyin input method is typing Chinese or English, once it has said so.
    mode: Mutex<&'static str>,
    /// Whether the input method is typing full-width letters and digits.
    full_width: AtomicBool,
    /// When the shortcut went down to start a recording, while it may still be held.
    held_since: Mutex<Option<Instant>>,
    /// The selected text the running recording is a command for, after Ctrl+Alt+E.
    rewrite: Mutex<Option<deliver::Selection>>,
}

impl Session {
    /// The input method's state as the window shows it, or empty before it has said anything.
    fn mode_label(&self) -> String {
        let mode = *self.mode.lock().unwrap();
        match (mode.is_empty(), self.full_width.load(Ordering::SeqCst)) {
            (true, false) => String::new(),
            (true, true) => "全形".into(),
            (false, false) => mode.into(),
            (false, true) => format!("{mode}・全形"),
        }
    }
}

fn draft_keys() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyV)
}

/// Ctrl+Alt+E rewrites the selected text, when the user turned that on.
fn rewrite_keys() -> Shortcut {
    hotkey::REWRITE.shortcut_unchecked()
}

/// Holds Ctrl+Alt+E while rewriting is on, and lets it go otherwise. Another program may hold it; the settings
/// page then says so.
pub fn rewrite_follows(app: &AppHandle, prefs: &Settings) -> bool {
    let shortcuts = app.global_shortcut();
    let held = shortcuts.is_registered(rewrite_keys());
    match (prefs.rewrite_selection, held) {
        (true, false) => shortcuts.register(rewrite_keys()).is_ok(),
        (false, true) => {
            let _ = shortcuts.unregister(rewrite_keys());
            true
        }
        _ => true,
    }
}

/// Alt+Shift+Z pastes the most recent dictation again, wherever the keyboard is now.
fn last_keys() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyZ)
}

/// Tells the window what happened, without changing its shape.
fn announce(app: &AppHandle, state: &'static str, message: impl Into<String>, text: impl Into<String>) {
    let (message, text) = (message.into(), text.into());
    let session = app.state::<Session>();
    *session.state.lock().unwrap() = state;
    *session.message.lock().unwrap() = message.clone();
    *session.preview.lock().unwrap() = text.clone();
    // Ctrl+Alt+V is taken only while there is text to paste, so other programs keep it otherwise.
    let wanted = state == "done" && !text.is_empty();
    if wanted != session.draft_shortcut.load(Ordering::SeqCst) {
        let shortcuts = app.global_shortcut();
        let changed = if wanted { shortcuts.register(draft_keys()).is_ok() } else { shortcuts.unregister(draft_keys()).is_ok() };
        if changed {
            session.draft_shortcut.store(wanted, Ordering::SeqCst);
        }
    }
    let _ = app.emit("voice", Voice { state, message, text });
}

/// Announces a change and gives the window the shape that suits it.
/// `expand` opens the editor so text that was not inserted is in front of the user.
fn show(app: &AppHandle, state: &'static str, message: impl Into<String>, text: impl Into<String>, expand: bool) {
    announce(app, state, message, text);
    layout::shorten(app, matches!(state, "recording" | "working"));
    let now = layout::current(app);
    match state {
        "done" if expand => layout::reshape(app, Layout::Editor),
        // A dictation started from the keycap opens the pill for as long as it runs.
        "recording" | "working" if now != Layout::Editor => layout::open_for_work(app),
        // A problem keeps the pill open until the user deals with it.
        "error" | "retry" if now != Layout::Editor => layout::reshape(app, Layout::Bar),
        _ if now == Layout::Editor => layout::reshape(app, Layout::Bar),
        _ => {}
    }
    if matches!(state, "done" | "idle") && !expand {
        layout::close_soon(app);
    }
}

fn idle(app: &AppHandle, message: &str) {
    show(app, "idle", message, "", false);
}

/// Traditional first, so the user's own words and replacement rules can be matched, then the script the user chose.
/// A dictation runs the same two halves with refinement between them; see `refined`.
#[cfg(test)]
fn polish(heard: &str, prefs: &Settings) -> String {
    correct(&clean(heard, prefs), prefs)
}

/// The steps before refinement: Traditional characters, then tidied speech.
fn clean(heard: &str, prefs: &Settings) -> String {
    let traditional = deliver::convert_script(heard.trim(), false);
    if prefs.tidy { tidy::tidy(&traditional) } else { traditional }
}

/// The steps after refinement, so the user's own words, words learned from fixes and replacement rules undo
/// any spelling the model changed; then the script the user chose.
fn correct(text: &str, prefs: &Settings) -> String {
    let corrected = if prefs.use_phrases { words::apply(text) } else { text.to_string() };
    let corrected = if prefs.learn_fixes { learn::apply(&corrected) } else { corrected };
    let replaced = if prefs.use_replacements { replace::apply(&corrected) } else { corrected };
    deliver::convert_script(&replaced, prefs.simplified)
}

/// Tidies cleaned speech into written text with the user's service, when this dictation is refined. `chosen` says
/// Refine is on in the main settings. A failure keeps the text as it was and says why, so the dictation still goes
/// through; the status is None only when Refine is off everywhere, so there is nothing to tell.
/// Blocks; call it on the dictation's own thread.
fn refined(app: &AppHandle, run: u64, cleaned: String, prefs: &Settings, chosen: bool) -> (String, Option<refine::Status>) {
    // Nothing is read from the key store, and nothing is sent, while refinement is off.
    if !prefs.refine {
        return (cleaned, chosen.then_some(refine::Status::OffHere));
    }
    if let Some(skipped) = refine::skipped(prefs, chosen, secret::refine_exists(), &cleaned) {
        return (cleaned, Some(skipped));
    }
    working(app, run, refine::WORKING);
    let started = std::time::Instant::now();
    match refine::refine(&cleaned, &prefs.refine_url, &prefs.refine_model, &refine::Ask::of(prefs)) {
        // The model may answer in Simplified characters; the user's words are matched in Traditional.
        Ok(text) => (deliver::convert_script(&text, false), Some(refine::Status::Refined(started.elapsed()))),
        Err(failure) => (cleaned, Some(refine::Status::Failed(failure))),
    }
}

/// Changes what the window says while a dictation is worked on, unless the user has cancelled it by now.
/// It runs on the main thread, as cancelling does, so a cancelled dictation never shows as working again.
fn working(app: &AppHandle, run: u64, message: &'static str) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let session = handle.state::<Session>();
        if session.run.load(Ordering::SeqCst) == run && session.working.load(Ordering::SeqCst) {
            show(&handle, "working", message, "", false);
        }
    });
}

/// The user's rule for the program they were typing in; None when there is no such place or no rule for it.
fn rule_for(target: Option<Spot>) -> Option<apps::AppRule> {
    let spot = target.filter(|spot| spot.process != 0 && !spot.ours())?;
    let program = apps::program(spot.process)?;
    apps::find(&apps::load(&apps::own_file()), &program).cloned()
}

enum Sound {
    Recording(audio::Recorder),
    Wav(Vec<u8>),
}

/// Recognises `sound` in the background and delivers the text.
/// `target` is where the user was typing; None shows the text without inserting it.
fn recognise(app: &AppHandle, sound: Sound, engine: Engine, target: Option<Spot>) {
    let session = app.state::<Session>();
    let run = session.run.fetch_add(1, Ordering::SeqCst) + 1;
    session.working.store(true, Ordering::SeqCst);
    let rewrite = session.rewrite.lock().unwrap().take();
    show(app, "working", recognise::working_label(engine, &Settings::load()), "", false);
    let app = app.clone();
    std::thread::spawn(move || {
        let session = app.state::<Session>();
        let cancelled = || session.run.load(Ordering::SeqCst) != run;
        if let Some(selection) = rewrite {
            return rewrite_with(&app, run, sound, engine, target, selection);
        }
        // The program being dictated into may change how the text is finished and delivered.
        let rule = rule_for(target);
        let main = Settings::load();
        let prefs = apps::effective(&main, rule.as_ref());
        let wav = match sound {
            Sound::Recording(recorder) => recorder.finish(),
            Sound::Wav(wav) => Ok(wav),
        };
        let heard = wav.clone().and_then(|wav| recognise::run(engine, &prefs, wav, &cancelled));
        if cancelled() {
            return;
        }
        // Refinement goes between tidying and the user's own words, so their words win over the model's.
        // What the text would have been without the model is kept with a refined dictation, to compare and get back.
        let (mut refinement, mut before) = (None, None);
        let text = heard.map(|heard| {
            let finish = |text: &str| apps::finish(&correct(text, &prefs), rule.as_ref());
            let cleaned = clean(&heard, &prefs);
            let (text, status) = refined(&app, run, cleaned.clone(), &prefs, main.refine);
            if matches!(status, Some(refine::Status::Refined(_))) {
                before = Some(finish(&cleaned));
            }
            refinement = status;
            finish(&text)
        });
        // A request cannot be stopped once sent, so a cancel while refining is honoured when it returns.
        if cancelled() {
            return;
        }
        // An online failure keeps the recording so it can be tried on this computer.
        let can_retry = engine != Engine::Local && recognise::problem(Engine::Local, &prefs).is_none();
        let (state, message, text, expand, settles) = match (text, wav) {
            (Ok(text), _) if !text.is_empty() => {
                let outcome = match target {
                    Some(spot) => deliver::deliver(&text, prefs.mode(), &mut Desktop::new(spot), &cancelled),
                    None => deliver::deliver(&text, Mode::Preview, &mut Desktop::new(Spot::default()), &cancelled),
                };
                if outcome.cancelled || cancelled() {
                    return;
                }
                let settles = outcome.settles();
                let Outcome { message, expand, inserted, .. } = outcome;
                if prefs.keep_history {
                    history::add(&text, before.filter(|before| *before != text), inserted);
                }
                // Words learned from the user's fixes since the last dictation are mentioned once.
                // Whether the text was refined, and why not, is told after the usual message.
                let message = match refinement {
                    Some(status) => format!("{message}{}", status.suffix()),
                    None => message.to_string(),
                };
                let learned = learn::take_fresh();
                let message = if learned.is_empty() { message } else { format!("{message}・已學會「{}」", learned.join("、")) };
                ("done", message, text, expand, settles)
            }
            (failed, Ok(wav)) if can_retry => {
                *session.kept.lock().unwrap() = Some(wav);
                let reason = failed.err().unwrap_or_else(|| "線上未回傳文字".into());
                ("retry", format!("{reason}・點圖示改用本機辨識"), String::new(), false, false)
            }
            (Ok(_), _) => ("error", "沒有辨識到文字，點圖示重新錄音".to_string(), String::new(), false, false),
            (Err(reason), _) => ("error", reason, String::new(), false, false),
        };
        if state != "retry" {
            deliver::end();
        }
        session.working.store(false, Ordering::SeqCst);
        show(&app, state, message, text, expand);
        // The page folds the preview back once the user has left it alone for this long.
        if settles {
            let _ = app.emit("settle", deliver::SETTLE_SECONDS);
        }
    });
}

/// The second half of Ctrl+Alt+E: recognises the spoken command, has the service rewrite the selection, and pastes
/// the result over it. A failure leaves the selected text and the clipboard as they were.
fn rewrite_with(app: &AppHandle, run: u64, sound: Sound, engine: Engine, target: Option<Spot>, selection: deliver::Selection) {
    let session = app.state::<Session>();
    let cancelled = || session.run.load(Ordering::SeqCst) != run;
    let prefs = Settings::load();
    let wav = match sound {
        Sound::Recording(recorder) => recorder.finish(),
        Sound::Wav(wav) => Ok(wav),
    };
    let heard = wav.and_then(|wav| recognise::run(engine, &prefs, wav, &cancelled));
    if cancelled() {
        return;
    }
    let (state, message, text, expand) = match heard {
        Ok(heard) if !heard.trim().is_empty() => {
            working(app, run, "改寫中・再點一下取消");
            let command = clean(&heard, &prefs);
            match refine::rewrite(&selection.text, &command, &prefs.refine_url, &prefs.refine_model) {
                _ if cancelled() => return,
                Ok(rewritten) => {
                    let text = correct(&deliver::convert_script(&rewritten, false), &prefs);
                    let spot = target.unwrap_or_default();
                    let outcome = deliver::deliver(&text, Mode::Insert, &mut Desktop::new(spot), &cancelled);
                    if outcome.cancelled || cancelled() {
                        return;
                    }
                    if outcome.inserted {
                        deliver::give_back(&text, selection.clipboard.as_deref());
                        ("done", format!("已照「{command}」改寫選取的文字"), text, false)
                    } else {
                        ("done", format!("已照「{command}」改寫・{}", outcome.message), text, outcome.expand)
                    }
                }
                Err(failure) => ("error", format!("沒有改寫：{}", failure.sentence()), String::new(), false),
            }
        }
        Ok(_) => ("error", "沒有聽到要怎麼改，選取的文字沒有變動".to_string(), String::new(), false),
        Err(reason) => ("error", reason, String::new(), false),
    };
    deliver::end();
    session.working.store(false, Ordering::SeqCst);
    show(app, state, message, text, expand);
}

/// Ctrl+Alt+E: copies the selected text and starts recording the command for it; pressing it again sends.
fn rewrite_key(app: &AppHandle) {
    let session = app.state::<Session>();
    if session.rewrite.lock().unwrap().is_some() {
        if session.recorder.lock().unwrap().is_some() {
            toggle(app);
        }
        return;
    }
    if busy(app) {
        return;
    }
    let prefs = Settings::load();
    if !prefs.rewrite_selection || refine::problem(&prefs.refine_url, &prefs.refine_model, secret::refine_exists()).is_some() {
        return show(app, "error", "用語音改寫選取的文字，要先在設定的「語音」頁填好 AI 潤稿服務", "", false);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let taken = deliver::take_selection();
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let session = handle.state::<Session>();
            match taken {
                Some(selection) if !busy(&handle) => {
                    *session.rewrite.lock().unwrap() = Some(selection);
                    toggle(&handle);
                    if session.recorder.lock().unwrap().is_some() {
                        let message = format!("說出要怎麼改，例如「改得客氣一點」，再按一次 {} 送出", hotkey::REWRITE.label());
                        show(&handle, "recording", message, "", false);
                    } else {
                        session.rewrite.lock().unwrap().take();
                    }
                }
                Some(_) => {}
                None => show(&handle, "error", "請先選取要改寫的文字，再按一次", "", false),
            }
        });
    });
}

/// The mic button and the recording shortcut: start, stop, cancel, or retry on this computer.
fn toggle(app: &AppHandle) {
    let session = app.state::<Session>();
    if session.working.swap(false, Ordering::SeqCst) {
        session.run.fetch_add(1, Ordering::SeqCst);
        session.rewrite.lock().unwrap().take();
        deliver::end();
        idle(app, "已取消");
        return;
    }
    if let Some(recorder) = session.recorder.lock().unwrap().take() {
        recognise(app, Sound::Recording(recorder), Settings::load().engine(), Some(deliver::target()));
        return;
    }
    if let Some(wav) = session.kept.lock().unwrap().take() {
        recognise(app, Sound::Wav(wav), Engine::Local, Some(deliver::target()));
        return;
    }
    let prefs = Settings::load();
    deliver::begin();
    let meter = app.clone();
    let started = audio::Recorder::start(prefs.microphone_name.clone(), move |peak| {
        let _ = meter.emit("level", peak);
    });
    match started {
        Ok(recorder) => {
            *session.recorder.lock().unwrap() = Some(recorder);
            let run = session.run.fetch_add(1, Ordering::SeqCst) + 1;
            // While the user speaks: the connection to an online service is opened, and words typed
            // to fix earlier dictations are picked up, which needs those dictations to have been kept.
            let ahead = prefs.clone();
            std::thread::spawn(move || {
                recognise::warm(ahead.engine(), &ahead);
                if ahead.learn_fixes && ahead.keep_history {
                    learn::catch_up(panel::typed_words());
                }
            });
            show(app, "recording", if prefs.hold_to_talk { "錄音中，放開快捷鍵送出" } else { "錄音中，再按一次停止" }, "", false);
            stop_at_limit(app.clone(), run);
        }
        Err(reason) => {
            deliver::end();
            show(app, "error", reason, "", false);
        }
    }
}

/// The recording shortcut going down or coming up.
/// With hold-to-talk, holding records and releasing sends; a tap shorter than a third of a second
/// latches instead, so the next press stops, as it does without hold-to-talk.
fn record_key(app: &AppHandle, pressed: bool) {
    let session = app.state::<Session>();
    let recording = || session.recorder.lock().unwrap().is_some();
    if pressed {
        let was_recording = recording();
        toggle(app);
        let started = !was_recording && recording();
        *session.held_since.lock().unwrap() = (started && Settings::load().hold_to_talk).then(Instant::now);
    } else if let Some(since) = session.held_since.lock().unwrap().take() {
        if since.elapsed() >= Duration::from_millis(350) && recording() {
            toggle(app);
        }
    }
}

/// Stops a recording that has run for ten minutes.
fn stop_at_limit(app: AppHandle, run: u64) {
    std::thread::spawn(move || {
        for _ in 0..audio::LONGEST_SECONDS {
            std::thread::sleep(Duration::from_secs(1));
            if app.state::<Session>().run.load(Ordering::SeqCst) != run {
                return;
            }
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let session = handle.state::<Session>();
            if session.run.load(Ordering::SeqCst) == run && session.recorder.lock().unwrap().is_some() {
                toggle(&handle);
            }
        });
    });
}

/// Drops whatever is in progress and any kept recording or text.
fn discard(app: &AppHandle) {
    let session = app.state::<Session>();
    session.run.fetch_add(1, Ordering::SeqCst);
    session.working.store(false, Ordering::SeqCst);
    session.recorder.lock().unwrap().take();
    session.kept.lock().unwrap().take();
    session.rewrite.lock().unwrap().take();
    deliver::end();
    idle(app, "");
}

/// Whether a recording is running or a dictation is being recognised, so nothing should interrupt the user.
pub fn busy(app: &AppHandle) -> bool {
    let session = app.state::<Session>();
    let recording = session.recorder.lock().unwrap().is_some();
    recording || session.working.load(Ordering::SeqCst)
}

/// Recognises a WAV file, asking the user to pick one when none is given. The text is shown, never inserted.
fn import(app: &AppHandle, chosen: Option<std::path::PathBuf>) {
    if busy(app) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(file) = chosen.or_else(|| dialogs::pick(0, dialogs::Kind::Wav, None)) else { return };
        let prefs = Settings::load();
        let local_wav = file.extension().is_some_and(|kind| kind.eq_ignore_ascii_case("wav")) && !file.to_string_lossy().starts_with(r"\\");
        if !local_wav {
            return show(&app, "error", "請選本機的 WAV 音檔", "", false);
        }
        if let Some(going) = recognise::destination(prefs.engine(), &prefs) {
            if !dialogs::confirm(0, &format!("將這個 WAV {going}辨識？")) {
                return;
            }
        }
        let sound = std::fs::read(&file).map_err(|_| "無法讀取這個音檔".to_string()).and_then(|bytes| audio::normalise(&bytes));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || match sound {
            Ok(wav) if !busy(&handle) => recognise(&handle, Sound::Wav(wav), prefs.engine(), None),
            Ok(_) => {}
            Err(reason) => show(&handle, "error", reason, "", false),
        });
    });
}

/// Ctrl+Alt+V: pastes the preview into Codex's message box, and nowhere else.
fn paste_draft(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let text = app.state::<Session>().preview.lock().unwrap().clone();
        if text.is_empty() {
            return;
        }
        let here = Spot::current();
        let pasted = codex::in_message_box(here.process) && deliver::deliver(&text, Mode::Insert, &mut Desktop::here(here), &|| false).inserted;
        if *app.state::<Session>().preview.lock().unwrap() == text {
            announce(&app, "done", if pasted { "已送出貼上鍵" } else { "請點複製，再到草稿貼上" }, text);
        }
    });
}

/// Alt+Shift+Z: pastes the most recent dictation where the keyboard is now.
fn paste_last(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(latest) = history::load().into_iter().next() else {
            let _ = app.emit("notice", "還沒有辨識紀錄");
            return;
        };
        let pasted = deliver::deliver(&latest.text, Mode::Insert, &mut Desktop::here(Spot::current()), &|| false).inserted;
        let _ = app.emit("notice", if pasted { "已貼上上一則" } else { "上一則已複製，請手動貼上" });
    });
}

fn copy_preview_text(app: &AppHandle) -> bool {
    let text = app.state::<Session>().preview.lock().unwrap().clone();
    let copied = !text.is_empty() && deliver::copy(&text);
    let _ = app.emit("notice", if copied { "已複製文字" } else if text.is_empty() { "目前沒有辨識文字" } else { "剪貼簿忙碌，請再試一次" });
    copied
}

/// Turns the clipboard text into the other script, for the bar's button and the tray menu.
fn convert_clipboard(app: &AppHandle) {
    // Delivery pastes through the clipboard, so it is left alone while a dictation is in progress.
    if busy(app) {
        return;
    }
    let message = match deliver::clipboard_text().and_then(|text| deliver::other_script(&text)) {
        None => "剪貼簿沒有可轉換的文字",
        Some((text, _)) if !deliver::copy(&text) => "剪貼簿忙碌，請再試一次",
        Some((_, true)) => "已轉成簡體",
        Some((_, false)) => "已轉成繁體",
    };
    let _ = app.emit("notice", message);
}

/// Opens the recognised text for editing.
fn edit(app: &AppHandle) {
    if app.state::<Session>().preview.lock().unwrap().is_empty() {
        let _ = app.emit("notice", "目前沒有辨識文字");
        return;
    }
    layout::arrange(app, Layout::Editor);
    if let Some(window) = app.get_webview_window("capsule") {
        let _ = window.set_focus();
    }
}

/// Carries out a request from a second launch or from the Zhuyin text service.
fn serve(app: &AppHandle, request: Request) {
    match request {
        Request::Show => layout::recover(app),
        Request::Settings => panel::open(app),
        Request::Edit => edit(app),
        Request::OpenWav(file) => import(app, file),
        Request::Chinese | Request::English | Request::FullWidth | Request::HalfWidth => {
            let session = app.state::<Session>();
            match request {
                Request::Chinese => *session.mode.lock().unwrap() = "中文",
                Request::English => *session.mode.lock().unwrap() = "英數",
                _ => session.full_width.store(request == Request::FullWidth, Ordering::SeqCst),
            }
            let _ = app.emit("mode", session.mode_label());
        }
    }
}

/// Starts one of the tools installed beside the word list.
fn run_tool(app: &AppHandle, name: &str, argument: Option<&str>) {
    let started = paths::tool(name).is_some_and(|tool| sys::quiet(std::process::Command::new(tool).args(argument)).spawn().is_ok());
    if !started {
        let _ = app.emit("notice", "找不到這個工具，請確認完整套件已安裝");
    }
}

#[tauri::command]
fn toggle_recording(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
fn set_preview(text: String, session: State<'_, Session>) {
    *session.preview.lock().unwrap() = text;
}

#[tauri::command]
fn copy_preview(app: AppHandle) -> bool {
    copy_preview_text(&app)
}

#[tauri::command]
fn convert_clipboard_script(app: AppHandle) {
    convert_clipboard(&app);
}

#[tauri::command(async)]
fn history_list() -> Vec<history::Item> {
    history::load()
}

/// Brings a past dictation back as the current text, so it can be edited, copied or pasted. `before` asks for
/// the text as it was before the model refined it.
#[tauri::command]
fn history_use(app: AppHandle, index: usize, before: Option<bool>) {
    if busy(&app) {
        return;
    }
    let Some(item) = history::load().into_iter().nth(index) else { return };
    match item.before.filter(|_| before == Some(true)) {
        Some(text) => announce(&app, "done", "從紀錄取回潤稿前的文字・可編輯後複製", text),
        None => announce(&app, "done", "從紀錄取回・可編輯後複製", item.text),
    }
}

#[tauri::command]
fn history_clear() -> bool {
    history::clear()
}

#[tauri::command]
fn discard_result(app: AppHandle) {
    discard(&app);
}

#[tauri::command]
fn arrange(app: AppHandle, shape: Layout) {
    layout::arrange(&app, shape);
}

/// The shape the window returns to once the text is copied or put away.
#[tauri::command]
fn home(app: AppHandle) -> Layout {
    layout::home(&app)
}

/// What the window needs to draw itself when it loads.
#[derive(Serialize)]
struct Facts {
    #[serde(flatten)]
    look: panel::Look,
    layout: Layout,
    side: layout::Side,
    state: &'static str,
    message: String,
    text: String,
    mode: String,
    /// "windows" or "macos", so the page can name this system's keys.
    platform: &'static str,
}

#[tauri::command]
fn facts(app: AppHandle, keys: State<'_, Keys>, session: State<'_, Session>) -> Facts {
    let state = *session.state.lock().unwrap();
    Facts {
        look: panel::look(&Settings::load(), keys.label()),
        layout: layout::current(&app),
        side: layout::side(&app),
        state: if state.is_empty() { "idle" } else { state },
        mode: session.mode_label(),
        platform: sys::PLATFORM,
        message: session.message.lock().unwrap().clone(),
        text: session.preview.lock().unwrap().clone(),
    }
}

/// The recording shortcut in use, so the settings page can change it while the app runs.
pub struct Keys {
    current: Mutex<Option<Shortcut>>,
    label: Mutex<String>,
    bound: AtomicBool,
}

impl Keys {
    fn new() -> Self {
        Self { current: Mutex::new(None), label: Mutex::new("停用".into()), bound: AtomicBool::new(false) }
    }

    pub fn label(&self) -> String {
        self.label.lock().unwrap().clone()
    }

    /// False when another program holds the shortcut the user chose.
    pub fn bound(&self) -> bool {
        self.bound.load(Ordering::SeqCst)
    }

    /// Lets go of the shortcut while the settings page listens for new keys, so pressing the current ones does not start a recording.
    /// `rebind` takes it back.
    pub fn suspend(&self, app: &AppHandle) {
        let current = self.current.lock().unwrap();
        if let Some(keys) = current.filter(|_| self.bound()) {
            let _ = app.global_shortcut().unregister(keys);
            self.bound.store(false, Ordering::SeqCst);
        }
    }

    /// Switches to the shortcut in `prefs`. Returns false, keeping the old one, when the system refuses the new one.
    pub fn rebind(&self, app: &AppHandle, prefs: &Settings) -> bool {
        let wanted = shortcut(prefs);
        let next = wanted.as_ref().map(|(keys, _)| *keys);
        let mut current = self.current.lock().unwrap();
        if *current != next || !self.bound() {
            if next.is_some_and(|keys| app.global_shortcut().register(keys).is_err()) {
                return false;
            }
            if let Some(old) = current.filter(|old| Some(*old) != next && self.bound()) {
                let _ = app.global_shortcut().unregister(old);
            }
            *current = next;
        }
        *self.label.lock().unwrap() = wanted.map_or_else(|| "停用".into(), |(_, name)| name);
        self.bound.store(true, Ordering::SeqCst);
        true
    }
}

/// The recording shortcut from the saved preferences; None when the user turned it off.
fn shortcut(prefs: &Settings) -> Option<(Shortcut, String)> {
    let mut keys = prefs.hotkey();
    // A development override keeps this app from fighting the installed voice window for the same keys.
    if let Some(letter) = std::env::var("TINGZI_RECORD_KEY").ok().and_then(|value| value.bytes().next()).filter(u8::is_ascii_alphabetic) {
        let held = keys.modifiers & (hotkey::CONTROL | hotkey::ALT | hotkey::SUPER) != 0;
        keys = hotkey::Hotkey { modifiers: if held { keys.modifiers } else { hotkey::DEFAULT.modifiers }, key: u32::from(letter.to_ascii_uppercase()) };
    }
    Some((keys.shortcut()?, keys.label()))
}

/// Keys the system would accept now, to offer when the chosen ones are taken.
pub fn free_keys(app: &AppHandle, taken: hotkey::Hotkey) -> Option<hotkey::Hotkey> {
    hotkey::suggestion(taken, |keys| {
        let Some(shortcut) = keys.shortcut() else { return false };
        // Keys this app holds already are the current shortcut; offering them would change nothing.
        if app.global_shortcut().is_registered(shortcut) {
            return false;
        }
        let free = app.global_shortcut().register(shortcut).is_ok();
        if free {
            let _ = app.global_shortcut().unregister(shortcut);
        }
        free
    })
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let item = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    let line = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &item("settings", "開啟設定…")?,
            &item("show", "顯示／找回浮窗")?,
            &item("record", "開始／停止錄音")?,
            &item("file", "辨識本機音檔…")?,
            &line()?,
            &item("edit", "編輯辨識文字…")?,
            &item("history", "最近的辨識紀錄…")?,
            &item("copy", "複製辨識文字")?,
            &item("convert", "剪貼簿文字簡繁互轉")?,
        ],
    )?;
    // The Zhuyin input method and its learning tool are part of the Windows package only.
    if cfg!(windows) {
        menu.append_items(&[
            &line()?,
            &item("zhuyin", "切換到注音輸入法")?,
            &item("learn-on", "啟用本機學習")?,
            &item("learn-off", "關閉本機學習")?,
        ])?;
    }
    menu.append_items(&[&line()?, &item("dock", "收合成圖示")?, &item("quit", "結束聽字")?])?;
    // A click on a menu bar icon opens its menu on macOS; on Windows it opens the settings.
    let menu_on_click = cfg!(target_os = "macos");
    let mut tray = TrayIconBuilder::new()
        .tooltip(sys::TITLE)
        .menu(&menu)
        .show_menu_on_left_click(menu_on_click)
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                if !menu_on_click {
                    panel::open(tray.app_handle());
                }
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => panel::open(app),
            "show" => layout::recover(app),
            "record" => toggle(app),
            "file" => import(app, None),
            "edit" => edit(app),
            "history" => {
                layout::arrange(app, Layout::Editor);
                if let Some(window) = app.get_webview_window("capsule") {
                    let _ = window.set_focus();
                }
            }
            "copy" => {
                copy_preview_text(app);
            }
            "convert" => convert_clipboard(app),
            // Without an argument the tool also adds Tingzi to this user's keyboards first, in case it is not there yet.
            "zhuyin" => run_tool(app, "TingziKeyboard.exe", None),
            "learn-on" => run_tool(app, "TingziLearning.exe", Some("on")),
            "learn-off" => run_tool(app, "TingziLearning.exe", Some("off")),
            "dock" => layout::arrange(app, Layout::Docked),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// `Tingzi --transcribe a.wav [b.wav ...]` checks recognition without the window.
/// `--local`, `--codex` or `--api` before the files picks the recogniser; otherwise it is Codex, as before.
fn transcribe_files(arguments: &[String]) -> i32 {
    let prefs = Settings::load();
    let (engine, files) = match arguments.first().map(String::as_str) {
        Some("--local") => (Engine::Local, &arguments[1..]),
        Some("--api") => (Engine::Api, &arguments[1..]),
        Some("--codex") => (Engine::Codex, &arguments[1..]),
        _ => (Engine::Codex, arguments),
    };
    for file in files {
        let started = std::time::Instant::now();
        let sound = std::fs::read(file).map_err(|_| "無法讀取音檔".to_string()).and_then(|bytes| audio::normalise(&bytes));
        let heard = sound.and_then(|wav| recognise::run(engine, &prefs, wav, &|| false));
        match heard {
            Ok(text) => println!("{}", deliver::convert_script(&text, false)),
            Err(reason) => {
                eprintln!("{reason}");
                return 1;
            }
        }
        eprintln!("{} ms", started.elapsed().as_millis());
    }
    0
}

/// `Tingzi --refine "文字"` tidies the text with the saved refinement service and prints it, without the window.
/// It is for trying the service, so it runs whether or not Refine is on, and sends short text too.
fn refine_text(text: &str) -> i32 {
    let prefs = Settings::load();
    if let Some(problem) = refine::problem(&prefs.refine_url, &prefs.refine_model, secret::refine_exists()) {
        eprintln!("{problem}");
        return 1;
    }
    let started = std::time::Instant::now();
    match refine::refine(text, &prefs.refine_url, &prefs.refine_model, &refine::Ask::of(&prefs)) {
        Ok(refined) => println!("{}", deliver::convert_script(&refined, false)),
        Err(failure) => {
            eprintln!("{}", failure.sentence());
            return 1;
        }
    }
    eprintln!("{} ms", started.elapsed().as_millis());
    0
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() >= 2 && arguments[0] == "--transcribe" {
        std::process::exit(transcribe_files(&arguments[1..]));
    }
    if arguments.len() >= 2 && arguments[0] == "--refine" {
        std::process::exit(refine_text(&arguments[1..].join(" ")));
    }
    let Some(asked) = instance::parse(&arguments) else { std::process::exit(2) };
    // A second launch hands its request to the copy already running, then leaves.
    if !instance::first() {
        std::process::exit(if instance::forward(&asked.unwrap_or(Request::Show)) { 0 } else { 4 });
    }

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, keys, event| {
                    let pressed = event.state() == ShortcutState::Pressed;
                    if *keys == rewrite_keys() {
                        if pressed {
                            rewrite_key(app);
                        }
                    } else if *keys == draft_keys() || *keys == last_keys() {
                        if pressed && *keys == draft_keys() {
                            paste_draft(app);
                        } else if pressed {
                            paste_last(app);
                        }
                    } else {
                        record_key(app, pressed);
                    }
                })
                .build(),
        )
        .manage(Session::default())
        .manage(Keys::new())
        .manage(panel::MicTest::default())
        .manage(fetch::Fetching::default())
        .manage(layout::start())
        .invoke_handler(tauri::generate_handler![
            toggle_recording,
            set_preview,
            copy_preview,
            discard_result,
            history_list,
            history_use,
            history_clear,
            convert_clipboard_script,
            arrange,
            home,
            facts,
            panel::open_settings,
            panel::take_welcome,
            panel::close_settings,
            panel::capture_shortcut,
            panel::load_settings,
            panel::save_settings,
            panel::set_api_key,
            panel::clear_api_key,
            panel::set_refine_key,
            panel::clear_refine_key,
            panel::refine_test,
            panel::test_microphone,
            panel::learning_state,
            panel::set_learning,
            panel::learning_data,
            panel::words_load,
            panel::words_save,
            panel::word_readings,
            panel::words_import,
            panel::words_export,
            panel::replacements_load,
            panel::replacements_save,
            panel::replacements_import,
            panel::replacements_export,
            panel::app_rules_load,
            panel::app_rules_save,
            panel::running_programs,
            panel::learned_words,
            panel::forget_learned_word,
            panel::open_target,
            panel::voice_learned,
            panel::forget_voice_learned,
            fetch::local_state,
            fetch::local_fetch,
            fetch::local_fetch_cancel,
            fetch::local_delete,
            update::update_state,
            update::update_check,
            update::update_install
        ])
        .on_window_event(|window, event| {
            // A minimised window is parked far off screen; that is not a place the user chose.
            if let (WindowEvent::Moved(to), "capsule") = (event, window.label()) {
                if window.is_minimized().unwrap_or(false) || layout::parked(*to) {
                    return;
                }
                layout::moved(window.app_handle(), *to);
            }
        })
        .setup(move |app| {
            sys::prepare(app);
            deliver::watch();
            layout::keep(app.handle().clone());
            layout::arrange(app.handle(), Layout::Docked);
            *app.state::<Session>().state.lock().unwrap() = "idle";
            // Another program may already hold the shortcut; the mic button still works.
            if !app.state::<Keys>().rebind(app.handle(), &Settings::load()) {
                show(app.handle(), "error", "錄音快捷鍵被其他程式占用，請點圖示錄音或到設定更換", "", false);
            }
            // Another program may hold this one; pasting the last dictation is then only in the window.
            let _ = app.global_shortcut().register(last_keys());
            rewrite_follows(app.handle(), &Settings::load());
            build_tray(app)?;
            panel::follow_upgrade();
            panel::welcome_first_run(app.handle());
            update::start(app.handle());
            let handle = app.handle().clone();
            instance::listen(move |request| {
                let app = handle.clone();
                let _ = handle.run_on_main_thread(move || serve(&app, request));
            });
            if let Some(request) = asked {
                serve(app.handle(), request);
            }
            // Reads the word list now so the first dictation does not wait for it.
            std::thread::spawn(|| words::apply("預先載入"));
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
        let [control, _, shift, _] = sys::MODIFIER_KEYS;
        assert_eq!(name, format!("{control}+{shift}+T"));
        let (_, name) = shortcut(&Settings { hotkey: Some(hotkey::Hotkey { modifiers: 0, key: 0x78 }), ..Settings::default() }).unwrap();
        assert_eq!(name, "F9");
        assert!(shortcut(&Settings { record_key: 0, ..Settings::default() }).is_none());
    }

    #[test]
    fn recognised_text_follows_the_script_choice() {
        let traditional = Settings { use_phrases: false, use_replacements: false, ..Settings::default() };
        assert_eq!(polish(" 语音输入 \n", &traditional), "語音輸入");
        assert_eq!(polish("語音輸入", &Settings { simplified: true, ..traditional }), "语音输入");
    }
}
