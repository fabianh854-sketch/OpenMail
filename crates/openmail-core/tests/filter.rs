//! Filterlogik (`filter_matches`): alle Operatoren × Felder, Groß-/Kleinschreibung, Unicode,
//! Verknüpfung any/all und deaktivierte Filter.

use openmail_core::filter_matches;
use openmail_core::types::{
    Action, Id, MailFilter, MatchMode, Name, Rule, RuleField, RuleOp, Text,
};
use openmail_core::{AppError, MailView};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const FIELDS: [RuleField; 4] = [
    RuleField::From,
    RuleField::To,
    RuleField::Subject,
    RuleField::Body,
];

fn rule(field: RuleField, op: RuleOp, value: &str) -> Result<Rule, AppError> {
    Ok(Rule {
        field,
        op,
        value: Text::try_from(value.to_owned())?,
    })
}

fn filter(mode: MatchMode, rules: Vec<Rule>) -> Result<MailFilter, AppError> {
    Ok(MailFilter {
        id: Id::try_from("f1".to_owned())?,
        name: Name::try_from("Test".to_owned())?,
        on: true,
        match_mode: mode,
        rules,
        actions: vec![Action::Read {}],
    })
}

fn one(field: RuleField, op: RuleOp, value: &str) -> Result<MailFilter, AppError> {
    filter(MatchMode::All, vec![rule(field, op, value)?])
}

/// Prüft eine einzelne Regel gegen eine Nachricht, deren Feld `field` den Text `hay` trägt.
fn check(field: RuleField, op: RuleOp, value: &str, hay: &str) -> Result<bool, AppError> {
    let mail = MailView {
        from: if field == RuleField::From { hay } else { "" },
        to: if field == RuleField::To { hay } else { "" },
        subject: if field == RuleField::Subject { hay } else { "" },
        body: if field == RuleField::Body { hay } else { "" },
    };
    Ok(filter_matches(&one(field, op, value)?, &mail))
}

/// Nachricht mit eindeutigem Inhalt pro Feld.
fn mail() -> MailView<'static> {
    MailView {
        from: "Lena Hoffmann lena@contoso.com",
        to: "Fabian Schulz fabian@fabrikam.com",
        subject: "AW: Projekt Atlas",
        body: "Die Lizenzfreigabe ist noch offen.",
    }
}

#[test]
fn jede_regel_prueft_nur_ihr_feld() -> TestResult {
    let m = mail();
    let marker = [
        (RuleField::From, "contoso"),
        (RuleField::To, "fabrikam"),
        (RuleField::Subject, "atlas"),
        (RuleField::Body, "lizenzfreigabe"),
    ];
    for (field, _) in marker {
        for (other, word) in marker {
            let hit = filter_matches(&one(field, RuleOp::Contains, word)?, &m);
            assert_eq!(hit, field == other, "{field:?} contains {word}");
            let not = filter_matches(&one(field, RuleOp::Not, word)?, &m);
            assert_eq!(not, field != other, "{field:?} not {word}");
        }
    }
    Ok(())
}

#[test]
fn alle_operatoren_auf_allen_feldern() -> TestResult {
    for field in FIELDS {
        let hay = "Rechnung Mai 2026";
        // contains
        assert!(check(field, RuleOp::Contains, "mai", hay)?);
        assert!(check(field, RuleOp::Contains, "nung m", hay)?);
        assert!(!check(field, RuleOp::Contains, "juni", hay)?);
        // not
        assert!(check(field, RuleOp::Not, "juni", hay)?);
        assert!(!check(field, RuleOp::Not, "MAI", hay)?);
        // is: ganzes Feld oder ein ganzes Wort
        assert!(check(field, RuleOp::Is, "rechnung mai 2026", hay)?);
        assert!(check(field, RuleOp::Is, "Mai", hay)?);
        assert!(!check(field, RuleOp::Is, "ma", hay)?);
        assert!(!check(field, RuleOp::Is, "rechnung mai", hay)?);
        // starts
        assert!(check(field, RuleOp::Starts, "RECHNUNG", hay)?);
        assert!(check(field, RuleOp::Starts, "rechnung mai 2026", hay)?);
        assert!(!check(field, RuleOp::Starts, "mai", hay)?);
    }
    Ok(())
}

#[test]
fn gross_kleinschreibung_wird_ignoriert() -> TestResult {
    assert!(check(
        RuleField::Subject,
        RuleOp::Contains,
        "ATLAS",
        "projekt atlas"
    )?);
    assert!(check(
        RuleField::Subject,
        RuleOp::Contains,
        "atlas",
        "PROJEKT ATLAS"
    )?);
    assert!(check(
        RuleField::From,
        RuleOp::Is,
        "LENA@CONTOSO.COM",
        "lena@contoso.com"
    )?);
    // Umlaute und andere Schriften.
    assert!(check(
        RuleField::Body,
        RuleOp::Contains,
        "ärger",
        "ÄRGER im Büro"
    )?);
    assert!(check(RuleField::Body, RuleOp::Starts, "ÖL", "öltank")?);
    assert!(check(RuleField::Body, RuleOp::Contains, "ΑΘΉΝΑ", "αθήνα")?);
    Ok(())
}

#[test]
fn unicode_vergleich_entspricht_to_lowercase_wie_im_prototyp() -> TestResult {
    // Der Vergleich nutzt `to_lowercase` (wie `toLowerCase` im JS-Prototyp), keine volle
    // Unicode-Faltung: „STRASSE“ und „straße“ gelten als verschieden. Dokumentiert den Ist-Stand.
    assert!(!check(
        RuleField::Subject,
        RuleOp::Contains,
        "straße",
        "HAUPTSTRASSE 1"
    )?);
    assert!(!check(
        RuleField::Subject,
        RuleOp::Contains,
        "STRASSE",
        "hauptstraße 1"
    )?);
    assert!(check(
        RuleField::Subject,
        RuleOp::Contains,
        "STRAßE",
        "hauptstraße 1"
    )?);
    // Emojis und Mehrbytezeichen am Wortrand.
    assert!(check(
        RuleField::Subject,
        RuleOp::Is,
        "🎉",
        "Party 🎉 heute"
    )?);
    assert!(check(RuleField::Subject, RuleOp::Starts, "🎉", "🎉 Party")?);
    Ok(())
}

#[test]
fn regelwert_wird_getrimmt_das_feld_bei_starts_nicht() -> TestResult {
    assert!(check(
        RuleField::Subject,
        RuleOp::Contains,
        "  atlas \n",
        "Projekt Atlas"
    )?);
    assert!(check(
        RuleField::Subject,
        RuleOp::Is,
        " atlas ",
        "  Atlas  "
    )?);
    // starts vergleicht das ungetrimmte Feld (wie der Prototyp).
    assert!(!check(
        RuleField::Subject,
        RuleOp::Starts,
        "atlas",
        "  Atlas"
    )?);
    // Mehrere Leerzeichen und Tabs trennen Wörter für `is`.
    assert!(check(
        RuleField::To,
        RuleOp::Is,
        "fabian@fabrikam.com",
        "Fabian\t\tfabian@fabrikam.com"
    )?);
    Ok(())
}

#[test]
fn leere_felder() -> TestResult {
    for field in FIELDS {
        assert!(!check(field, RuleOp::Contains, "x", "")?);
        assert!(check(field, RuleOp::Not, "x", "")?);
        assert!(!check(field, RuleOp::Is, "x", "")?);
        assert!(!check(field, RuleOp::Starts, "x", "")?);
    }
    Ok(())
}

#[test]
fn leerer_regelwert_trifft_nie() -> TestResult {
    // Ungültige Regeln (an `validate` vorbei gebaut) dürfen nichts auslösen, auch nicht über `not`.
    for op in [RuleOp::Contains, RuleOp::Not, RuleOp::Is, RuleOp::Starts] {
        assert!(
            !check(RuleField::Subject, op, "   ", "irgendwas")?,
            "{op:?}"
        );
        assert!(!check(RuleField::Subject, op, "", "")?, "{op:?}");
    }
    Ok(())
}

#[test]
fn verknuepfung_any_und_all() -> TestResult {
    let m = mail();
    let yes = rule(RuleField::Subject, RuleOp::Contains, "atlas")?;
    let yes2 = rule(RuleField::From, RuleOp::Is, "lena@contoso.com")?;
    let no = rule(RuleField::Body, RuleOp::Contains, "rechnung")?;

    assert!(filter_matches(
        &filter(MatchMode::All, vec![yes.clone(), yes2.clone()])?,
        &m
    ));
    assert!(!filter_matches(
        &filter(MatchMode::All, vec![yes.clone(), no.clone()])?,
        &m
    ));
    assert!(!filter_matches(
        &filter(MatchMode::All, vec![no.clone(), yes.clone()])?,
        &m
    ));
    assert!(filter_matches(
        &filter(MatchMode::Any, vec![no.clone(), yes.clone()])?,
        &m
    ));
    assert!(filter_matches(
        &filter(MatchMode::Any, vec![yes, no.clone()])?,
        &m
    ));
    assert!(!filter_matches(
        &filter(MatchMode::Any, vec![no.clone(), no])?,
        &m
    ));
    Ok(())
}

#[test]
fn deaktivierte_und_leere_filter_treffen_nie() -> TestResult {
    let m = mail();
    for mode in [MatchMode::Any, MatchMode::All] {
        let mut f = filter(
            mode,
            vec![rule(RuleField::Subject, RuleOp::Contains, "atlas")?],
        )?;
        assert!(filter_matches(&f, &m));
        f.on = false;
        assert!(!filter_matches(&f, &m));
        // Ohne Regeln wäre `all` leer wahr – darf aber nie treffen.
        let mut empty = filter(mode, Vec::new())?;
        assert!(!filter_matches(&empty, &m));
        empty.on = false;
        assert!(!filter_matches(&empty, &m));
    }
    Ok(())
}

#[test]
fn not_mit_all_als_ausschluss() -> TestResult {
    let f = filter(
        MatchMode::All,
        vec![
            rule(RuleField::From, RuleOp::Contains, "contoso")?,
            rule(RuleField::Subject, RuleOp::Not, "newsletter")?,
        ],
    )?;
    assert!(filter_matches(&f, &mail()));
    let newsletter = MailView {
        subject: "Contoso Newsletter November",
        ..mail()
    };
    assert!(!filter_matches(&f, &newsletter));
    Ok(())
}

#[test]
fn sonderzeichen_im_regelwert_sind_wortwoertlich() -> TestResult {
    // Keine Regex- oder Wildcard-Auswertung.
    assert!(!check(
        RuleField::Subject,
        RuleOp::Contains,
        ".*",
        "beliebig"
    )?);
    assert!(check(RuleField::Subject, RuleOp::Contains, ".*", "a.*b")?);
    assert!(!check(RuleField::From, RuleOp::Contains, "%", "alles")?);
    assert!(check(
        RuleField::Subject,
        RuleOp::Starts,
        "[news]",
        "[NEWS] Ausgabe 12"
    )?);
    Ok(())
}
