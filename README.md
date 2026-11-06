# OpenMail

Mail-, Kalender-, Aufgaben- und Chat-Client nach dem Vorbild von Thunderbird, mit eigener Oberfläche
(Designsystem „Kiesel“). Ziel: komplett in Rust, erweiterbar.

## Stand

| Teil | Ordner | Stand |
| --- | --- | --- |
| Design-Prototyp (HTML/JS, nur Referenz) | `fluent-mail.html` | läuft im Browser, Layout und Funktionen sind die Vorlage für die Rust-Oberfläche |
| Rust-Kern | `crates/openmail-core` | **fertig und geprüft**: Typen, Validierung, Speicher, Erweiterungspunkte, Befehlsschicht |
| Desktop-Shell (Tauri v2) | `src-tauri` | Gerüst, **noch nicht gebaut** (`cargo-tauri` fehlt) |
| Oberfläche (Leptos, WASM) | `ui` | **noch nicht angelegt** (`trunk` und WASM-Target fehlen) |

Der Kern deckt bisher Kontakte, Termine, Aufgaben und Nachrichtenfilter ab. Noch offen im Kern:
Mailinglisten, Kalender- und Adressbuchverwaltung, Konten, Chat, Einstellungen, Suche, Import/Export
und die echten Protokolle (IMAP, SMTP, CalDAV, CardDAV, Matrix).

## Architektur

```
ui (Leptos/WASM) ──invoke──▶ src-tauri (Commands) ──▶ openmail-core (Api)
                                                          ├─ types   validierte Typen, serde(deny_unknown_fields)
                                                          ├─ store   begrenzte, thread-sichere Sammlungen
                                                          └─ ext     Provider-Traits, Register, Plugin-Rechte
```

- **Eine Befehlsschicht:** `openmail_core::Api` wird von Tauri-Commands genutzt. Später kann eine
  Axum-HTTP-API (Server-Betrieb) dieselbe Schicht verwenden.
- **Validierung am Rand:** Alles Externe wird beim Parsen in `Id`, `Name`, `Text`, `Email` und Enums
  gezwungen. Cross-Field-Regeln (`CalEvent::validate`, `MailFilter::validate`) laufen vor dem Speichern.
- **Fehler:** Nach außen geht nur `AppError::public_message()`; interne Details bleiben im Backend.

### Erweiterbarkeit

- **Provider-Traits** in `ext.rs`: `MailProvider`, `CalendarProvider`, `ContactProvider`, `ChatProtocol`.
  Neue Protokolle sind neue Implementierungen, die sich in der `Registry` anmelden.
- **Plugins** (später, z. B. WASM): beschreiben sich mit einem `ExtensionManifest`. Rechte
  (`ReadMail`, `SendMail`, `Network { hosts }` …) sind **verboten, solange nicht ausdrücklich aufgeführt**;
  Platzhalter-Hosts werden abgelehnt.
- Die Traits sind heute synchron. Sobald echte Netzwerkprotokolle kommen, werden sie async
  (Entscheidung dann: `async-trait` oder eigene Futures).

## Vertrag Oberfläche ↔ Backend

Der HTML-Prototyp spricht nur über `backend.invoke(befehl, args)`. Die Befehlsnamen und Datenformen
entsprechen den Tauri-Commands in `src-tauri/src/main.rs`: snake_case-Befehle, camelCase-Felder,
Zeit als `i64` in Millisekunden, Enums kleingeschrieben, Argumentname = Parametername
(`{ contact }`, `{ event }`, `{ task }`, `{ filter }`, `{ id }`).

## Einrichten

Einmalig (lädt Programme aus dem Internet, bitte bewusst ausführen):

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cargo install tauri-cli --version "^2"
```

Kern prüfen (läuft heute schon):

```bash
cargo test -p openmail-core
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Sicherheit

- Lints im Workspace: `unsafe_code = "forbid"`, `unwrap_used`/`panic` verboten, Release mit `overflow-checks`.
- Tauri: strikte CSP, keine Plugin-Rechte (`capabilities/default.json` leer), kein globales Tauri-Objekt.
  Die CSP enthält `'wasm-unsafe-eval'`, weil die Leptos-Oberfläche als WebAssembly läuft.
- Noch einzubauen: Tokens und Passwörter im Betriebssystem-Schlüsselbund (`keyring`), `secrecy::SecretString`,
  `cargo audit` und `cargo deny` in der CI, Rate-Limits und Body-Limits sobald ein Server existiert.

## Nächste Schritte

1. Toolchain einrichten (oben), dann `ui/` mit Leptos anlegen und den Prototyp Bereich für Bereich nachbauen
   (Seitenleiste, Liste, Lesebereich; Kiesel-Tokens als CSS übernehmen).
2. Kern erweitern: Mailinglisten, Kalender, Adressbücher, Konten, Chat, Suche, Einstellungen.
3. SQLite-Speicher (`rusqlite`, gebundene Parameter) hinter `store.rs`.
4. Erste echte Provider: IMAP/SMTP, danach CalDAV/CardDAV und Matrix.
