//! Zugriff auf den Hermes-Server.
//!
//! Alles, was Netz ist, passiert hier — mit einer Verbindung, die wiederverwendet
//! wird, und mit einem Ereignisstrom, der in 16-ms-Stücken zusammengefasst wird.
//! Dadurch bekommt die Oberfläche rund 60 Nachrichten pro Sekunde statt einer
//! pro Token.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

use crate::model::{turns_from, Session, Turn};
use crate::sse::{Frame, SseParser};

/// Frist für kurze JSON-Aufrufe. Gilt je Anfrage, nicht für Ströme.
const JSON_TIMEOUT: Duration = Duration::from_secs(30);

/// Frist zwischen zwei Lesevorgängen. Ein Strom darf beliebig lange laufen,
/// solange Daten nachkommen; nur Stillstand wird abgebrochen.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Server nicht erreichbar")]
    Unreachable,
    #[error("Zeitüberschreitung")]
    Timeout,
    #[error("Server meldet Status {0}: {1}")]
    Status(u16, String),
    #[error("Antwort nicht lesbar")]
    Decode,
    #[error("{0}")]
    Stream(String),
    #[error("{0}")]
    Config(String),
}

/// Klartext für die Oberfläche. Wichtig: `permission_denied` ist kein Netzfehler —
/// der Server lässt Läufe nur auf Sitzungen zu, die über die Schnittstelle
/// angelegt wurden (`api_server`) oder aus einem Raum stammen. Sitzungen aus
/// Terminal, Desktop oder Mobilteil sind hier nur lesbar.
pub fn error_text(e: &ApiError) -> String {
    match e {
        ApiError::Status(401, _) => "Der Server hat den Schlüssel abgelehnt.".to_string(),
        ApiError::Status(403, body) if body.contains("permission_denied") => {
            "Diese Sitzung gehört einem anderen Programm und lässt sich hier nicht fortsetzen. Lege eine neue Sitzung an."
                .to_string()
        }
        ApiError::Status(403, _) => "Der Server hat die Anfrage abgelehnt.".to_string(),
        ApiError::Status(404, _) => "Die Sitzung gibt es auf dem Server nicht mehr.".to_string(),
        ApiError::Timeout => "Zeitüberschreitung — der Server hat nicht geantwortet.".to_string(),
        ApiError::Unreachable => "Server nicht erreichbar.".to_string(),
        other => other.to_string(),
    }
}

/// Was die Oberfläche während eines Laufs erfährt.
///
/// Jedes Ereignis trägt die `run_id` des Laufs, zu dem es gehört — leer, wenn
/// sie (noch) nicht bekannt ist. Die Oberfläche kann damit Ereignisse eines
/// abgelösten Laufs verwerfen.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiEvent {
    RunStarted { run_id: String },
    Delta { run_id: String, text: String },
    Commentary { run_id: String, text: String },
    ToolStarted { run_id: String, name: String, args: String, preview: String },
    ToolFinished { run_id: String, name: String, ok: bool, preview: String },
    Approval {
        run_id: String,
        request_id: String,
        command: String,
        choices: Vec<String>,
        /// Laufgeneration, die der Server zur Freigabe verlangt. Ohne sie
        /// lehnt der Authority-Pfad den Aufruf mit `invalid_params` ab.
        execution_generation: u64,
    },
    /// Der Server hat die Freigabe zurückgezogen (Laufgeneration gewechselt,
    /// anderweitig beantwortet oder Lauf beendet). Die Karte in der
    /// Oberfläche ist damit ungültig — ein Klick darauf liefe in ein 409.
    ApprovalSettled { run_id: String, request_id: String },
    Completed { run_id: String, content: String },
    /// Gemessene Kontextgröße des Laufs: die Prompt-Token, die der Server
    /// für die letzte Anfrage gezählt hat.
    /// `input_tokens` ist der Mittelwert über die Anfragen des Laufs; `api_calls`
    /// nennt deren Anzahl (Werkzeugrunden plus Abschluss). Der Server meldet am
    /// Laufende die **Summe** aller Anfragen (`session_prompt_tokens`), nicht den
    /// Kontext einer Anfrage — deshalb teilt der Kern durch `api_calls`.
    Usage { run_id: String, input_tokens: u64, output_tokens: u64, api_calls: u32 },
    Failed { run_id: String, message: String },
    Cancelled { run_id: String },
    Error { run_id: String, message: String },
    Reconnecting { run_id: String, attempt: u32, wait_ms: u64 },
    Recovered { run_id: String, status: String },
    StreamEnded { run_id: String },
    Finished { run_id: String, ok: bool },
}

impl UiEvent {
    /// Kennung des Laufs, zu dem das Ereignis gehört — leer, wenn unbekannt.
    pub fn run_id(&self) -> &str {
        match self {
            UiEvent::RunStarted { run_id }
            | UiEvent::Delta { run_id, .. }
            | UiEvent::Commentary { run_id, .. }
            | UiEvent::ToolStarted { run_id, .. }
            | UiEvent::ToolFinished { run_id, .. }
            | UiEvent::Approval { run_id, .. }
            | UiEvent::ApprovalSettled { run_id, .. }
            | UiEvent::Completed { run_id, .. }
            | UiEvent::Usage { run_id, .. }
            | UiEvent::Failed { run_id, .. }
            | UiEvent::Cancelled { run_id }
            | UiEvent::Error { run_id, .. }
            | UiEvent::Reconnecting { run_id, .. }
            | UiEvent::Recovered { run_id, .. }
            | UiEvent::StreamEnded { run_id }
            | UiEvent::Finished { run_id, .. } => run_id.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Recovered,
    Failed,
    Aborted,
}

/// Ergebnis eines Laufs samt Kennung, damit auch das abschließende Ereignis
/// zugeordnet werden kann.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub outcome: Outcome,
    pub run_id: String,
}

pub struct Api {
    http: reqwest::Client,
    base: String,
    token: String,
}

fn seg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn classify(e: &reqwest::Error) -> ApiError {
    if e.is_timeout() {
        ApiError::Timeout
    } else {
        ApiError::Unreachable
    }
}

impl Api {
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(4)
            .tcp_nodelay(true)
            .connect_timeout(Duration::from_secs(6))
            // Kein Gesamtlimit: eine Antwort darf länger als zwei Minuten
            // streamen. Begrenzt wird nur der Abstand zwischen zwei Stücken.
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(|_| ApiError::Unreachable)?;
        Ok(Self { http, base: base.into(), token: token.into() })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send_json(&self, req: reqwest::RequestBuilder) -> Result<Value, ApiError> {
        let resp = req
            .timeout(JSON_TIMEOUT)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| classify(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| classify(&e))?;
        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16(), shorten(&body, 240)));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|_| ApiError::Decode)
    }

    pub async fn sessions(&self) -> Result<Vec<Session>, ApiError> {
        let v = self.send_json(self.http.get(self.url("/api/sessions"))).await?;
        Ok(Session::visible(&v))
    }

    pub async fn turns(&self, session_id: &str) -> Result<Vec<Turn>, ApiError> {
        let path = format!("/api/sessions/{}/messages?limit=500", seg(session_id));
        let v = self.send_json(self.http.get(self.url(&path))).await?;
        Ok(turns_from(&v))
    }

    /// Eine einzelne Sitzung mit Zählern, Modell und Zeitstempeln.
    pub async fn session(&self, session_id: &str) -> Result<Value, ApiError> {
        let path = format!("/api/sessions/{}", seg(session_id));
        self.send_json(self.http.get(self.url(&path))).await
    }

    /// Die auf dem Server freigeschalteten Werkzeuggruppen.
    pub async fn toolsets(&self) -> Result<Value, ApiError> {
        self.send_json(self.http.get(self.url("/v1/toolsets"))).await
    }

    /// Die auf dem Server installierten Fähigkeiten.
    pub async fn skills(&self) -> Result<Value, ApiError> {
        self.send_json(self.http.get(self.url("/v1/skills"))).await
    }

    /// Sperrt ein Modell für die Sitzung (Server-seitiger Modellwechsel).
    pub async fn set_model(&self, session_id: &str, model: &str) -> Result<Value, ApiError> {
        let path = format!("/api/sessions/{}/model", seg(session_id));
        self.send_json(self.http.post(self.url(&path)).json(&json!({ "model": model })))
            .await
    }

    /// Legt eine benannte Sitzung an und gibt ihre ID zurück.
    pub async fn new_session(&self, title: &str) -> Result<String, ApiError> {
        let v = self
            .send_json(self.http.post(self.url("/api/sessions")).json(&json!({ "title": title })))
            .await?;
        v.pointer("/session/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(ApiError::Decode)
    }

    /// Schickt eine Nachricht in eine Sitzung und liest den Lauf bis zum Ende.
    pub async fn chat_send(&self, session_id: &str, message: &str) -> Result<(), ApiError> {
        let path = format!("/api/sessions/{}/chat", seg(session_id));
        // Ein Lauf kann mehrere Minuten dauern und schickt währenddessen
        // nicht zwingend Bytes. Der gemeinsame Client begrenzt aber den
        // Abstand zwischen zwei Stücken auf 30 s — hier deshalb ein eigener
        // Client ohne Zwischenstück-Limit, dafür mit großzügigem Gesamtlimit.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(900))
            .build()
            .map_err(|_| ApiError::Unreachable)?;
        let res = http
            .post(self.url(&path))
            .bearer_auth(&self.token)
            .json(&json!({ "message": message }))
            .send()
            .await
            .map_err(|e| if e.is_timeout() { ApiError::Timeout } else { ApiError::Unreachable })?;
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(ApiError::Status(status.as_u16(), body.chars().take(300).collect()));
        }
        let _ = res.text().await;
        Ok(())
    }

    /// Eine einzelne Antwort des Modells ohne Sitzung (Chat-Completions).
    pub async fn complete(&self, prompt: &str) -> Result<String, ApiError> {
        // Zusammenfassungen über ein langes Transkript dauern deutlich
        // länger als die 30 s des gemeinsamen JSON-Pfads.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|_| ApiError::Unreachable)?;
        let body = json!({
            "model": "hermes-agent",
            "messages": [{ "role": "user", "content": prompt }],
            "stream": false,
        });
        let resp = http
            .post(self.url("/v1/chat/completions"))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| classify(&e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| classify(&e))?;
        if !status.is_success() {
            return Err(ApiError::Status(status.as_u16(), shorten(&text, 240)));
        }
        let v: Value = serde_json::from_str(&text).map_err(|_| ApiError::Decode)?;
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(ApiError::Decode)
    }

    pub async fn create_session(&self) -> Result<Session, ApiError> {
        let v = self
            .send_json(self.http.post(self.url("/api/sessions")).json(&json!({})))
            .await?;
        let inner = v.get("session").or_else(|| v.get("data")).unwrap_or(&v);
        Session::from_value(inner).ok_or(ApiError::Decode)
    }

    pub async fn patch_session(&self, session_id: &str, patch: Value) -> Result<(), ApiError> {
        let path = format!("/api/sessions/{}", seg(session_id));
        self.send_json(self.http.patch(self.url(&path)).json(&patch)).await?;
        Ok(())
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        let path = format!("/api/sessions/{}", seg(session_id));
        self.send_json(self.http.delete(self.url(&path))).await?;
        Ok(())
    }

    pub async fn run_status(&self, run_id: &str) -> Result<String, ApiError> {
        let path = format!("/v1/runs/{}", seg(run_id));
        let v = self.send_json(self.http.get(self.url(&path))).await?;
        let status = v
            .get("status")
            .and_then(Value::as_str)
            .or_else(|| v.get("run").and_then(|r| r.get("status")).and_then(Value::as_str))
            .or_else(|| v.get("data").and_then(|r| r.get("status")).and_then(Value::as_str))
            .unwrap_or("unknown");
        Ok(status.to_string())
    }

    /// Vollständiger Laufzustand: `status`, `execution_generation` und
    /// `pending_controls`. Genau das braucht die Oberfläche, wenn eine
    /// Freigabe mit `stale_generation` abgelehnt wurde: Sie erfährt damit die
    /// aktuelle Generation und ob noch eine Freigabe offen ist.
    pub async fn run_state(&self, run_id: &str) -> Result<Value, ApiError> {
        let path = format!("/v1/runs/{}", seg(run_id));
        self.send_json(self.http.get(self.url(&path))).await
    }

    pub async fn approve(
        &self,
        run_id: &str,
        choice: &str,
        request_id: &str,
        execution_generation: u64,
    ) -> Result<(), ApiError> {
        let path = format!("/v1/runs/{}/approval", seg(run_id));
        // Der Server prüft die Feldmenge exakt: request_id, execution_generation, choice.
        let body = json!({
            "choice": choice,
            "request_id": request_id,
            "execution_generation": execution_generation,
        });
        self.send_json(self.http.post(self.url(&path)).json(&body)).await?;
        Ok(())
    }

    pub async fn steer(&self, run_id: &str, message: &str) -> Result<(), ApiError> {
        let path = format!("/v1/runs/{}/steer", seg(run_id));
        self.send_json(self.http.post(self.url(&path)).json(&json!({ "message": message }))).await?;
        Ok(())
    }

    /// Schickt eine Nachricht und übersetzt den Ereignisstrom in UiEvents.
    ///
    /// Die Anfrage selbst bekommt bewusst keine Gesamtfrist: ein Lauf kann
    /// Stunden dauern. Nur der Abstand zwischen zwei Netzstücken ist begrenzt.
    pub async fn chat_stream(
        &self,
        session_id: &str,
        message: &str,
        tx: &Sender<UiEvent>,
    ) -> Result<RunResult, ApiError> {
        let path = format!("/api/sessions/{}/chat/stream", seg(session_id));
        let req = self
            .http
            .post(self.url(&path))
            .header("Accept", "text/event-stream")
            .bearer_auth(&self.token)
            .json(&json!({ "message": message }));

        let resp = req.send().await.map_err(|e| classify(&e))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Status(status.as_u16(), shorten(&body, 240)));
        }

        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::default();
        let mut st = StreamState::default();

        // 16 ms Takt: sammelt Textstücke, bevor sie über die Brücke gehen.
        let mut tick = tokio::time::interval(Duration::from_millis(16));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        'outer: loop {
            tokio::select! {
                biased;
                _ = tick.tick() => {
                    if let Some(text) = st.take_pending() {
                        let tag = st.tag();
                        emit(tx, &mut st, UiEvent::Delta { run_id: tag, text }).await;
                        if st.aborted { break 'outer; }
                    }
                }
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            let frames = match parser.push(&bytes) {
                                Ok(frames) => frames,
                                Err(e) => return Err(ApiError::Stream(e.to_string())),
                            };
                            for f in frames {
                                dispatch(f, &mut st, tx).await;
                                if st.done || st.aborted {
                                    break 'outer;
                                }
                            }
                        }
                        Some(Err(_)) => break 'outer,
                        None => break 'outer,
                    }
                }
            }
        }

        // Endet der Strom mit einem Block ohne abschließende Leerzeile, wäre
        // `done` sonst verloren.
        if !st.aborted && !st.done {
            if let Some(frame) = parser.finish() {
                dispatch(frame, &mut st, tx).await;
            }
        }

        if st.aborted {
            return Ok(RunResult { outcome: Outcome::Aborted, run_id: st.tag() });
        }

        if let Some(text) = st.take_pending() {
            let tag = st.tag();
            let _ = tx.send(UiEvent::Delta { run_id: tag, text }).await;
        }
        let tag = st.tag();
        let _ = tx.send(UiEvent::StreamEnded { run_id: tag }).await;

        if st.done {
            return Ok(RunResult { outcome: Outcome::Completed, run_id: st.tag() });
        }

        // Strom ohne Abschluss: der Lauf läuft auf dem Server weiter. Also
        // nachsehen statt neu senden — sonst entstehen doppelte Nachrichten.
        let outcome = self.recover(st.run_id.as_deref(), tx).await?;
        Ok(RunResult { outcome, run_id: st.run_id.clone().unwrap_or_default() })
    }

    async fn recover(&self, run_id: Option<&str>, tx: &Sender<UiEvent>) -> Result<Outcome, ApiError> {
        let start = Instant::now();
        let mut wait = 1000u64;
        let mut attempt = 0u32;
        let tag = run_id.unwrap_or_default().to_string();

        while start.elapsed() < Duration::from_secs(180) {
            attempt += 1;
            let _ = tx.send(UiEvent::Reconnecting { run_id: tag.clone(), attempt, wait_ms: wait }).await;
            tokio::time::sleep(Duration::from_millis(wait)).await;
            wait = (wait * 2).min(15_000);

            if let Some(id) = run_id {
                if let Ok(status) = self.run_status(id).await {
                    match status.as_str() {
                        "completed" | "done" | "finished" => {
                            let _ = tx.send(UiEvent::Recovered { run_id: tag.clone(), status }).await;
                            return Ok(Outcome::Recovered);
                        }
                        // Ein gescheiterter oder abgebrochener Lauf ist nicht
                        // „bereit" — sonst meldet die Oberfläche Erfolg.
                        "failed" | "cancelled" | "error" => {
                            let message = format!("Lauf endete mit Status „{status}“");
                            let _ = tx.send(UiEvent::Failed { run_id: tag.clone(), message }).await;
                            return Ok(Outcome::Failed);
                        }
                        _ => continue,
                    }
                }
            } else if attempt >= 3 {
                // Ohne Kennung bleibt nur der Verlauf selbst als Beweis — und
                // der beweist keinen Erfolg.
                let message = "Laufzustand nicht feststellbar".to_string();
                let _ = tx.send(UiEvent::Error { run_id: String::new(), message }).await;
                return Ok(Outcome::Failed);
            }
        }

        let _ = tx.send(UiEvent::Error { run_id: tag, message: "keine Verbindung".into() }).await;
        Ok(Outcome::Failed)
    }
}

/* ── Ereignisstrom ──────────────────────────────────────────────────── */

/// Laufender Zustand eines Stroms. `run_id` wird beim ersten `run.started`
/// gesetzt und danach jedem Ereignis mitgegeben.
#[derive(Debug, Default)]
struct StreamState {
    pending: String,
    run_id: Option<String>,
    done: bool,
    aborted: bool,
    /// Anzahl der Werkzeugrunden im Lauf. Jede Runde kostet eine eigene Anfrage;
    /// zusammen mit dem Abschluss ergibt das die Zahl der Prompt-Messungen.
    tools: u32,
}

impl StreamState {
    fn tag(&self) -> String {
        self.run_id.clone().unwrap_or_default()
    }

    fn take_pending(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

async fn emit(tx: &Sender<UiEvent>, st: &mut StreamState, event: UiEvent) {
    if tx.send(event).await.is_err() {
        st.aborted = true;
    }
}

async fn dispatch(frame: Frame, st: &mut StreamState, tx: &Sender<UiEvent>) {
    match frame.event.as_str() {
        "run.started" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                if let Some(id) = v
                    .get("run_id")
                    .or_else(|| v.get("run").and_then(|r| r.get("id")))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                {
                    st.run_id = Some(id.clone());
                    emit(tx, st, UiEvent::RunStarted { run_id: id }).await;
                }
            }
        }
        "assistant.delta" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                if let Some(d) = v.get("delta").and_then(Value::as_str) {
                    st.pending.push_str(d);
                    if st.pending.len() >= 4096 {
                        if let Some(text) = st.take_pending() {
                            let tag = st.tag();
                            emit(tx, st, UiEvent::Delta { run_id: tag, text }).await;
                        }
                    }
                }
            }
        }
        "assistant.commentary" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                if let Some(t) = v.get("text").and_then(Value::as_str) {
                    if let Some(text) = st.take_pending() {
                        let tag = st.tag();
                        emit(tx, st, UiEvent::Delta { run_id: tag, text }).await;
                    }
                    let text = t.to_string();
                    let tag = st.tag();
                    emit(tx, st, UiEvent::Commentary { run_id: tag, text }).await;
                }
            }
        }
        "tool.started" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                st.tools = st.tools.saturating_add(1);
                let tag = st.tag();
                let event = UiEvent::ToolStarted {
                    run_id: tag,
                    name: tool_name(&v),
                    args: v.get("args").map(|a| a.to_string()).unwrap_or_default(),
                    preview: short_field(&v, "preview"),
                };
                emit(tx, st, event).await;
            }
        }
        "tool.completed" | "tool.failed" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                let tag = st.tag();
                let event = UiEvent::ToolFinished {
                    run_id: tag,
                    name: tool_name(&v),
                    ok: frame.event == "tool.completed",
                    preview: short_field(&v, "preview"),
                };
                emit(tx, st, event).await;
            }
        }
        "approval.request" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                let choices = v
                    .get("choices")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_else(|| vec!["once".into(), "session".into(), "deny".into()]);
                let run_id = st
                    .run_id
                    .clone()
                    .or_else(|| short_field_opt(&v, "run_id"))
                    .unwrap_or_default();
                let event = UiEvent::Approval {
                    run_id,
                    request_id: short_field(&v, "request_id"),
                    command: v
                        .get("command")
                        .or_else(|| v.get("summary"))
                        .or_else(|| v.get("description"))
                        .and_then(Value::as_str)
                        .unwrap_or("Werkzeugaufruf")
                        .to_string(),
                    choices,
                    execution_generation: v
                        .get("execution_generation")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                };
                emit(tx, st, event).await;
            }
        }
        "approval.settled" | "clarify.settled" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                let event = UiEvent::ApprovalSettled {
                    run_id: st.run_id.clone().unwrap_or_default(),
                    request_id: short_field(&v, "prompt_id"),
                };
                emit(tx, st, event).await;
            }
        }
        "assistant.completed" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                let content = v.get("content").and_then(Value::as_str).unwrap_or("").to_string();
                let tag = st.tag();
                emit(tx, st, UiEvent::Completed { run_id: tag, content }).await;
            }
        }
        "run.completed" => {
            if let Ok(v) = serde_json::from_str::<Value>(&frame.data) {
                let usage = v.get("usage").unwrap_or(&Value::Null);
                let input = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                let output = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                if input > 0 {
                    // `usage.input_tokens` ist die Summe über alle Anfragen des Laufs
                    // (`session_prompt_tokens`), nicht der Kontext einer Anfrage. Bei
                    // einem Werkzeuglauf summiert sie den stetig wachsenden Prompt
                    // mehrfach auf und übertreibt dadurch um ein Vielfaches. Deshalb
                    // durch die Zahl der Anfragen teilen: Werkzeugrunden plus Abschluss.
                    let calls = st.tools.saturating_add(1);
                    let per_call = (input as f64 / calls as f64).round().max(1.0) as u64;
                    let tag = st.tag();
                    emit(
                        tx,
                        st,
                        UiEvent::Usage { run_id: tag, input_tokens: per_call, output_tokens: output, api_calls: calls },
                    )
                    .await;
                }
            }
        }
        "run.failed" => {
            let message = serde_json::from_str::<Value>(&frame.data)
                .ok()
                .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "Lauf fehlgeschlagen".into());
            let tag = st.tag();
            emit(tx, st, UiEvent::Failed { run_id: tag, message: error_code_text(&message) }).await;
        }
        "run.cancelled" => {
            let tag = st.tag();
            emit(tx, st, UiEvent::Cancelled { run_id: tag }).await;
        }
        "error" => {
            let message = serde_json::from_str::<Value>(&frame.data)
                .ok()
                .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "Fehler".into());
            let tag = st.tag();
            emit(tx, st, UiEvent::Error { run_id: tag, message: error_code_text(&message) }).await;
        }
        "done" => {
            st.done = true;
        }
        _ => {}
    }
}

/// Serverfehler im Ereignisstrom kommen als kurzer Code. Für die Oberfläche
/// zählt der Satz, den ein Mensch versteht.
fn error_code_text(code: &str) -> String {
    match code {
        "permission_denied" => "Diese Sitzung gehört einem anderen Programm und lässt sich hier nicht fortsetzen. Lege eine neue Sitzung an.",
        "session_busy" => "In dieser Sitzung läuft schon etwas — warte, bis der Lauf endet.",
        "admission_conflict" => "Ein anderer Lauf hat die Sitzung gerade übernommen.",
        "unknown_execution" => "Der Server kennt diesen Lauf nicht mehr.",
        "not_found" => "Die Sitzung gibt es auf dem Server nicht mehr.",
        "timeout" => "Der Lauf hat zu lange gebraucht und wurde abgebrochen.",
        "" => "Der Lauf ist fehlgeschlagen.",
        other => other,
    }
    .to_string()
}

fn tool_name(v: &Value) -> String {
    v.get("tool_name")
        .or_else(|| v.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("werkzeug")
        .to_string()
}

fn short_field(v: &Value, key: &str) -> String {
    short_field_opt(v, key).unwrap_or_default()
}

fn short_field_opt(v: &Value, key: &str) -> Option<String> {
    /* Fehlt der Wert oder ist er null, bleibt das Feld leer — „null" ist kein Text. */
    match v.get(key) {
        None | Some(Value::Null) => None,
        Some(p) => Some(shorten(p.to_string().trim_matches('"'), 400)),
    }
}

fn shorten(s: &str, n: usize) -> String {
    let clean = s.trim();
    if clean.chars().count() <= n {
        return clean.to_string();
    }
    let cut: String = clean.chars().take(n).collect();
    format!("{cut} …")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Arc;

    /// Winziger HTTP-Server auf einem eigenen Thread: bekommt den Pfad und
    /// liefert die rohe Antwort. Reicht für Ereignisstrom und Statusabfrage.
    fn serve(handler: impl Fn(&str) -> String + Send + Sync + 'static) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = Arc::new(handler);
        std::thread::spawn(move || {
            for sock in listener.incoming() {
                let Ok(mut sock) = sock else { break };
                let mut buf = [0u8; 8192];
                let _ = sock.read(&mut buf);
                let head = String::from_utf8_lossy(&buf).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();
                let resp = handler(&path);
                let _ = sock.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn http(status: &str, content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn sse(body: &str) -> String {
        http("200 OK", "text/event-stream", body)
    }

    fn json(body: &str) -> String {
        http("200 OK", "application/json", body)
    }

    #[tokio::test]
    async fn letzter_block_ohne_leerzeile_beendet_den_lauf() {
        let body = concat!(
            "event: run.started\ndata: {\"run_id\":\"r1\"}\n\n",
            "event: assistant.delta\ndata: {\"delta\":\"hallo\"}\n\n",
            "event: done\ndata: {}"
        );
        let base = serve(move |_| sse(body));
        let api = Api::new(base, "test").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);

        let result = api.chat_stream("s1", "hi", &tx).await.expect("Strom");
        assert_eq!(result.outcome, Outcome::Completed, "Restblock wurde nicht ausgewertet");
        assert_eq!(result.run_id, "r1");

        drop(tx);
        let mut kinds = Vec::new();
        while let Some(event) = rx.recv().await {
            let v = serde_json::to_value(&event).unwrap();
            assert_eq!(v["run_id"], "r1", "Ereignis ohne run_id: {v}");
            kinds.push(v["kind"].as_str().unwrap().to_string());
        }
        assert!(kinds.contains(&"run_started".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"delta".to_string()), "{kinds:?}");
        assert!(kinds.contains(&"stream_ended".to_string()), "{kinds:?}");
    }

    #[tokio::test]
    async fn meldet_die_prompt_token() {
        let body = concat!(
            "event: run.started\ndata: {\"run_id\":\"r4\"}\n\n",
            "event: assistant.delta\ndata: {\"delta\":\"ok\"}\n\n",
            "event: run.completed\ndata: {\"usage\":{\"input_tokens\":17446,\"output_tokens\":3}}\n\n",
            "event: done\ndata: {}\n\n"
        );
        let base = serve(move |_| sse(body));
        let api = Api::new(base, "test").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);

        api.chat_stream("s1", "hi", &tx).await.expect("Strom");
        drop(tx);
        let mut seen = 0;
        while let Some(event) = rx.recv().await {
            if let UiEvent::Usage { input_tokens, .. } = event {
                assert_eq!(input_tokens, 17446);
                seen += 1;
            }
        }
        assert_eq!(seen, 1, "Prompt-Token wurden nicht gemeldet");
    }

    #[tokio::test]
    async fn teilt_die_prompt_summe_durch_die_anfragen() {
        // Ein Werkzeuglauf kostet zwei Anfragen; der Server meldet die Summe beider
        // Prompts. Der Kern muss daraus den Kontext einer Anfrage machen, sonst
        // zeigt die Fußzeile das Doppelte.
        let body = concat!(
            "event: run.started\ndata: {\"run_id\":\"r5\"}\n\n",
            "event: tool.started\ndata: {\"tool_name\":\"terminal\"}\n\n",
            "event: tool.completed\ndata: {\"tool_name\":\"terminal\"}\n\n",
            "event: assistant.delta\ndata: {\"delta\":\"ok\"}\n\n",
            "event: run.completed\ndata: {\"usage\":{\"input_tokens\":36000,\"output_tokens\":4}}\n\n",
            "event: done\ndata: {}\n\n"
        );
        let base = serve(move |_| sse(body));
        let api = Api::new(base, "test").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);

        api.chat_stream("s1", "hi", &tx).await.expect("Strom");
        drop(tx);
        let mut gesehen: Option<(u64, u32)> = None;
        while let Some(event) = rx.recv().await {
            if let UiEvent::Usage { input_tokens, api_calls, .. } = event {
                gesehen = Some((input_tokens, api_calls));
            }
        }
        assert_eq!(gesehen, Some((18000, 2)), "Summe wurde nicht auf eine Anfrage umgelegt");
    }

    #[tokio::test]
    async fn statuscode_wird_gemeldet() {
        let base = serve(|_| http("401 Unauthorized", "application/json", "{\"error\":\"no\"}"));
        let api = Api::new(base, "test").unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let err = api.chat_stream("s1", "hi", &tx).await.expect_err("401 fehlt");
        assert!(matches!(err, ApiError::Status(401, _)), "war {err:?}");
    }

    #[tokio::test]
    async fn recover_meldet_fehlschlag_statt_bereit() {
        let base = serve(|path| {
            if path.starts_with("/v1/runs/") {
                json("{\"status\":\"failed\"}")
            } else {
                sse(concat!(
                    "event: run.started\ndata: {\"run_id\":\"r9\"}\n\n",
                    "event: assistant.delta\ndata: {\"delta\":\"x\"}\n\n"
                ))
            }
        });
        let api = Api::new(base, "test").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);

        let result = api.chat_stream("s1", "hi", &tx).await.expect("Strom");
        assert_eq!(result.outcome, Outcome::Failed, "gescheiterter Lauf galt als bereit");
        assert_eq!(result.run_id, "r9");

        drop(tx);
        let mut failed = false;
        while let Some(event) = rx.recv().await {
            if let UiEvent::Failed { .. } = event {
                failed = true;
            }
        }
        assert!(failed, "kein Failed-Ereignis");
    }
}
