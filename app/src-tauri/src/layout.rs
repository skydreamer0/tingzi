//! The voice window's three shapes and where each one sits on screen.
//!
//! The keycap is the anchor. Docked, the window is only the keycap, against the left or right edge.
//! Opened, a pill grows from the keycap towards the middle of the screen, and the editor hangs
//! below the pill, or above it when there is no room below, so the keycap never moves.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// The keycap alone, at the left or right edge of the screen.
    Docked,
    /// The keycap with a pill beside it holding the status and the recognised text.
    Bar,
    /// The pill with the recognised text open for editing on a card beside it.
    Editor,
}

/// The height of the keycap's row, which every shape shares, in logical pixels.
const ROW: f64 = 72.0;

impl Layout {
    /// Width and height in logical pixels.
    fn size(self) -> (f64, f64) {
        match self {
            Layout::Docked => (ROW, ROW),
            Layout::Bar => (460.0, ROW),
            Layout::Editor => (460.0, 280.0),
        }
    }
}

/// Where the user last left the keycap, in physical pixels.
#[derive(Default, Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
struct Saved {
    /// The top of the keycap's row.
    dock_y: Option<i32>,
    /// Whether the keycap sits at the left edge; files from before keep it on the right.
    #[serde(default)]
    dock_left: bool,
}

pub struct Place {
    pub layout: Layout,
    saved: Saved,
    unsaved: bool,
    /// Whether the pill opened by itself for a dictation, and so closes by itself afterwards.
    opened_for_work: bool,
    /// Whether the editor last opened above the keycap's row.
    above: bool,
}

/// The part of a monitor not covered by the taskbar.
#[derive(Clone, Copy)]
struct Area {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Area {
    fn of(monitor: &Monitor) -> Self {
        let work = monitor.work_area();
        Self {
            left: work.position.x,
            top: work.position.y,
            right: work.position.x + work.size.width as i32,
            bottom: work.position.y + work.size.height as i32,
        }
    }
}

/// Where a window of `size` goes, and whether the editor opens above the keycap's row.
/// The keycap's row stays where the user left it; the window is kept fully inside `area`.
fn corner(layout: Layout, size: (i32, i32), area: Area, scale: f64, saved: &Saved) -> ((i32, i32), bool) {
    let row = (ROW * scale).round() as i32;
    let x = if saved.dock_left { area.left } else { area.right - size.0 };
    let top = saved.dock_y.unwrap_or(area.bottom - row - (120.0 * scale).round() as i32).min(area.bottom - row).max(area.top);
    let above = layout == Layout::Editor && top + size.1 > area.bottom;
    let y = if above { top + row - size.1 } else { top };
    ((x.min(area.right - size.0).max(area.left), y.min(area.bottom - size.1).max(area.top)), above)
}

/// Which way the window grows, for the page to lay out the keycap, pill and card.
#[derive(Clone, Copy, Serialize)]
pub struct Side {
    pub left: bool,
    pub above: bool,
}

fn file() -> std::path::PathBuf {
    crate::paths::data().join("voice-window.json")
}

pub fn start() -> Mutex<Place> {
    let mut saved: Saved = std::fs::read_to_string(file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    // Earlier versions could save the parked position of a minimised window.
    saved.dock_y = saved.dock_y.filter(|y| *y > -30000);
    Mutex::new(Place { layout: Layout::Docked, saved, unsaved: false, opened_for_work: false, above: false })
}

pub fn current(app: &AppHandle) -> Layout {
    app.state::<Mutex<Place>>().lock().map(|place| place.layout).unwrap_or(Layout::Bar)
}

/// The side the keycap is on, as the page last heard it.
pub fn side(app: &AppHandle) -> Side {
    app.state::<Mutex<Place>>().lock().map(|place| Side { left: place.saved.dock_left, above: place.above }).unwrap_or(Side { left: false, above: false })
}

/// Gives the window the shape `layout` and moves it to that shape's place.
/// The user chose this shape, so it stays until they or an error change it.
pub fn arrange(app: &AppHandle, layout: Layout) {
    place(app, layout, false);
}

/// Counts the folds asked for; a newer dictation or fold makes an older one stand down.
static ROUND: AtomicU64 = AtomicU64::new(0);

/// Opens the pill for a dictation started from the docked keycap; `close_soon` folds it back.
/// A dictation started while a fold is waiting cancels the fold.
pub fn open_for_work(app: &AppHandle) {
    ROUND.fetch_add(1, Ordering::SeqCst);
    if current(app) == Layout::Docked {
        place(app, Layout::Bar, true);
    }
}

/// Folds a pill that opened by itself back into the keycap, after the result has been seen.
pub fn close_soon(app: &AppHandle) {
    let opened = app.state::<Mutex<Place>>().lock().map(|place| place.opened_for_work && place.layout == Layout::Bar).unwrap_or(false);
    if !opened {
        return;
    }
    let round = ROUND.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2600));
        let still = |app: &AppHandle| app.state::<Mutex<Place>>().lock().map(|place| place.opened_for_work && place.layout == Layout::Bar).unwrap_or(false);
        if !still(&app) || ROUND.load(Ordering::SeqCst) != round {
            return;
        }
        // The page folds the pill into the keycap, then the window shrinks behind it.
        let _ = app.emit("closing", ());
        std::thread::sleep(Duration::from_millis(200));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let still = handle.state::<Mutex<Place>>().lock().map(|place| place.opened_for_work && place.layout == Layout::Bar).unwrap_or(false);
            if still && ROUND.load(Ordering::SeqCst) == round {
                place(&handle, Layout::Docked, false);
            }
        });
    });
}

fn place(app: &AppHandle, layout: Layout, for_work: bool) {
    let Some(window) = app.get_webview_window("capsule") else { return };
    let monitor = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let (width, height) = layout.size();
    let size = ((width * scale).round() as i32, (height * scale).round() as i32);
    let ((x, y), above, left) = {
        let state = app.state::<Mutex<Place>>();
        let mut place = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        place.layout = layout;
        place.opened_for_work = for_work;
        let (at, above) = corner(layout, size, Area::of(&monitor), scale, &place.saved);
        place.above = above;
        (at, above, place.saved.dock_left)
    };
    // Show-desktop and similar commands can minimise the window; bring it back first.
    let _ = window.unminimize();
    let _ = window.set_size(LogicalSize::new(width, height));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.show();
    let _ = app.emit("side", Side { left, above });
    let _ = app.emit("layout", layout);
}

/// Brings the keycap back to its default place on the monitor the pointer is on, with the pill open.
pub fn recover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("capsule") {
        let under_pointer = app.cursor_position().ok().and_then(|at| app.monitor_from_point(at.x, at.y).ok().flatten());
        if let Some(monitor) = under_pointer {
            let work = monitor.work_area();
            let _ = window.set_position(work.position);
        }
    }
    if let Ok(mut place) = app.state::<Mutex<Place>>().lock() {
        place.saved = Saved::default();
        place.unsaved = true;
    }
    arrange(app, Layout::Bar);
}

/// Remembers where the user dragged the keycap, and settles it against the nearer edge
/// once the mouse button is let go.
pub fn moved(app: &AppHandle, to: PhysicalPosition<i32>) {
    {
        let state = app.state::<Mutex<Place>>();
        let Ok(mut place) = state.lock() else { return };
        // The editor may sit above the keycap's row, so its top is not the row's.
        if place.layout == Layout::Editor {
            return;
        }
        place.saved.dock_y = Some(to.y);
        place.unsaved = true;
    }
    settle(app.clone());
}

/// Waits for the drag to end, then puts the window against the nearer side of its screen.
fn settle(app: AppHandle) {
    static SETTLING: AtomicBool = AtomicBool::new(false);
    if SETTLING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        while button_held() {
            std::thread::sleep(Duration::from_millis(30));
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            snap(&handle);
            SETTLING.store(false, Ordering::SeqCst);
        });
    });
}

fn button_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    // SAFETY: GetAsyncKeyState only reads input state.
    unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0 }
}

fn snap(app: &AppHandle) {
    let Some(window) = app.get_webview_window("capsule") else { return };
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let (Ok(at), Ok(size), Ok(Some(monitor))) = (window.outer_position(), window.outer_size(), window.current_monitor()) else { return };
    let area = Area::of(&monitor);
    let left = nearer_left(at.x + size.width as i32 / 2, area);
    let (layout, changed_side) = {
        let state = app.state::<Mutex<Place>>();
        let mut place = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if place.layout == Layout::Editor {
            return;
        }
        let changed_side = place.saved.dock_left != left;
        place.saved.dock_left = left;
        place.saved.dock_y = Some(at.y);
        place.unsaved = true;
        (place.layout, changed_side)
    };
    let (target, _) = {
        let state = app.state::<Mutex<Place>>();
        let place = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        corner(layout, (size.width as i32, size.height as i32), area, monitor.scale_factor(), &place.saved)
    };
    // Moving to where it already is would only start another round.
    if (at.x, at.y) != target {
        let _ = window.set_position(PhysicalPosition::new(target.0, target.1));
    }
    if changed_side {
        let _ = app.emit("side", Side { left, above: false });
    }
}

/// Whether a window centred at `x` is closer to the left edge of `area` than the right.
fn nearer_left(x: i32, area: Area) -> bool {
    x - area.left < area.right - x
}

/// Where Windows parks a minimised window, at -32000 on both axes.
pub fn parked(to: PhysicalPosition<i32>) -> bool {
    to.x <= -30000 || to.y <= -30000
}

/// Writes the position to disk a little after the window stops moving.
pub fn keep(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let pending = app.state::<Mutex<Place>>().lock().ok().and_then(|mut place| std::mem::take(&mut place.unsaved).then_some(place.saved));
        if let Some(saved) = pending {
            let _ = std::fs::create_dir_all(crate::paths::data());
            let _ = serde_json::to_string(&saved).map(|text| std::fs::write(file(), text));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area { left: 0, top: 0, right: 1920, bottom: 1040 };

    #[test]
    fn docked_keycap_hugs_the_right_edge() {
        assert_eq!(corner(Layout::Docked, (72, 72), SCREEN, 1.0, &Saved::default()), ((1848, 848), false));
        let dragged = Saved { dock_y: Some(5000), ..Saved::default() };
        assert_eq!(corner(Layout::Docked, (72, 72), SCREEN, 1.0, &dragged), ((1848, 968), false));
    }

    #[test]
    fn a_minimised_window_is_not_a_place() {
        assert!(parked(PhysicalPosition::new(-32000, -32000)));
        assert!(!parked(PhysicalPosition::new(-1920, 200)));
    }

    #[test]
    fn docked_keycap_can_live_on_the_left() {
        let left = Saved { dock_y: Some(300), dock_left: true };
        assert_eq!(corner(Layout::Docked, (72, 72), SCREEN, 1.0, &left), ((0, 300), false));
        assert!(nearer_left(900, SCREEN));
        assert!(!nearer_left(1000, SCREEN));
        // Files from before the choice keep the keycap on the right, and an old bar position is ignored.
        let old: Saved = serde_json::from_str(r#"{"bar":[10,10],"dock_y":40}"#).unwrap();
        assert_eq!(old, Saved { dock_y: Some(40), dock_left: false });
    }

    #[test]
    fn the_pill_grows_from_the_keycap_towards_the_middle() {
        let right = Saved { dock_y: Some(300), dock_left: false };
        assert_eq!(corner(Layout::Bar, (460, 72), SCREEN, 1.0, &right), ((1460, 300), false));
        let left = Saved { dock_y: Some(300), dock_left: true };
        assert_eq!(corner(Layout::Bar, (460, 72), SCREEN, 1.0, &left), ((0, 300), false));
        // Scaled, the keycap's row keeps its place.
        assert_eq!(corner(Layout::Bar, (690, 108), SCREEN, 1.5, &right), ((1230, 300), false));
    }

    #[test]
    fn the_editor_opens_above_when_there_is_no_room_below() {
        let high = Saved { dock_y: Some(100), dock_left: false };
        assert_eq!(corner(Layout::Editor, (460, 280), SCREEN, 1.0, &high), ((1460, 100), false));
        // Near the bottom the card goes above, and the keycap's row stays at 900.
        let low = Saved { dock_y: Some(900), dock_left: false };
        assert_eq!(corner(Layout::Editor, (460, 280), SCREEN, 1.0, &low), ((1460, 692), true));
    }

    #[test]
    fn a_position_from_a_missing_monitor_is_pulled_back_on_screen() {
        let second = Area { left: 1920, top: 0, right: 3840, bottom: 1040 };
        let lost = Saved { dock_y: Some(2000), dock_left: true };
        assert_eq!(corner(Layout::Docked, (72, 72), second, 1.0, &lost), ((1920, 968), false));
    }
}
