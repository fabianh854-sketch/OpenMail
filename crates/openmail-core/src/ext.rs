//! Erweiterungspunkte. Neue Protokolle und Dienste kommen als Implementierungen dieser Traits
//! (IMAP, POP3, JMAP, CalDAV, CardDAV, Matrix, IRC, XMPP). Spätere Plugins (z. B. WASM) melden
//! ihre Rechte über ein [`ExtensionManifest`]; ohne ausdrücklich erteilte Rechte ist alles verboten.

use crate::error::AppError;
use crate::types::{Id, Name};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Mailprotokoll eines Kontos (IMAP, POP3, JMAP …).
pub trait MailProvider: Send + Sync {
    /// Eindeutiger Schlüssel, zum Beispiel `imap`.
    fn protocol(&self) -> &'static str;
}

/// Kalenderquelle (lokal, CalDAV …).
pub trait CalendarProvider: Send + Sync {
    fn kind(&self) -> &'static str;
}

/// Adressbuchquelle (lokal, CardDAV …).
pub trait ContactProvider: Send + Sync {
    fn kind(&self) -> &'static str;
}

/// Chat-Protokoll (Matrix, IRC, XMPP …).
pub trait ChatProtocol: Send + Sync {
    fn protocol(&self) -> &'static str;
}

/// Register für Implementierungen eines Erweiterungspunkts. Doppelte Schlüssel werden abgelehnt.
pub struct Registry<T: ?Sized> {
    entries: RwLock<BTreeMap<String, Arc<T>>>,
}

impl<T: ?Sized + Send + Sync> Registry<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn register(&self, key: &str, provider: Arc<T>) -> Result<(), AppError> {
        let key = Id::try_from(key.to_owned())?;
        let mut guard = self
            .entries
            .write()
            .map_err(|_| AppError::Internal("lock"))?;
        if guard.contains_key(key.as_str()) {
            return Err(AppError::InvalidInput("schluessel bereits vergeben"));
        }
        guard.insert(key.as_str().to_owned(), provider);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Arc<T>, AppError> {
        let guard = self
            .entries
            .read()
            .map_err(|_| AppError::Internal("lock"))?;
        guard.get(key).cloned().ok_or(AppError::NotFound)
    }

    pub fn keys(&self) -> Result<Vec<String>, AppError> {
        let guard = self
            .entries
            .read()
            .map_err(|_| AppError::Internal("lock"))?;
        Ok(guard.keys().cloned().collect())
    }
}

impl<T: ?Sized + Send + Sync> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Alle Register der Anwendung.
#[derive(Default)]
pub struct Providers {
    pub mail: Registry<dyn MailProvider>,
    pub calendar: Registry<dyn CalendarProvider>,
    pub contacts: Registry<dyn ContactProvider>,
    pub chat: Registry<dyn ChatProtocol>,
}

const MAX_CAPABILITIES: usize = 16;
const MAX_HOSTS: usize = 16;

/// Recht, das eine Erweiterung anfordern kann. Alles, was nicht aufgeführt ist, ist verboten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capability {
    ReadMail,
    SendMail,
    ReadContacts,
    ReadCalendar,
    /// Netzwerkzugriff nur auf die genannten Hosts (keine Platzhalter).
    Network {
        hosts: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: Id,
    pub name: Name,
    pub version: Name,
    pub capabilities: Vec<Capability>,
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

impl ExtensionManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.capabilities.len() > MAX_CAPABILITIES {
            return Err(AppError::InvalidInput("zu viele rechte"));
        }
        for cap in &self.capabilities {
            if let Capability::Network { hosts } = cap {
                if hosts.is_empty()
                    || hosts.len() > MAX_HOSTS
                    || !hosts.iter().all(|h| valid_host(h))
                {
                    return Err(AppError::InvalidInput("netzwerk-hosts"));
                }
            }
        }
        Ok(())
    }

    /// Ist das Recht im Manifest ausdrücklich enthalten?
    #[must_use]
    pub fn allows(&self, wanted: &Capability) -> bool {
        self.capabilities
            .iter()
            .any(|granted| match (granted, wanted) {
                (Capability::Network { hosts: have }, Capability::Network { hosts: want }) => {
                    want.iter().all(|h| have.contains(h))
                }
                (a, b) => a == b,
            })
    }
}
