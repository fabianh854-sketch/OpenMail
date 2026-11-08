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

Der Kern deckt bisher Kontakte, Adressbücher, Termine, Aufgaben und Nachrichtenfilter ab. Noch offen im Kern:
Mailinglisten, Kalenderverwaltung, Konten, Chat, Einstellungen, Suche, Import/Export
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

Prototyp in der Desktop-Shell starten (Übergang, bis `ui/` existiert; `ui/dist` ist in `.gitignore`):

```bash
mkdir -p ui/dist && cp fluent-mail.html ui/dist/index.html
```

Danach in `src-tauri` mit `cargo build` (bzw. `cargo run`) bauen. `cargo tauri build` würde vorher
`beforeBuildCommand` (trunk) ausführen und ohne `ui/` abbrechen; dann mit
`cargo tauri build --debug --config '{"build":{"beforeBuildCommand":""}}'` aufrufen.
Hinweis: `src-tauri/icons/icon.png` fehlt noch, ohne Icon bricht der Build ab.

Kern prüfen (läuft heute schon):

```bash
cargo test -p openmail-core
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Sicherheitsprüfungen (laufen auch in der CI, `.github/workflows/ci.yml`; lokal einmalig
`cargo install cargo-deny cargo-audit --locked`):

```bash
cargo deny check
cargo audit --deny warnings
```

## Tests

| Ebene | Ordner | Start |
| --- | --- | --- |
| Rust-Kern, Unit-Tests | `crates/openmail-core/src/tests.rs` | `cargo test -p openmail-core` |
| Rust-Kern, Integrationstests (Blackbox über die öffentliche API) | `crates/openmail-core/tests/` | `cargo test -p openmail-core` |
| Vertrag Oberfläche ↔ Backend (statisch, mit Fixtures aus dem Prototyp) | `tests/contract/` | `python3 tests/contract/check_contract.py` |
| Prototyp im Browser (Playwright/Chromium) | `tests/e2e/` | `node tests/e2e/run.cjs` |

Gefundene, noch nicht behobene Fehler sind als `#[ignore = "Bug: …"]` (Rust) bzw. Status `BEKANNT` (E2E)
markiert, damit die Suites grün bleiben. Details in den READMEs der Ordner.

## Sicherheit

- Lints im Workspace: `unsafe_code = "forbid"`, `unwrap_used`/`panic` verboten, Release mit `overflow-checks`.
- Tauri: strikte CSP, keine Plugin-Rechte (`capabilities/default.json` leer).
  Die CSP enthält `'wasm-unsafe-eval'`, weil die Leptos-Oberfläche als WebAssembly läuft. Inline-`<script>`/`<style>`-Blöcke
  gebündelter Seiten gibt Tauri beim Build automatisch in der CSP frei; `style="…"`-Attribute bleiben verboten
  (der Prototyp kommt ohne aus).
- Offen: Command-Allowlist (`AppManifest` in `build.rs` + Rechte in `capabilities/`), damit nicht jedes Skript im
  Fenster alle eigenen Commands aufrufen kann.
- `withGlobalTauri: true` gilt nur für den Prototyp `fluent-mail.html` (er nutzt `window.__TAURI__.core.invoke`). Mit der
  Leptos-Oberfläche, die `invoke` per Import nutzt, wird es wieder auf `false` gestellt. Risiko: jedes Skript im Fenster
  kann Commands aufrufen; abgemildert durch die strikte CSP und leere Plugin-Rechte.
- Plugin-Rechte: Abfragen nur über ein geprüftes `ValidatedManifest`. Netzwerkrechte nur für öffentliche
  DNS-Namen (keine IP-Adressen in irgendeiner Schreibweise, keine lokalen oder internen Endungen wie
  `localhost`, `.local`, `.internal`, `.arpa`). Beim späteren Verbindungsaufbau muss zusätzlich die
  **aufgelöste** IP gegen private, Loopback- und Link-Local-Bereiche geprüft werden (DNS-Rebinding).
- Eingehendes JSON: unbekannte Felder werden überall abgelehnt, auch in getaggten Enums (`Action`, `Capability`).
- CI: `cargo fmt`, `clippy`, Tests, `cargo audit` (RustSec) und `cargo deny` (Lizenzen, Quellen, Bans);
  minimale Workflow-Rechte (`contents: read`).
- Noch einzubauen: Tokens und Passwörter im Betriebssystem-Schlüsselbund (`keyring`), `secrecy::SecretString`,
  Rate-Limits und Body-Limits sobald ein Server existiert.

## Nächste Schritte

1. Toolchain einrichten (oben), dann `ui/` mit Leptos anlegen und den Prototyp Bereich für Bereich nachbauen
   (Seitenleiste, Liste, Lesebereich; Kiesel-Tokens als CSS übernehmen).
2. Kern erweitern: Mailinglisten, Kalender, Konten, Chat, Suche, Einstellungen.
3. SQLite-Speicher (`rusqlite`, gebundene Parameter) hinter `store.rs`.
4. Erste echte Provider: IMAP/SMTP, danach CalDAV/CardDAV und Matrix.
