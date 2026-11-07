//! Erweiterungspunkte: `Registry`, `Providers`, `ExtensionManifest` und `ValidatedManifest`.

use openmail_core::ext::{
    Capability, ChatProtocol, ExtensionManifest, MailProvider, Providers, Registry,
    ValidatedManifest,
};
use openmail_core::AppError;
use serde_json::{json, Value};
use std::sync::Arc;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Imap;
impl MailProvider for Imap {
    fn protocol(&self) -> &'static str {
        "imap"
    }
}

struct Jmap;
impl MailProvider for Jmap {
    fn protocol(&self) -> &'static str {
        "jmap"
    }
}

struct Matrix;
impl ChatProtocol for Matrix {
    fn protocol(&self) -> &'static str {
        "matrix"
    }
}

fn manifest_json(capabilities: Value) -> Value {
    json!({ "id": "demo-ext", "name": "Demo", "version": "1.0.0", "capabilities": capabilities })
}

fn parse(capabilities: Value) -> Result<ValidatedManifest, serde_json::Error> {
    serde_json::from_value(manifest_json(capabilities))
}

fn net(hosts: &[&str]) -> Value {
    json!({ "kind": "network", "hosts": hosts })
}

fn want_net(hosts: &[&str]) -> Capability {
    Capability::Network {
        hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
    }
}

fn host_ok(host: &str) -> bool {
    parse(json!([net(&[host])])).is_ok()
}

#[test]
fn registry_anmelden_abfragen_auflisten() -> TestResult {
    let reg: Registry<dyn MailProvider> = Registry::new();
    assert!(reg.keys()?.is_empty());
    reg.register("jmap", Arc::new(Jmap))?;
    reg.register("imap", Arc::new(Imap))?;
    assert_eq!(reg.get("imap")?.protocol(), "imap");
    assert_eq!(reg.get("jmap")?.protocol(), "jmap");
    assert_eq!(reg.keys()?, vec!["imap".to_owned(), "jmap".to_owned()]);
    assert!(matches!(reg.get("pop3"), Err(AppError::NotFound)));
    Ok(())
}

#[test]
fn registry_lehnt_doppelte_und_ungueltige_schluessel_ab() -> TestResult {
    let reg: Registry<dyn MailProvider> = Registry::default();
    reg.register("imap", Arc::new(Imap))?;
    assert_eq!(
        reg.register("imap", Arc::new(Jmap)),
        Err(AppError::InvalidInput("schluessel bereits vergeben"))
    );
    // Der erste Eintrag bleibt erhalten.
    assert_eq!(reg.get("imap")?.protocol(), "imap");
    let too_long = "k".repeat(65);
    for bad in ["", "i map", "imap/", "ïmap", "../x", too_long.as_str()] {
        assert_eq!(
            reg.register(bad, Arc::new(Imap)),
            Err(AppError::InvalidInput("id")),
            "{bad:?}"
        );
    }
    assert!(reg.register(&"k".repeat(64), Arc::new(Imap)).is_ok());
    // Ungültige Schlüssel finden einfach nichts.
    assert!(matches!(reg.get(""), Err(AppError::NotFound)));
    assert!(matches!(reg.get("../imap"), Err(AppError::NotFound)));
    Ok(())
}

#[test]
fn registry_unterscheidet_gross_klein() -> TestResult {
    let reg: Registry<dyn MailProvider> = Registry::new();
    reg.register("imap", Arc::new(Imap))?;
    assert!(matches!(reg.get("IMAP"), Err(AppError::NotFound)));
    Ok(())
}

#[test]
fn providers_haben_getrennte_register() -> TestResult {
    let providers = Providers::default();
    providers.mail.register("x", Arc::new(Imap))?;
    providers.chat.register("x", Arc::new(Matrix))?;
    assert_eq!(providers.chat.get("x")?.protocol(), "matrix");
    assert!(providers.calendar.keys()?.is_empty());
    assert!(providers.contacts.keys()?.is_empty());
    Ok(())
}

#[test]
fn manifest_ohne_rechte_erlaubt_nichts() -> TestResult {
    let m = parse(json!([]))?;
    for cap in [
        Capability::ReadMail {},
        Capability::SendMail {},
        Capability::ReadContacts {},
        Capability::ReadCalendar {},
        want_net(&["example.com"]),
    ] {
        assert!(!m.allows(&cap), "{cap:?}");
    }
    Ok(())
}

#[test]
fn manifest_erlaubt_nur_ausdruecklich_genannte_rechte() -> TestResult {
    let m = parse(json!([
        { "kind": "read_mail" },
        net(&["api.example.com", "xn--bcher-kva.de"])
    ]))?;
    assert!(m.allows(&Capability::ReadMail {}));
    assert!(!m.allows(&Capability::SendMail {}));
    assert!(!m.allows(&Capability::ReadContacts {}));
    assert!(m.allows(&want_net(&["api.example.com"])));
    assert!(m.allows(&want_net(&["API.Example.COM"])));
    assert!(m.allows(&want_net(&["xn--bcher-kva.de", "api.example.com"])));
    assert!(!m.allows(&want_net(&["example.com"])));
    assert!(!m.allows(&want_net(&["evil.api.example.com"])));
    assert!(!m.allows(&want_net(&["api.example.com", "evil.com"])));
    assert!(!m.allows(&want_net(&[])));
    assert_eq!(m.manifest().capabilities.len(), 2);
    Ok(())
}

#[test]
fn manifest_json_form() -> TestResult {
    let raw = manifest_json(json!([
        { "kind": "send_mail" },
        { "kind": "read_contacts" },
        { "kind": "read_calendar" },
        net(&["mail.example.org"])
    ]));
    let m: ValidatedManifest = serde_json::from_value(raw.clone())?;
    assert_eq!(serde_json::to_value(&m)?, raw);
    let plain: ExtensionManifest = serde_json::from_value(raw)?;
    assert_eq!(&plain, m.manifest());
    Ok(())
}

#[test]
fn manifest_json_wird_streng_geprueft() {
    for bad in [
        json!([{ "kind": "ReadMail" }]),
        json!([{ "kind": "readMail" }]),
        json!([{ "kind": "admin" }]),
        json!([{ "kind": "read_mail", "scope": "all" }]),
        json!([{ "kind": "network" }]),
        json!([{ "kind": "network", "hosts": "example.com" }]),
        json!([{ "kind": "network", "hosts": ["example.com"], "ports": [443] }]),
        json!(["read_mail"]),
        json!(null),
    ] {
        assert!(parse(bad.clone()).is_err(), "{bad}");
    }
    let mut extra = manifest_json(json!([]));
    if let Some(obj) = extra.as_object_mut() {
        obj.insert("permissions".to_owned(), json!(["*"]));
    }
    assert!(serde_json::from_value::<ValidatedManifest>(extra).is_err());
    for (key, bad) in [
        ("id", json!("demo ext")),
        ("name", json!("")),
        ("version", json!(" ")),
    ] {
        let mut m = manifest_json(json!([]));
        if let Some(obj) = m.as_object_mut() {
            obj.insert(key.to_owned(), bad);
        }
        assert!(
            serde_json::from_value::<ValidatedManifest>(m).is_err(),
            "{key}"
        );
    }
}

#[test]
fn gueltige_hosts() {
    for ok in [
        "example.com",
        "api.example.com",
        "a-b.example.co",
        "xn--bcher-kva.de",
        "example.xn--p1ai",
        "0.example.com",
        "123.example.com",
    ] {
        assert!(host_ok(ok), "{ok}");
    }
}

#[test]
fn ungueltige_hosts() {
    for bad in [
        "",
        "com",
        "Example.com",
        "*.example.com",
        "example.com.",
        ".example.com",
        "exa_mple.com",
        "-a.example.com",
        "a-.example.com",
        "example.com:443",
        "https://example.com",
        "example.com/pfad",
        "user@example.com",
        "bücher.de",
        "127.0.0.1",
        "::1",
        "[::1]",
        "127.1",
        "0x7f.0x1",
        "example.123",
        "example.c",
        "localhost",
        "foo.localhost",
        "printer.local",
        "db.internal",
        "nas.lan",
        "router.home",
        "home.arpa",
        "1.0.0.127.in-addr.arpa",
        "x.test",
        "x.invalid",
        "example.xn--",
        "example.com ",
    ] {
        assert!(!host_ok(bad), "{bad:?}");
    }
}

#[test]
fn host_laengengrenzen() {
    let l63 = "a".repeat(63);
    assert!(host_ok(&format!("{l63}.com")));
    assert!(!host_ok(&format!("{}.com", "a".repeat(64))));
    // 253 Byte erlaubt, 254 nicht.
    let at_limit = format!("{l63}.{l63}.{l63}.{}.com", "b".repeat(57));
    assert_eq!(at_limit.len(), 253);
    assert!(host_ok(&at_limit));
    let over = format!("{l63}.{l63}.{l63}.{}.com", "b".repeat(58));
    assert_eq!(over.len(), 254);
    assert!(!host_ok(&over));
}

#[test]
fn host_listen_grenzen_und_duplikate() {
    let hosts: Vec<String> = (0..17).map(|i| format!("h{i}.example.com")).collect();
    let refs: Vec<&str> = hosts.iter().map(String::as_str).collect();
    assert!(parse(json!([net(refs.get(..16).unwrap_or_default())])).is_ok());
    assert!(parse(json!([net(&refs)])).is_err());
    assert!(parse(json!([net(&[])])).is_err());
    assert!(parse(json!([net(&["a.example.com", "a.example.com"])])).is_err());
}

#[test]
fn rechte_grenzen_und_duplikate() {
    assert!(parse(json!([{ "kind": "read_mail" }, { "kind": "read_mail" }])).is_err());
    assert!(parse(json!([net(&["a.example.com"]), net(&["a.example.com"])])).is_err());
    // 16 verschiedene Rechte sind erlaubt, 17 nicht.
    let caps = |n: usize| -> Value {
        Value::Array(
            (0..n)
                .map(|i| net(&[format!("h{i}.example.com").as_str()]))
                .collect(),
        )
    };
    assert!(parse(caps(16)).is_ok());
    assert!(parse(caps(17)).is_err());
}

#[test]
fn ungepruefte_manifeste_lassen_sich_pruefen() -> TestResult {
    let raw: ExtensionManifest =
        serde_json::from_value(manifest_json(json!([net(&["localhost.example.com"])])))?;
    assert!(raw.validate().is_ok());
    let bad: ExtensionManifest =
        serde_json::from_value(manifest_json(json!([net(&["intranet"])])))?;
    assert_eq!(
        bad.validate(),
        Err(AppError::InvalidInput("netzwerk-hosts"))
    );
    assert!(ValidatedManifest::try_from(bad).is_err());
    Ok(())
}
