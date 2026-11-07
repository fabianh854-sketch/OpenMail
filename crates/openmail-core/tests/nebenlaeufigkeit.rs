//! Nebenläufigkeit: Viele Threads schreiben und lesen gleichzeitig. Der Endzustand muss
//! konsistent sein und Grenzen dürfen auch unter Konkurrenz nicht überschritten werden.

use openmail_core::ext::{MailProvider, Registry};
use openmail_core::store::Collection;
use openmail_core::types::{Contact, Id};
use openmail_core::{Api, AppError};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const THREADS: usize = 16;
const PER_THREAD: usize = 100;

fn contact(id: &str, name: &str) -> Result<Contact, serde_json::Error> {
    serde_json::from_value(json!({ "id": id, "name": name, "book": "personal" }))
}

fn id(v: &str) -> Result<Id, AppError> {
    Id::try_from(v.to_owned())
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn api_und_speicher_sind_thread_sicher() {
    assert_send_sync::<Api>();
    assert_send_sync::<Collection<Contact>>();
    assert_send_sync::<Registry<dyn MailProvider>>();
}

#[test]
fn paralleles_schreiben_und_lesen_ergibt_konsistenten_bestand() -> TestResult {
    let api = Api::new();
    let results: Vec<Result<(), String>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let api = &api;
                s.spawn(move || -> Result<(), String> {
                    for i in 0..PER_THREAD {
                        let c = contact(&format!("t{t}-{i}"), &format!("Kontakt {t}/{i}"))
                            .map_err(|e| e.to_string())?;
                        api.save_contact(c).map_err(|e| e.to_string())?;
                        // Lesen während andere schreiben: nie mehr als möglich.
                        let n = api.list_contacts().map_err(|e| e.to_string())?.len();
                        if n > THREADS.saturating_mul(PER_THREAD) {
                            return Err(format!("zu viele Einträge: {n}"));
                        }
                    }
                    // Jeder Thread löscht seine geraden Einträge wieder.
                    for i in (0..PER_THREAD).step_by(2) {
                        let key = id(&format!("t{t}-{i}")).map_err(|e| e.to_string())?;
                        api.delete_contact(&key).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("Thread abgestürzt".to_owned()))
            })
            .collect()
    });
    for r in results {
        r?;
    }
    let list = api.list_contacts()?;
    assert_eq!(
        list.len(),
        THREADS
            .saturating_mul(PER_THREAD)
            .checked_div(2)
            .unwrap_or_default()
    );
    // Übrig sind genau die ungeraden Einträge jedes Threads.
    for t in 0..THREADS {
        for i in 0..PER_THREAD {
            let key = id(&format!("t{t}-{i}"))?;
            let present = list.iter().any(|c| c.id == key);
            assert_eq!(present, i % 2 == 1, "t{t}-{i}");
        }
    }
    Ok(())
}

#[test]
fn konkurrierende_updates_derselben_kennung() -> TestResult {
    let api = Api::new();
    let written: Vec<String> = (0..THREADS).map(|t| format!("Version {t}")).collect();
    std::thread::scope(|s| {
        for name in &written {
            let api = &api;
            s.spawn(move || {
                for _ in 0..PER_THREAD {
                    if let Ok(c) = contact("p1", name) {
                        let _ = api.save_contact(c);
                    }
                }
            });
        }
    });
    let list = api.list_contacts()?;
    assert_eq!(list.len(), 1);
    let final_name = list
        .first()
        .map(|c| c.name.as_str().to_owned())
        .unwrap_or_default();
    assert!(written.contains(&final_name), "{final_name}");
    Ok(())
}

#[test]
fn limit_wird_unter_konkurrenz_nicht_ueberschritten() -> TestResult {
    const LIMIT: usize = 10;
    let store: Collection<Contact> = Collection::new(LIMIT);
    let ok = AtomicUsize::new(0);
    let full = AtomicUsize::new(0);
    let other = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (store, ok, full, other) = (&store, &ok, &full, &other);
            s.spawn(move || {
                for i in 0..5 {
                    let Ok(c) = contact(&format!("t{t}-{i}"), "X") else {
                        other.fetch_add(1, Ordering::SeqCst);
                        continue;
                    };
                    match store.upsert(c) {
                        Ok(_) => ok.fetch_add(1, Ordering::SeqCst),
                        Err(AppError::LimitReached(_)) => full.fetch_add(1, Ordering::SeqCst),
                        Err(_) => other.fetch_add(1, Ordering::SeqCst),
                    };
                }
            });
        }
    });
    assert_eq!(ok.load(Ordering::SeqCst), LIMIT);
    assert_eq!(
        full.load(Ordering::SeqCst),
        THREADS.saturating_mul(5).saturating_sub(LIMIT)
    );
    assert_eq!(other.load(Ordering::SeqCst), 0);
    assert_eq!(store.list()?.len(), LIMIT);
    Ok(())
}

#[test]
fn limit_mit_gleichzeitigem_loeschen() -> TestResult {
    const LIMIT: usize = 8;
    let store: Collection<Contact> = Collection::new(LIMIT);
    let max_seen = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (store, max_seen) = (&store, &max_seen);
            s.spawn(move || {
                for i in 0..50 {
                    let key = format!("t{t}-{i}");
                    if let Ok(c) = contact(&key, "X") {
                        let _ = store.upsert(c);
                    }
                    if let Ok(list) = store.list() {
                        max_seen.fetch_max(list.len(), Ordering::SeqCst);
                    }
                    if let Ok(k) = id(&key) {
                        let _ = store.remove(&k);
                    }
                }
            });
        }
    });
    assert!(max_seen.load(Ordering::SeqCst) <= LIMIT);
    assert!(store.list()?.is_empty());
    Ok(())
}

struct Imap;
impl MailProvider for Imap {
    fn protocol(&self) -> &'static str {
        "imap"
    }
}

#[test]
fn registry_vergibt_einen_schluessel_nur_einmal() -> TestResult {
    let reg: Registry<dyn MailProvider> = Registry::new();
    let ok = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..THREADS {
            let (reg, ok) = (&reg, &ok);
            s.spawn(move || {
                if reg.register("imap", Arc::new(Imap)).is_ok() {
                    ok.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(ok.load(Ordering::SeqCst), 1);
    assert_eq!(reg.keys()?, vec!["imap".to_owned()]);
    Ok(())
}

#[test]
fn api_ueber_arc_zwischen_threads_teilen() -> TestResult {
    let api = Arc::new(Api::new());
    let mut handles = Vec::new();
    for t in 0..4 {
        let api = Arc::clone(&api);
        handles.push(std::thread::spawn(move || -> Result<(), String> {
            let c = contact(&format!("p{t}"), "X").map_err(|e| e.to_string())?;
            api.save_contact(c)
                .map(|_| ())
                .map_err(|e| e.public_message())
        }));
    }
    for h in handles {
        h.join().map_err(|_| "Thread abgestürzt")??;
    }
    assert_eq!(api.list_contacts()?.len(), 4);
    Ok(())
}
