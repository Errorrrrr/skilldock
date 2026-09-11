use std::sync::Mutex;
use tauri::{Emitter, Manager, PhysicalPosition, Rect};

#[derive(Default)]
pub struct Popup(Mutex<Lifecycle>);

#[derive(Default)]
struct Lifecycle {
    ready: bool,
    requested: bool,
    revision: u64,
    anchor: Option<crate::tray_position::Placement>,
}
impl Lifecycle {
    fn toggle(&mut self) {
        self.revision += 1;
        self.requested = !self.requested;
    }
    fn close(&mut self) {
        self.revision += 1;
        self.requested = false;
    }
    fn should_blur(&self, revision: u64, focused: bool) -> bool {
        self.should_close(revision) && !focused
    }
    fn should_close(&self, revision: u64) -> bool {
        self.requested && self.revision == revision
    }
}

// Native visibility owns closing. There is no frontend exit animation to wait for.
pub fn hide(app: &tauri::AppHandle) {
    app.state::<Popup>().0.lock().unwrap().close();
    if let Some(window) = app.get_webview_window("tray") {
        let _ = window.hide();
    }
}
pub fn dismiss(app: &tauri::AppHandle) {
    hide(app);
}

#[cfg(target_os = "macos")]
pub fn outside_click(app: &tauri::AppHandle) {
    let revision = {
        let state = app.state::<Popup>();
        let state = state.0.lock().unwrap();
        if !state.requested {
            return;
        }
        state.revision
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // The local event monitor precedes the status-item handler. Allow that
        // handler to toggle/invalidate this click before deciding to dismiss.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let close = handle
                .state::<Popup>()
                .0
                .lock()
                .unwrap()
                .should_close(revision);
            if close {
                hide(&handle);
            }
        });
    });
}

fn present(app: &tauri::AppHandle) {
    let anchor = {
        let state = app.state::<Popup>();
        let state = state.0.lock().unwrap();
        if !state.ready || !state.requested {
            return;
        }
        state.anchor
    };
    if let (Some(window), Some(placement)) = (app.get_webview_window("tray"), anchor) {
        let result = crate::tray_position::present(&window, placement);
        if let Err(error) = result {
            tracing::warn!(%error, "tray_open_failed");
            hide(app);
        } else {
            let _ = window.emit("skilldock:tray-opened", ());
        }
    }
}
pub fn click(app: &tauri::AppHandle, rect: Rect, position: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window("tray") else {
        return;
    };
    // Capture before any show/focus operation; a pending first open must not
    // use a later cursor location or the main window's display.
    let placement = match crate::tray_position::capture(&window, rect, position) {
        Ok(placement) => placement,
        Err(error) => {
            tracing::warn!(%error, "tray_anchor_failed");
            hide(app);
            return;
        }
    };
    let requested = {
        let state = app.state::<Popup>();
        let mut state = state.0.lock().unwrap();
        state.toggle();
        state.anchor = Some(placement);
        state.requested
    };
    if requested {
        present(app);
    } else {
        dismiss(app);
    }
}
pub fn ready(app: &tauri::AppHandle) {
    app.state::<Popup>().0.lock().unwrap().ready = true;
    present(app);
}
pub fn blur(app: &tauri::AppHandle) {
    let revision = app.state::<Popup>().0.lock().unwrap().revision;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Clicking the status item can blur the popup before its mouse-down
        // callback arrives. Let that click invalidate this blur, then recheck focus.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let focused = handle
                .get_webview_window("tray")
                .is_some_and(|w| w.is_focused().unwrap_or(false));
            let close = handle
                .state::<Popup>()
                .0
                .lock()
                .unwrap()
                .should_blur(revision, focused);
            if close {
                dismiss(&handle);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn click(state: &mut Lifecycle) {
        state.toggle();
    }
    #[test]
    fn outside_click_does_not_require_a_focus_transition() {
        let mut state = Lifecycle::default();
        state.toggle();
        assert!(!state.should_blur(state.revision, true));
        assert!(state.should_close(state.revision));
        state.close();
        assert!(!state.should_close(state.revision));
    }
    #[test]
    fn outside_click_queued_before_status_toggle_cannot_close_new_popup() {
        let mut state = Lifecycle::default();
        state.toggle();
        let old = state.revision;
        state.toggle();
        state.toggle();
        assert!(!state.should_close(old));
        assert!(state.should_close(state.revision));
    }
    #[test]
    fn rapid_clicks_preserve_toggle_intent() {
        let mut state = Lifecycle {
            ready: true,
            ..Default::default()
        };
        for i in 1..=100 {
            click(&mut state);
            assert_eq!(state.requested, i % 2 == 1);
        }
    }
    #[test]
    fn first_open_can_be_cancelled_before_webview_ready() {
        let mut state = Lifecycle::default();
        click(&mut state);
        assert!(state.requested && !state.ready);
        click(&mut state);
        state.ready = true;
        assert!(!state.requested);
    }
    #[test]
    fn blur_before_click_cannot_close_reopened_popup() {
        let mut state = Lifecycle::default();
        click(&mut state);
        let stale = state.revision;
        click(&mut state);
        click(&mut state);
        assert!(!state.should_blur(stale, false));
        assert!(!state.should_blur(state.revision, true));
        assert!(state.should_blur(state.revision, false));
    }
}
