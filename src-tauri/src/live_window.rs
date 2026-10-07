use tauri::{AppHandle, Manager};

#[derive(Clone, Copy)]
pub(super) enum MainWindowRequest {
    ExplicitMenu,
    SystemReopen,
}

pub(super) fn main_window_should_open(request: MainWindowRequest) -> bool {
    // Launch and macOS reopen remain quiet. Settings/onboarding are still reachable by an
    // explicit menu action; a wake request uses the separate transparent microphone handoff.
    matches!(request, MainWindowRequest::ExplicitMenu)
}

pub(super) fn live_phase_hides_main_window(phase: &str) -> bool {
    !matches!(phase, "idle" | "preparing")
}

pub(super) fn wake_activation_window_phase() -> &'static str {
    "preparing"
}

#[cfg(target_os = "macos")]
fn set_main_window_alpha(window: &tauri::WebviewWindow, alpha: f64) {
    let Ok(pointer) = window.ns_window() else {
        return;
    };
    if pointer.is_null() {
        return;
    }
    // SAFETY: Tauri owns this live NSWindow; the selector accepts macOS CGFloat (f64), does not
    // retain the receiver, and this synchronous window command runs on Tauri's UI thread.
    unsafe {
        let native_window = &*pointer.cast::<objc2::runtime::AnyObject>();
        let _: () = objc2::msg_send![native_window, setAlphaValue: alpha];
    }
}

fn keep_alive_for_microphone(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    set_main_window_alpha(window, 0.0);
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_focusable(false);
    let _ = window.unminimize();
    let _ = window.show();
}

fn finish_microphone_handoff(window: &tauri::WebviewWindow) {
    let _ = window.hide();
    #[cfg(target_os = "macos")]
    set_main_window_alpha(window, 1.0);
    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_focusable(true);
}

pub(super) fn sync_main_window_for_live_phase(app: &AppHandle, phase: &str) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if phase == "preparing" {
        keep_alive_for_microphone(&window);
    } else if phase == "idle" || live_phase_hides_main_window(phase) {
        finish_microphone_handoff(&window);
    }
}
