use tauri::AppHandle;
#[cfg(debug_assertions)]
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) const LIVE_DIAGNOSTICS_AVAILABLE: bool = cfg!(debug_assertions);

#[tauri::command]
pub(crate) fn open_live_diagnostics(app: AppHandle, language: String) -> Result<(), String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (app, language);
        Err("Диагностичният прозорец е наличен само в development версията.".into())
    }

    #[cfg(debug_assertions)]
    {
        let language = if language == "en" { "en" } else { "bg" };
        let window = if let Some(window) = app.get_webview_window("live-diagnostics") {
            window
        } else {
            WebviewWindowBuilder::new(
                &app,
                "live-diagnostics",
                WebviewUrl::App(
                    format!("index.html?window=live-diagnostics&lang={language}").into(),
                ),
            )
            .title(if language == "en" {
                "AIDOO Live diagnostics"
            } else {
                "AIDOO Live диагностика"
            })
            .inner_size(760.0, 680.0)
            .min_inner_size(560.0, 420.0)
            .center()
            .build()
            .map_err(|error| error.to_string())?
        };
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        let _ = app.emit_to("live-diagnostics", "live-diagnostics:language", language);
        Ok(())
    }
}
