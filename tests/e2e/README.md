# E2E-Tests für den Prototyp `fluent-mail.html`

Browser-Tests mit Playwright (Chromium) gegen den HTML-Prototyp, geladen per `file://`.
Keine Abhängigkeiten im Repository: Playwright und Chromium müssen auf dem Rechner vorhanden sein
(`require('playwright')` muss auflösbar sein).

## Start

```bash
node tests/e2e/run.cjs           # alle Tests
node tests/e2e/run.cjs xss       # nur Tests, deren Name „xss“ enthält
```

Exit-Code 0, solange kein Test den Status `FEHLER` hat. Laufzeit etwa 40 s.

## Status

| Status | Bedeutung |
| --- | --- |
| `ok` | bestanden |
| `FEHLER` | fehlgeschlagen, Exit-Code 1 |
| `BEKANNT` | bekannter Fehler im Prototyp; der Test prüft das *richtige* Verhalten und schlägt erwartungsgemäß fehl. Zählt nicht als Fehlschlag. |
| `BEHOBEN?` | ein als bekannt markierter Test besteht – Fehler behoben, Markierung (`known(...)` → `test(...)`) entfernen |

Screenshots fehlgeschlagener Tests landen in `$E2E_SHOTS` (Standard: `<tmp>/openmail-e2e`), nie im Repository.

## Abdeckung

- Laden ohne Konsolenfehler/Exceptions/externe Requests, offline
- Navigation aller Ordner, Kategorien, gespeicherten Suchen; Einstellungen (alle Abschnitte)
- Mail: lesen, (un)gelesen, Stern, löschen + Rückgängig, verschieben, Papierkorb leeren, Mehrfachauswahl, Tastatur
- Verfassen: leere/ungültige Empfänger, ohne Betreff, Senden, Entwurf
- Suche: Treffer, Hervorhebung, Regex-Sonderzeichen, erweiterte Filter, gespeicherte Suche
- Adressbuch, Kalender, Aufgaben, Chat: im Prototyp als „Coming soon“ ausgeblendet (`SOON`); die Tests blenden
  sie per `SOON.clear()` ein und prüfen sie trotzdem
- Filter anlegen/validieren/ausführen (`runFilters`), Theme hell/dunkel/automatisch, Persistenz (Reload, kaputter
  oder gesperrter `localStorage`)
- XSS in Betreff, Empfänger, Mail-Body, Ordner, Kategorie, Kontakt, Filter, Chat, Termin, Aufgabe, Suche
- Barrierefreiheit (grob): zugängliche Namen, sichtbarer Fokus, Skip-Link, Escape, Fokusfalle, `inert`
- Responsiv 375×812, Backend-Shim (`backend.invoke`, Tauri-Weiterleitung, Fehlerhinweis)
