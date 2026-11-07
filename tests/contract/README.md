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

- `0` – nur erwartete Lücken: Befehle aus `BACKEND_COMMANDS`, die in `main.rs` noch fehlen, stehen unter „OFFEN“.
- `1` – mindestens ein Widerspruch bei einem **implementierten** Command (falscher Argumentname,
  Feld, das Rust ablehnt, fehlendes Pflichtfeld, Fixture, die Rust ablehnen würde, Command nicht registriert …).
- `2` – eine erwartete Stelle wurde in den Quellen nicht gefunden (Parser anpassen).

## Fixtures

`fixtures/*.json` sind Nutzlasten genau so, wie der Prototyp sie an `backend.invoke` übergibt:

```json
{ "source": "fluent-mail.html:4496 …", "severity": "hoch|mittel|niedrig (nur bei bekannten Problemen)",
  "command": "save_task", "args": { "task": { … } } }
```

`args` lässt sich direkt in Rust-Tests verwenden, z. B.
`serde_json::from_value::<Task>(fixture["args"]["task"].clone())`. Fixtures mit `severity`
beschreiben Eingaben, die der Prototyp heute erzeugt, Rust aber ablehnt – sie sind Bugs, keine
Negativtests, und lassen den Test fehlschlagen, bis Prototyp oder Kern angepasst sind.

## Grenzen

- Statische Analyse mit regulären Ausdrücken und Klammerabgleich, kein JS-/Rust-Parser. Ändert sich der
  Aufbau stark (z. B. `uid()`, `formDialog`, Namen der Editorfunktionen), meldet das Skript Exit-Code 2
  oder eine Warnung „nicht auflösbar“.
- Werte werden nur geprüft, wo sie im Prototyp als Literal stehen; Werte aus Formularen prüfen die Fixtures.
- Die Nachbildung der Rust-Regeln in Python ist bewusst knapp; maßgeblich bleibt `types.rs`.
