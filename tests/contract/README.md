# Vertragstest Oberfläche ↔ Backend

Statischer Abgleich zwischen dem Design-Prototyp `fluent-mail.html` und dem Backend
(`src-tauri/src/main.rs`, `crates/openmail-core/src/types.rs`). Nur Python 3 (≥ 3.10),
keine Fremdpakete; der Prototyp wird nicht ausgeführt, nur gelesen.

```bash
python3 tests/contract/check_contract.py        # Bericht, Exit-Code 0/1/2
python3 tests/contract/check_contract.py -v     # zusätzlich alle erfolgreichen Prüfungen
python3 tests/contract/check_contract.py --report-only   # immer Exit-Code 0
```

## Was geprüft wird

| Prüfung | Quelle Prototyp | Quelle Backend |
| --- | --- | --- |
| a) Befehle | `BACKEND_COMMANDS` | `#[tauri::command]`-Funktionen und `generate_handler![…]` |
| b) Argumentnamen | `commit('befehl', { … })`, `backend.invoke('befehl', { … })` | Parameternamen (ohne `State`); Tauri v2 bildet JS-camelCase auf Rust-snake_case ab |
| c) Feldnamen und Werte | Objektliterale mit `id: uid('p'/'e'/'t'/'fl')`, aufgelöste Spreads (`...result` aus `formDialog`-Feldern, `...o` aus den Startdaten, `...c` aus `parseVcf`) | Structs in `types.rs` (`rename_all`, `rename`, `default`, `Option`, `deny_unknown_fields`) |
| c) Enums und Ids | `REPEATS`, `PRIOS`, `FILTER_FIELDS`, `FILTER_OPS`, `FILTER_ACTIONS`, `REMINDERS`, `moveOptions`, `uid()` | `Repeat`, `Prio`, `RuleField`, `RuleOp`, `Action`, `MAX_REMIND_MINUTES`, `Id`-Regeln |
| d) Beispiel-Nutzlasten | `fixtures/*.json` | Nachbildung der Parse- und `validate()`-Regeln aus `types.rs` |

## Exit-Codes

- `0` – keine Widersprüche. Erwartete Lücken stehen trotzdem im Bericht:
  „OFFEN“ (Befehle aus `BACKEND_COMMANDS`, die in `main.rs` noch fehlen), „PROTOTYP-BUG“
  (Fixtures mit `"expect": "reject"`, die Rust zu Recht ablehnt) und „WARNUNG“ (u. a. Konstrukte in
  `types.rs`, die der Test nicht abbildet, z. B. Tupel-Varianten).
- `1` – mindestens ein Widerspruch bei einem **implementierten** Command: falscher Argumentname,
  Feld, das Rust ablehnt, fehlendes Pflichtfeld, Fixture mit `"expect": "accept"`, die Rust ablehnt
  (oder `"reject"`, die Rust annimmt), Command nicht registriert …
- `2` – eine erwartete Stelle wurde nicht gefunden oder lieferte 0 Treffer (keine `commit`-Aufrufe,
  leeres `BACKEND_COMMANDS`, keine erzeugten Kontakt-/Termin-/Aufgaben-/Filter-Objekte, leere
  Auswahlliste, Editorfunktion fehlt …). Dann den Parser anpassen.

## Fixtures

`fixtures/*.json` sind Nutzlasten genau so, wie der Prototyp sie an `backend.invoke` übergibt:

```json
{ "source": "fluent-mail.html:4496 …", "expect": "accept|reject",
  "severity": "hoch|mittel|niedrig (nur bei bekannten Problemen)", "note": "…",
  "command": "save_task", "args": { "task": { … } } }
```

- `"expect": "accept"` – Rust muss die Nutzlast annehmen. Lehnt Rust ab, ist das ein Widerspruch
  (z. B. `save_contact_without_email.json`: Rust liest `""` als „keine Adresse“).
- `"expect": "reject"` – der Prototyp erzeugt die Nutzlast heute, sie ist aber ungültig; Rusts
  Ablehnung ist gewollt, der Fehler liegt im Prototyp. Wird als „PROTOTYP-BUG“ berichtet, nicht als
  Widerspruch (z. B. `save_event_start_cleared.json`, `save_filter_label_without_categories.json`).

`args` lässt sich direkt in Rust-Tests verwenden, z. B.
`serde_json::from_value::<Task>(fixture["args"]["task"].clone())`. Fixtures mit `severity`
und `expect` beschreiben bekannte Probleme; Rust-Tests sollten `expect` übernehmen
(accept → `Ok`, reject → `Err`).

## Grenzen

- Statische Analyse mit regulären Ausdrücken und Klammerabgleich, kein JS-/Rust-Parser. JS-Strings in
  einfachen und doppelten Anführungszeichen werden erkannt, Leerzeichen sind beliebig. In `types.rs`
  werden Kommentare vorher entfernt, `default`/`rename`/`deny_unknown_fields` nur aus `#[serde(…)]`
  gelesen und Felder mit Kommas in Generics (`HashMap<K, V>`) korrekt getrennt. Ändert sich der
  Aufbau stark (z. B. `uid()`, `formDialog`, Namen der Editorfunktionen), meldet das Skript Exit-Code 2
  oder eine Warnung „nicht auflösbar“.
- Werte werden nur geprüft, wo sie im Prototyp als Literal stehen; Werte aus Formularen prüfen die Fixtures.
- Die Nachbildung der Rust-Regeln in Python ist bewusst knapp; maßgeblich bleibt `types.rs`.
