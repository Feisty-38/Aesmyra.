//! Nur für die Entwicklung: der Kern als HTTP-Dienst.
//!
//! Die Oberfläche läuft im Browser ohne Tauri-Hülle. Dieses Programm bietet
//! dieselben Befehle über HTTP an und benutzt dabei denselben Kern wie die
//! Anwendung: echte Sitzungen, echter Verlauf, echter Ereignisstrom. Damit
//! lässt sich die Oberfläche im Browser prüfen, ohne mit erfundenen Daten zu
//! arbeiten.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aesmyra_core::api::UiEvent;
use aesmyra_core::{Api, ApiError, Config, Outcome, SettingsView};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const ADDR: &str = "127.0.0.1:8121";
const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 2 * 1024 * 1024;

struct Run {
    id: u64,
    sid: String,
    tx: mpsc::Sender<UiEvent>,
    task: tokio::task::JoinHandle<()>,
}

struct Inner {
    cfg: Config,
    api: Arc<Api>,
    /// Ein Lauf je Gespräch. Mehrere gleichzeitig, seit die Oberfläche in einem
    /// zweiten Gespräch schreiben darf, während das erste noch antwortet.
    runs: Vec<Run>,
}

static STATE: Mutex<Option<Inner>> = Mutex::new(None);
static RUN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/* ── Zustand ────────────────────────────────────────────────────────── */

fn api() -> Option<Arc<Api>> {
    STATE.lock().unwrap().as_ref().map(|s| s.api.clone())
}

fn settings_view() -> Option<SettingsView> {
    STATE.lock().unwrap().as_ref().map(|s| s.cfg.view())
}

/// Läufe eines Gesprächs abholen — nur diese, andere Gespräche laufen weiter.
fn take_session(sid: &str) -> Vec<Run> {
    let mut guard = STATE.lock().unwrap();
    let Some(inner) = guard.as_mut() else { return Vec::new() };
    let mut out = Vec::new();
    let mut keep = Vec::new();
    for r in inner.runs.drain(..) {
        if r.sid == sid { out.push(r) } else { keep.push(r) }
    }
    inner.runs = keep;
    out
}

fn take_run_id(id: u64) -> Option<Run> {
    let mut guard = STATE.lock().unwrap();
    let inner = guard.as_mut()?;
    if let Some(pos) = inner.runs.iter().position(|r| r.id == id) {
        Some(inner.runs.remove(pos))
    } else {
        None
    }
}

fn take_all() -> Vec<Run> {
    let mut guard = STATE.lock().unwrap();
    match guard.as_mut() {
        Some(inner) => std::mem::take(&mut inner.runs),
        None => Vec::new(),
    }
}

fn store_run(run: Run) {
    let mut guard = STATE.lock().unwrap();
    if let Some(inner) = guard.as_mut() {
        /* Ein zweiter Auftrag im selben Gespräch löst den ersten ab. */
        let mut keep = Vec::new();
        for old in inner.runs.drain(..) {
            if old.sid == run.sid {
                old.task.abort();
            } else {
                keep.push(old);
            }
        }
        keep.push(run);
        inner.runs = keep;
    }
}

/* ── Anfrage lesen ──────────────────────────────────────────────────── */

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: Vec<u8>,
}

impl Req {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn arg(&self, key: &str) -> String {
        let val = self.json()[key].as_str().unwrap_or("").to_string();
        if !val.is_empty() {
            return val;
        }
        self.query.get(key).cloned().unwrap_or_default()
    }
}

fn find_head(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn read_req(stream: &mut TcpStream) -> Option<Req> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = find_head(&buf) {
            break pos;
        }
        if buf.len() > MAX_HEAD {
            return None;
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split("\r\n");
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if length > MAX_BODY {
        return None;
    }

    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);

    let (path, rest) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let mut query = HashMap::new();
    for pair in rest.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(decode(k), decode(v));
    }

    Some(Req { method, path, query, body })
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/* ── Antwort schreiben ──────────────────────────────────────────────── */

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600 + 1, (s % 3600) / 60, s % 60)
}

async fn send(stream: &mut TcpStream, code: u16, value: &Value) {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.flush().await;
}

async fn send_ok(stream: &mut TcpStream, value: Value) {
    send(stream, 200, &value).await;
}

async fn send_err(stream: &mut TcpStream, code: u16, message: impl Into<String>) {
    send(stream, code, &json!({ "error": message.into() })).await;
}

async fn start_stream(stream: &mut TcpStream) -> bool {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
                Cache-Control: no-store\r\nX-Accel-Buffering: no\r\n\
                Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
    stream.write_all(head.as_bytes()).await.is_ok() && stream.flush().await.is_ok()
}

async fn send_event(stream: &mut TcpStream, event: &UiEvent) -> bool {
    let text = match serde_json::to_string(event) {
        Ok(t) => t,
        Err(_) => return true,
    };
    let frame = format!("data: {text}\n\n");
    stream.write_all(frame.as_bytes()).await.is_ok() && stream.flush().await.is_ok()
}

/* ── Wege ───────────────────────────────────────────────────────────── */

async fn route(req: Req, stream: &mut TcpStream) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/health") => send_ok(stream, json!({ "ok": true })).await,

        ("GET", "/state") => match settings_view() {
            Some(view) => {
                let sessions = match api() {
                    Some(client) => client.sessions().await.unwrap_or_default(),
                    None => Vec::new(),
                };
                let session_json: Vec<Value> =
                    sessions.iter().filter_map(|s| serde_json::to_value(s).ok()).collect();
                send_ok(stream, json!({ "settings": view, "sessions": session_json })).await;
            }
            None => send_err(stream, 500, "Zustand fehlt").await,
        },

        ("GET", "/sessions") => match api() {
            Some(client) => match client.sessions().await {
                Ok(list) => send_ok(stream, json!(list)).await,
                Err(e) => send_err(stream, 500, e.to_string()).await,
            },
            None => send_err(stream, 500, "Zustand fehlt").await,
        },

        ("GET", "/messages") => {
            let session = req.arg("session");
            match api() {
                Some(client) => match client.turns(&session).await {
                    Ok(list) => send_ok(stream, json!(list)).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("POST", "/session/create") => match api() {
            Some(client) => match client.create_session().await {
                Ok(s) => send_ok(stream, json!(s)).await,
                Err(e) => send_err(stream, 500, e.to_string()).await,
            },
            None => send_err(stream, 500, "Zustand fehlt").await,
        },

        ("POST", "/session/patch") => {
            let body = req.json();
            let id = body["sessionId"].as_str().unwrap_or("").to_string();
            let mut patch = serde_json::Map::new();
            for key in ["title", "pinned", "archived", "hidden"] {
                if let Some(v) = body.get(key) {
                    if !v.is_null() {
                        patch.insert(key.to_string(), v.clone());
                    }
                }
            }
            match api() {
                Some(client) => match client.patch_session(&id, Value::Object(patch)).await {
                    Ok(()) => send_ok(stream, json!({ "ok": true })).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("POST", "/session/delete") => {
            let id = req.arg("sessionId");
            match api() {
                Some(client) => match client.delete_session(&id).await {
                    Ok(()) => send_ok(stream, json!({ "ok": true })).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("POST", "/slash") => {
            let body = req.json();
            let sid = body["sessionId"].as_str().unwrap_or("").to_string();
            let cmd = body["command"].as_str().unwrap_or("").to_string();
            println!("[{}] slash {} · {}", stamp(), sid, cmd);
            let outcome = match api() {
                Some(api) => aesmyra_core::slash::run(&api, &sid, &cmd).await,
                None => Err("Kein Zugangsschlüssel hinterlegt.".to_string()),
            };
            match outcome {
                Ok(v) => send_ok(stream, v).await,
                Err(e) => send_err(stream, 500, e).await,
            }
        }

        ("POST", "/chat") => {
            println!(
                "[{}] chat {} · {} Zeichen",
                stamp(),
                req.arg("sessionId"),
                req.arg("text").chars().count()
            );
            chat(req, stream).await
        }

        ("POST", "/media") => {
            let path = req.arg("path");
            match aesmyra_core::media::data_url(&path) {
                Ok(url) => send_ok(stream, json!(url)).await,
                Err(e) => send_err(stream, 400, e).await,
            }
        }

        ("POST", "/stop") => {
            let sid = req.arg("sessionId");
            let stopped: Vec<Run> = if sid.is_empty() { take_all() } else { take_session(&sid) };
            if stopped.is_empty() {
                send_ok(stream, json!({ "ok": true, "stopped": 0 })).await;
                return;
            }
            let n = stopped.len();
            for run in stopped {
                run.task.abort();
                let _ = run.tx.send(UiEvent::Cancelled { run_id: String::new() }).await;
                let _ = run.tx.send(UiEvent::Finished { run_id: String::new(), ok: false }).await;
            }
            send_ok(stream, json!({ "ok": true, "stopped": n })).await;
        }

        ("POST", "/steer") => {
            let run_id = req.arg("runId");
            let message = req.arg("message");
            match api() {
                Some(client) => match client.steer(&run_id, &message).await {
                    Ok(()) => send_ok(stream, json!({ "ok": true })).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("GET", "/run/state") => {
            let run_id = req.arg("runId");
            match api() {
                Some(client) => match client.run_state(&run_id).await {
                    Ok(v) => send_ok(stream, v).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("POST", "/approve") => {
            let run_id = req.arg("runId");
            let choice = req.arg("choice");
            let request_id = req.arg("requestId");
            let generation = req
                .json()
                .get("executionGeneration")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            match api() {
                Some(client) => match client.approve(&run_id, &choice, &request_id, generation).await {
                    Ok(()) => send_ok(stream, json!({ "ok": true })).await,
                    Err(e) => send_err(stream, 500, e.to_string()).await,
                },
                None => send_err(stream, 500, "Zustand fehlt").await,
            }
        }

        ("POST", "/settings") => {
            let body = req.json();
            let url = body["serverUrl"].as_str().unwrap_or("");
            let token = body["token"].as_str();
            match Config::save(url, token) {
                Ok(cfg) => match Api::new(cfg.server_url.clone(), cfg.token.clone()) {
                    Ok(client) => {
                        let view = cfg.view();
                        if let Some(inner) = STATE.lock().unwrap().as_mut() {
                            inner.cfg = cfg;
                            inner.api = Arc::new(client);
                        }
                        send_ok(stream, json!(view)).await;
                    }
                    Err(e) => send_err(stream, 400, e.to_string()).await,
                },
                Err(e) => send_err(stream, 400, e).await,
            }
        }

        ("POST", "/settings/test") => match api() {
            Some(client) => match client.sessions().await {
                Ok(list) => send_ok(stream, json!({ "message": format!("{} Gespräche erreichbar", list.len()) })).await,
                Err(ApiError::Status(401 | 403, _)) => {
                    send_err(stream, 500, "Der Server hat den Schlüssel abgelehnt.").await
                }
                Err(e) => send_err(stream, 500, e.to_string()).await,
            },
            None => send_err(stream, 500, "Zustand fehlt").await,
        },

        _ => send_err(stream, 404, "unbekannter Weg").await,
    }
}

/* ── Lauf ───────────────────────────────────────────────────────────── */

fn run_error_text(e: &ApiError) -> String {
    match e {
        ApiError::Status(401 | 403, _) => "Der Server hat den Schlüssel abgelehnt.".to_string(),
        ApiError::Timeout => "Zeitüberschreitung".to_string(),
        ApiError::Unreachable => "Server nicht erreichbar.".to_string(),
        other => other.to_string(),
    }
}

async fn chat(req: Req, stream: &mut TcpStream) {
    let Some(client) = api() else {
        send_err(stream, 500, "Zustand fehlt").await;
        return;
    };
    let session_id = req.arg("sessionId");
    let text = req.arg("text");
    if session_id.is_empty() || text.is_empty() {
        send_err(stream, 400, "Sitzung oder Text fehlt").await;
        return;
    }

    let (tx, mut rx) = mpsc::channel::<UiEvent>(512);
    let id = RUN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let producer = {
        let tx = tx.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            match client.chat_stream(&sid, &text, &tx).await {
                Ok(result) => {
                    let ok = matches!(result.outcome, Outcome::Completed | Outcome::Recovered);
                    let _ = tx.send(UiEvent::Finished { run_id: result.run_id, ok }).await;
                }
                Err(e) => {
                    let message = run_error_text(&e);
                    let _ = tx.send(UiEvent::Error { run_id: String::new(), message }).await;
                    let _ = tx.send(UiEvent::Finished { run_id: String::new(), ok: false }).await;
                }
            }
        })
    };
    store_run(Run { id, sid: session_id.clone(), tx, task: producer });

    if !start_stream(stream).await {
        take_run_id(id);
        return;
    }

    while let Some(event) = rx.recv().await {
        if !send_event(stream, &event).await {
            break;
        }
        if matches!(event, UiEvent::Finished { .. }) {
            break;
        }
    }

    let _ = take_run_id(id);
}

/* ── Start ──────────────────────────────────────────────────────────── */

async fn handle(mut stream: TcpStream) {
    let req = match tokio::time::timeout(Duration::from_secs(10), read_req(&mut stream)).await {
        Ok(Some(req)) => req,
        _ => return,
    };
    route(req, &mut stream).await;
}

#[tokio::main]
async fn main() {
    let cfg = Config::load();
    let client = Api::new(cfg.server_url.clone(), cfg.token.clone()).unwrap_or_else(|_| {
        Api::new("http://127.0.0.1:8642", cfg.token.clone()).expect("Vorgabeadresse")
    });
    *STATE.lock().unwrap() = Some(Inner { cfg, api: Arc::new(client), runs: Vec::new() });

    let listener = TcpListener::bind(ADDR).await.expect("Port 8121 belegt");
    println!("Kern für die Vorschau auf http://{ADDR}/");
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle(stream));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}
