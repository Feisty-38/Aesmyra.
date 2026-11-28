//! Zustand der Anwendung: Sitzungen, Verlauf, laufender Auftrag, Einstellungen.
//!
//! Alles Netzwerkliche läuft auf einer eigenen Tokio-Laufzeit; die Ereignisse
//! kommen über einen Kanal zurück in die Oberfläche. Damit bleibt GPUI frei
//! von Blockaden, und der Kern (`aesmyra-core`) kann unverändert bleiben, weil
//! er genau dieselbe Schnittstelle schon für die Tauri-Fassung bedient hat.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aesmyra_core::api::{Api, Outcome, UiEvent};
use aesmyra_core::config::Config;
use aesmyra_core::model::{Session, Tool, Turn};

use gpui::{App, Context, ScrollHandle, WeakEntity};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::field::{self, Command, Field};

/// Eine Tokio-Laufzeit für alle Netzarbeiten (reqwest + tokio::time im Kern).
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(3)
            .enable_all()
            .thread_name("aesmyra-net")
            .build()
            .expect("Netz-Laufzeit")
    })
}

pub enum Msg {
    Sessions(Vec<Session>),
    Turns(Vec<Turn>),
    Event(UiEvent),
    RunDone(Outcome),
    Notice(String, bool),
    Created(Box<Session>),
    Deleted(String),
    SetModel(String),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Index,
    Reading,
    Settings,
}

/// Der sichtbare, noch laufende Auftrag.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub run_id: String,
    pub text: String,
    pub reasoning: String,
    pub tools: Vec<Tool>,
    pub approvals: Vec<Approval>,
    pub started: Option<Instant>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u32,
    pub reconnecting: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Approval {
    pub request_id: String,
    pub command: String,
    pub choices: Vec<String>,
    pub execution_generation: u64,
    /// Wann die Anfrage ankam — Grundlage für den Hinweis, wie lange sie offen ist.
    pub since: Instant,
}

#[derive(Clone, Debug)]
pub struct Open {
    pub id: String,
    pub title: String,
    pub model: String,
    pub last_active: f64,
    pub turns: Vec<Turn>,
    /// Sitzungen aus Terminal, Desktop oder Mobilteil gehören anderen
    /// Oberflächen. Der Server lässt dort keine Läufe zu (permission_denied);
    /// lesen geht, schreiben nicht.
    pub read_only: bool,
}

pub struct SettingsForm {
    pub url: Field,
    pub token: Field,
    pub hint: String,
    pub has_token: bool,
    pub focus: usize, // 0 = Adresse, 1 = Token
}

pub struct AppState {
    pub tx: UnboundedSender<Msg>,
    rx: Option<UnboundedReceiver<Msg>>,
    pub api: Option<Arc<Api>>,
    pub cfg_ok: bool,
    pub mode: Mode,
    pub sessions: Vec<Session>,
    pub sel: usize,
    pub open: Option<Open>,
    pub live: Live,
    pub form: SettingsForm,
    pub notice: Option<(String, bool)>,
    pub picker: Vec<Command>,
    pub commands: Vec<Command>,
    pub demo: bool,
    pub scroll: ScrollHandle,
    pub index_scroll: ScrollHandle,
    pub composer: Field,
    pub show_reasoning: bool,
    pub part_budget: f32,
    pub confirm_delete: Option<String>,
    pub loading: bool,
    pub blink: bool,
    pub clock: Instant,
    pub busy_since: Option<Instant>,
    pub server: String,
}

impl AppState {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let (tx, rx) = unbounded_channel();
        let cfg = Config::load();
        let view = cfg.view();
        let demo = std::env::var("AESMYRA_DEMO").is_ok();
        let api = if demo {
            None
        } else {
            Api::new(cfg.server_url.clone(), cfg.token.clone())
                .ok()
                .map(Arc::new)
        };
        let mut form = SettingsForm {
            url: Field::new(),
            token: Field::new(),
            hint: view.token_hint.clone(),
            has_token: view.has_token,
            focus: 0,
        };
        form.url.set(&view.server_url);
        let mut state = Self {
            tx,
            rx: Some(rx),
            api,
            cfg_ok: view.has_token,
            mode: Mode::Index,
            sessions: Vec::new(),
            sel: 0,
            open: None,
            live: Live::default(),
            form,
            notice: None,
            picker: Vec::new(),
            commands: field::load_commands(),
            demo,
            scroll: ScrollHandle::new(),
            index_scroll: ScrollHandle::new(),
            composer: Field::new(),
            show_reasoning: true,
            part_budget: 5200.0,
            confirm_delete: None,
            loading: true,
            blink: true,
            clock: Instant::now(),
            busy_since: None,
            server: view.server_url,
        };
        if demo {
            state.seed_demo();
            state.loading = false;
        }
        state
    }

    pub fn take_rx(&mut self) -> Option<UnboundedReceiver<Msg>> {
        self.rx.take()
    }

    /// Ereignisse aus dem Netz in den Zustand übernehmen.
    pub fn drain(&mut self, msg: Msg, cx: &mut Context<Self>) {
        match msg {
            Msg::Sessions(v) => {
                self.loading = false;
                let keep = self.open.as_ref().map(|o| o.id.clone());
                self.sessions = v;
                if let Some(id) = keep {
                    if let Some(i) = self.sessions.iter().position(|s| s.id == id) {
                        self.sel = i;
                    }
                }
                if self.sel >= self.sessions.len() {
                    self.sel = self.sessions.len().saturating_sub(1);
                }
                if let Ok(v) = std::env::var("AESMYRA_OPEN") {
                    if self.open.is_none() && !self.sessions.is_empty() {
                        let i = v.parse::<usize>().unwrap_or(0);
                        self.open_session(i, cx);
                    }
                }
            }
            Msg::Turns(v) => {
                if let Some(open) = self.open.as_mut() {
                    open.turns = v;
                }
                self.loading = false;
                cx.notify();
            }
            Msg::Event(e) => self.on_event(e, cx),
            Msg::RunDone(outcome) => {
                self.live.started = None;
                self.busy_since = None;
                if outcome == Outcome::Failed {
                    self.notice = Some(("Lauf fehlgeschlagen".into(), false));
                }
                self.reload_turns(cx);
                cx.notify();
            }
            Msg::Notice(text, ok) => {
                self.notice = Some((text, ok));
                cx.notify();
            }
            Msg::Created(s) => {
                self.sessions.insert(0, *s);
                self.sel = 0;
                self.notice = Some(("Neue Sitzung angelegt".into(), true));
                cx.notify();
            }
            Msg::Deleted(id) => {
                self.sessions.retain(|s| s.id != id);
                if self.sel >= self.sessions.len() {
                    self.sel = self.sessions.len().saturating_sub(1);
                }
                self.confirm_delete = None;
                self.notice = Some(("Sitzung gelöscht".into(), true));
                cx.notify();
            }
            Msg::SetModel(m) => {
                if let Some(open) = self.open.as_mut() {
                    open.model = m.clone();
                }
                self.notice = Some((format!("Modell: {m}"), true));
                cx.notify();
            }
            Msg::Failed(text) => {
                self.live.error = Some(text.clone());
                self.live.started = None;
                self.busy_since = None;
                self.loading = false;
                self.notice = Some((text, false));
                cx.notify();
            }
        }
    }

    pub fn on_event(&mut self, e: UiEvent, cx: &mut Context<Self>) {
        match e {
            UiEvent::RunStarted { run_id } => {
                self.live.run_id = run_id;
                self.live.started = Some(Instant::now());
            }
            UiEvent::Delta { text, .. } => self.live.text.push_str(&text),
            UiEvent::Commentary { text, .. } => self.live.reasoning.push_str(&text),
            UiEvent::ToolStarted { name, args, .. } => self.live.tools.push(Tool {
                name,
                args,
                out: String::new(),
            }),
            UiEvent::ToolFinished { name, ok, preview, .. } => {
                if let Some(t) = self.live.tools.iter_mut().rev().find(|t| t.name == name) {
                    t.out = format!("{} {}", if ok { "✓" } else { "✗" }, preview);
                }
            }
            UiEvent::Approval {
                request_id,
                command,
                choices,
                execution_generation,
                ..
            } => {
                self.live.approvals.push(Approval {
                    request_id,
                    command,
                    choices,
                    execution_generation,
                    since: Instant::now(),
                });
            }
            UiEvent::ApprovalSettled { request_id, .. } => {
                // Steht die Anfrage noch offen, hat nicht der Nutzer entschieden,
                // sondern der Server: Zeitablauf, neuer Lauf oder Abschluss.
                // Ohne Hinweis bliebe die Karte einfach wortlos verschwunden.
                let offen = self.live.approvals.iter().any(|a| a.request_id == request_id);
                self.live.approvals.retain(|a| a.request_id != request_id);
                if offen {
                    self.notice = Some((
                        "Freigabe zurückgezogen — der Lauf endet ohne Antwort.".into(),
                        false,
                    ));
                }
            }
            UiEvent::Completed { content, .. } => {
                if !content.trim().is_empty() {
                    self.live.text = content;
                }
            }
            UiEvent::Usage {
                input_tokens,
                output_tokens,
                api_calls,
                ..
            } => {
                self.live.input_tokens = input_tokens;
                self.live.output_tokens = output_tokens;
                self.live.calls = api_calls;
            }
            UiEvent::Failed { message, .. } => self.live.error = Some(message),
            UiEvent::Cancelled { .. } => self.live.error = Some("Abgebrochen".into()),
            UiEvent::Error { message, .. } => self.live.error = Some(message),
            UiEvent::Reconnecting { .. } => self.live.reconnecting = true,
            UiEvent::Recovered { .. } => self.live.reconnecting = false,
            UiEvent::StreamEnded { .. } => {}
            UiEvent::Finished { .. } => self.live.started = None,
        }
        cx.notify();
    }

    pub fn busy(&self) -> bool {
        self.live.started.is_some()
    }

    // ── Netzarbeiten anstoßen ─────────────────────────────────────────────

    pub fn load_sessions(&mut self, cx: &mut Context<Self>) {
        if self.demo {
            self.loading = false;
            return;
        }
        let Some(api) = self.api.clone() else {
            self.loading = false;
            return;
        };
        let tx = self.tx.clone();
        self.loading = true;
        runtime().spawn(async move {
            match api.sessions().await {
                Ok(v) => {
                    let _ = tx.send(Msg::Sessions(v));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    pub fn reload_turns(&mut self, cx: &mut Context<Self>) {
        if self.demo {
            return;
        }
        let (Some(api), Some(open)) = (self.api.clone(), self.open.as_ref()) else {
            return;
        };
        let sid = open.id.clone();
        let tx = self.tx.clone();
        runtime().spawn(async move {
            if let Ok(v) = api.turns(&sid).await {
                let _ = tx.send(Msg::Turns(v));
            }
        });
        cx.notify();
    }

    pub fn open_session(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(s) = self.sessions.get(index).cloned() else {
            return;
        };
        self.sel = index;
        self.open = Some(Open {
            id: s.id.clone(),
            title: if s.title.trim().is_empty() {
                s.preview.clone()
            } else {
                s.title.clone()
            },
            model: s.model.clone(),
            last_active: s.last_active,
            turns: Vec::new(),
            read_only: !matches!(s.source.as_str(), "api_server" | "bot_room"),
        });
        self.mode = Mode::Reading;
        self.live = Live::default();
        self.notice = None;
        self.composer.clear();
        self.picker.clear();
        self.scroll = ScrollHandle::new();
        if !self.demo {
            self.loading = true;
            let (Some(api), Some(open)) = (self.api.clone(), self.open.as_ref()) else {
                return;
            };
            let sid = open.id.clone();
            let tx = self.tx.clone();
            runtime().spawn(async move {
                match api.turns(&sid).await {
                    Ok(v) => {
                        let _ = tx.send(Msg::Turns(v));
                    }
                    Err(e) => {
                        let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                    }
                }
            });
        }
        cx.notify();
    }

    /// Enter in der Eingabezeile: senden, Befehl ausführen oder Vorschlag wählen.
    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.text();
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        // Fremde Sitzung: der Server lässt hier keinen Lauf zu. Text bleibt
        // stehen, damit nichts verloren geht; Befehle wirken weiter.
        if !trimmed.starts_with('/') && self.open.as_ref().is_some_and(|o| o.read_only) {
            self.notice = Some((
                "Diese Sitzung gehört einem anderen Programm — hier nur lesbar. Links eine neue Sitzung anlegen."
                    .into(),
                false,
            ));
            cx.notify();
            return;
        }

        // Ein hervorgehobener Vorschlag hat Vorrang.
        if let Some(c) = self.picker.first().cloned() {
            let prefix = format!("/{}", c.n);
            if trimmed != prefix {
                self.composer.set(&format!("{prefix} "));
                self.picker.clear();
                cx.notify();
                return;
            }
        }
        self.composer.take();
        self.picker.clear();

        if let Some(cmd) = trimmed.strip_prefix('/') {
            self.run_command(cmd, cx);
            return;
        }

        let Some(open) = self.open.as_mut() else { return };
        let sid = open.id.clone();
        // Sofort sichtbar: die eigene Zeile im Verlauf.
        open.turns.push(Turn {
            user: Some(trimmed.clone()),
            steered: false,
            steer: Vec::new(),
            parts: Vec::new(),
        });
        self.live = Live {
            started: Some(Instant::now()),
            ..Default::default()
        };
        self.busy_since = Some(Instant::now());
        self.notice = None;
        self.scroll_to_bottom();

        if self.demo {
            self.demo_answer(&trimmed, cx);
            return;
        }

        let Some(api) = self.api.clone() else {
            self.live.started = None;
            self.notice = Some(("Kein Zugang eingerichtet".into(), false));
            cx.notify();
            return;
        };
        let tx = self.tx.clone();
        runtime().spawn(async move {
            let (etx, mut erx) = tokio::sync::mpsc::channel::<UiEvent>(64);
            let pump_tx = tx.clone();
            let pump = tokio::spawn(async move {
                while let Some(e) = erx.recv().await {
                    if pump_tx.send(Msg::Event(e)).is_err() {
                        break;
                    }
                }
            });
            let result = api.chat_stream(&sid, &trimmed, &etx).await;
            drop(etx);
            let _ = pump.await;
            match result {
                Ok(r) => {
                    let _ = tx.send(Msg::RunDone(r.outcome));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    pub fn choose_approval(
        &mut self,
        request: String,
        run: String,
        generation: u64,
        choice: String,
        cx: &mut Context<Self>,
    ) {
        self.live.approvals.retain(|a| a.request_id != request);
        let Some(api) = self.api.clone() else {
            self.notice = Some(("Kein Zugang eingerichtet".into(), false));
            cx.notify();
            return;
        };
        let tx = self.tx.clone();
        runtime().spawn(async move {
            match api.approve(&run, &choice, &request, generation).await {
                Ok(()) => {
                    let _ = tx.send(Msg::Notice(format!("Freigabe: {choice}"), true));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    fn run_command(&mut self, cmd: &str, cx: &mut Context<Self>) {
        let (name, rest) = match cmd.split_once(char::is_whitespace) {
            Some((n, r)) => (n.trim(), r.trim()),
            None => (cmd.trim(), ""),
        };
        match name {
            "help" | "commands" => {
                let list = self
                    .commands
                    .iter()
                    .take(24)
                    .map(|c| format!("/{} — {}", c.n, c.d))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.live.reasoning.push_str(&format!("\n\nBefehle\n\n{list}"));
            }
            "new" => self.create_session(rest, cx),
            "model" if !rest.is_empty() => self.apply_model(rest, cx),
            "model" => {
                let m = self.open.as_ref().map(|o| o.model.clone()).unwrap_or_default();
                self.live.reasoning.push_str(&format!("\n\nModell: {m}"));
            }
            "status" => {
                let (title, n) = self
                    .open
                    .as_ref()
                    .map(|o| (o.title.clone(), o.turns.len()))
                    .unwrap_or_default();
                let m = self.open.as_ref().map(|o| o.model.clone()).unwrap_or_default();
                self.live
                    .reasoning
                    .push_str(&format!("\n\nSitzung: {title}\nModell: {m}\nAbschnitte: {n}"));
            }
            "title" if !rest.is_empty() => self.set_title(rest, cx),
            "steer" if !rest.is_empty() => {
                let Some(api) = self.api.clone() else {
                    self.notice = Some(("Kein Zugang eingerichtet".into(), false));
                    return;
                };
                let run = self.live.run_id.clone();
                let msg = rest.to_string();
                let tx = self.tx.clone();
                runtime().spawn(async move {
                    match api.steer(&run, &msg).await {
                        Ok(()) => {
                            let _ = tx.send(Msg::Notice("Nachricht eingeschoben".into(), true));
                        }
                        Err(e) => {
                            let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                        }
                    }
                });
            }
            "retry" => {
                let last = self
                    .open
                    .as_ref()
                    .and_then(|o| o.turns.iter().rev().find_map(|t| t.user.clone()));
                match last {
                    Some(q) if !self.busy() => {
                        self.composer.set(&q);
                        self.submit(cx);
                    }
                    Some(_) => {
                        self.notice = Some(("Es läuft bereits ein Auftrag".into(), false));
                    }
                    None => {
                        self.notice = Some(("Keine frühere Frage".into(), false));
                    }
                }
            }
            "sessions" | "resume" => {
                self.mode = Mode::Index;
            }
            _ => {
                self.notice = Some((format!("/{name} gehört nicht zu dieser Fassung"), false));
            }
        }
        cx.notify();
    }

    pub fn create_session(&mut self, title: &str, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            self.notice = Some(("Kein Zugang eingerichtet".into(), false));
            return;
        };
        let tx = self.tx.clone();
        let title = title.to_string();
        runtime().spawn(async move {
            match api.create_session().await {
                Ok(s) => {
                    if !title.trim().is_empty() {
                        let _ = api.patch_session(&s.id, serde_json::json!({"title": title})).await;
                    }
                    let _ = tx.send(Msg::Created(Box::new(s)));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    pub fn apply_model(&mut self, model: &str, cx: &mut Context<Self>) {
        let (Some(api), Some(open)) = (self.api.clone(), self.open.as_ref()) else {
            return;
        };
        let sid = open.id.clone();
        let m = model.to_string();
        let tx = self.tx.clone();
        runtime().spawn(async move {
            match api.set_model(&sid, &m).await {
                Ok(_) => {
                    let _ = tx.send(Msg::SetModel(m));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    pub fn set_title(&mut self, title: &str, cx: &mut Context<Self>) {
        let (Some(api), Some(open)) = (self.api.clone(), self.open.as_mut()) else {
            return;
        };
        let sid = open.id.clone();
        open.title = title.to_string();
        let t = title.to_string();
        runtime().spawn(async move {
            let _ = api.patch_session(&sid, serde_json::json!({"title": t})).await;
        });
        cx.notify();
    }

    pub fn delete_session(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else { return };
        let tx = self.tx.clone();
        runtime().spawn(async move {
            match api.delete_session(&id).await {
                Ok(()) => {
                    let _ = tx.send(Msg::Deleted(id));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Failed(aesmyra_core::api::error_text(&e)));
                }
            }
        });
        cx.notify();
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let url = self.form.url.text();
        let token = self.form.token.text();
        let token_opt = if token.trim().is_empty() {
            None
        } else {
            Some(token.as_str())
        };
        match Config::save(&url, token_opt) {
            Ok(cfg) => {
                self.server = cfg.server_url.clone();
                self.api = Api::new(cfg.server_url.clone(), cfg.token.clone())
                    .ok()
                    .map(Arc::new);
                self.cfg_ok = true;
                let v = cfg.view();
                self.form.hint = v.token_hint;
                self.form.has_token = v.has_token;
                self.form.token.clear();
                self.notice = Some(("Gespeichert".into(), true));
                self.mode = Mode::Index;
                self.loading = true;
                self.load_sessions(cx);
            }
            Err(e) => {
                self.notice = Some((e, false));
            }
        }
        cx.notify();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll.scroll_to_bottom();
    }

    pub fn update_picker(&mut self) {
        self.picker = field::suggest(&self.commands, &self.composer.text())
            .into_iter()
            .cloned()
            .collect();
    }

    // ── Beispielinhalt, damit die Oberfläche ohne Server prüfbar ist ──────

    pub fn seed_demo(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        self.sessions = vec![
            Session {
                id: "demo-1".into(),
                title: "KWin-Skript für Fensterraster".into(),
                preview: "Das Skript rastet Fenster jetzt am Raster ein.".into(),
                started_at: now - 5400.0,
                last_active: now - 420.0,
                message_count: 12,
                estimated_cost_usd: 0.42,
                pinned: false,
                source: "api".into(),
                model: "deepseek/deepseek-v4.1-flash-1017".into(),
            },
            Session {
                id: "demo-2".into(),
                title: "Bodyslide-Ausgabe prüfen".into(),
                preview: "Die NIFs liegen im Data-Ordner, Vortex sieht sie.".into(),
                started_at: now - 86400.0,
                last_active: now - 7200.0,
                message_count: 6,
                estimated_cost_usd: 0.11,
                pinned: false,
                source: "api".into(),
                model: "claude-sonnet-4.5".into(),
            },
            Session {
                id: "demo-3".into(),
                title: "Headset-Abstimmung".into(),
                preview: "Tiefbass angehoben, Höhen um 2 dB gesenkt.".into(),
                started_at: now - 172800.0,
                last_active: now - 90000.0,
                message_count: 22,
                estimated_cost_usd: 0.77,
                pinned: true,
                source: "api".into(),
                model: "gpt-5.1".into(),
            },
        ];
        self.open = Some(Open {
            id: "demo-1".into(),
            title: "KWin-Skript für Fensterraster".into(),
            model: "deepseek/deepseek-v4.1-flash-1017".into(),
            last_active: now - 420.0,
            read_only: false,
            turns: vec![
                Turn {
                    user: Some("Schreib mir ein KWin-Skript, das das aktive Fenster in die linke Hälfte rastet.".into()),
                    steered: false,
                    steer: Vec::new(),
                    parts: vec![aesmyra_core::model::Part {
                        reasoning: "Ich prüfe zuerst, welche Skriptschnittstelle KWin 6 anbietet. Für das Raster genügt `workspace.activeWindow.frameGeometry`; die Hälfte ergibt sich aus `workspace.clientArea`."
                            .into(),
                        text: "KWin 6 bringt die Skriptschnittstelle weiterhin mit. Das Skript rastet das aktive Fenster auf die linke Bildschirmhälfte — inklusive Rand, damit nichts unter der Panelleiste verschwindet.\n\n```javascript\nconst a = workspace.clientArea(KWin.MaximizeArea, workspace.activeScreen, workspace.currentDesktop);\nconst w = workspace.activeWindow;\nif (w) {\n  w.frameGeometry = { x: a.x, y: a.y, width: Math.round(a.width / 2), height: a.height };\n}\n```\n\nWichtig: `frameGeometry` erst nach `w.setMaximize(false, false)` setzen, sonst überschreibt der Maximierer die Werte.".into(),
                        tools: vec![
                            Tool { name: "read_file".into(), args: "~/.local/share/kwin/scripts/raster/metadata.json".into(), out: "✓ metadata.json gelesen (412 B)".into() },
                            Tool { name: "terminal".into(), args: "qdbus6 org.kde.KWin /Scripting".into(), out: "✓ Laden und Starten bestätigt".into() },
                        ],
                    }],
                },
                Turn {
                    user: Some("Gut. Jetzt noch rechts und oben.".into()),
                    steered: false,
                    steer: Vec::new(),
                    parts: vec![aesmyra_core::model::Part {
                        reasoning: String::new(),
                        text: "Hier die drei restlichen Lagen in einer Tabelle:\n\n| Taste | Lage | Breite | Höhe |\n|---|---|---|---|\n| Meta+Left | links | 50 % | 100 % |\n| Meta+Right | rechts | 50 % | 100 % |\n| Meta+Up | oben | 100 % | 50 % |\n\nDie Aktionen hängen an einem Kurzbefehl; das Skript selbst rechnet nur.".into(),
                        tools: vec![],
                    }],
                },
            ],
        });
        self.loading = false;

        // Der Wert von AESMYRA_DEMO wählt die Ansicht für Bildschirmprüfungen.
        let flavour = std::env::var("AESMYRA_DEMO").unwrap_or_default();
        match flavour.as_str() {
            "index" => {
                self.open = None;
                self.mode = Mode::Index;
                self.sel = 1;
            }
            "settings" => {
                self.mode = Mode::Settings;
                self.form.url.set(&self.server);
                self.form.focus = 0;
            }
            "live" => {
                self.mode = Mode::Reading;
                if let Some(open) = self.open.as_mut() {
                    open.turns.clear();
                }
                self.live.run_id = "demo-lauf".into();
                self.live.reasoning =
                    "Ich lese zuerst den Rasterzustand und prüfe, ob die Fensterkanten am Raster liegen. Dann setze ich die Geometrie.".into();
                self.live.text = "Der Rasterzustand ist geprüft. Für die letzte Lage fehlt noch ein Befehl mit erhöhten Rechten.\n\n- **Links:** 50 % Breite, volle Höhe\n- **Rechts:** 50 % Breite, volle Höhe\n- **Oben:** volle Breite, 50 % Höhe".into();
                self.live.tools.push(Tool {
                    name: "read_file".into(),
                    args: "~/.local/share/kwin/scripts/raster/state.json".into(),
                    out: "✓ gelesen (1,2 kB)".into(),
                });
                self.live.tools.push(Tool {
                    name: "terminal".into(),
                    args: "qdbus6 org.kde.KWin /Scripting loadScript /home/kev/raster.js".into(),
                    out: String::new(),
                });
                self.live.approvals.push(Approval {
                    request_id: "demo-freigabe".into(),
                    command: "qdbus6 org.kde.KWin /Scripting loadScript /home/kev/raster.js".into(),
                    choices: vec!["Ausführen".into(), "Einmalig".into(), "Ablehnen".into()],
                    execution_generation: 1,
                    since: Instant::now(),
                });
                self.live.started = Some(Instant::now());
                self.live.input_tokens = 12_840;
                self.live.output_tokens = 412;
                self.live.calls = 3;
                self.busy_since = Some(Instant::now());
            }
            "commands" => {
                self.mode = Mode::Reading;
                self.composer.set("/");
                self.update_picker();
            }
            _ => {}
        }
    }

    /// Antwort ohne Server, damit sich der Lesefluss prüfen lässt.
    fn demo_answer(&mut self, question: &str, cx: &mut Context<Self>) {
        let text = format!(
            "Beispielantwort auf „{question}“.\n\nDer Lauf ist nicht mit einem Server verbunden; diese Zeilen entstehen lokal, damit sich Satz, Zeilenlänge und Werkzeugfußnoten prüfen lassen.\n\n- **Erstens:** Absätze brechen bei 68 Zeichen um.\n- **Zweitens:** Werkzeuge stehen als Fußnote unter dem Abschnitt.\n\n```text\n$ ein Beispielbefehl\nAusgabe in Monospace\n```"
        );
        self.live.text = text;
        self.live.tools.push(Tool {
            name: "terminal".into(),
            args: "beispiel --lauf".into(),
            out: "✓ 0,3 s".into(),
        });
        self.live.started = Some(Instant::now());
        self.live.output_tokens = 128;
        self.live.input_tokens = 4096;
        self.live.calls = 2;
        self.busy_since = Some(Instant::now());
        cx.notify();
    }

    pub fn finish_demo(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_mut() {
            if !self.live.text.is_empty() {
                open.turns.push(Turn {
                    user: None,
                    steered: false,
                    steer: Vec::new(),
                    parts: vec![aesmyra_core::model::Part {
                        text: std::mem::take(&mut self.live.text),
                        reasoning: std::mem::take(&mut self.live.reasoning),
                        tools: std::mem::take(&mut self.live.tools),
                    }],
                });
            }
        }
        self.live.started = None;
        self.busy_since = None;
        cx.notify();
    }
}

/// Spanne des laufenden Auftrags als „0:12“.
pub fn elapsed(start: Option<Instant>) -> String {
    let Some(s) = start else { return String::new() };
    let d = s.elapsed();
    if d.as_secs() < 3600 {
        format!("{}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
    } else {
        format!("{}:{:02}:{:02}", d.as_secs() / 3600, (d.as_secs() % 3600) / 60, d.as_secs() % 60)
    }
}

/// „vor 4 Min.“ aus einem Unix-Zeitstempel.
pub fn ago(ts: f64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let d = (now - ts).max(0.0);
    if d < 60.0 {
        "gerade".into()
    } else if d < 3600.0 {
        format!("vor {} Min.", (d / 60.0) as u64)
    } else if d < 86400.0 {
        format!("vor {} Std.", (d / 3600.0) as u64)
    } else {
        format!("vor {} Tagen", (d / 86400.0) as u64)
    }
}

/// Hält den Takt für Uhrzeiten und Laufdauer.
pub fn tick_loop(state: WeakEntity<AppState>, cx: &mut App) {
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        let alive = state.update(cx, |s, cx| {
            s.clock = Instant::now();
            s.blink = !s.blink;
            cx.notify();
        });
        if alive.is_err() {
            break;
        }
    })
    .detach();
}
