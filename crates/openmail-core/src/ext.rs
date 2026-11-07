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
const MAX_HOST_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;

/// Domain-Endungen, die nur lokal, intern oder gar nicht auflösen (u. a. RFC 6761, RFC 8375).
/// Erweiterungen dürfen sie nie anfordern, damit sie keine Dienste im Heim- oder Firmennetz erreichen.
///
/// Die Namensprüfung allein reicht nicht: Öffentliche Namen können auf private Adressen zeigen
/// (z. B. `127.0.0.1.nip.io`, DNS-Rebinding). Wer später Verbindungen für Erweiterungen aufbaut,
/// muss deshalb zusätzlich die **aufgelöste** IP gegen Loopback-, private, Link-Local- und
/// reservierte Bereiche prüfen.
const BLOCKED_SUFFIXES: [&str; 11] = [
    "localhost",
    "localdomain",
    "local",
    "internal",
    "intranet",
    "lan",
    "home",
    "corp",
    "test",
    "invalid",
    // Infrastruktur-Zone (Reverse-DNS, `home.arpa`, `ipv4only.arpa` …), für Erweiterungen nie nötig.
    "arpa",
];

/// Recht, das eine Erweiterung anfordern kann. Alles, was nicht aufgeführt ist, ist verboten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Capability {
    // Leere Struct-Varianten, damit `deny_unknown_fields` auch hier zusätzliche Felder ablehnt.
    ReadMail {},
    SendMail {},
    ReadContacts {},
    ReadCalendar {},
    /// Netzwerkzugriff nur auf die genannten Hosts: öffentliche DNS-Namen in Kleinbuchstaben,
    /// keine Platzhalter, keine IP-Adressen, keine lokalen Namen.
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

fn valid_label(label: &str) -> bool {
    (1..=MAX_LABEL_LEN).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Endung eines öffentlichen Namens: nur Buchstaben (mindestens zwei) oder Punycode (`xn--…`).
/// Schließt damit alle IP-Schreibweisen aus, die Resolver akzeptieren (`127.1`, `0x7f.0x1`, `017.1`).
fn valid_tld(tld: &str) -> bool {
    (tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_lowercase()))
        || (tld.starts_with("xn--") && tld.len() > 4)
}

/// Öffentlicher DNS-Name: mindestens zwei gültige Labels, gültige Endung (siehe [`valid_tld`]),
/// keine IP-Adresse und keine lokale oder interne Endung.
fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > MAX_HOST_LEN || host.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let Some(tld) = labels.last() else {
        return false;
    };
    let blocked = BLOCKED_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")));
    labels.len() >= 2 && labels.iter().all(|l| valid_label(l)) && valid_tld(tld) && !blocked
}

impl ExtensionManifest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.capabilities.len() > MAX_CAPABILITIES {
            return Err(AppError::InvalidInput("zu viele rechte"));
        }
        for (i, cap) in self.capabilities.iter().enumerate() {
            if self
                .capabilities
                .iter()
                .skip(i.saturating_add(1))
                .any(|c| c == cap)
            {
                return Err(AppError::InvalidInput("doppeltes recht"));
            }
            if let Capability::Network { hosts } = cap {
                // Erst die Länge prüfen, damit die quadratische Duplikatsuche begrenzt bleibt.
                if hosts.is_empty() || hosts.len() > MAX_HOSTS {
                    return Err(AppError::InvalidInput("netzwerk-hosts"));
                }
                let unique = hosts
                    .iter()
                    .enumerate()
                    .all(|(j, h)| !hosts.iter().skip(j.saturating_add(1)).any(|o| o == h));
                if !unique || !hosts.iter().all(|h| valid_host(h)) {
                    return Err(AppError::InvalidInput("netzwerk-hosts"));
                }
            }
        }
        Ok(())
    }
}

/// Ein geprüftes Manifest. Rechte lassen sich nur hier abfragen, damit nie ein ungeprüftes
/// Manifest über Zugriffe entscheidet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ExtensionManifest", into = "ExtensionManifest")]
pub struct ValidatedManifest(ExtensionManifest);

impl TryFrom<ExtensionManifest> for ValidatedManifest {
    type Error = AppError;

    fn try_from(manifest: ExtensionManifest) -> Result<Self, Self::Error> {
        manifest.validate()?;
        Ok(Self(manifest))
    }
}

impl From<ValidatedManifest> for ExtensionManifest {
    fn from(value: ValidatedManifest) -> Self {
        value.0
    }
}

impl ValidatedManifest {
    #[must_use]
    pub fn manifest(&self) -> &ExtensionManifest {
        &self.0
    }

    /// Ist das Recht im Manifest ausdrücklich enthalten? Eine Netzwerkanfrage ohne Host wird
    /// abgelehnt; Hosts werden ohne Beachtung der Groß-/Kleinschreibung verglichen.
    #[must_use]
    pub fn allows(&self, wanted: &Capability) -> bool {
        self.0
            .capabilities
            .iter()
            .any(|granted| match (granted, wanted) {
                (Capability::Network { hosts: have }, Capability::Network { hosts: want }) => {
                    !want.is_empty()
                        && want
                            .iter()
                            .all(|w| have.iter().any(|h| h.eq_ignore_ascii_case(w)))
                }
                (a, b) => a == b,
            })
    }
}
