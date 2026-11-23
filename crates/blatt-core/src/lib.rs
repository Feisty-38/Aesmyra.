//! Blatt-Kern.
//!
//! Enthält alles, was nicht Oberfläche ist: Zugangsdaten, HTTP-Zugriff auf den
//! Hermes-Server, Auswertung des Ereignisstroms und Aufbereitung des Verlaufs
//! zu Abschnitten. Die Oberfläche bekommt ausschließlich fertige, kleine
//! Strukturen — sie muss keine 500 Nachrichten mehr selbst durchlaufen.

pub mod api;
pub mod config;
pub mod model;
pub mod sse;

pub use api::{Api, ApiError, Outcome, RunResult};
pub use config::{Config, SettingsView};
pub use model::{Part, Session, Tool, Turn};
pub use sse::{Frame, SseError, SseParser};
