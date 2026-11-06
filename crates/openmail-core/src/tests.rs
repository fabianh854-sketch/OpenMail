use crate::api::{filter_matches, Api, MailView};
use crate::error::AppError;
use crate::ext::{Capability, ChatProtocol, ExtensionManifest, Providers};
use crate::store::Collection;
use crate::types::{
    Action, CalEvent, Contact, Email, Id, MailFilter, MatchMode, Name, Prio, Repeat, Rule,
    RuleField, RuleOp, Task, Text,
};
use std::sync::Arc;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn id(value: &str) -> Result<Id, AppError> {
    Id::try_from(value.to_owned())
}

fn name(value: &str) -> Result<Name, AppError> {
    Name::try_from(value.to_owned())
}

fn text(value: &str) -> Result<Text, AppError> {
    Text::try_from(value.to_owned())
}

fn event(start: i64, end: i64) -> Result<CalEvent, AppError> {
    Ok(CalEvent {
        id: id("e1")?,
        cal: id("c1")?,
        title: name("Statusrunde")?,
        place: Text::default(),
        all_day: false,
        start,
        end,
        repeat: Repeat::Weekly,
        remind: 15,
        attendees: Text::default(),
        desc: Text::default(),
    })
}

fn filter(mode: MatchMode, rules: Vec<Rule>) -> Result<MailFilter, AppError> {
    Ok(MailFilter {
        id: id("f1")?,
        name: name("Atlas")?,
        on: true,
        match_mode: mode,
        rules,
        actions: vec![Action::Star],
    })
}

fn rule(field: RuleField, op: RuleOp, value: &str) -> Result<Rule, AppError> {
    Ok(Rule {
        field,
        op,
        value: text(value)?,
    })
}

#[test]
fn id_accepts_safe_characters_only() {
    assert!(id("abc-123_X").is_ok());
    assert!(id("").is_err());
    assert!(id("a b").is_err());
    assert!(id("../etc").is_err());
    assert!(id(&"a".repeat(65)).is_err());
}

#[test]
fn email_validation_rejects_malformed_addresses() {
    for ok in ["a@b.de", "lena.hoffmann@contoso.com"] {
        assert!(Email::try_from(ok.to_owned()).is_ok(), "{ok}");
    }
    for bad in [
        "", "a@", "@b.de", "a@b", "a@@b.de", "a b@c.de", "a@.de", "a@b.", "a\n@b.de",
    ] {
        assert!(Email::try_from(bad.to_owned()).is_err(), "{bad:?}");
    }
}

#[test]
fn name_is_trimmed_and_bounded() -> TestResult {
    assert_eq!(name("  Lena  ")?.as_str(), "Lena");
    assert!(name("   ").is_err());
    assert!(name(&"x".repeat(201)).is_err());
    assert!(name("a\u{0007}b").is_err());
    Ok(())
}

#[test]
fn json_input_is_validated_and_unknown_fields_are_rejected() -> TestResult {
    let good = r#"{"id":"p1","name":"Lena","email":"lena@contoso.com","book":"personal"}"#;
    let contact: Contact = serde_json::from_str(good)?;
    assert_eq!(contact.name.as_str(), "Lena");
    assert!(!contact.fav);

    assert!(serde_json::from_str::<Contact>(r#"{"id":"p1","name":"","book":"b"}"#).is_err());
    assert!(
        serde_json::from_str::<Contact>(r#"{"id":"p1","name":"X","book":"b","admin":true}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<Contact>(r#"{"id":"p 1","name":"X","book":"b"}"#).is_err());
    Ok(())
}

#[test]
fn event_json_uses_the_ui_contract() -> TestResult {
    let json = r#"{"id":"e1","cal":"c1","title":"Termin","allDay":true,"start":1000,"end":2000,"repeat":"yearly","remind":-1}"#;
    let parsed: CalEvent = serde_json::from_str(json)?;
    assert!(parsed.all_day);
    assert_eq!(parsed.repeat, Repeat::Yearly);
    assert!(serde_json::from_str::<CalEvent>(&json.replace("yearly", "hourly")).is_err());
    Ok(())
}

#[test]
fn event_validation_checks_order_and_range() -> TestResult {
    assert!(event(1000, 2000)?.validate().is_ok());
    assert_eq!(
        event(2000, 1000)?.validate(),
        Err(AppError::InvalidInput("ende liegt vor beginn"))
    );
    assert!(event(i64::MAX, i64::MAX)?.validate().is_err());
    let mut late = event(1000, 2000)?;
    late.remind = 600_000;
    assert!(late.validate().is_err());
    Ok(())
}

#[test]
fn collection_is_bounded_and_reports_missing_items() -> TestResult {
    let tasks: Collection<Task> = Collection::new(1);
    let make = |key: &str| -> Result<Task, AppError> {
        Ok(Task {
            id: id(key)?,
            cal: id("c1")?,
            title: name("Aufgabe")?,
            due: None,
            prio: Prio::Normal,
            done: false,
            note: Text::default(),
        })
    };
    tasks.upsert(make("t1")?)?;
    tasks.upsert(make("t1")?)?; // Aktualisieren bleibt erlaubt
    assert_eq!(
        tasks.upsert(make("t2")?).err(),
        Some(AppError::LimitReached("sammlung voll"))
    );
    assert_eq!(tasks.get(&id("nope")?).err(), Some(AppError::NotFound));
    assert_eq!(tasks.remove(&id("nope")?).err(), Some(AppError::NotFound));
    Ok(())
}

#[test]
fn api_rejects_invalid_events_and_filters() -> TestResult {
    let api = Api::new();
    assert!(api.save_event(event(5, 1)?).is_err());
    assert!(api.save_event(event(1, 5)?).is_ok());
    assert!(api.save_filter(filter(MatchMode::Any, vec![])?).is_err());
    assert!(api
        .save_filter(filter(
            MatchMode::Any,
            vec![rule(RuleField::Subject, RuleOp::Contains, "   ")?]
        )?)
        .is_err());
    assert_eq!(api.list_events()?.len(), 1);
    Ok(())
}

#[test]
fn filters_match_like_the_ui_prototype() -> TestResult {
    let mail = MailView {
        from: "Lena Hoffmann lena@contoso.com",
        to: "Fabian",
        subject: "AW: Projekt Atlas",
        body: "Lizenzfreigabe offen",
    };
    let atlas = filter(
        MatchMode::Any,
        vec![rule(RuleField::Subject, RuleOp::Contains, "atlas")?],
    )?;
    assert!(filter_matches(&atlas, &mail));

    let both = filter(
        MatchMode::All,
        vec![
            rule(RuleField::Subject, RuleOp::Contains, "atlas")?,
            rule(RuleField::From, RuleOp::Is, "lena@contoso.com")?,
        ],
    )?;
    assert!(filter_matches(&both, &mail));

    let not_all = filter(
        MatchMode::All,
        vec![
            rule(RuleField::Subject, RuleOp::Contains, "atlas")?,
            rule(RuleField::Body, RuleOp::Not, "lizenz")?,
        ],
    )?;
    assert!(!filter_matches(&not_all, &mail));

    let mut disabled = atlas;
    disabled.on = false;
    assert!(!filter_matches(&disabled, &mail));
    Ok(())
}

#[test]
fn filter_action_json_matches_the_ui_shape() -> TestResult {
    let actions: Vec<Action> =
        serde_json::from_str(r#"[{"type":"move","value":"archive"},{"type":"star"}]"#)?;
    assert_eq!(
        actions,
        vec![
            Action::Move {
                value: id("archive")?
            },
            Action::Star
        ]
    );
    assert!(serde_json::from_str::<Vec<Action>>(r#"[{"type":"move","value":"../x"}]"#).is_err());
    Ok(())
}

struct Matrix;

impl ChatProtocol for Matrix {
    fn protocol(&self) -> &'static str {
        "matrix"
    }
}

#[test]
fn registry_rejects_duplicates_and_bad_keys() -> TestResult {
    let providers = Providers::default();
    providers.chat.register("matrix", Arc::new(Matrix))?;
    assert!(providers.chat.register("matrix", Arc::new(Matrix)).is_err());
    assert!(providers
        .chat
        .register("../matrix", Arc::new(Matrix))
        .is_err());
    assert_eq!(providers.chat.get("matrix")?.protocol(), "matrix");
    assert_eq!(providers.chat.get("irc").err(), Some(AppError::NotFound));
    assert_eq!(providers.chat.keys()?, vec!["matrix".to_owned()]);
    Ok(())
}

#[test]
fn manifest_denies_by_default_and_rejects_wildcards() -> TestResult {
    let manifest = ExtensionManifest {
        id: id("zusammenfassung")?,
        name: name("Thread-Zusammenfassung")?,
        version: name("0.4")?,
        capabilities: vec![
            Capability::ReadMail,
            Capability::Network {
                hosts: vec!["api.example.org".to_owned()],
            },
        ],
    };
    manifest.validate()?;
    assert!(manifest.allows(&Capability::ReadMail));
    assert!(!manifest.allows(&Capability::SendMail));
    assert!(manifest.allows(&Capability::Network {
        hosts: vec!["api.example.org".to_owned()]
    }));
    assert!(!manifest.allows(&Capability::Network {
        hosts: vec!["evil.example.org".to_owned()]
    }));

    let mut wild = manifest;
    wild.capabilities = vec![Capability::Network {
        hosts: vec!["*".to_owned()],
    }];
    assert!(wild.validate().is_err());
    Ok(())
}

#[test]
fn internal_errors_do_not_leak_details() {
    let message = AppError::Internal("sql: tabelle contacts fehlt").public_message();
    assert!(!message.contains("sql"));
    assert!(!message.contains("contacts"));
}
