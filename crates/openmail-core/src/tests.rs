use crate::api::{filter_matches, Api, MailView};
use crate::error::AppError;
use crate::ext::{Capability, ChatProtocol, ExtensionManifest, Providers, ValidatedManifest};
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
        actions: vec![Action::Star {}],
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
    for ok in ["a@b.de", "lena.hoffmann@contoso.com", "x@mail-1.müller.de"] {
        assert!(Email::try_from(ok.to_owned()).is_ok(), "{ok}");
    }
    for bad in [
        "", "a@", "@b.de", "a@b", "a@@b.de", "a b@c.de", "a@.de", "a@b.", "a\n@b.de", "a@b..de",
        "a@-b.de", "a@b-.de", "a@b_c.de",
    ] {
        assert!(Email::try_from(bad.to_owned()).is_err(), "{bad:?}");
    }
    let long_label = format!("a@{}.de", "b".repeat(64));
    assert!(Email::try_from(long_label).is_err());
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
            Action::Star {}
        ]
    );
    for bad in [
        r#"[{"type":"move","value":"../x"}]"#,
        r#"[{"type":"move","value":"archive","extra":1}]"#,
        r#"[{"type":"star","value":"archive"}]"#,
        r#"[{"type":"forward","value":"x"}]"#,
    ] {
        assert!(serde_json::from_str::<Vec<Action>>(bad).is_err(), "{bad}");
    }
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

fn manifest(capabilities: Vec<Capability>) -> Result<ExtensionManifest, AppError> {
    Ok(ExtensionManifest {
        id: id("zusammenfassung")?,
        name: name("Thread-Zusammenfassung")?,
        version: name("0.4")?,
        capabilities,
    })
}

fn network(hosts: &[&str]) -> Capability {
    Capability::Network {
        hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
    }
}

#[test]
fn manifest_denies_by_default_and_rejects_wildcards() -> TestResult {
    let checked = ValidatedManifest::try_from(manifest(vec![
        Capability::ReadMail {},
        network(&["api.example.org"]),
    ])?)?;
    assert!(checked.allows(&Capability::ReadMail {}));
    assert!(!checked.allows(&Capability::SendMail {}));
    assert!(checked.allows(&network(&["api.example.org"])));
    assert!(checked.allows(&network(&["API.Example.org"])));
    assert!(!checked.allows(&network(&["evil.example.org"])));
    assert!(!checked.allows(&network(&["api.example.org", "evil.example.org"])));
    // Eine Netzwerkanfrage ohne Host darf nie als erlaubt gelten.
    assert!(!checked.allows(&network(&[])));

    assert!(ValidatedManifest::try_from(manifest(vec![network(&["*"])])?).is_err());
    Ok(())
}

#[test]
fn manifest_rejects_local_and_ip_hosts() -> TestResult {
    for bad in [
        "localhost",
        "intranet",
        "printer.local",
        "db.internal",
        "nas.home.arpa",
        "1.0.0.127.in-addr.arpa",
        "router.lan",
        "box.localdomain",
        "app.test",
        "x.invalid",
        "nas.home",
        "git.corp",
        "1.0.0.0.ip6.arpa",
        "ipv4only.arpa",
        "127.1",
        "0x7f.1",
        "0x7f.0x1",
        "0x7f.0x0.0x0.0x1",
        "0xa.0xa",
        "017.0x1",
        "example.123",
        "example.x",
        "127.0.0.1",
        "192.168.0.1",
        "10.0.0.1",
        "::1",
        "[::1]",
        "Example.org",
        "a..b.org",
        ".example.org",
        "example.org.",
        "-a.example.org",
        "a-.example.org",
        "exa_mple.org",
        "*.example.org",
        "",
    ] {
        let result = ValidatedManifest::try_from(manifest(vec![network(&[bad])])?);
        assert!(result.is_err(), "{bad:?}");
    }
    let long_label = format!("{}.org", "a".repeat(64));
    assert!(ValidatedManifest::try_from(manifest(vec![network(&[&long_label])])?).is_err());
    for ok in [
        "api.example.org",
        "mail-1.example.co.uk",
        "xn--mnchen-3ya.de",
    ] {
        assert!(
            ValidatedManifest::try_from(manifest(vec![network(&[ok])])?).is_ok(),
            "{ok}"
        );
    }
    Ok(())
}

#[test]
fn manifest_rejects_duplicates_and_too_many_entries() -> TestResult {
    let dup_cap = manifest(vec![Capability::ReadMail {}, Capability::ReadMail {}])?;
    assert!(ValidatedManifest::try_from(dup_cap).is_err());
    let dup_host = manifest(vec![network(&["a.example.org", "a.example.org"])])?;
    assert!(ValidatedManifest::try_from(dup_host).is_err());
    let hosts: Vec<String> = (0..17).map(|i| format!("h{i}.example.org")).collect();
    let refs: Vec<&str> = hosts.iter().map(String::as_str).collect();
    assert!(ValidatedManifest::try_from(manifest(vec![network(&refs)])?).is_err());
    Ok(())
}

#[test]
fn oversized_host_list_is_rejected_before_duplicate_scan() -> TestResult {
    let hosts = vec!["a.example.org".to_owned(); 100_000];
    let big = manifest(vec![Capability::Network { hosts }])?;
    assert!(ValidatedManifest::try_from(big).is_err());
    Ok(())
}

#[test]
fn tagged_enums_serialize_without_extra_fields() -> TestResult {
    assert_eq!(
        serde_json::to_string(&Action::Star {})?,
        r#"{"type":"star"}"#
    );
    assert_eq!(
        serde_json::to_string(&Capability::ReadMail {})?,
        r#"{"kind":"read_mail"}"#
    );
    Ok(())
}

#[test]
fn manifest_json_is_validated_on_parse() -> TestResult {
    let ok = r#"{"id":"x","name":"X","version":"1","capabilities":[{"kind":"read_mail"},{"kind":"network","hosts":["api.example.org"]}]}"#;
    let checked: ValidatedManifest = serde_json::from_str(ok)?;
    assert!(checked.allows(&Capability::ReadMail {}));
    for bad in [
        r#"{"id":"x","name":"X","version":"1","capabilities":[{"kind":"network","hosts":["localhost"]}]}"#,
        r#"{"id":"x","name":"X","version":"1","capabilities":[{"kind":"network","hosts":["a.example.org"],"extra":1}]}"#,
        r#"{"id":"x","name":"X","version":"1","capabilities":[{"kind":"read_mail","hosts":["a.example.org"]}]}"#,
        r#"{"id":"x","name":"X","version":"1","capabilities":[{"kind":"root"}]}"#,
        r#"{"id":"x","name":"X","version":"1","capabilities":[],"extra":true}"#,
    ] {
        assert!(
            serde_json::from_str::<ValidatedManifest>(bad).is_err(),
            "{bad}"
        );
    }
    Ok(())
}

#[test]
fn internal_errors_do_not_leak_details() {
    let message = AppError::Internal("sql: tabelle contacts fehlt").public_message();
    assert!(!message.contains("sql"));
    assert!(!message.contains("contacts"));
}
