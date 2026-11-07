//! Befehlsschicht `Api` und Speicher `Collection` über die öffentliche Schnittstelle:
//! CRUD aller Sammlungen, Überschreiben, Löschen, Grenzen, Fehlermeldungen.

use openmail_core::store::Collection;
use openmail_core::types::{CalEvent, Contact, Id, MailFilter, Task};
use openmail_core::{Api, AppError, MailView};
use serde_json::{json, Value};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn id(v: &str) -> Result<Id, AppError> {
    Id::try_from(v.to_owned())
}

fn contact(id: &str, name: &str) -> Result<Contact, serde_json::Error> {
    serde_json::from_value(json!({ "id": id, "name": name, "book": "personal" }))
}

fn event(id: &str, title: &str, start: i64, end: i64) -> Result<CalEvent, serde_json::Error> {
    serde_json::from_value(json!({
        "id": id, "cal": "c1", "title": title, "start": start, "end": end,
        "repeat": "none", "remind": 15
    }))
}

fn task(id: &str, title: &str, due: Value) -> Result<Task, serde_json::Error> {
    serde_json::from_value(json!({
        "id": id, "cal": "c1", "title": title, "due": due, "prio": "normal"
    }))
}

fn filter(id: &str, on: bool, field: &str, value: &str) -> Result<MailFilter, serde_json::Error> {
    serde_json::from_value(json!({
        "id": id, "name": format!("Filter {id}"), "on": on, "match": "any",
        "rules": [{ "field": field, "op": "contains", "value": value }],
        "actions": [{ "type": "star" }]
    }))
}

fn ids<T>(items: &[T], get: impl Fn(&T) -> &Id) -> Vec<String> {
    items.iter().map(|i| get(i).as_str().to_owned()).collect()
}

#[test]
fn neue_api_ist_leer() -> TestResult {
    for api in [Api::new(), Api::default()] {
        assert!(api.list_contacts()?.is_empty());
        assert!(api.list_events()?.is_empty());
        assert!(api.list_tasks()?.is_empty());
        assert!(api.list_filters()?.is_empty());
    }
    Ok(())
}

#[test]
fn kontakte_crud() -> TestResult {
    let api = Api::new();
    let lena = contact("p1", "Lena")?;
    assert_eq!(api.save_contact(lena.clone())?, lena);
    assert_eq!(api.list_contacts()?, vec![lena]);

    let renamed = contact("p1", "Lena Hoffmann")?;
    api.save_contact(renamed.clone())?;
    assert_eq!(api.list_contacts()?, vec![renamed]);

    api.delete_contact(&id("p1")?)?;
    assert!(api.list_contacts()?.is_empty());
    assert_eq!(api.delete_contact(&id("p1")?), Err(AppError::NotFound));
    Ok(())
}

#[test]
fn termine_crud() -> TestResult {
    let api = Api::new();
    let e = event("e1", "Zahnarzt", 0, 3_600_000)?;
    assert_eq!(api.save_event(e.clone())?, e);
    let moved = event("e1", "Zahnarzt", 7_200_000, 10_800_000)?;
    api.save_event(moved.clone())?;
    assert_eq!(api.list_events()?, vec![moved]);
    api.delete_event(&id("e1")?)?;
    assert!(api.list_events()?.is_empty());
    assert_eq!(api.delete_event(&id("e1")?), Err(AppError::NotFound));
    Ok(())
}

#[test]
fn aufgaben_crud() -> TestResult {
    let api = Api::new();
    let t = task("t1", "Kuchen backen", json!(1_000))?;
    assert_eq!(api.save_task(t.clone())?, t);
    let mut done = task("t1", "Kuchen backen", Value::Null)?;
    done.done = true;
    api.save_task(done.clone())?;
    assert_eq!(api.list_tasks()?, vec![done]);
    api.delete_task(&id("t1")?)?;
    assert!(api.list_tasks()?.is_empty());
    assert_eq!(api.delete_task(&id("t1")?), Err(AppError::NotFound));
    Ok(())
}

#[test]
fn filter_crud() -> TestResult {
    let api = Api::new();
    let f = filter("f1", true, "subject", "atlas")?;
    assert_eq!(api.save_filter(f.clone())?, f);
    let off = filter("f1", false, "subject", "atlas")?;
    api.save_filter(off.clone())?;
    assert_eq!(api.list_filters()?, vec![off]);
    api.delete_filter(&id("f1")?)?;
    assert!(api.list_filters()?.is_empty());
    assert_eq!(api.delete_filter(&id("f1")?), Err(AppError::NotFound));
    Ok(())
}

#[test]
fn loeschen_unbekannter_kennungen_meldet_not_found() -> TestResult {
    let api = Api::new();
    api.save_contact(contact("p1", "Lena")?)?;
    let unknown = id("gibt-es-nicht")?;
    assert_eq!(api.delete_contact(&unknown), Err(AppError::NotFound));
    assert_eq!(api.delete_event(&unknown), Err(AppError::NotFound));
    assert_eq!(api.delete_task(&unknown), Err(AppError::NotFound));
    assert_eq!(api.delete_filter(&unknown), Err(AppError::NotFound));
    // Ein fehlgeschlagenes Löschen lässt den Bestand unberührt.
    assert_eq!(api.list_contacts()?.len(), 1);
    // Kennungen unterscheiden Groß- und Kleinschreibung.
    assert_eq!(api.delete_contact(&id("P1")?), Err(AppError::NotFound));
    Ok(())
}

#[test]
fn sammlungen_sind_unabhaengig() -> TestResult {
    let api = Api::new();
    api.save_contact(contact("x1", "Kontakt")?)?;
    api.save_event(event("x1", "Termin", 0, 0)?)?;
    api.save_task(task("x1", "Aufgabe", Value::Null)?)?;
    api.save_filter(filter("x1", true, "from", "a")?)?;
    api.delete_event(&id("x1")?)?;
    assert_eq!(api.list_contacts()?.len(), 1);
    assert!(api.list_events()?.is_empty());
    assert_eq!(api.list_tasks()?.len(), 1);
    assert_eq!(api.list_filters()?.len(), 1);
    Ok(())
}

#[test]
fn listen_sind_nach_kennung_sortiert() -> TestResult {
    let api = Api::new();
    for key in ["p3", "p1", "p2", "A0", "_z"] {
        api.save_contact(contact(key, "N")?)?;
    }
    // Byte-Reihenfolge: Großbuchstaben vor `_` vor Kleinbuchstaben.
    assert_eq!(
        ids(&api.list_contacts()?, |c| &c.id),
        vec!["A0", "_z", "p1", "p2", "p3"]
    );
    Ok(())
}

#[test]
fn ungueltige_objekte_werden_nicht_gespeichert() -> TestResult {
    let api = Api::new();
    let good = event("e1", "Gut", 0, 10)?;
    api.save_event(good.clone())?;
    // Ungültiges Update derselben Kennung überschreibt den gültigen Stand nicht.
    let bad = event("e1", "Kaputt", 10, 0)?;
    assert_eq!(
        api.save_event(bad),
        Err(AppError::InvalidInput("ende liegt vor beginn"))
    );
    assert_eq!(api.list_events()?, vec![good]);

    let mut bad_event = event("e2", "Erinnerung", 0, 0)?;
    bad_event.remind = -5;
    assert!(api.save_event(bad_event).is_err());

    let good_task = task("t1", "Gut", json!(0))?;
    api.save_task(good_task.clone())?;
    assert!(api
        .save_task(task("t1", "Kaputt", json!(i64::MAX))?)
        .is_err());
    assert_eq!(api.list_tasks()?, vec![good_task]);

    let good_filter = filter("f1", true, "from", "a")?;
    api.save_filter(good_filter.clone())?;
    let mut empty_rules = filter("f1", true, "from", "a")?;
    empty_rules.rules.clear();
    assert!(api.save_filter(empty_rules).is_err());
    let mut no_actions = filter("f2", true, "from", "a")?;
    no_actions.actions.clear();
    assert!(api.save_filter(no_actions).is_err());
    assert_eq!(api.list_filters()?, vec![good_filter]);
    assert_eq!(api.list_events()?.len(), 1);
    Ok(())
}

#[test]
fn filter_limit_der_api() -> TestResult {
    let api = Api::new();
    for i in 0..500 {
        api.save_filter(filter(&format!("f{i}"), true, "from", "a")?)?;
    }
    assert_eq!(
        api.save_filter(filter("f500", true, "from", "a")?),
        Err(AppError::LimitReached("sammlung voll"))
    );
    // Bestehende Filter lassen sich auch bei voller Sammlung ändern.
    assert!(api.save_filter(filter("f0", false, "from", "b")?).is_ok());
    // Nach dem Löschen ist wieder Platz.
    api.delete_filter(&id("f1")?)?;
    assert!(api.save_filter(filter("f500", true, "from", "a")?).is_ok());
    assert_eq!(api.list_filters()?.len(), 500);
    Ok(())
}

#[test]
fn matching_filters_liefert_aktive_treffer_nach_kennung() -> TestResult {
    let api = Api::new();
    api.save_filter(filter("f3", true, "subject", "atlas")?)?;
    api.save_filter(filter("f1", true, "from", "contoso")?)?;
    api.save_filter(filter("f2", false, "subject", "atlas")?)?;
    api.save_filter(filter("f4", true, "body", "rechnung")?)?;
    let mail = MailView {
        from: "Lena lena@contoso.com",
        to: "fabian@example.org",
        subject: "AW: Projekt Atlas",
        body: "Lizenzfreigabe offen",
    };
    let hits = api.matching_filters(&mail)?;
    assert_eq!(ids(&hits, |f| &f.id), vec!["f1", "f3"]);

    let none = MailView {
        from: "",
        to: "",
        subject: "",
        body: "",
    };
    assert!(api.matching_filters(&none)?.is_empty());
    assert!(Api::new().matching_filters(&mail)?.is_empty());
    Ok(())
}

#[test]
fn collection_grenzen() -> TestResult {
    let zero: Collection<Contact> = Collection::new(0);
    assert_eq!(
        zero.upsert(contact("p1", "A")?),
        Err(AppError::LimitReached("sammlung voll"))
    );
    assert!(zero.list()?.is_empty());

    let two: Collection<Contact> = Collection::new(2);
    two.upsert(contact("p1", "A")?)?;
    two.upsert(contact("p2", "B")?)?;
    assert!(two.upsert(contact("p3", "C")?).is_err());
    // Überschreiben ist bei voller Sammlung erlaubt.
    two.upsert(contact("p2", "B2")?)?;
    assert_eq!(two.get(&id("p2")?)?.name.as_str(), "B2");
    assert_eq!(two.get(&id("p3")?), Err(AppError::NotFound));
    two.remove(&id("p1")?)?;
    assert_eq!(two.remove(&id("p1")?), Err(AppError::NotFound));
    two.upsert(contact("p3", "C")?)?;
    assert_eq!(two.list()?.len(), 2);
    Ok(())
}

#[test]
fn get_liefert_eine_kopie() -> TestResult {
    let store: Collection<Contact> = Collection::new(10);
    store.upsert(contact("p1", "Lena")?)?;
    let mut copy = store.get(&id("p1")?)?;
    copy.fav = true;
    assert!(!store.get(&id("p1")?)?.fav);
    Ok(())
}

#[test]
fn oeffentliche_meldungen_leaken_keine_internas() {
    let internal = AppError::Internal("lock vergiftet in store.rs:42, SELECT * FROM contacts");
    let msg = internal.public_message();
    assert_eq!(msg, "Es ist ein interner Fehler aufgetreten.");
    for secret in ["lock", "store.rs", "SELECT", "contacts", "42"] {
        assert!(!msg.contains(secret), "{secret}");
    }
    // Auch Display (z. B. in Logs, die versehentlich ausgegeben werden) enthält den Kontext nicht.
    assert!(!internal.to_string().contains("SELECT"));

    assert_eq!(AppError::NotFound.public_message(), "nicht gefunden");
    assert_eq!(AppError::Forbidden.public_message(), "keine Berechtigung");
    assert_eq!(
        AppError::InvalidInput("zeitpunkt").public_message(),
        "ungültige Eingabe: zeitpunkt"
    );
    assert_eq!(
        AppError::LimitReached("sammlung voll").public_message(),
        "Grenze erreicht: sammlung voll"
    );
}

#[test]
fn fehler_der_api_sind_oeffentlich_darstellbar() -> TestResult {
    let api = Api::new();
    let err = match api.save_event(event("e1", "X", 10, 0)?) {
        Err(e) => e,
        Ok(_) => return Err("ungültiger Termin wurde gespeichert".into()),
    };
    let msg = err.public_message();
    assert!(msg.starts_with("ungültige Eingabe"));
    assert!(!msg.contains("CalEvent"));
    assert!(!msg.contains("validate"));
    Ok(())
}
