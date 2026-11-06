//! Befehlsschicht. Jede Methode entspricht einem Befehl des Oberflächen-Vertrags
//! (`save_contact`, `delete_event`, …) und wird später 1:1 von `#[tauri::command]`-Funktionen aufgerufen.

use crate::error::AppError;
use crate::store::Collection;
use crate::types::{CalEvent, Contact, Id, MailFilter, MatchMode, Rule, RuleField, RuleOp, Task};

const LIMIT_CONTACTS: usize = 50_000;
const LIMIT_EVENTS: usize = 100_000;
const LIMIT_TASKS: usize = 50_000;
const LIMIT_FILTERS: usize = 500;

/// Die für Filter relevanten Felder einer Nachricht.
pub struct MailView<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
}

fn rule_matches(mail: &MailView<'_>, rule: &Rule) -> bool {
    let needle = rule.value.as_str().trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    let hay = match rule.field {
        RuleField::From => mail.from,
        RuleField::To => mail.to,
        RuleField::Subject => mail.subject,
        RuleField::Body => mail.body,
    }
    .to_lowercase();
    match rule.op {
        RuleOp::Contains => hay.contains(&needle),
        RuleOp::Not => !hay.contains(&needle),
        RuleOp::Is => hay.trim() == needle || hay.split_whitespace().any(|word| word == needle),
        RuleOp::Starts => hay.starts_with(&needle),
    }
}

/// Trifft der Filter auf die Nachricht zu? Deaktivierte Filter und Filter ohne Bedingung treffen nie.
#[must_use]
pub fn filter_matches(filter: &MailFilter, mail: &MailView<'_>) -> bool {
    if !filter.on || filter.rules.is_empty() {
        return false;
    }
    match filter.match_mode {
        MatchMode::All => filter.rules.iter().all(|r| rule_matches(mail, r)),
        MatchMode::Any => filter.rules.iter().any(|r| rule_matches(mail, r)),
    }
}

pub struct Api {
    contacts: Collection<Contact>,
    events: Collection<CalEvent>,
    tasks: Collection<Task>,
    filters: Collection<MailFilter>,
}

impl Default for Api {
    fn default() -> Self {
        Self::new()
    }
}

impl Api {
    #[must_use]
    pub fn new() -> Self {
        Self {
            contacts: Collection::new(LIMIT_CONTACTS),
            events: Collection::new(LIMIT_EVENTS),
            tasks: Collection::new(LIMIT_TASKS),
            filters: Collection::new(LIMIT_FILTERS),
        }
    }

    pub fn list_contacts(&self) -> Result<Vec<Contact>, AppError> {
        self.contacts.list()
    }

    pub fn save_contact(&self, contact: Contact) -> Result<Contact, AppError> {
        self.contacts.upsert(contact)
    }

    pub fn delete_contact(&self, id: &Id) -> Result<(), AppError> {
        self.contacts.remove(id)
    }

    pub fn list_events(&self) -> Result<Vec<CalEvent>, AppError> {
        self.events.list()
    }

    pub fn save_event(&self, event: CalEvent) -> Result<CalEvent, AppError> {
        event.validate()?;
        self.events.upsert(event)
    }

    pub fn delete_event(&self, id: &Id) -> Result<(), AppError> {
        self.events.remove(id)
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, AppError> {
        self.tasks.list()
    }

    pub fn save_task(&self, task: Task) -> Result<Task, AppError> {
        task.validate()?;
        self.tasks.upsert(task)
    }

    pub fn delete_task(&self, id: &Id) -> Result<(), AppError> {
        self.tasks.remove(id)
    }

    pub fn list_filters(&self) -> Result<Vec<MailFilter>, AppError> {
        self.filters.list()
    }

    pub fn save_filter(&self, filter: MailFilter) -> Result<MailFilter, AppError> {
        filter.validate()?;
        self.filters.upsert(filter)
    }

    pub fn delete_filter(&self, id: &Id) -> Result<(), AppError> {
        self.filters.remove(id)
    }

    /// Alle aktiven Filter, die auf die Nachricht zutreffen (in gespeicherter Reihenfolge der Kennungen).
    pub fn matching_filters(&self, mail: &MailView<'_>) -> Result<Vec<MailFilter>, AppError> {
        Ok(self
            .filters
            .list()?
            .into_iter()
            .filter(|f| filter_matches(f, mail))
            .collect())
    }
}
