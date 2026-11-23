//! Zugangsdaten und Serveradresse.
//!
//! Der Schlüssel liegt ausschließlich hier im Rust-Prozess. Er wird nie an die
//! Oberfläche ausgeliefert — die Oberfläche erfährt nur, *ob* einer hinterlegt
//! ist. Gespeichert wird mit Rechten 0600 im Konfigurationsverzeichnis des
//! Benutzers; auf Wunsch wird zusätzlich `~/.hermes/.env` gelesen.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server_url: String,
    #[serde(default)]
    pub token: String,
}

fn default_server() -> String {
    "http://127.0.0.1:8642".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self { server_url: default_server(), token: String::new() }
    }
}

/// Was die Oberfläche sehen darf.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub server_url: String,
    pub has_token: bool,
    pub token_hint: String,
}

/// Auf Android gibt es kein Benutzer-Konfigurationsverzeichnis: `dirs` liefert
/// dort nichts, und ein relativer Pfad landet im schreibgeschützten Wurzel-
/// verzeichnis. Die Anwendung setzt den Pfad deshalb selbst.
static BASE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Setzt das Verzeichnis und prüft sofort, dass sich darin schreiben lässt.
/// Ein stiller Fehlschlag beim ersten Speichern ist damit ausgeschlossen.
pub fn set_base_dir(dir: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&dir)?;
    let probe = dir.join(".write-probe");
    std::fs::write(&probe, b"")?;
    let _ = std::fs::remove_file(&probe);
    *BASE_DIR.write().unwrap() = Some(dir);
    Ok(())
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = BASE_DIR.read().unwrap().clone() {
        return dir;
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Aesmyra.")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// Liest `API_SERVER_KEY` aus der Hermes-Umgebungsdatei, falls vorhanden.
fn hermes_token() -> String {
    let Ok(home) = std::env::var("HOME") else { return String::new() };
    let path = PathBuf::from(home).join(".hermes").join(".env");
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "API_SERVER_KEY" {
                return v.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }
    String::new()
}

/// Andeutung für die Oberfläche: nur bei mindestens zwölf Zeichen die letzten
/// vier zeigen. Bei kürzeren Schlüsseln wäre das schon der halbe Schlüssel.
pub fn token_hint(token: &str) -> String {
    let n = token.chars().count();
    if n < 12 {
        return "…".to_string();
    }
    let tail: String = token.chars().skip(n - 4).collect();
    format!("…{tail}")
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = std::fs::read_to_string(config_path())
            .ok()
            .and_then(|t| serde_json::from_str::<Config>(&t).ok())
            .unwrap_or_default();

        if cfg.server_url.trim().is_empty() {
            cfg.server_url = default_server();
        }
        if cfg.token.is_empty() {
            cfg.token = hermes_token();
        }
        cfg
    }

    pub fn view(&self) -> SettingsView {
        SettingsView {
            server_url: self.server_url.clone(),
            has_token: !self.token.is_empty(),
            token_hint: if self.token.is_empty() { String::new() } else { token_hint(&self.token) },
        }
    }

    pub fn save(server_url: &str, token: Option<&str>) -> Result<Self, String> {
        let url = normalize_server(server_url)?;
        let mut cfg = Config::load();
        cfg.server_url = url;
        if let Some(t) = token {
            let t = t.trim();
            if !t.is_empty() {
                cfg.token = t.to_string();
            }
        }

        let dir = config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Verzeichnis: {e}"))?;
        let path = config_path();
        let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        write_private(&path, json.as_bytes()).map_err(|e| format!("Schreiben: {e}"))?;
        Ok(cfg)
    }
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Schreibt über eine Temp-Datei und benennt sie atomar um. So gibt es nie
/// einen Moment, in dem eine halb geschriebene Datei liegt.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Pfad ohne Verzeichnis")
    })?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("config.json");
    let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}.{seq}.tmp", std::process::id()));

    let written = (|| -> std::io::Result<()> {
        let mut file = create_private(&tmp)?;
        file.write_all(data)?;
        file.sync_all()
    })();

    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Legt die Datei gleich mit 0600 an — `fs::write` würde die umask gelten
/// lassen (typisch 0644) und der Schlüssel wäre für andere lesbar.
fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let file = options.open(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // `mode` gilt nur beim Anlegen; eine liegengebliebene Temp-Datei
        // könnte andere Rechte haben.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(file)
}

/// Nur http/https, keine Zugangsdaten in der Adresse, kein Pfad-Rest.
pub fn normalize_server(input: &str) -> Result<String, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("Serveradresse fehlt".into());
    }
    let with_scheme = if raw.contains("://") { raw.to_string() } else { format!("http://{raw}") };
    let mut url = Url::parse(&with_scheme).map_err(|_| "Adresse ist keine gültige URL".to_string())?;

    match url.scheme() {
        "http" | "https" => {}
        s => return Err(format!("Schema {s} ist nicht erlaubt")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Zugangsdaten gehören nicht in die Adresse".into());
    }
    if url.host_str().is_none() {
        return Err("Adresse ohne Rechnernamen".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisiert() {
        assert_eq!(normalize_server("127.0.0.1:8642").unwrap(), "http://127.0.0.1:8642");
        assert_eq!(normalize_server("http://host:8642/").unwrap(), "http://host:8642");
        assert!(normalize_server("ftp://host").is_err());
        assert!(normalize_server("http://u:p@host").is_err());
    }

    #[test]
    fn schluesselanzeige_bei_kurzen_schluesseln() {
        assert_eq!(token_hint(""), "…");
        assert_eq!(token_hint("abcd"), "…");
        assert_eq!(token_hint("0123456789a"), "…", "elf Zeichen dürfen nichts zeigen");
        assert_eq!(token_hint("0123456789ab"), "…89ab");
        assert_eq!(token_hint("sk-abcdefghijklmnop"), "…mnop");
    }

    #[test]
    fn speichert_mit_rechten_0600() {
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aesmyra-test-{}-{seq}", std::process::id()));
        set_base_dir(dir.clone()).expect("Testverzeichnis");

        Config::save("127.0.0.1:8642", Some("sk-test-0123456789")).expect("speichern");
        let path = config_path();
        let text = std::fs::read_to_string(&path).expect("gelesen");
        assert!(text.contains("sk-test-0123456789"));
        assert!(
            text.contains("\"server_url\": \"http://127.0.0.1:8642\""),
            "Adresse nicht gespeichert: {text}"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Rechte sind {mode:o}");
        }

        // Keine Temp-Reste.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "Temp-Dateien übrig: {leftovers:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
