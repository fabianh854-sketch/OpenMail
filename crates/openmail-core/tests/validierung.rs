//! Grenzwerte der validierten Typen und der Cross-Field-Regeln (`validate()`).

use openmail_core::types::{
    Action, CalEvent, Email, Id, MailFilter, MatchMode, Name, Prio, Repeat, Rule, RuleField,
    RuleOp, Task, Text, MAX_ACTIONS, MAX_ID_LEN, MAX_NAME_LEN, MAX_REMIND_MINUTES, MAX_RULES,
    MAX_RULE_VALUE_LEN, MAX_TEXT_LEN, MAX_TS, MIN_TS,
};
use openmail_core::{Api, AppError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn plus(n: usize) -> usize {
    n.saturating_add(1)
}

fn ts_plus(ts: i64, d: i64) -> i64 {
    ts.saturating_add(d)
}

fn id(v: &str) -> Result<Id, AppError> {
    Id::try_from(v.to_owned())
}

fn name(v: &str) -> Result<Name, AppError> {
    Name::try_from(v.to_owned())
}

fn text(v: &str) -> Result<Text, AppError> {
    Text::try_from(v.to_owned())
}

fn email(v: &str) -> Result<Email, AppError> {
    Email::try_from(v.to_owned())
}

fn event(start: i64, end: i64, remind: i32) -> Result<CalEvent, AppError> {
    Ok(CalEvent {
        id: id("e1")?,
        cal: id("c1")?,
        title: name("Termin")?,
        place: Text::default(),
        all_day: false,
        start,
        end,
        repeat: Repeat::None,
        remind,
        attendees: Text::default(),
        desc: Text::default(),
    })
}

fn task(due: Option<i64>) -> Result<Task, AppError> {
    Ok(Task {
        id: id("t1")?,
        cal: id("c1")?,
        title: name("Aufgabe")?,
        due,
        prio: Prio::Normal,
        done: false,
        note: Text::default(),
    })
}

fn rule(value: &str) -> Result<Rule, AppError> {
    Ok(Rule {
        field: RuleField::Subject,
        op: RuleOp::Contains,
        value: text(value)?,
    })
}

fn filter(rules: usize, actions: usize) -> Result<MailFilter, AppError> {
    let mut rule_list = Vec::new();
    for _ in 0..rules {
        rule_list.push(rule("atlas")?);
    }
    let mut action_list = Vec::new();
    for _ in 0..actions {
        action_list.push(Action::Star {});
    }
    Ok(MailFilter {
        id: id("f1")?,
        name: name("Filter")?,
        on: true,
        match_mode: MatchMode::Any,
        rules: rule_list,
        actions: action_list,
    })
}

#[test]
fn id_grenzen() {
    assert!(id(&"a".repeat(MAX_ID_LEN)).is_ok());
    assert_eq!(
        id(&"a".repeat(plus(MAX_ID_LEN))),
        Err(AppError::InvalidInput("id"))
    );
    assert!(id("").is_err());
    assert!(id("A-z_09").is_ok());
    for bad in [
        "ä", "a.b", "a/b", "..", "a b", "a\u{0}", "a\n", "é1", "a+b", "%2e",
    ] {
        assert!(id(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn id_zufall_ist_gueltig_und_eindeutig() -> TestResult {
    let a = Id::random();
    let b = Id::random();
    assert_ne!(a, b);
    // Die zufällige Kennung muss den eigenen Regeln genügen.
    assert_eq!(id(a.as_str())?, a);
    Ok(())
}

#[test]
fn name_grenzen_in_zeichen_nicht_byte() {
    for ch in ["a", "ä", "😀", "語"] {
        let ok = ch.repeat(MAX_NAME_LEN);
        assert!(name(&ok).is_ok(), "{ch} × {MAX_NAME_LEN}");
        let too_long = ch.repeat(plus(MAX_NAME_LEN));
        assert!(name(&too_long).is_err(), "{ch} × {}", plus(MAX_NAME_LEN));
    }
    // Rand-Leerraum zählt nicht mit, weil vorher getrimmt wird.
    let padded = format!("  {}  ", "a".repeat(MAX_NAME_LEN));
    assert!(name(&padded).is_ok());
}

#[test]
fn name_lehnt_leere_und_steuerzeichen_ab() -> TestResult {
    for bad in [
        "",
        " ",
        "\t\n",
        "a\u{0}b",
        "a\u{7}b",
        "a\u{1b}[31mb",
        "a\nb",
        "a\u{7f}b",
        "a\u{9f}b",
    ] {
        assert!(name(bad).is_err(), "{bad:?}");
    }
    // Rand-Steuerzeichen, die Leerraum sind, werden weggetrimmt.
    assert_eq!(name("\nLena\r\n")?.as_str(), "Lena");
    assert_eq!(name("Müller-Lüdenscheidt")?.as_str(), "Müller-Lüdenscheidt");
    Ok(())
}

#[test]
fn text_grenzen_in_byte() {
    assert!(text("").is_ok());
    assert!(text(&"a".repeat(MAX_TEXT_LEN)).is_ok());
    assert!(text(&"a".repeat(plus(MAX_TEXT_LEN))).is_err());
    // „ä“ hat zwei Byte: 10 000 × ä = 20 000 Byte.
    let half = MAX_TEXT_LEN.checked_div(2).unwrap_or_default();
    let umlauts = "ä".repeat(half);
    assert_eq!(umlauts.len(), MAX_TEXT_LEN);
    assert!(text(&umlauts).is_ok());
    assert!(text(&format!("{umlauts}a")).is_err());
}

#[test]
fn text_erlaubt_nur_zeilenumbruch_und_tab() {
    assert!(text("Zeile 1\nZeile 2\r\n\tEingerückt").is_ok());
    for bad in ["\u{0}", "a\u{1b}b", "\u{7f}", "\u{85}", "\u{8}", "\u{c}"] {
        assert!(text(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn email_gueltige_adressen() -> TestResult {
    for ok in [
        "a@b.de",
        "lena.hoffmann@contoso.com",
        "first+tag@sub.example.org",
        "müller@bücher.de",
        "x@a-b.de",
    ] {
        assert!(email(ok).is_ok(), "{ok}");
    }
    assert_eq!(email("  a@b.de  ")?.as_str(), "a@b.de");
    Ok(())
}

#[test]
fn email_ungueltige_adressen() {
    for bad in [
        "",
        "a",
        "@b.de",
        "a@",
        "a@b",
        "a@b.",
        "a@.de",
        "a@b..de",
        "a@-b.de",
        "a@b-.de",
        "a@1.2.3.4",
        "a@[127.0.0.1]",
        "a b@c.de",
        "a@b c.de",
        "a@b.de\u{0}",
        "a@b_c.de",
        "a@@b.de",
    ] {
        assert!(email(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn email_laengengrenzen() -> TestResult {
    let local_ok = "l".repeat(64);
    assert!(email(&format!("{local_ok}@b.de")).is_ok());
    assert!(email(&format!("{}@b.de", "l".repeat(65))).is_err());

    let label63 = "d".repeat(63);
    assert!(email(&format!("a@{label63}.de")).is_ok());
    assert!(email(&format!("a@{}.de", "d".repeat(64))).is_err());

    // Gesamtlänge 254 erlaubt, 255 nicht.
    let domain = format!("{label63}.{label63}.{}.de", "c".repeat(58));
    let at_limit = format!("{local_ok}@{domain}");
    assert_eq!(at_limit.len(), 254);
    assert!(email(&at_limit).is_ok());
    let domain_long = format!("{label63}.{label63}.{}.de", "c".repeat(59));
    let over = format!("{local_ok}@{domain_long}");
    assert_eq!(over.len(), 255);
    assert!(email(&over).is_err());
    Ok(())
}

#[test]
fn termin_zeitgrenzen() -> TestResult {
    assert!(event(MIN_TS, MIN_TS, -1)?.validate().is_ok());
    assert!(event(MAX_TS, MAX_TS, -1)?.validate().is_ok());
    assert!(event(MIN_TS, MAX_TS, -1)?.validate().is_ok());
    assert_eq!(
        event(ts_plus(MIN_TS, -1), 0, -1)?.validate(),
        Err(AppError::InvalidInput("zeitpunkt"))
    );
    assert_eq!(
        event(0, ts_plus(MAX_TS, 1), -1)?.validate(),
        Err(AppError::InvalidInput("zeitpunkt"))
    );
    assert!(event(i64::MIN, 0, -1)?.validate().is_err());
    assert!(event(0, i64::MAX, -1)?.validate().is_err());
    // Ende vor Beginn um eine Millisekunde.
    assert_eq!(
        event(1_000, 999, -1)?.validate(),
        Err(AppError::InvalidInput("ende liegt vor beginn"))
    );
    // Gleicher Beginn und Ende (z. B. Ganztagstermin auf Tagesanfang) ist erlaubt.
    assert!(event(1_000, 1_000, -1)?.validate().is_ok());
    Ok(())
}

#[test]
fn termin_erinnerungsgrenzen() -> TestResult {
    for ok in [-1, 0, 15, MAX_REMIND_MINUTES] {
        assert!(event(0, 0, ok)?.validate().is_ok(), "{ok}");
    }
    for bad in [-2, i32::MIN, MAX_REMIND_MINUTES.saturating_add(1), i32::MAX] {
        assert_eq!(
            event(0, 0, bad)?.validate(),
            Err(AppError::InvalidInput("erinnerung")),
            "{bad}"
        );
    }
    Ok(())
}

#[test]
fn aufgaben_faelligkeitsgrenzen() -> TestResult {
    assert!(task(None)?.validate().is_ok());
    assert!(task(Some(MIN_TS))?.validate().is_ok());
    assert!(task(Some(MAX_TS))?.validate().is_ok());
    assert!(task(Some(0))?.validate().is_ok());
    assert_eq!(
        task(Some(ts_plus(MIN_TS, -1)))?.validate(),
        Err(AppError::InvalidInput("faelligkeit"))
    );
    assert_eq!(
        task(Some(ts_plus(MAX_TS, 1)))?.validate(),
        Err(AppError::InvalidInput("faelligkeit"))
    );
    Ok(())
}

#[test]
fn filter_anzahl_der_regeln_und_aktionen() -> TestResult {
    assert!(filter(1, 1)?.validate().is_ok());
    assert!(filter(MAX_RULES, MAX_ACTIONS)?.validate().is_ok());
    assert_eq!(
        filter(0, 1)?.validate(),
        Err(AppError::InvalidInput("bedingungen"))
    );
    assert_eq!(
        filter(plus(MAX_RULES), 1)?.validate(),
        Err(AppError::InvalidInput("bedingungen"))
    );
    assert_eq!(
        filter(1, 0)?.validate(),
        Err(AppError::InvalidInput("aktionen"))
    );
    assert_eq!(
        filter(1, plus(MAX_ACTIONS))?.validate(),
        Err(AppError::InvalidInput("aktionen"))
    );
    Ok(())
}

#[test]
fn filter_regelwert_grenzen() -> TestResult {
    let mut f = filter(1, 1)?;
    f.rules = vec![rule(&"x".repeat(MAX_RULE_VALUE_LEN))?];
    assert!(f.validate().is_ok());
    f.rules = vec![rule(&"x".repeat(plus(MAX_RULE_VALUE_LEN)))?];
    assert_eq!(f.validate(), Err(AppError::InvalidInput("bedingungswert")));
    // Die Grenze gilt in Byte: 250 × „ä“ = 500 Byte.
    let half = MAX_RULE_VALUE_LEN.checked_div(2).unwrap_or_default();
    f.rules = vec![rule(&"ä".repeat(half))?];
    assert!(f.validate().is_ok());
    f.rules = vec![rule(&"ä".repeat(plus(half)))?];
    assert!(f.validate().is_err());
    // Leere oder nur aus Leerraum bestehende Werte sind ungültig, auch neben gültigen Regeln.
    for blank in ["", " ", "\n\t "] {
        f.rules = vec![rule("atlas")?, rule(blank)?];
        assert_eq!(
            f.validate(),
            Err(AppError::InvalidInput("bedingungswert")),
            "{blank:?}"
        );
    }
    Ok(())
}

#[test]
fn api_speichert_nur_gueltige_grenzwerte() -> TestResult {
    let api = Api::new();
    assert!(api
        .save_event(event(MIN_TS, MAX_TS, MAX_REMIND_MINUTES)?)
        .is_ok());
    assert!(api.save_task(task(Some(MAX_TS))?).is_ok());
    assert!(api.save_filter(filter(MAX_RULES, MAX_ACTIONS)?).is_ok());

    let fresh = Api::new();
    assert!(fresh.save_event(event(0, ts_plus(MAX_TS, 1), -1)?).is_err());
    assert!(fresh.save_task(task(Some(ts_plus(MIN_TS, -1)))?).is_err());
    assert!(fresh.save_filter(filter(plus(MAX_RULES), 1)?).is_err());
    assert!(fresh.list_events()?.is_empty());
    assert!(fresh.list_tasks()?.is_empty());
    assert!(fresh.list_filters()?.is_empty());
    Ok(())
}
