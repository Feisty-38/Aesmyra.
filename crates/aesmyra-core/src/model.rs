//! Datenmodell.
//!
//! Der Server liefert reichlich Felder, von denen Aesmyra. nur wenige braucht.
//! Hier wird einmal ausgewertet und auf das Nötige eingedampft: Sitzungen als
//! Gespräche, Nachrichten als Abschnitte mit Text, Denknotiz und Werkzeugen.
//! Die Oberfläche bekommt damit wenige Kilobyte statt eines halben Megabytes.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub started_at: f64,
    pub last_active: f64,
    pub message_count: u64,
    pub estimated_cost_usd: f64,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Tool {
    pub name: String,
    pub args: String,
    pub out: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Part {
    pub text: String,
    pub reasoning: String,
    pub tools: Vec<Tool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Turn {
    pub user: Option<String>,
    pub parts: Vec<Part>,
}

/* ── Hilfen für unbeständige Felder ─────────────────────────────────── */

fn text_of(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            // Inhalte können als Liste von Teilen kommen: [{type:"text",text:"…"}]
            let mut out = String::new();
            for item in items {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                } else if let Some(t) = item.get("content").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
            out
        }
        Value::Object(map) => map
            .get("text")
            .or_else(|| map.get("content"))
            .map(text_of)
            .unwrap_or_default(),
        other => other.to_string(),
    }
}

fn num_of(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn flag_of(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => s == "true" || s == "1",
        _ => false,
    }
}

fn str_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn epoch(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::String(s)) => s.parse().unwrap_or(0.0),
        other => num_of(other),
    }
}

/* ── Sitzungen ──────────────────────────────────────────────────────── */

impl Session {
    pub fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let started = epoch(v.get("started_at"));
        let last = epoch(v.get("last_active"));
        Some(Session {
            id,
            title: str_of(v.get("title")),
            preview: str_of(v.get("preview")),
            started_at: started,
            last_active: if last > 0.0 { last } else { started },
            message_count: num_of(v.get("message_count")) as u64,
            estimated_cost_usd: num_of(v.get("estimated_cost_usd")),
            pinned: flag_of(v.get("pinned")),
        })
    }

    /// Sitzungen, die im Verzeichnis erscheinen sollen, in Aesmyra.-Reihenfolge.
    pub fn visible(v: &Value) -> Vec<Session> {
        let list = v
            .get("data")
            .or_else(|| v.get("sessions"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut out: Vec<Session> = list
            .iter()
            .filter(|s| !flag_of(s.get("hidden")))
            .filter(|s| !flag_of(s.get("is_internal_child")))
            .filter(|s| !flag_of(s.get("archived")))
            .filter(|s| s.get("source").and_then(Value::as_str) != Some("cron"))
            .filter_map(Session::from_value)
            .collect();

        out.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.last_active.partial_cmp(&a.last_active).unwrap_or(std::cmp::Ordering::Equal))
                .then(b.started_at.partial_cmp(&a.started_at).unwrap_or(std::cmp::Ordering::Equal))
        });
        out
    }
}

/* ── Verlauf zu Abschnitten ─────────────────────────────────────────── */

fn tool_calls_of(m: &Value) -> Vec<Tool> {
    let Some(list) = m.get("tool_calls").and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .map(|tc| {
            let f = tc.get("function");
            let name = f
                .and_then(|f| f.get("name"))
                .and_then(Value::as_str)
                .or_else(|| tc.get("name").and_then(Value::as_str))
                .unwrap_or("werkzeug")
                .to_string();
            let args = f
                .and_then(|f| f.get("arguments"))
                .map(text_of)
                .or_else(|| tc.get("arguments").map(text_of))
                .unwrap_or_default();
            Tool { name, args, out: String::new() }
        })
        .collect()
}

/// Werkzeugausgaben den Aufrufen zuordnen, wie es der Verlauf vorsieht.
fn outputs_by_call(msgs: &[Value]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for m in msgs {
        if m.get("role").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        if let Some(id) = m.get("tool_call_id").and_then(Value::as_str) {
            map.insert(id.to_string(), text_of(m.get("content").unwrap_or(&Value::Null)));
        }
    }
    map
}

fn call_ids(m: &Value) -> Vec<String> {
    let Some(list) = m.get("tool_calls").and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|tc| {
            tc.get("call_id")
                .and_then(Value::as_str)
                .or_else(|| tc.get("id").and_then(Value::as_str))
                .map(str::to_string)
        })
        .collect()
}

fn clean_text(raw: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for line in raw.lines() {
        if line.trim_start().to_lowercase().starts_with("operation interrupted:") {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n").trim().to_string()
}

/// Wandelt den rohen Verlauf in Abschnitte: je Nutzernachricht ein Abschnitt,
/// darin die Teile des Assistenten mit Text, Denknotiz und Werkzeugen.
pub fn turns_from(v: &Value) -> Vec<Turn> {
    let msgs: Vec<Value> = v
        .get("data")
        .or_else(|| v.get("messages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let outs = outputs_by_call(&msgs);
    let mut turns: Vec<Turn> = Vec::new();
    let mut cur: Option<usize> = None;

    for m in &msgs {
        match m.get("role").and_then(Value::as_str) {
            Some("user") => {
                turns.push(Turn { user: Some(text_of(m.get("content").unwrap_or(&Value::Null))), parts: Vec::new() });
                cur = Some(turns.len() - 1);
            }
            Some("assistant") => {
                let ids = call_ids(m);
                let mut tools = tool_calls_of(m);
                for (i, t) in tools.iter_mut().enumerate() {
                    if let Some(id) = ids.get(i) {
                        if let Some(out) = outs.get(id) {
                            t.out = out.clone();
                        }
                    }
                }

                let text = clean_text(&text_of(m.get("content").unwrap_or(&Value::Null)));
                let reasoning = {
                    let a = text_of(m.get("reasoning_content").unwrap_or(&Value::Null));
                    if a.trim().is_empty() {
                        text_of(m.get("reasoning").unwrap_or(&Value::Null)).trim().to_string()
                    } else {
                        a.trim().to_string()
                    }
                };
                if text.is_empty() && tools.is_empty() && reasoning.is_empty() {
                    continue;
                }

                let idx = match cur {
                    Some(i) => i,
                    None => {
                        turns.push(Turn::default());
                        let i = turns.len() - 1;
                        cur = Some(i);
                        i
                    }
                };
                turns[idx].parts.push(Part { text, reasoning, tools });
            }
            _ => {}
        }
    }
    turns
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn baut_abschnitte() {
        let v = json!({"data": [
            {"role":"user","content":"hallo"},
            {"role":"assistant","content":"","tool_calls":[{"call_id":"c1","function":{"name":"shell","arguments":"{\"command\":\"ls\"}"}}]},
            {"role":"tool","tool_call_id":"c1","content":"datei.txt"},
            {"role":"assistant","content":"Fertig."}
        ]});
        let turns = turns_from(&v);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].user.as_deref(), Some("hallo"));
        assert_eq!(turns[0].parts.len(), 2);
        assert_eq!(turns[0].parts[0].tools[0].out, "datei.txt");
        assert_eq!(turns[0].parts[1].text, "Fertig.");
    }

    #[test]
    fn filtert_sitzungen() {
        let v = json!({"data": [
            {"id":"a","title":"A","last_active":10,"pinned":true},
            {"id":"b","title":"B","last_active":20},
            {"id":"c","hidden":true,"last_active":30},
            {"id":"d","source":"cron","last_active":40}
        ]});
        let list = Session::visible(&v);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a");
        assert_eq!(list[1].id, "b");
    }
}
