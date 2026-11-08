//! Adressbücher: Typ (Form des Prototyps) und Befehle `list/save/delete_address_book`.

use openmail_core::types::{AddressBook, Contact, Id};
use openmail_core::{Api, AppError};
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn id(v: &str) -> Result<Id, AppError> {
    Id::try_from(v.to_owned())
}

fn book(id: &str, name: &str) -> Result<AddressBook, serde_json::Error> {
    serde_json::from_value(json!({ "id": id, "name": name }))
}

fn contact(id: &str, book: &str) -> Result<Contact, serde_json::Error> {
    serde_json::from_value(json!({ "id": id, "name": "Lena", "book": book }))
}

#[test]
fn json_roundtrip_in_der_form_des_prototyps() -> TestResult {
    let value = json!({ "id": "personal", "name": "Persönliches Adressbuch" });
    let parsed: AddressBook = serde_json::from_value(value.clone())?;
    assert_eq!(serde_json::to_value(&parsed)?, value);
    Ok(())
}

#[test]
fn unbekannte_felder_und_ungueltige_werte_werden_abgelehnt() {
    assert!(serde_json::from_value::<AddressBook>(
        json!({ "id": "b1", "name": "A", "url": "https://x" })
    )
    .is_err());
    assert!(serde_json::from_value::<AddressBook>(json!({ "id": "b1" })).is_err());
    assert!(serde_json::from_value::<AddressBook>(json!({ "name": "A" })).is_err());
    assert!(serde_json::from_value::<AddressBook>(json!({ "id": "b 1", "name": "A" })).is_err());
    assert!(serde_json::from_value::<AddressBook>(json!({ "id": "b1", "name": "   " })).is_err());
}

#[test]
fn adressbuecher_crud() -> TestResult {
    let api = Api::new();
    assert!(api.list_address_books()?.is_empty());
    let kunden = book("b1", "Kunden")?;
    assert_eq!(api.save_address_book(kunden.clone())?, kunden);
    let umbenannt = book("b1", "Kundschaft")?;
    api.save_address_book(umbenannt.clone())?;
    assert_eq!(api.list_address_books()?, vec![umbenannt]);
    api.delete_address_book(&id("b1")?)?;
    assert!(api.list_address_books()?.is_empty());
    assert!(matches!(
        api.delete_address_book(&id("b1")?),
        Err(AppError::NotFound)
    ));
    Ok(())
}

#[test]
fn loeschen_mit_kontakten_wird_abgelehnt() -> TestResult {
    let api = Api::new();
    api.save_address_book(book("b1", "Kunden")?)?;
    api.save_address_book(book("b2", "Leer")?)?;
    api.save_contact(contact("p1", "b1")?)?;
    assert!(matches!(
        api.delete_address_book(&id("b1")?),
        Err(AppError::InvalidInput("adressbuch nicht leer"))
    ));
    assert_eq!(api.list_address_books()?.len(), 2);
    api.delete_address_book(&id("b2")?)?;
    api.delete_contact(&id("p1")?)?;
    api.delete_address_book(&id("b1")?)?;
    assert!(api.list_address_books()?.is_empty());
    Ok(())
}

#[test]
fn limit_von_100_adressbuechern() -> TestResult {
    let api = Api::new();
    for i in 0..100 {
        api.save_address_book(book(&format!("b{i}"), "Buch")?)?;
    }
    assert!(matches!(
        api.save_address_book(book("b100", "Buch")?),
        Err(AppError::LimitReached(_))
    ));
    api.save_address_book(book("b0", "Umbenannt")?)?;
    Ok(())
}

#[test]
fn kontakt_mit_unbekanntem_buch_wird_gespeichert() -> TestResult {
    let api = Api::new();
    let c = contact("p1", "unbekannt")?;
    assert_eq!(api.save_contact(c.clone())?, c);
    assert_eq!(api.list_contacts()?, vec![c]);
    Ok(())
}

#[test]
fn loeschen_und_speichern_parallel_verwaist_keine_kontakte() -> TestResult {
    for runde in 0..20 {
        let api = std::sync::Arc::new(Api::new());
        api.save_address_book(book("b1", "Kunden")?)?;
        let mut handles = Vec::new();
        for t in 0..4 {
            let api = std::sync::Arc::clone(&api);
            handles.push(std::thread::spawn(move || -> Result<(), String> {
                for i in 0..50 {
                    let c = contact(&format!("p{t}-{i}"), "b1").map_err(|e| e.to_string())?;
                    api.save_contact(c).map_err(|e| e.to_string())?;
                }
                Ok(())
            }));
        }
        let deleter = {
            let api = std::sync::Arc::clone(&api);
            std::thread::spawn(move || -> Result<(), String> {
                let b1 = id("b1").map_err(|e| e.to_string())?;
                for _ in 0..200 {
                    // Fehler (nicht leer / schon gelöscht) sind erlaubt.
                    let _ = api.delete_address_book(&b1);
                }
                Ok(())
            })
        };
        for h in handles {
            h.join().map_err(|_| "thread panic")??;
        }
        deleter.join().map_err(|_| "thread panic")??;
        let buch_da = !api.list_address_books()?.is_empty();
        let verweise = api.list_contacts()?.iter().any(|c| c.book.as_str() == "b1");
        assert!(buch_da || !verweise, "Runde {runde}: verwaiste Kontakte");
    }
    Ok(())
}
