//! Speicherschicht. Heute im Arbeitsspeicher; später dahinter SQLite (rusqlite/SQLx mit gebundenen Parametern).

use crate::error::AppError;
use crate::types::{CalEvent, Contact, Id, MailFilter, Task};
use std::collections::BTreeMap;
use std::sync::RwLock;

/// Alles, was in einer [`Collection`] liegen kann.
pub trait Entity: Clone {
    fn id(&self) -> &Id;
}

macro_rules! entity {
    ($($ty:ty),+ $(,)?) => {
        $(impl Entity for $ty {
            fn id(&self) -> &Id {
                &self.id
            }
        })+
    };
}
entity!(Contact, CalEvent, Task, MailFilter);

/// Thread-sichere, in der Größe begrenzte Sammlung. Ein vergiftetes Lock wird als interner Fehler gemeldet.
pub struct Collection<T> {
    items: RwLock<BTreeMap<Id, T>>,
    limit: usize,
}

impl<T: Entity> Collection<T> {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            items: RwLock::new(BTreeMap::new()),
            limit,
        }
    }

    pub fn list(&self) -> Result<Vec<T>, AppError> {
        let guard = self.items.read().map_err(|_| AppError::Internal("lock"))?;
        Ok(guard.values().cloned().collect())
    }

    pub fn get(&self, id: &Id) -> Result<T, AppError> {
        let guard = self.items.read().map_err(|_| AppError::Internal("lock"))?;
        guard.get(id).cloned().ok_or(AppError::NotFound)
    }

    pub fn upsert(&self, item: T) -> Result<T, AppError> {
        let mut guard = self.items.write().map_err(|_| AppError::Internal("lock"))?;
        if !guard.contains_key(item.id()) && guard.len() >= self.limit {
            return Err(AppError::LimitReached("sammlung voll"));
        }
        guard.insert(item.id().clone(), item.clone());
        Ok(item)
    }

    pub fn remove(&self, id: &Id) -> Result<(), AppError> {
        let mut guard = self.items.write().map_err(|_| AppError::Internal("lock"))?;
        guard.remove(id).map(|_| ()).ok_or(AppError::NotFound)
    }
}
