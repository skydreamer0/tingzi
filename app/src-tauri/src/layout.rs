//! The voice window's three shapes and where each one sits on screen.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// A round microphone at the right edge of the screen.
    Docked,
    /// One line: microphone, status and the recognised text.
    Bar,
    /// The bar with the recognised text open for editing.
    Editor,
}

impl Layout {
    /// Width and height in logical pixels.
    fn size(self) -> (f64, f64) {
        match self {
            Layout::Docked => (72.0, 72.0),
            Layout::Bar => (480.0, 92.0),
            Layout::Editor => (480.0, 300.0),
        }
    }
}

/// Where the user last left the window, in physical pixels.
#[derive(Default, Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
struct Saved {
    bar: Option<(i32, i32)>,
    dock_y: Option<i32>,
}

pub struct Place {
    pub layout: Layout,
    saved: Saved,
    unsaved: bool,
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

/// The top-left corner for a window of `size`, kept fully inside `area`.
fn corner(layout: Layout, size: (i32, i32), area: Area, scale: f64, saved: &Saved) -> (i32, i32) {
    let gap = |logical: f64| (logical * scale).round() as i32;
    let (x, y) = match layout {
        Layout::Docked => (area.right - size.0, saved.dock_y.unwrap_or(area.bottom - size.1 - gap(120.0))),
        Layout::Bar | Layout::Editor => saved.bar.unwrap_or((area.right - size.0 - gap(24.0), area.bottom - size.1 - gap(48.0))),
    };
    (x.min(area.right - size.0).max(area.left), y.min(area.bottom - size.1).max(area.top))
}

fn file() -> std::path::PathBuf {
    crate::paths::data().join("voice-window.json")
}

pub fn start() -> Mutex<Place> {
    let saved = std::fs::read_to_string(file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    Mutex::new(Place { layout: Layout::Docked, saved, unsaved: false })
}

pub fn current(app: &AppHandle) -> Layout {
    app.state::<Mutex<Place>>().lock().map(|place| place.layout).unwrap_or(Layout::Bar)
}

/// Gives the window the shape `layout` and moves it to that shape's place.
pub fn arrange(app: &AppHandle, layout: Layout) {
    let Some(window) = app.get_webview_window("capsule") else { return };
    let monitor = window.current_monitor().ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let (width, height) = layout.size();
    let size = ((width * scale).round() as i32, (height * scale).round() as i32);
    let (x, y) = {
        let state = app.state::<Mutex<Place>>();
        let mut place = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        place.layout = layout;
        corner(layout, size, Area::of(&monitor), scale, &place.saved)
    };
    let _ = window.set_size(LogicalSize::new(width, height));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.show();
    let _ = app.emit("layout", layout);
}

/// Brings the window back to its default place on the monitor the pointer is on.
pub fn recover(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("capsule") {
        let under_pointer = app.cursor_position().ok().and_then(|at| app.monitor_from_point(at.x, at.y).ok().flatten());
        if let Some(monitor) = under_pointer {
            let work = monitor.work_area();
            let _ = window.set_position(work.position);
        }
    }
    if let Ok(mut place) = app.state::<Mutex<Place>>().lock() {
        place.saved.bar = None;
        place.unsaved = true;
    }
    arrange(app, Layout::Bar);
}

/// Remembers where the user dragged the window.
pub fn moved(app: &AppHandle, to: PhysicalPosition<i32>) {
    if let Ok(mut place) = app.state::<Mutex<Place>>().lock() {
        match place.layout {
            Layout::Bar => place.saved.bar = Some((to.x, to.y)),
            Layout::Docked => place.saved.dock_y = Some(to.y),
            Layout::Editor => return,
        }
        place.unsaved = true;
    }
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
    fn docked_microphone_hugs_the_right_edge() {
        assert_eq!(corner(Layout::Docked, (72, 72), SCREEN, 1.0, &Saved::default()), (1848, 848));
        let dragged = Saved { dock_y: Some(5000), ..Saved::default() };
        assert_eq!(corner(Layout::Docked, (72, 72), SCREEN, 1.0, &dragged), (1848, 968));
    }

    #[test]
    fn bar_opens_bottom_right_and_follows_scale() {
        assert_eq!(corner(Layout::Bar, (480, 92), SCREEN, 1.0, &Saved::default()), (1416, 900));
        assert_eq!(corner(Layout::Bar, (720, 138), SCREEN, 1.5, &Saved::default()), (1164, 830));
    }

    #[test]
    fn a_position_from_a_missing_monitor_is_pulled_back_on_screen() {
        let second = Area { left: 1920, top: 0, right: 3840, bottom: 1040 };
        let saved = Saved { bar: Some((-900, 2000)), ..Saved::default() };
        assert_eq!(corner(Layout::Bar, (480, 92), second, 1.0, &saved), (1920, 948));
        // The editor is taller, so it moves up rather than hanging off the bottom.
        let low = Saved { bar: Some((2000, 900)), ..Saved::default() };
        assert_eq!(corner(Layout::Editor, (480, 300), second, 1.0, &low), (2000, 740));
    }
}
