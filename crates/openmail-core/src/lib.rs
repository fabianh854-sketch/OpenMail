//! OpenMail-Kern: reines Rust, ohne UI- und Plattformabhängigkeiten.
//!
//! - Modul `types`: validierte Domänentypen (Eingaben werden beim Parsen geprüft)
//! - Modul `store`: begrenzte, thread-sichere Speicherschicht
//! - Modul `ext`: Erweiterungspunkte (Provider-Traits, Register, Plugin-Rechte)
//! - Modul `api`: Befehlsschicht, die Tauri-Commands und eine spätere HTTP-API gemeinsam nutzen

pub mod api;
pub mod error;
pub mod ext;
pub mod store;
pub mod types;

pub use api::{filter_matches, Api, MailView};
pub use error::AppError;

#[cfg(test)]
mod tests;
