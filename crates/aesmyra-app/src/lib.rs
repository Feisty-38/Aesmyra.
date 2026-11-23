//! Aesmyra. als Anwendung.
//!
//! Die Oberfläche sieht nur diese Befehle. Sie kann nicht ins Dateisystem,
//! nicht in die Shell und nicht selbst ins Netz — alles läuft über den Kern.

use std::sync::{Arc, Mutex, RwLock};

use aesmyra_core::api::UiEvent;
use aesmyra_core::{Api, Config, Session, SettingsView, Turn};
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::State;

pub struct AppState {
    cfg: RwLock<Config>,
    api: RwLock<Option<Arc<Api>>>,
    run: Mutex<Option<RunHandles>>,
}

/// Ein Lauf besteht aus zwei Tasks: dem Leser, der den Ereignisstrom auswertet,
/// und dem Zusteller, der die Ereignisse in die Oberfläche schreibt. Beide
/// müssen enden — sonst leert der alte Lauf weiter den Puffer in den alten
/// Kanal und schreibt in die neue Ansicht.
struct RunHandles {
    producer: tauri::async_runtime::JoinHandle<()>,
    sink: tauri::async_runtime::JoinHandle<()>,
}

impl AppState {
    fn new() -> Self {
        let cfg = Config::load();
        #[cfg(target_os = "android")]
        eprintln!(
            "[aesmyra] Konfiguration {}: server={} schluessel={}",
            aesmyra_core::config::config_path().display(),
            cfg.server_url,
            if cfg.token.is_empty() { "fehlt" } else { "gesetzt" }
        );
        Self { cfg: RwLock::new(cfg), api: RwLock::new(None), run: Mutex::new(None) }
    }

    /// Verbindung wiederverwenden: ein Client mit einem Verbindungspool.
    fn api(&self) -> Result<Arc<Api>, String> {
        if let Some(api) = self.api.read().unwrap().clone() {
            return Ok(api);
        }
        let cfg = self.cfg.read().unwrap().clone();
        if cfg.token.is_empty() {
            return Err("Kein Zugangsschlüssel hinterlegt. Öffne die Einrichtung.".into());
        }
        let api = Arc::new(Api::new(cfg.server_url, cfg.token).map_err(|e| e.to_string())?);
        *self.api.write().unwrap() = Some(api.clone());
        Ok(api)
    }

    fn stop_run(&self) {
        if let Some(handles) = self.run.lock().unwrap().take() {
            handles.producer.abort();
            handles.sink.abort();
        }
    }
}

/* ── Einrichtung ────────────────────────────────────────────────────── */

#[tauri::command]
fn settings_get(state: State<'_, AppState>) -> SettingsView {
    state.cfg.read().unwrap().view()
}

#[tauri::command]
fn settings_save(state: State<'_, AppState>, server_url: String, token: Option<String>) -> Result<SettingsView, String> {
    let cfg = Config::save(&server_url, token.as_deref())?;
    *state.api.write().unwrap() = None;
    let view = cfg.view();
    *state.cfg.write().unwrap() = cfg;
    Ok(view)
}

#[tauri::command]
async fn settings_test(state: State<'_, AppState>) -> Result<String, String> {
    state.stop_run();
    let before = state.cfg.read().unwrap().clone();
    let api = Api::new(before.server_url.clone(), before.token.clone()).map_err(|e| e.to_string())?;
    if before.token.is_empty() {
        return Err("Kein Zugangsschlüssel hinterlegt.".into());
    }
    match api.sessions().await {
        Ok(list) => Ok(format!("{} Gespräche erreichbar", list.len())),
        Err(aesmyra_core::ApiError::Status(code, _)) if code == 401 || code == 403 => {
            Err("Der Server hat den Schlüssel abgelehnt.".into())
        }
        Err(aesmyra_core::ApiError::Unreachable) => Err("Server nicht erreichbar.".into()),
        Err(e) => Err(e.to_string()),
    }
}

/* ── Gespräche ────────────────────────────────────────────────────────── */

#[tauri::command]
async fn sessions_list(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    let api = state.api()?;
    api.sessions().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn messages_load(state: State<'_, AppState>, session_id: String) -> Result<Vec<Turn>, String> {
    let api = state.api()?;
    api.turns(&session_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn session_create(state: State<'_, AppState>) -> Result<Session, String> {
    let api = state.api()?;
    api.create_session().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn session_patch(
    state: State<'_, AppState>,
    session_id: String,
    title: Option<String>,
    pinned: Option<bool>,
    archived: Option<bool>,
    hidden: Option<bool>,
) -> Result<(), String> {
    let api = state.api()?;
    let mut patch = serde_json::Map::new();
    if let Some(t) = title {
        patch.insert("title".into(), Value::String(t));
    }
    if let Some(p) = pinned {
        patch.insert("pinned".into(), Value::Bool(p));
    }
    if let Some(a) = archived {
        patch.insert("archived".into(), Value::Bool(a));
    }
    if let Some(h) = hidden {
        patch.insert("hidden".into(), Value::Bool(h));
    }
    if patch.is_empty() {
        return Ok(());
    }
    api.patch_session(&session_id, Value::Object(patch)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn session_delete(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let api = state.api()?;
    api.delete_session(&session_id).await.map_err(|e| e.to_string())
}

/* ── Lauf ───────────────────────────────────────────────────────────── */

#[tauri::command]
async fn chat_start(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    on_event: Channel<UiEvent>,
) -> Result<(), String> {
    let api = state.api()?;
    state.stop_run();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<UiEvent>(512);
    let sink_channel = on_event.clone();
    let sink = tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if sink_channel.send(event).is_err() {
                break;
            }
        }
    });

    let producer = tauri::async_runtime::spawn(async move {
        match api.chat_stream(&session_id, &text, &tx).await {
            Ok(result) => {
                let ok = matches!(
                    result.outcome,
                    aesmyra_core::Outcome::Completed | aesmyra_core::Outcome::Recovered
                );
                let _ = tx.send(UiEvent::Finished { run_id: result.run_id, ok }).await;
            }
            Err(e) => {
                // Ohne Grund bekäme die Oberfläche nur „fertig, aber nicht ok"
                // und wüsste nicht, ob Schlüssel, Netz oder Server schuld sind.
                // Kam der Fehler vor dem ersten Ereignis, ist keine Laufkennung
                // bekannt — dann bleibt sie leer.
                let message = run_error_text(&e);
                let _ = tx.send(UiEvent::Error { run_id: String::new(), message }).await;
                let _ = tx.send(UiEvent::Finished { run_id: String::new(), ok: false }).await;
            }
        }
    });

    *state.run.lock().unwrap() = Some(RunHandles { producer, sink });
    Ok(())
}

/// Klartext für die Oberfläche. 401/403 heißt fast immer: falscher Schlüssel.
fn run_error_text(e: &aesmyra_core::ApiError) -> String {
    use aesmyra_core::ApiError;
    match e {
        ApiError::Status(401 | 403, _) => "Der Server hat den Schlüssel abgelehnt.".to_string(),
        ApiError::Timeout => "Zeitüberschreitung".to_string(),
        ApiError::Unreachable => "Server nicht erreichbar.".to_string(),
        other => other.to_string(),
    }
}

#[tauri::command]
fn chat_stop(state: State<'_, AppState>) -> Result<(), String> {
    state.stop_run();
    Ok(())
}

#[tauri::command]
async fn run_steer(state: State<'_, AppState>, run_id: String, message: String) -> Result<(), String> {
    let api = state.api()?;
    api.steer(&run_id, &message).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_approve(
    state: State<'_, AppState>,
    run_id: String,
    choice: String,
    request_id: String,
) -> Result<(), String> {
    let api = state.api()?;
    api.approve(&run_id, &choice, &request_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn app_info() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

/// Zeichen, die eine Shell, `cmd` oder ein Kommandozeilen-Werkzeug als
/// Steuerzeichen auslegen könnte. Ein Link aus einer Serverantwort darf
/// niemals einen Befehl auslösen — deshalb unabhängig vom Betriebssystem
/// abgelehnt.
const FORBIDDEN_URL_CHARS: [char; 8] = ['&', '|', '^', '<', '>', '%', '"', '!'];

fn acceptable_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || FORBIDDEN_URL_CHARS.contains(&c))
}

/// Links nur nach ausdrücklicher Prüfung im Systembrowser öffnen.
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !acceptable_url(trimmed) {
        return Err("Nur geprüfte http- und https-Adressen".into());
    }

    #[cfg(target_os = "linux")]
    let launched = std::process::Command::new("xdg-open").arg(trimmed).spawn();
    #[cfg(target_os = "macos")]
    let launched = std::process::Command::new("open").arg(trimmed).spawn();
    // `url.dll,FileProtocolHandler` ruft ShellExecuteW auf. Kein `cmd`, also
    // keine Shell, die Sonderzeichen als Befehlstrenner auslegt.
    #[cfg(target_os = "windows")]
    let launched = std::process::Command::new("rundll32")
        .arg("url.dll,FileProtocolHandler")
        .arg(trimmed)
        .spawn();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        launched.map(|_| ()).map_err(|_| "Konnte nicht geöffnet werden".to_string())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Auf dieser Plattform nicht vorgesehen".into())
    }
}

/// WebKitGTK und der DMA-BUF-Renderer vertragen sich mit dem NVIDIA-Treiber
/// nicht: das Fenster bleibt schwarz. Betroffen ist nur Linux mit NVIDIA.
fn tune_webkit() {
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/proc/driver/nvidia").exists()
            && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
        {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tune_webkit();

    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;

            // Android kennt kein Benutzer-Konfigurationsverzeichnis; der Pfad
            // kommt von der Anwendung selbst. `app_config_dir` liefert dort
            // `/data/user/0/<paket>` — dorthin schreibt auch `Config::save`.
            #[cfg(target_os = "android")]
            {
                match app.path().app_config_dir() {
                    Ok(dir) => {
                        if let Err(e) = aesmyra_core::config::set_base_dir(dir) {
                            eprintln!("[aesmyra] Konfigurationsverzeichnis nicht beschreibbar: {e}");
                        }
                    }
                    Err(e) => eprintln!("[aesmyra] app_config_dir fehlgeschlagen: {e}"),
                }
            }

            app.manage(AppState::new());

            #[cfg(desktop)]
            {
                if let Some(win) = app.get_webview_window("main") {
                    if let Ok(Some(mon)) = win.current_monitor() {
                        let screen = mon.size();
                        let origin = mon.position();
                        let w = (screen.width as f64 * 0.62) as u32;
                        let h = (screen.height as f64 * 0.78) as u32;
                        let _ = win.set_size(tauri::PhysicalSize::new(w, h));
                        let _ = win.set_position(tauri::PhysicalPosition::new(
                            origin.x + ((screen.width - w) / 2) as i32,
                            origin.y + ((screen.height - h) / 2) as i32,
                        ));
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_save,
            settings_test,
            sessions_list,
            messages_load,
            session_create,
            session_patch,
            session_delete,
            chat_start,
            chat_stop,
            run_steer,
            run_approve,
            app_info,
            open_external
        ])
        .run(tauri::generate_context!())
        .expect("Aesmyra. konnte nicht starten");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresspruefung_lehnt_kommandozeichen_ab() {
        for u in [
            "https://a.test/?x=1&calc",
            "https://a.test/?x=1|calc",
            "https://a.test/^calc",
            "https://a.test/<calc>",
            "https://a.test/%63alc",
            "https://a.test/\"calc",
            "https://a.test/!calc",
            "javascript:alert(1)",
            "https://a.test/ mit leerzeichen",
            "https://a.test/\ncalc",
        ] {
            assert!(!acceptable_url(u), "hätte abgelehnt werden müssen: {u:?}");
        }
        assert!(acceptable_url("https://example.test/pfad?x=1#frag"));
        assert!(acceptable_url("http://127.0.0.1:8642/docs"));
    }

    #[test]
    fn fehlertexte_nennen_den_grund() {
        use aesmyra_core::ApiError;
        assert_eq!(
            run_error_text(&ApiError::Status(401, "nein".into())),
            "Der Server hat den Schlüssel abgelehnt."
        );
        assert_eq!(
            run_error_text(&ApiError::Status(403, "nein".into())),
            "Der Server hat den Schlüssel abgelehnt."
        );
        assert_eq!(run_error_text(&ApiError::Timeout), "Zeitüberschreitung");
        assert_eq!(run_error_text(&ApiError::Unreachable), "Server nicht erreichbar.");
        assert_eq!(
            run_error_text(&ApiError::Status(500, "kaputt".into())),
            "Server meldet Status 500: kaputt"
        );
    }
}
