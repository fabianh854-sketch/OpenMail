//! Blackbox-Tests für den JSON-Vertrag zwischen Oberfläche und Kern:
//! camelCase-Felder, kleingeschriebene Enums, Default-Felder, `match`-Umbenennung,
//! Ablehnung unbekannter Felder, falscher Typen und fehlender Pflichtfelder.

use openmail_core::types::{
    Action, CalEvent, Contact, MailFilter, MatchMode, Prio, Repeat, Rule, RuleField, RuleOp, Task,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Deserialisiert, serialisiert erneut und prüft, dass die JSON-Form unverändert bleibt.
fn roundtrip<T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug>(
    input: &Value,
) -> TestResult {
    let parsed: T = serde_json::from_value(input.clone())?;
    let output = serde_json::to_value(&parsed)?;
    assert_eq!(&output, input, "Roundtrip verändert die JSON-Form");
    let again: T = serde_json::from_value(output)?;
    assert_eq!(again, parsed);
    Ok(())
}

/// Entfernt ein Feld aus einem JSON-Objekt (für „fehlendes Pflichtfeld“).
fn without(value: &Value, key: &str) -> Value {
    let mut copy = value.clone();
    if let Some(obj) = copy.as_object_mut() {
        obj.remove(key);
    }
    copy
}

/// Setzt ein Feld in einem JSON-Objekt.
fn with(value: &Value, key: &str, field: Value) -> Value {
    let mut copy = value.clone();
    if let Some(obj) = copy.as_object_mut() {
        obj.insert(key.to_owned(), field);
    }
    copy
}

fn full_contact() -> Value {
    json!({
        "id": "p1a2b3",
        "name": "Lena Hoffmann",
        "email": "lena.hoffmann@contoso.com",
        "phone": "+49 30 5550 1037",
        "org": "Contoso GmbH",
        "title": "Projektleitung",
        "note": "Ansprechpartnerin Projekt Atlas.\nBitte vormittags anrufen.",
        "fav": true,
        "book": "personal"
    })
}

fn full_event() -> Value {
    json!({
        "id": "e17x",
        "cal": "c2",
        "title": "Urlaub",
        "place": "Gardasee",
        "allDay": true,
        "start": 1_797_894_000_000_i64,
        "end": 1_798_671_600_000_i64,
        "repeat": "yearly",
        "remind": -1,
        "attendees": "markus.weber@fabrikam.com, sofia@contoso.com",
        "desc": "Abwesenheitsnotiz nicht vergessen."
    })
}

fn full_task() -> Value {
    json!({
        "id": "t9",
        "cal": "c1",
        "title": "Risikoliste von Lena bestätigen",
        "due": 1_762_516_800_000_i64,
        "prio": "high",
        "done": false,
        "note": "Eskalation an den Einkauf bis Freitag."
    })
}

fn full_filter() -> Value {
    json!({
        "id": "flmh2k3",
        "name": "Newsletter archivieren",
        "on": false,
        "match": "any",
        "rules": [
            { "field": "from", "op": "contains", "value": "newsletter@" },
            { "field": "subject", "op": "starts", "value": "[News]" }
        ],
        "actions": [
            { "type": "read" },
            { "type": "move", "value": "archive" },
            { "type": "label", "value": "l2" },
            { "type": "star" },
            { "type": "delete" }
        ]
    })
}

#[test]
fn contact_roundtrip_mit_realistischen_daten() -> TestResult {
    roundtrip::<Contact>(&full_contact())
}

#[test]
fn event_roundtrip_mit_all_day_in_camel_case() -> TestResult {
    roundtrip::<CalEvent>(&full_event())?;
    let event: CalEvent = serde_json::from_value(full_event())?;
    assert!(event.all_day);
    let out = serde_json::to_value(&event)?;
    assert!(out.get("allDay").is_some());
    assert!(out.get("all_day").is_none());
    Ok(())
}

#[test]
fn task_roundtrip_mit_und_ohne_faelligkeit() -> TestResult {
    roundtrip::<Task>(&full_task())?;
    roundtrip::<Task>(&with(&full_task(), "due", Value::Null))
}

#[test]
fn filter_roundtrip_mit_match_umbenennung_und_allen_aktionen() -> TestResult {
    roundtrip::<MailFilter>(&full_filter())?;
    let filter: MailFilter = serde_json::from_value(full_filter())?;
    assert_eq!(filter.match_mode, MatchMode::Any);
    assert_eq!(filter.actions.len(), 5);
    let out = serde_json::to_value(&filter)?;
    assert!(out.get("match").is_some());
    assert!(out.get("matchMode").is_none());
    assert!(out.get("match_mode").is_none());
    Ok(())
}

#[test]
fn contact_minimal_setzt_defaults() -> TestResult {
    let contact: Contact =
        serde_json::from_value(json!({ "id": "p1", "name": "Jonas", "book": "personal" }))?;
    assert!(contact.email.is_none());
    assert_eq!(contact.phone.as_str(), "");
    assert_eq!(contact.org.as_str(), "");
    assert_eq!(contact.title.as_str(), "");
    assert_eq!(contact.note.as_str(), "");
    assert!(!contact.fav);
    // Serialisiert werden alle Felder, die Oberfläche bekommt also immer die volle Form.
    let out = serde_json::to_value(&contact)?;
    assert_eq!(
        out,
        json!({
            "id": "p1", "name": "Jonas", "email": null, "phone": "", "org": "",
            "title": "", "note": "", "fav": false, "book": "personal"
        })
    );
    Ok(())
}

#[test]
fn contact_email_null_bedeutet_keine_adresse() -> TestResult {
    let contact: Contact = serde_json::from_value(with(&full_contact(), "email", Value::Null))?;
    assert!(contact.email.is_none());
    Ok(())
}

/// Das Kontaktformular des Prototyps (`fluent-mail.html`, `collect()` + `save_contact`) schickt
/// ein leeres Feld als `""`, nicht als `null`. Ein Kontakt ohne Adresse muss speicherbar sein.
#[test]
fn contact_leere_email_aus_dem_formular_ist_keine_adresse() -> TestResult {
    for leer in [json!(""), json!("   "), Value::Null] {
        let contact: Contact = serde_json::from_value(with(&full_contact(), "email", leer))?;
        assert!(contact.email.is_none());
    }
    let mut ohne = full_contact();
    if let Some(o) = ohne.as_object_mut() {
        o.remove("email");
    }
    let contact: Contact = serde_json::from_value(ohne)?;
    assert!(contact.email.is_none());
    // Umgebender Leerraum wird wie bei `Email::try_from` entfernt.
    let getrimmt: Contact =
        serde_json::from_value(with(&full_contact(), "email", json!("  a@b.de ")))?;
    assert_eq!(getrimmt.email.as_ref().map(|e| e.as_str()), Some("a@b.de"));
    for falsch in [
        json!("kein-at"),
        json!(42),
        json!("\u{1c}"),
        json!(["a@b.de"]),
    ] {
        assert!(
            serde_json::from_value::<Contact>(with(&full_contact(), "email", falsch.clone()))
                .is_err(),
            "{falsch}"
        );
    }
    Ok(())
}

#[test]
fn event_minimal_setzt_defaults() -> TestResult {
    let event: CalEvent = serde_json::from_value(json!({
        "id": "e1", "cal": "c1", "title": "Zahnarzt",
        "start": 0, "end": 3_600_000, "repeat": "none", "remind": 60
    }))?;
    assert!(!event.all_day);
    assert_eq!(event.place.as_str(), "");
    assert_eq!(event.attendees.as_str(), "");
    assert_eq!(event.desc.as_str(), "");
    Ok(())
}

#[test]
fn task_minimal_setzt_defaults() -> TestResult {
    let task: Task = serde_json::from_value(json!({
        "id": "t1", "cal": "c1", "title": "Kuchen backen", "prio": "low"
    }))?;
    assert_eq!(task.due, None);
    assert!(!task.done);
    assert_eq!(task.note.as_str(), "");
    Ok(())
}

#[test]
fn enums_sind_kleingeschrieben() -> TestResult {
    for (raw, want) in [
        ("none", Repeat::None),
        ("daily", Repeat::Daily),
        ("weekly", Repeat::Weekly),
        ("monthly", Repeat::Monthly),
        ("yearly", Repeat::Yearly),
    ] {
        assert_eq!(serde_json::from_value::<Repeat>(json!(raw))?, want);
        assert_eq!(serde_json::to_value(want)?, json!(raw));
    }
    for (raw, want) in [
        ("high", Prio::High),
        ("normal", Prio::Normal),
        ("low", Prio::Low),
    ] {
        assert_eq!(serde_json::from_value::<Prio>(json!(raw))?, want);
        assert_eq!(serde_json::to_value(want)?, json!(raw));
    }
    for (raw, want) in [("any", MatchMode::Any), ("all", MatchMode::All)] {
        assert_eq!(serde_json::from_value::<MatchMode>(json!(raw))?, want);
        assert_eq!(serde_json::to_value(want)?, json!(raw));
    }
    for (raw, want) in [
        ("from", RuleField::From),
        ("to", RuleField::To),
        ("subject", RuleField::Subject),
        ("body", RuleField::Body),
    ] {
        assert_eq!(serde_json::from_value::<RuleField>(json!(raw))?, want);
        assert_eq!(serde_json::to_value(want)?, json!(raw));
    }
    for (raw, want) in [
        ("contains", RuleOp::Contains),
        ("not", RuleOp::Not),
        ("is", RuleOp::Is),
        ("starts", RuleOp::Starts),
    ] {
        assert_eq!(serde_json::from_value::<RuleOp>(json!(raw))?, want);
        assert_eq!(serde_json::to_value(want)?, json!(raw));
    }
    Ok(())
}

#[test]
fn enums_lehnen_andere_schreibweisen_ab() {
    for bad in ["Daily", "DAILY", "hourly", "", " daily"] {
        assert!(
            serde_json::from_value::<Repeat>(json!(bad)).is_err(),
            "{bad}"
        );
    }
    for bad in ["High", "urgent", "medium"] {
        assert!(serde_json::from_value::<Prio>(json!(bad)).is_err(), "{bad}");
    }
    for bad in ["ANY", "some", "true"] {
        assert!(
            serde_json::from_value::<MatchMode>(json!(bad)).is_err(),
            "{bad}"
        );
    }
    for bad in ["cc", "From", "header"] {
        assert!(
            serde_json::from_value::<RuleField>(json!(bad)).is_err(),
            "{bad}"
        );
    }
    for bad in ["equals", "Is", "startsWith", "regex"] {
        assert!(
            serde_json::from_value::<RuleOp>(json!(bad)).is_err(),
            "{bad}"
        );
    }
    // Zahlen statt Strings (z. B. Enum-Index) sind kein gültiges Enum.
    assert!(serde_json::from_value::<Repeat>(json!(0)).is_err());
}

#[test]
fn unbekannte_felder_werden_abgelehnt() {
    assert!(
        serde_json::from_value::<Contact>(with(&full_contact(), "admin", json!(true))).is_err()
    );
    assert!(
        serde_json::from_value::<CalEvent>(with(&full_event(), "all_day", json!(true))).is_err()
    );
    assert!(
        serde_json::from_value::<CalEvent>(with(&full_event(), "color", json!("red"))).is_err()
    );
    assert!(serde_json::from_value::<Task>(with(&full_task(), "owner", json!("x"))).is_err());
    assert!(
        serde_json::from_value::<MailFilter>(with(&full_filter(), "matchAll", json!(true)))
            .is_err()
    );
    // Rust-Feldname statt Vertragsname.
    let snake = with(
        &without(&full_filter(), "match"),
        "match_mode",
        json!("any"),
    );
    assert!(serde_json::from_value::<MailFilter>(snake).is_err());
    let camel = with(&without(&full_filter(), "match"), "matchMode", json!("any"));
    assert!(serde_json::from_value::<MailFilter>(camel).is_err());
    // Unbekanntes Feld in einer verschachtelten Regel.
    let nested = with(
        &full_filter(),
        "rules",
        json!([{ "field": "from", "op": "is", "value": "a", "caseSensitive": true }]),
    );
    assert!(serde_json::from_value::<MailFilter>(nested).is_err());
    assert!(serde_json::from_value::<Rule>(
        json!({ "field": "from", "op": "is", "value": "a", "x": 1 })
    )
    .is_err());
}

#[test]
fn fehlende_pflichtfelder_werden_abgelehnt() {
    for key in ["id", "name", "book"] {
        assert!(
            serde_json::from_value::<Contact>(without(&full_contact(), key)).is_err(),
            "Contact ohne {key}"
        );
    }
    for key in ["id", "cal", "title", "start", "end", "repeat", "remind"] {
        assert!(
            serde_json::from_value::<CalEvent>(without(&full_event(), key)).is_err(),
            "CalEvent ohne {key}"
        );
    }
    for key in ["id", "cal", "title", "prio"] {
        assert!(
            serde_json::from_value::<Task>(without(&full_task(), key)).is_err(),
            "Task ohne {key}"
        );
    }
    for key in ["id", "name", "on", "match", "rules", "actions"] {
        assert!(
            serde_json::from_value::<MailFilter>(without(&full_filter(), key)).is_err(),
            "MailFilter ohne {key}"
        );
    }
    for key in ["field", "op", "value"] {
        let rule = json!({ "field": "from", "op": "is", "value": "a" });
        assert!(
            serde_json::from_value::<Rule>(without(&rule, key)).is_err(),
            "Rule ohne {key}"
        );
    }
}

#[test]
fn falsche_typen_werden_abgelehnt() {
    let event = full_event();
    for (key, bad) in [
        ("start", json!("1797894000000")),
        ("start", json!(1.5)),
        ("start", json!(null)),
        ("end", json!(18_446_744_073_709_551_615_u64)),
        ("remind", json!(2_147_483_648_i64)),
        ("remind", json!(-2_147_483_649_i64)),
        ("remind", json!("15")),
        ("allDay", json!("true")),
        ("allDay", json!(1)),
        ("title", json!(42)),
        ("title", json!(null)),
        ("id", json!(17)),
        ("place", json!(null)),
        ("repeat", json!(null)),
    ] {
        assert!(
            serde_json::from_value::<CalEvent>(with(&event, key, bad.clone())).is_err(),
            "CalEvent {key} = {bad}"
        );
    }
    let contact = full_contact();
    for (key, bad) in [
        ("fav", json!("ja")),
        ("fav", json!(null)),
        ("email", json!(["a@b.de"])),
        ("book", json!(null)),
    ] {
        assert!(
            serde_json::from_value::<Contact>(with(&contact, key, bad.clone())).is_err(),
            "Contact {key} = {bad}"
        );
    }
    let task = full_task();
    for (key, bad) in [
        ("due", json!("morgen")),
        ("done", json!(0)),
        ("due", json!(1.0e3)),
    ] {
        assert!(
            serde_json::from_value::<Task>(with(&task, key, bad.clone())).is_err(),
            "Task {key} = {bad}"
        );
    }
    let filter = full_filter();
    for (key, bad) in [
        ("on", json!("true")),
        ("rules", json!({})),
        ("actions", json!(null)),
        ("match", json!(true)),
    ] {
        assert!(
            serde_json::from_value::<MailFilter>(with(&filter, key, bad.clone())).is_err(),
            "MailFilter {key} = {bad}"
        );
    }
    // Kein Objekt auf oberster Ebene.
    assert!(serde_json::from_value::<Contact>(json!([])).is_err());
    assert!(serde_json::from_value::<Contact>(json!(null)).is_err());
}

#[test]
fn doppelte_schluessel_werden_abgelehnt() {
    let raw = r#"{"id":"p1","name":"A","name":"B","book":"b"}"#;
    assert!(serde_json::from_str::<Contact>(raw).is_err());
}

#[test]
fn aktionen_haben_die_form_der_oberflaeche() -> TestResult {
    for (raw, want) in [
        (json!({ "type": "read" }), Action::Read {}),
        (json!({ "type": "star" }), Action::Star {}),
        (json!({ "type": "delete" }), Action::Delete {}),
    ] {
        let parsed: Action = serde_json::from_value(raw.clone())?;
        assert_eq!(parsed, want);
        assert_eq!(serde_json::to_value(&parsed)?, raw);
    }
    let label: Action = serde_json::from_value(json!({ "type": "label", "value": "l2" }))?;
    assert!(matches!(label, Action::Label { ref value } if value.as_str() == "l2"));
    for bad in [
        json!({ "type": "move" }),
        json!({ "type": "label", "value": "" }),
        json!({ "type": "label", "value": "Projekt Atlas" }),
        json!({ "type": "Move", "value": "archive" }),
        json!({ "type": "read", "value": "x" }),
        json!({ "type": "delete", "permanent": true }),
        json!({ "value": "archive" }),
        json!("read"),
    ] {
        assert!(
            serde_json::from_value::<Action>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    Ok(())
}

#[test]
fn namen_werden_beim_parsen_getrimmt() -> TestResult {
    let contact: Contact =
        serde_json::from_value(with(&full_contact(), "name", json!("  Lena \t")))?;
    assert_eq!(contact.name.as_str(), "Lena");
    let out = serde_json::to_value(&contact)?;
    assert_eq!(out.get("name"), Some(&json!("Lena")));
    Ok(())
}

#[test]
fn regelwerte_bleiben_unveraendert() -> TestResult {
    // Text wird nicht getrimmt: Der Wert gehört dem Nutzer, getrimmt wird erst beim Vergleich.
    let rule: Rule =
        serde_json::from_value(json!({ "field": "body", "op": "contains", "value": "  Atlas " }))?;
    assert_eq!(rule.value.as_str(), "  Atlas ");
    Ok(())
}
