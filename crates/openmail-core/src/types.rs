//! Domänentypen. Alles, was von außen kommt, wird in diese validierten Typen geparst.
//! Die JSON-Form entspricht dem Vertrag der Oberfläche (camelCase, Zeit als i64 in ms).

use crate::error::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_ID_LEN: usize = 64;
pub const MAX_NAME_LEN: usize = 200;
pub const MAX_TEXT_LEN: usize = 20_000;
pub const MAX_RULE_VALUE_LEN: usize = 500;
pub const MAX_RULES: usize = 20;
pub const MAX_ACTIONS: usize = 10;
/// Kleinster und größter gültiger Zeitpunkt (Grenzen von JavaScript-`Date`).
pub const MIN_TS: i64 = -8_640_000_000_000_000;
pub const MAX_TS: i64 = 8_640_000_000_000_000;
pub const MAX_REMIND_MINUTES: i32 = 525_600;

fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// Kennung: 1 bis 64 Zeichen aus `A-Z a-z 0-9 - _`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

impl Id {
    /// Neue, zufällige Kennung (UUID v4 aus dem Betriebssystem-Zufall).
    #[must_use]
    pub fn random() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Id {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let ok = (1..=MAX_ID_LEN).contains(&value.len())
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if ok {
            Ok(Self(value))
        } else {
            Err(AppError::InvalidInput("id"))
        }
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}

/// Anzeigename oder Titel: getrimmt, nicht leer, höchstens 200 Zeichen, keine Steuerzeichen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Name(String);

impl Name {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Name {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        let ok =
            !trimmed.is_empty() && trimmed.chars().count() <= MAX_NAME_LEN && !has_control(trimmed);
        if ok {
            Ok(Self(trimmed.to_owned()))
        } else {
            Err(AppError::InvalidInput("name"))
        }
    }
}

impl From<Name> for String {
    fn from(value: Name) -> Self {
        value.0
    }
}

/// Freitext: höchstens 20 000 Byte; Zeilenumbrüche und Tabs erlaubt, andere Steuerzeichen nicht.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Text(String);

impl Text {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Text {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let ok = value.len() <= MAX_TEXT_LEN
            && value
                .chars()
                .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'));
        if ok {
            Ok(Self(value))
        } else {
            Err(AppError::InvalidInput("text"))
        }
    }
}

impl From<Text> for String {
    fn from(value: Text) -> Self {
        value.0
    }
}

/// Domain-Label einer Adresse: 1 bis 63 Byte, Buchstaben (auch internationale), Ziffern und `-`,
/// nicht am Anfang oder Ende.
fn valid_domain_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|c| c.is_alphanumeric() || c == '-')
}

/// E-Mail-Adresse (pragmatische Prüfung, keine vollständige RFC-5322-Grammatik).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Email(String);

impl Email {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Email {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        const ERR: AppError = AppError::InvalidInput("email");
        let v = value.trim();
        if v.len() > 254 || has_control(v) || v.chars().any(char::is_whitespace) {
            return Err(ERR);
        }
        let (local, domain) = v.split_once('@').ok_or(ERR)?;
        let ok = !local.is_empty()
            && local.len() <= 64
            && domain.contains('.')
            && domain.split('.').all(valid_domain_label)
            // Endung braucht einen Buchstaben: schließt IP-Literale wie `a@1.2.3.4` aus.
            && domain
                .rsplit('.')
                .next()
                .is_some_and(|tld| tld.chars().any(char::is_alphabetic));
        if ok {
            Ok(Self(v.to_owned()))
        } else {
            Err(ERR)
        }
    }
}

impl From<Email> for String {
    fn from(value: Email) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Prio {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    Any,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleField {
    From,
    To,
    Subject,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleOp {
    Contains,
    Not,
    Is,
    Starts,
}

/// Liest eine optionale E-Mail-Adresse: `null`, leer oder nur Whitespace (so sendet das
/// Kontaktformular ein leeres Feld) ergibt `None`; alles andere muss eine gültige Adresse sein.
fn optionale_email<'de, D>(deserializer: D) -> Result<Option<Email>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<String>::deserialize(deserializer)? {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => Email::try_from(s)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Contact {
    pub id: Id,
    pub name: Name,
    #[serde(default, deserialize_with = "optionale_email")]
    pub email: Option<Email>,
    #[serde(default)]
    pub phone: Text,
    #[serde(default)]
    pub org: Text,
    #[serde(default)]
    pub title: Text,
    #[serde(default)]
    pub note: Text,
    #[serde(default)]
    pub fav: bool,
    pub book: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CalEvent {
    pub id: Id,
    pub cal: Id,
    pub title: Name,
    #[serde(default)]
    pub place: Text,
    #[serde(default)]
    pub all_day: bool,
    pub start: i64,
    pub end: i64,
    pub repeat: Repeat,
    /// Minuten vor Beginn; `-1` bedeutet keine Erinnerung.
    pub remind: i32,
    #[serde(default)]
    pub attendees: Text,
    #[serde(default)]
    pub desc: Text,
}

impl CalEvent {
    /// Prüft Feldkombinationen, die Serde nicht abdecken kann.
    pub fn validate(&self) -> Result<(), AppError> {
        let range = MIN_TS..=MAX_TS;
        if !range.contains(&self.start) || !range.contains(&self.end) {
            return Err(AppError::InvalidInput("zeitpunkt"));
        }
        if self.end < self.start {
            return Err(AppError::InvalidInput("ende liegt vor beginn"));
        }
        if !(-1..=MAX_REMIND_MINUTES).contains(&self.remind) {
            return Err(AppError::InvalidInput("erinnerung"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Task {
    pub id: Id,
    pub cal: Id,
    pub title: Name,
    #[serde(default)]
    pub due: Option<i64>,
    pub prio: Prio,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub note: Text,
}

impl Task {
    pub fn validate(&self) -> Result<(), AppError> {
        match self.due {
            Some(due) if !(MIN_TS..=MAX_TS).contains(&due) => {
                Err(AppError::InvalidInput("faelligkeit"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub field: RuleField,
    pub op: RuleOp,
    pub value: Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Action {
    Move { value: Id },
    Label { value: Id },
    // Leere Struct-Varianten statt Unit-Varianten: Nur so lehnt Serde bei intern getaggten
    // Enums zusätzliche Felder ab (`{"type":"star","value":…}`).
    Star {},
    Read {},
    Delete {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MailFilter {
    pub id: Id,
    pub name: Name,
    pub on: bool,
    #[serde(rename = "match")]
    pub match_mode: MatchMode,
    pub rules: Vec<Rule>,
    pub actions: Vec<Action>,
}

impl MailFilter {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.rules.is_empty() || self.rules.len() > MAX_RULES {
            return Err(AppError::InvalidInput("bedingungen"));
        }
        if self.actions.is_empty() || self.actions.len() > MAX_ACTIONS {
            return Err(AppError::InvalidInput("aktionen"));
        }
        let bad_rule = self.rules.iter().any(|r| {
            r.value.as_str().trim().is_empty() || r.value.as_str().len() > MAX_RULE_VALUE_LEN
        });
        if bad_rule {
            Err(AppError::InvalidInput("bedingungswert"))
        } else {
            Ok(())
        }
    }
}
