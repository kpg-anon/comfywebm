//! Remembers the main window's position, size and maximized state between
//! runs, in `window.json` in the app config dir. The window is created hidden
//! (tauri.conf.json) and shown here after the restore, so it does not appear
//! centered first and then jump.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, WindowEvent};

/// Outer position and inner size in physical pixels, of the window when it
/// was last neither maximized nor minimized.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct Bounds {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    maximized: bool,
}

const MIN_W: u32 = 940;
const MIN_H: u32 = 600;

fn current<R: Runtime>(win: &WebviewWindow<R>) -> Option<Bounds> {
    let p = win.outer_position().ok()?;
    let s = win.inner_size().ok()?;
    Some(Bounds { x: p.x, y: p.y, w: s.width, h: s.height, maximized: false })
}

/// A saved position is used only when the window's title bar area lands on a
/// connected monitor (a monitor may have been unplugged since).
fn on_screen<R: Runtime>(win: &WebviewWindow<R>, b: &Bounds) -> bool {
    let Ok(monitors) = win.available_monitors() else { return false };
    let (tx, ty) = (b.x + 100, b.y + 20);
    monitors.iter().any(|m| {
        let (p, s) = (m.position(), m.size());
        tx >= p.x && ty >= p.y && tx < p.x + s.width as i32 && ty < p.y + s.height as i32
    })
}

pub fn install<R: Runtime>(win: &WebviewWindow<R>, path: PathBuf) {
    let saved: Option<Bounds> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(b) = saved {
        if on_screen(win, &b) {
            let _ = win.set_size(PhysicalSize::new(b.w.max(MIN_W), b.h.max(MIN_H)));
            let _ = win.set_position(PhysicalPosition::new(b.x, b.y));
        }
        if b.maximized {
            let _ = win.maximize();
        }
    }
    let _ = win.show();
    let _ = win.set_focus();

    let normal = Mutex::new(saved.or_else(|| current(win)));
    let w = win.clone();
    win.on_window_event(move |e| match e {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if !w.is_maximized().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
                if let Some(b) = current(&w) {
                    *normal.lock().unwrap() = Some(b);
                }
            }
        }
        WindowEvent::CloseRequested { .. } => {
            if let Some(mut b) = *normal.lock().unwrap() {
                b.maximized = w.is_maximized().unwrap_or(false);
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Ok(json) = serde_json::to_vec_pretty(&b) {
                    let _ = std::fs::write(&path, json);
                }
            }
        }
        _ => {}
    });
}
