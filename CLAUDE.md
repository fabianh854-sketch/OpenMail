# CLAUDE.md – Arbeitsanleitung für Claude in OpenMail

Diese Datei gilt für jede Sitzung in diesem Repository. Sprache im Projekt: **Deutsch**
(Doku, Kommentare, Commit-Messages).

## Projekt in Kürze

Mail-, Kalender-, Aufgaben- und Chat-Client in Rust, erweiterbar. Stand und Architektur stehen in
`README.md` – das ist die Quelle der Wahrheit; diese Datei wiederholt nur, was zum Arbeiten nötig ist.

| Ordner / Datei | Inhalt | Hinweis |
| --- | --- | --- |
| `crates/openmail-core` | Kern: `types`, `store`, `ext`, `api`, `error`, `tests` | einziges Workspace-Mitglied, getestet |
| `src-tauri` | Desktop-Shell (Tauri v2), Commands in `src/main.rs` | aus dem Workspace ausgeschlossen, ohne `cargo-tauri` nicht baubar |
| `ui/` | Leptos/WASM-Oberfläche | noch nicht angelegt |
| `fluent-mail.html` | Design-Prototyp (~300 KB), nur Referenz | **nie komplett lesen**, gezielt mit Grep suchen |

## Befehle

```bash
cargo test -p openmail-core
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Alle drei müssen vor jedem Commit grün sein.

## Regeln für Code

- **Lints sind Gesetz:** kein `unsafe`, kein `unwrap`/`panic`, `expect` nur begründet. Keine Lints abschalten,
  um Warnungen loszuwerden.
- **Validierung am Rand:** Externe Daten werden beim Parsen in die Typen aus `types.rs` gezwungen
  (`Id`, `Name`, `Text`, `Email`, Enums). Cross-Field-Regeln über `validate()` vor dem Speichern.
  Structs mit `serde(deny_unknown_fields)`.
- **Fehler:** Nach außen geht nur `AppError::public_message()`; interne Details nicht leaken.
- **Speicher:** Sammlungen in `store.rs` sind begrenzt und thread-sicher – neue Sammlungen genauso.
- **Neue Befehle:** zuerst in `Api` (`crates/openmail-core/src/api.rs`), dann als Tauri-Command in
  `src-tauri/src/main.rs`.
- **Vertrag UI ↔ Backend:** snake_case-Befehle, camelCase-Felder, Zeit als `i64` in Millisekunden, Enums
  kleingeschrieben, Argumentname = Parametername.
- **Erweiterungen:** neue Protokolle als Implementierung der Provider-Traits in `ext.rs`, Anmeldung in der
  `Registry`. Plugin-Rechte sind verboten, solange nicht ausdrücklich aufgeführt.
- **Tests:** jede neue Logik bekommt Tests in `crates/openmail-core/src/tests.rs` (auch Fehlerfälle).
- **Doku:** Ändert sich der Stand, die README-Tabelle „Stand“ und „Nächste Schritte“ mitpflegen.

## Arbeitsweise mit Subagents

Arbeite in festen Phasen. Subagents halten den Hauptkontext schlank und arbeiten parallel – nutze sie
bewusst, nicht reflexhaft.

### 1. Verstehen
- Bekannte, wenige Dateien: selbst lesen.
- Unklarer Umfang: **1–3 `Explore`-Agents parallel** (eine Nachricht, mehrere Aufrufe), jeder mit klar
  getrenntem Suchauftrag, z. B. Kern / Tauri-Shell / Prototyp `fluent-mail.html`.

### 2. Planen
- Nicht-triviale Aufgaben (mehrere Dateien, neue Typen, neue Befehle): einen **`Plan`-Agent** mit vollem
  Kontext beauftragen (betroffene Dateien, Regeln von oben, Ziel).
- Aufgabe in Teilschritte zerlegen und mit der Task-Liste verfolgen; jeweils nur ein Schritt „in Arbeit“.
- Bei echten Entscheidungsfragen den Nutzer fragen, statt zu raten.

### 3. Umsetzen
- Der Hauptagent schreibt den Code und behält den Überblick.
- Unabhängige Teilaufgaben (verschiedene Module/Dateien) dürfen an parallele `general-purpose`-Agents
  gehen, bei Bedarf mit `isolation: "worktree"`.
- **Nie zwei Agents an derselben Datei.** Ergebnisse von Subagents vor der Übernahme selbst prüfen.

### 4. Prüfen (immer, parallel)
- **`verifier`** (`.claude/agents/verifier.md`): führt fmt, clippy und Tests aus und meldet Fehler.
- **`core-reviewer`** (`.claude/agents/core-reviewer.md`): prüft die Änderungen gegen die Regeln oben.
- Gefundene Probleme beheben und Schritt 4 wiederholen, bis beide sauber sind.

### 5. Abschließen
- README aktualisieren, falls sich der Stand geändert hat.
- Committen (siehe Git), kurze Zusammenfassung an den Nutzer: was geändert, wie geprüft, was offen ist.

### Prompts an Subagents
Ein Subagent startet ohne Kontext. Jeder Auftrag nennt deshalb:
1. Ziel in einem Satz,
2. betroffene Dateien mit Pfad,
3. relevante Regeln aus dieser Datei,
4. erwartetes Rückgabeformat (z. B. „Liste mit Datei:Zeile und Befund“).

### Wann keine Subagents
Tippfehler, Einzeiler, Umbenennungen, Fragen zu bereits bekannten Dateien – das erledigt der Hauptagent
direkt.

## Git

- Kleine, thematisch geschlossene Commits; Messages auf Deutsch im bestehenden Stil
  (z. B. „Rust-Kern openmail-core: Typen, Speicher, …“).
- Vor dem Commit: alle drei Prüfbefehle grün.
- Keine Geheimnisse einchecken (`.env`, Schlüssel, Tokens).
