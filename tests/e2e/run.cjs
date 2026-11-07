#!/usr/bin/env node
/*
 * E2E-Tests für den Design-Prototyp fluent-mail.html (Playwright + Chromium).
 *
 * Start:   node tests/e2e/run.cjs            (alle Tests)
 *          node tests/e2e/run.cjs xss        (nur Tests, deren Name „xss“ enthält)
 *
 * Status je Test:
 *   ok        – bestanden
 *   FEHLER    – fehlgeschlagen (Exit-Code 1)
 *   BEKANNT   – bekannter Fehler im Prototyp, Test schlägt erwartungsgemäß fehl (zählt nicht als Fehlschlag)
 *   BEHOBEN?  – als bekannt markierter Test besteht inzwischen; Markierung entfernen
 *
 * Screenshots (nur bei FEHLER) landen in $E2E_SHOTS oder im System-Temp-Ordner, nie im Repository.
 */
'use strict';

const path = require('path');
const fs = require('fs');
const os = require('os');
const { chromium } = require('playwright');

const HTML = path.resolve(__dirname, '..', '..', 'fluent-mail.html');
const URL = `file://${HTML}`;
const SOURCE = fs.readFileSync(HTML, 'utf8');
const SHOTS = process.env.E2E_SHOTS || path.join(os.tmpdir(), 'openmail-e2e');
const STORAGE_KEY = 'fluent-mail-state-v1';
const XSS = '<img src=x onerror=window.__xss=1>';

/* ---------------------------------------------------------------- Harness */
const tests = [];
const test = (name, fn, opts = {}) => tests.push({ name, fn, ...opts });
/** Bekannter Fehler: der Test prüft das richtige Verhalten, der Prototyp erfüllt es (noch) nicht. */
const known = (name, bug, fn, opts = {}) => test(name, fn, { ...opts, bug });

class AssertionError extends Error {}
function assert(cond, msg) { if (!cond) throw new AssertionError(msg); }
function eq(actual, expected, msg) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new AssertionError(`${msg}: erwartet ${JSON.stringify(expected)}, erhalten ${JSON.stringify(actual)}`);
}

let browser;

/** Pollt `fn` bis es truthy liefert oder die Frist abläuft; gibt den letzten Wert zurück (wirft nicht). */
async function waitUntil(fn, { timeout = 3000, interval = 50 } = {}) {
  const end = Date.now() + timeout;
  let value;
  for (;;) {
    try { value = await fn(); } catch { value = undefined; }
    if (value || Date.now() >= end) return value;
    await new Promise(r => setTimeout(r, interval));
  }
}
/** Wartet, bis der Browser zwei Frames gerendert hat und anstehende Tasks gelaufen sind. */
const settle = page => page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 0)))));

/** Neue, isolierte Seite (eigener Kontext = leerer localStorage). */
async function open({ viewport = { width: 1400, height: 900 }, colorScheme = 'light', init, storage, wait = true, ctx } = {}) {
  const context = ctx || await browser.newContext({ viewport, colorScheme, acceptDownloads: true });
  const page = await context.newPage();
  page.setDefaultTimeout(5000);
  const issues = { console: [], pageErrors: [], failed: [], external: [] };
  page.on('console', m => { if (m.type() === 'error') issues.console.push(m.text()); });
  page.on('pageerror', e => issues.pageErrors.push(e.message));
  page.on('requestfailed', r => issues.failed.push(`${r.url()} (${r.failure()?.errorText})`));
  page.on('request', r => { if (!/^(file|data|blob|about):/.test(r.url())) issues.external.push(r.url()); });
  page.on('dialog', d => d.dismiss());
  if (init) await page.addInitScript(init);
  if (storage !== undefined) {
    await page.addInitScript(([key, value]) => {
      if (!sessionStorage.getItem('__seeded')) { localStorage.setItem(key, value); sessionStorage.setItem('__seeded', '1'); }
    }, [STORAGE_KEY, storage]);
  }
  await page.goto(URL);
  if (wait) await page.waitForSelector('.mail-item', { timeout: 5000 });
  return { page, context, issues };
}

const noErrors = issues => {
  assert(!issues.pageErrors.length, `Uncaught Exceptions: ${issues.pageErrors.join(' | ')}`);
  assert(!issues.console.length, `Konsolenfehler: ${issues.console.join(' | ')}`);
};

/** Blendet die als „Coming soon“ versteckten Bereiche (Adressbuch, Kalender, Aufgaben, Chat) ein. */
async function enableSpaces(page) {
  await page.evaluate(() => { SOON.clear(); for (const s of SPACES) s.soon = false; render(); });
}
async function gotoSpace(page, id) {
  await enableSpaces(page);
  await page.click(`.space-tab[data-fk="space:${id}"]`);
  await page.waitForFunction(i => state.view.space === i, id);
}

const dialog = page => page.locator('#layer .dialog').last();
async function fillField(page, label, value) {
  await dialog(page).getByLabel(label, { exact: true }).fill(value);
}
async function clickDialogButton(page, name) {
  await dialog(page).locator('.dialog-actions button', { hasText: name }).first().click();
}
const toastText = page => page.locator('#toast-host').innerText();
const xssFired = page => page.evaluate(() => window.__xss !== undefined);

async function openCompose(page) {
  await page.click('#btn-compose');
  await page.waitForSelector('.compose');
}
async function addRecipient(page, text) {
  await page.fill('#rcpt-to', text);
  await page.press('#rcpt-to', 'Enter');
}
const composeSend = page => page.locator('.compose button', { hasText: 'Senden' }).first().click();

/** Zählt sichtbare Buttons ohne zugänglichen Namen (aria-label, aria-labelledby, Text, title). */
const unnamedButtons = page => page.evaluate(() => [...document.querySelectorAll('button, [role="button"], [role="tab"], [role="menuitem"], [role="checkbox"]')]
  .filter(b => b.offsetParent !== null || b.getClientRects().length)
  .filter(b => {
    const lb = b.getAttribute('aria-labelledby');
    const name = (b.getAttribute('aria-label') || (lb && lb.split(' ').map(id => document.getElementById(id)?.textContent || '').join('')) || b.textContent || b.title || '').trim();
    return !name;
  }).map(b => b.outerHTML.slice(0, 120)));

/* =================================================================== Tests */

/* ---- Laden & Offline ---- */
test('laden: keine Konsolenfehler, Exceptions oder fehlgeschlagenen Requests', async () => {
  const { page, issues } = await open();
  await page.waitForLoadState('load');
  await settle(page);
  noErrors(issues);
  eq(issues.failed, [], 'Fehlgeschlagene Requests');
  eq(issues.external, [], 'Externe Requests');
  assert((await page.locator('.mail-item').count()) > 0, 'Nachrichtenliste ist leer');
});

test('laden: Datei referenziert keine externen Ressourcen (offline-fähig)', async () => {
  const ext = [
    ...SOURCE.matchAll(/<(?:script|link|img|iframe)[^>]+(?:src|href)=["']?(https?:)?\/\//gi),
    ...SOURCE.matchAll(/@import|url\(\s*["']?https?:/gi),
  ];
  eq(ext.map(m => m[0]), [], 'Externe Ressourcen');
});

test('laden: funktioniert ohne Netzwerk', async () => {
  const context = await browser.newContext({ offline: true });
  const { page, issues } = await open({ ctx: context });
  noErrors(issues);
  assert((await page.locator('.mail-item').count()) > 0, 'Keine Nachrichten offline');
});

/* ---- Bereiche ---- */
test('bereiche: alle Systemordner, eigene Ordner, Kategorien und gespeicherte Suchen sind erreichbar', async () => {
  const { page, issues } = await open();
  const rows = await page.locator('#nav .nav-item[data-action="folder"]').evaluateAll(els => els.map(e => e.dataset.folder));
  assert(rows.length >= 9, `Zu wenige Navigationseinträge: ${rows.length}`);
  for (const folder of rows) {
    await page.click(`#nav .nav-item[data-folder="${folder}"]`);
    const view = await page.evaluate(() => state.view.folder);
    eq(view, folder, `Ordner ${folder} nicht aktiv`);
    const title = await page.textContent('#list-title');
    assert(title.trim().length > 0, `Kein Titel für ${folder}`);
  }
  noErrors(issues);
});

test('bereiche: Adressbuch/Kalender/Aufgaben/Chat sind als „Coming soon“ ausgeblendet', async () => {
  const { page, issues } = await open();
  eq(await page.locator('.space-tab').count(), 0, 'Bereichsumschalter sichtbar');
  await page.evaluate(() => setSpace('calendar'));
  eq(await page.evaluate(() => state.view.space), 'mail', 'Ausgeblendeter Bereich wurde geöffnet');
  noErrors(issues);
});

test('bereiche: eingeblendete Bereiche rendern ohne Fehler (Adressbuch, Kalender, Aufgaben, Chat)', async () => {
  const { page, issues } = await open();
  await enableSpaces(page);
  eq(await page.locator('.space-tab').count(), 5, 'Anzahl Bereichs-Tabs');
  for (const id of ['contacts', 'calendar', 'tasks', 'chat', 'mail']) {
    await page.click(`.space-tab[data-fk="space:${id}"]`);
    const shown = await page.evaluate(i => ({ space: state.view.space, hidden: document.querySelector('#space-view').hidden, kids: document.querySelector('#space-view').childElementCount }), id);
    eq(shown.space, id, 'aktiver Bereich');
    if (id !== 'mail') assert(!shown.hidden && shown.kids > 0, `Bereich ${id} leer`);
  }
  noErrors(issues);
});

test('bereiche: Kalender-Ansichten (Tag/Woche/Monat) und Blättern', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'calendar');
  for (const view of ['day', 'week', 'month']) {
    await page.evaluate(v => { sp.cal.view = v; render(); }, view);
    for (const label of ['Weiter', 'Zurück']) {
      const b = page.locator(`#space-view button[aria-label="${label}"]`);
      if (await b.count()) await b.first().click();
    }
  }
  noErrors(issues);
});

test('bereiche: Einstellungen – jeder Abschnitt lässt sich öffnen', async () => {
  const { page, issues } = await open();
  await page.click('#btn-settings');
  const sections = await page.locator('#layer .settings-nav button').allInnerTexts();
  assert(sections.length > 10, `Zu wenige Abschnitte: ${sections.length}`);
  for (const name of sections) {
    await page.locator('#layer .settings-nav button', { hasText: name }).first().click();
    eq((await page.textContent('#layer .set-heading')).trim(), name.trim(), 'Überschrift des Abschnitts');
  }
  noErrors(issues);
});

/* ---- Mail ---- */
test('mail: Nachricht öffnen markiert sie als gelesen und zeigt den Lesebereich', async () => {
  const { page, issues } = await open();
  const row = page.locator('.mail-item[data-unread="true"]').first();
  const id = await row.getAttribute('data-id');
  const before = await page.evaluate(() => derived.counts.unread.inbox);
  await row.click();
  await page.waitForSelector('#reader .msg');
  const after = await page.evaluate(() => derived.counts.unread.inbox);
  eq(after, before - 1, 'Ungelesen-Zähler');
  eq(await page.evaluate(i => findMail(i).read, id), true, 'gelesen');
  noErrors(issues);
});

test('mail: als ungelesen markieren und Stern setzen', async () => {
  const { page, issues } = await open();
  await page.locator('.mail-item').first().click();
  await page.click('#reader [data-action="toggle-read"]');
  eq(await page.evaluate(() => findMail(ui.selectedId).read), false, 'wieder ungelesen');
  await page.click('#reader [data-action="star-mail"] >> nth=0');
  eq(await page.evaluate(() => state.mails.some(m => m.starred && m.id === document.querySelector('#reader [data-action="star-mail"]').dataset.id)), true, 'Stern');
  await page.click('#nav .nav-item[data-folder="starred"]');
  assert((await page.locator('.mail-item').count()) > 0, 'Ordner „Markiert“ leer');
  noErrors(issues);
});

test('mail: löschen verschiebt in den Papierkorb, „Rückgängig“ stellt wieder her', async () => {
  const { page, issues } = await open();
  await page.locator('.mail-item').first().click();
  const id = await page.evaluate(() => ui.selectedId);
  await page.click('#reader [data-action="delete"]');
  eq(await page.evaluate(i => findMail(i).folder, id), 'trash', 'Ordner nach Löschen');
  assert((await toastText(page)).includes('gelöscht'), 'Kein Hinweis');
  await page.locator('#toast-host button', { hasText: 'Rückgängig' }).click();
  eq(await page.evaluate(i => findMail(i).folder, id), 'inbox', 'Ordner nach Rückgängig');
  noErrors(issues);
});

test('mail: verschieben über das Menü (Archiv) und Papierkorb leeren', async () => {
  const { page, issues } = await open();
  await page.locator('.mail-item').first().click();
  const id = await page.evaluate(() => ui.selectedId);
  await page.click('#reader [data-action="move-menu"]');
  await page.locator('#layer [role^="menuitem"]', { hasText: 'Archiv' }).first().click();
  eq(await page.evaluate(i => findMail(i).folder, id), 'archive', 'Ordner nach Verschieben');

  await page.click('#nav .nav-item[data-folder="inbox"]');
  await page.locator('.mail-item').first().click();
  await page.click('#reader [data-action="delete"]');
  await page.click('#nav .nav-item[data-folder="trash"]');
  assert((await page.locator('.mail-item').count()) > 0, 'Papierkorb leer');
  await page.click('#btn-empty-trash');
  await dialog(page).locator('.dialog-actions .btn--danger, .dialog-actions .btn--accent').first().click();
  eq(await page.evaluate(() => state.mails.filter(m => m.folder === 'trash' && m.account === state.activeAccount).length), 0, 'Papierkorb nicht geleert');
  noErrors(issues);
});

test('mail: Mehrfachauswahl und Tastatur (Pfeil, Leertaste, Entf)', async () => {
  const { page, issues } = await open();
  const first = page.locator('.mail-item').first();
  await first.focus();
  await page.keyboard.press(' ');
  await page.keyboard.press('ArrowDown'); // ohne Auswahl: erste Zeile wird aktiv
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press(' ');
  eq(await page.evaluate(() => ui.checked.size), 2, 'Ausgewählte Nachrichten');
  assert(await page.isVisible('#selection-bar'), 'Auswahlleiste nicht sichtbar');
  const before = await page.evaluate(() => state.mails.filter(m => m.folder === 'trash').length);
  await page.keyboard.press('Delete');
  const after = await page.evaluate(() => state.mails.filter(m => m.folder === 'trash').length);
  assert(after > before, 'Entf hat nichts gelöscht');
  noErrors(issues);
});

known('mail: Pfeil-nach-unten geht von der fokussierten Zeile aus',
  'moveSelection ignoriert den Tastaturfokus: ohne geöffnete Nachricht springt ↓ immer auf die erste Zeile statt auf die nächste',
  async () => {
    const { page } = await open();
    const ids = await page.locator('.mail-item').evaluateAll(els => els.map(e => e.dataset.id));
    await page.locator('.mail-item').nth(2).focus();
    await page.keyboard.press('ArrowDown');
    eq(await page.evaluate(() => document.activeElement.dataset.id), ids[3], 'Fokus nach ↓');
  });

test('verfassen: ohne Empfänger wird nicht gesendet', async () => {
  const { page, issues } = await open();
  const sent = await page.evaluate(() => state.mails.filter(m => m.folder === 'sent').length);
  await openCompose(page);
  await page.fill('#compose-subject', 'Test');
  await composeSend(page);
  assert((await toastText(page)).includes('mindestens einen Empfänger'), 'Kein Hinweis auf fehlenden Empfänger');
  eq(await page.evaluate(() => state.mails.filter(m => m.folder === 'sent').length), sent, 'Gesendet-Zähler');
  assert(await page.isVisible('.compose'), 'Fenster wurde geschlossen');
  noErrors(issues);
});

test('verfassen: ungültige Adresse wird abgelehnt', async () => {
  const { page, issues } = await open();
  await openCompose(page);
  await addRecipient(page, 'kein-at-zeichen');
  await page.fill('#compose-subject', 'Test');
  await composeSend(page);
  assert((await toastText(page)).includes('Ungültige E-Mail-Adresse'), 'Kein Hinweis auf ungültige Adresse');
  assert(await page.isVisible('.compose .rcpt-chip.is-invalid'), 'Ungültiger Empfänger nicht markiert');
  noErrors(issues);
});

test('verfassen: ohne Betreff fragt nach, gültige Nachricht landet in „Gesendet“', async () => {
  const { page, issues } = await open();
  await openCompose(page);
  await addRecipient(page, 'max@example.org');
  await page.click('.compose .editor');
  await page.keyboard.type('Hallo Max');
  await composeSend(page);
  await page.waitForSelector('#layer .dialog');
  assert((await dialog(page).innerText()).includes('Ohne Betreff'), 'Keine Rückfrage zum Betreff');
  await clickDialogButton(page, 'Senden');
  await page.waitForSelector('.compose', { state: 'detached' });
  const sent = await page.evaluate(() => state.mails.filter(m => m.folder === 'sent').at(-1));
  eq(sent.subject, '(Kein Betreff)', 'Betreff');
  eq(sent.to.map(t => t.email), ['max@example.org'], 'Empfänger');
  assert(sent.body.includes('Hallo Max'), 'Text fehlt');
  noErrors(issues);
});

test('verfassen: Schließen speichert einen Entwurf', async () => {
  const { page, issues } = await open();
  const drafts = await page.evaluate(() => state.mails.filter(m => m.folder === 'drafts').length);
  await openCompose(page);
  await page.fill('#compose-subject', 'Entwurf E2E');
  await page.click('.compose button[aria-label^="Schließen"]');
  eq(await page.evaluate(() => state.mails.filter(m => m.folder === 'drafts').length), drafts + 1, 'Anzahl Entwürfe');
  noErrors(issues);
});

test('verfassen: Escape im Verfassen-Fenster lässt den Entwurf offen', async () => {
  const { page, issues } = await open();
  await openCompose(page);
  await page.fill('#compose-subject', 'bleibt');
  await page.press('#compose-subject', 'Escape');
  assert(await page.isVisible('.compose'), 'Verfassen-Fenster durch Escape geschlossen (Datenverlust)');
  noErrors(issues);
});

test('suche: filtert die Liste, hebt Treffer hervor, Escape leert', async () => {
  const { page, issues } = await open();
  const all = await page.locator('.mail-item').count();
  await page.fill('#search', 'Atlas');
  const hits = await page.locator('.mail-item').count();
  assert(hits > 0 && hits < all, `Treffer ${hits} von ${all}`);
  assert((await page.locator('.mail-item mark').count()) > 0, 'Keine Hervorhebung');
  await page.fill('#search', 'zzzz-kein-treffer');
  eq(await page.locator('.mail-item').count(), 0, 'Treffer bei Unsinn');
  assert(await page.isVisible('#list-empty'), 'Kein Leerzustand');
  await page.press('#search', 'Escape');
  eq(await page.inputValue('#search'), '', 'Suche nicht geleert');
  eq(await page.locator('.mail-item').count(), all, 'Liste nicht wiederhergestellt');
  noErrors(issues);
});

test('suche: Sonderzeichen (Regex) brechen nichts', async () => {
  const { page, issues } = await open();
  for (const q of ['(', '[a-', '\\', '*+?', '$^', '.*']) await page.fill('#search', q);
  noErrors(issues);
});

test('suche: erweiterte Filter und Suche speichern', async () => {
  const { page, issues } = await open();
  await page.click('#btn-adv');
  await page.check('#adv-attach');
  const n = await page.locator('.mail-item').count();
  assert(n > 0, 'Keine Nachrichten mit Anhang');
  await page.click('[data-action="save-search"]');
  await fillField(page, 'Name der Suche', 'Mit Anhang E2E');
  await clickDialogButton(page, 'Speichern');
  assert(await page.evaluate(() => state.savedSearches.some(s => s.name === 'Mit Anhang E2E')), 'Suche nicht gespeichert');
  noErrors(issues);
});

test('tastatur: „/“ fokussiert die Suche, „c“ öffnet Verfassen, „?“ zeigt Kürzel', async () => {
  const { page, issues } = await open();
  await page.locator('body').click({ position: { x: 5, y: 5 } }).catch(() => {});
  await page.evaluate(() => document.activeElement?.blur());
  await page.keyboard.press('/');
  eq(await page.evaluate(() => document.activeElement.id), 'search', 'Fokus');
  await page.evaluate(() => document.activeElement.blur());
  await page.keyboard.press('?');
  await page.waitForSelector('#layer .dialog');
  await page.keyboard.press('Escape');
  await page.keyboard.press('c');
  await page.waitForSelector('.compose');
  noErrors(issues);
});

/* ---- Kontakte, Termine, Aufgaben (versteckte Bereiche, eingeblendet) ---- */
test('kontakte: anlegen, bearbeiten, löschen', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'contacts');
  const count = await page.evaluate(() => state.people.length);
  await page.evaluate(() => { editPerson(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Name', 'Erika Muster');
  await fillField(page, 'E-Mail', 'erika@example.org');
  await clickDialogButton(page, 'Speichern');
  eq(await page.evaluate(() => state.people.length), count + 1, 'Anzahl Kontakte');
  assert((await page.locator('#space-view').innerText()).includes('Erika Muster'), 'Kontakt nicht angezeigt');

  await page.evaluate(() => { editPerson(state.people.find(p => p.name === 'Erika Muster')); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Name', 'Erika Beispiel');
  await clickDialogButton(page, 'Speichern');
  assert(await page.evaluate(() => state.people.some(p => p.name === 'Erika Beispiel')), 'Nicht umbenannt');

  await page.evaluate(() => { editPerson(state.people.find(p => p.name === 'Erika Beispiel')); });
  await page.waitForSelector('#layer .dialog');
  await clickDialogButton(page, 'Löschen');
  await clickDialogButton(page, 'Löschen');
  eq(await page.evaluate(() => state.people.length), count, 'Anzahl nach Löschen');
  noErrors(issues);
});

test('kontakte: ungültige E-Mail und leerer Name werden abgelehnt', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'contacts');
  await page.evaluate(() => { editPerson(); });
  await page.waitForSelector('#layer .dialog');
  await clickDialogButton(page, 'Speichern');
  assert((await dialog(page).innerText()).includes('Bitte „Name“ ausfüllen'), 'Pflichtfeld nicht geprüft');
  await fillField(page, 'Name', 'X');
  await fillField(page, 'E-Mail', 'falsch');
  await clickDialogButton(page, 'Speichern');
  assert((await dialog(page).innerText()).includes('gültige E-Mail'), 'E-Mail nicht geprüft');
  noErrors(issues);
});

test('termine: Ende vor Beginn wird abgelehnt', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'calendar');
  const count = await page.evaluate(() => state.events.length);
  await page.evaluate(() => { editEvent(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Titel', 'Rückwärts');
  await fillField(page, 'Beginn', '2026-11-10T12:00');
  await fillField(page, 'Ende', '2026-11-10T10:00');
  await clickDialogButton(page, 'Speichern');
  assert((await dialog(page).innerText()).includes('Das Ende liegt vor dem Beginn'), 'Keine Fehlermeldung');
  eq(await page.evaluate(() => state.events.length), count, 'Termin trotzdem gespeichert');
  noErrors(issues);
});

test('termine: gültigen Termin anlegen erscheint im Kalender', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'calendar');
  await page.evaluate(() => { editEvent(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Titel', 'E2E-Termin');
  await clickDialogButton(page, 'Speichern');
  const ev = await page.evaluate(() => state.events.find(e => e.title === 'E2E-Termin'));
  assert(ev && ev.end >= ev.start && typeof ev.start === 'number', 'Termin fehlt oder ungültig');
  await page.evaluate(() => { sp.cal.view = 'week'; render(); });
  assert((await page.locator('#space-view').innerText()).includes('E2E-Termin'), 'Termin nicht sichtbar');
  noErrors(issues);
});

known('termine: leerer Beginn/Ende wird abgelehnt',
  'Termin ohne Beginn/Ende wird mit start=null gespeichert (fromInput("") → null, Validierung null < null ist false)',
  async () => {
    const { page } = await open();
    await gotoSpace(page, 'calendar');
    const count = await page.evaluate(() => state.events.length);
    await page.evaluate(() => { editEvent(); });
    await page.waitForSelector('#layer .dialog');
    await fillField(page, 'Titel', 'Ohne Zeit');
    await fillField(page, 'Beginn', '');
    await fillField(page, 'Ende', '');
    await clickDialogButton(page, 'Speichern');
    eq(await page.evaluate(() => state.events.length), count, 'Termin ohne Zeit gespeichert');
  });

test('aufgaben: anlegen und erledigen', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'tasks');
  await page.evaluate(() => { editTask(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Titel', 'E2E-Aufgabe');
  await clickDialogButton(page, 'Speichern');
  assert(await page.evaluate(() => state.tasks.some(t => t.title === 'E2E-Aufgabe' && !t.done)), 'Aufgabe fehlt');
  assert((await page.locator('#space-view').innerText()).includes('E2E-Aufgabe'), 'Aufgabe nicht in Liste');
  await page.evaluate(() => toggleTask(state.tasks.find(t => t.title === 'E2E-Aufgabe')));
  eq(await page.evaluate(() => state.tasks.find(t => t.title === 'E2E-Aufgabe').done), true, 'nicht erledigt');
  assert(!(await page.locator('#space-view').innerText()).includes('E2E-Aufgabe'), 'Erledigte Aufgabe noch unter „Offen“');
  noErrors(issues);
});

test('chat: Nachricht senden', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'chat');
  await page.locator('#space-view .contact-item').first().click();
  const input = page.locator('#space-view textarea[aria-label="Nachricht schreiben"]');
  await input.fill('Hallo aus dem Test');
  await input.press('Enter');
  assert((await page.locator('#space-view .chat-log').innerText()).includes('Hallo aus dem Test'), 'Nachricht nicht im Verlauf');
  noErrors(issues);
});

known('chat: Raumnamen werden nicht doppelt mit „#“ angezeigt',
  'Demo-Raum r3 heißt „#open-source“ und wird als „# #open-source“ angezeigt (joinRoom entfernt das #, die Demo-Daten nicht)',
  async () => {
    const { page } = await open();
    await gotoSpace(page, 'chat');
    assert(!(await page.locator('#space-view').innerText()).includes('# #'), '„# #“ in der Raumliste');
  });

/* ---- Filter ---- */
async function createFilterViaUi(page, { name, value, op = 'contains' }) {
  await page.click('#btn-settings');
  await page.locator('#layer .settings-nav button', { hasText: 'Nachrichtenfilter' }).click();
  await page.locator('#layer button', { hasText: 'Nachrichtenfilter verwalten' }).click();
  await page.locator('#layer button', { hasText: 'Neuer Filter' }).click();
  const d = dialog(page);
  await d.getByLabel('Name des Filters').fill(name);
  await d.getByLabel('Bedingung', { exact: true }).first().selectOption(op);
  await d.getByLabel('Wert', { exact: true }).first().fill(value);
  await clickDialogButton(page, 'Speichern');
}

test('filter: Validierung (Name und Wert sind Pflicht)', async () => {
  const { page, issues } = await open();
  await page.click('#btn-settings');
  await page.locator('#layer .settings-nav button', { hasText: 'Nachrichtenfilter' }).click();
  await page.locator('#layer button', { hasText: 'Nachrichtenfilter verwalten' }).click();
  await page.locator('#layer button', { hasText: 'Neuer Filter' }).click();
  await clickDialogButton(page, 'Speichern');
  assert((await dialog(page).innerText()).includes('Bitte einen Namen eingeben'), 'Name nicht geprüft');
  await dialog(page).getByLabel('Name des Filters').fill('F');
  await clickDialogButton(page, 'Speichern');
  assert((await dialog(page).innerText()).includes('Jede Bedingung braucht einen Wert'), 'Wert nicht geprüft');
  noErrors(issues);
});

test('filter: anlegen und ausführen (runFilters) verschiebt passende Nachrichten', async () => {
  const { page, issues } = await open();
  await createFilterViaUi(page, { name: 'Lena archivieren', value: 'lena.hoffmann@' });
  assert(await page.evaluate(() => state.filters.some(f => f.name === 'Lena archivieren')), 'Filter nicht gespeichert');
  const targets = await page.evaluate(() => state.mails.filter(m => m.account === state.activeAccount && m.folder === 'inbox' && m.from.email.startsWith('lena.hoffmann@')).map(m => m.id));
  assert(targets.length > 0, 'Keine passenden Testnachrichten');
  await page.locator('#layer button', { hasText: 'Filter jetzt ausführen' }).click();
  const folders = await page.evaluate(ids => ids.map(i => findMail(i).folder), targets);
  assert(folders.every(f => f === 'archive'), `Nicht archiviert: ${folders}`);
  assert((await toastText(page)).includes('gefiltert'), 'Kein Hinweis');
  noErrors(issues);
});

known('filter: Operator „ist“ vergleicht den ganzen Wert, nicht einzelne Wörter',
  'ruleMatches: op "is" trifft auch, wenn nur ein Wort gleich ist (z. B. „ist: lena“ trifft „Lena Hoffmann lena.hoffmann@…“)',
  async () => {
    const { page } = await open();
    const hit = await page.evaluate(() => ruleMatches({ from: { name: 'Lena Hoffmann', email: 'lena.hoffmann@contoso.com' } }, { field: 'from', op: 'is', value: 'lena' }));
    eq(hit, false, '„ist lena“ trifft „Lena Hoffmann“');
  });

known('filter: Löschen eines Zielordners lässt keine Filter auf den gelöschten Ordner zurück',
  'deleteFolder bereinigt state.filters nicht; runFilters verschiebt Nachrichten danach in die nicht mehr existierende Ordner-ID – sie verschwinden aus allen Ansichten',
  async () => {
    const { page } = await open();
    const folderId = await page.evaluate(() => {
      const f = customFolders()[0];
      state.filters.push({ id: 'flE2E', name: 'Nach Kunden', on: true, match: 'any', rules: [{ field: 'subject', op: 'contains', value: 'atlas' }], actions: [{ type: 'move', value: f.id }] });
      persist();
      deleteFolder(f.id);
      return f.id;
    });
    await clickDialogButton(page, 'Löschen');
    await page.evaluate(() => runFilters());
    const lost = await page.evaluate(() => state.mails.filter(m => !SYSTEM_FOLDERS.some(f => f.id === m.folder) && !state.folders.some(f => f.id === m.folder)).length);
    eq(lost, 0, `Nachrichten in gelöschtem Ordner ${folderId}`);
  });

/* ---- Einstellungen / Theme ---- */
test('theme: Dunkel/Hell/Automatisch wirkt und bleibt nach Neuladen', async () => {
  const { page, issues } = await open({ colorScheme: 'light' });
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const light = await bg();
  await page.click('#btn-settings');
  await page.locator('#layer button', { hasText: 'Dunkel' }).click();
  eq(await page.evaluate(() => document.documentElement.dataset.theme), 'dark', 'data-theme');
  const dark = await bg();
  assert(dark !== light, `Hintergrund unverändert (${dark})`);
  await page.reload();
  await page.waitForSelector('.mail-item');
  eq(await page.evaluate(() => document.documentElement.dataset.theme), 'dark', 'Theme nach Reload');
  await page.click('#btn-settings');
  await page.locator('#layer button', { hasText: 'Hell' }).click();
  eq(await bg(), light, 'Hell');
  await page.locator('#layer button', { hasText: 'Automatisch' }).click();
  eq(await page.evaluate(() => document.documentElement.dataset.theme), undefined, 'Automatisch entfernt data-theme');
  await page.emulateMedia({ colorScheme: 'dark' });
  eq(await bg(), dark, 'Automatisch folgt dem System (dunkel)');
  noErrors(issues);
});

test('einstellungen: Dichte und Lesebereich-Position werden übernommen', async () => {
  const { page, issues } = await open();
  await page.click('#btn-settings');
  await page.locator('#layer button', { hasText: 'Kompakt' }).click();
  eq(await page.evaluate(() => state.settings.density), 'compact', 'Dichte');
  await page.locator('#layer .settings-nav button', { hasText: 'Lesen und Anzeige' }).click();
  await page.locator('#layer button', { hasText: 'Unten' }).click();
  eq(await page.evaluate(() => state.settings.pane), 'bottom', 'Lesebereich');
  noErrors(issues);
});

/* ---- Persistenz ---- */
test('persistenz: Löschen, Stern, Filter und Ordner überleben einen Reload', async () => {
  const { page, issues } = await open();
  await page.locator('.mail-item').first().click();
  const id = await page.evaluate(() => ui.selectedId);
  await page.click('#reader [data-action="star-mail"] >> nth=0');
  const starredId = await page.locator('#reader [data-action="star-mail"]').first().getAttribute('data-id');
  await page.click('#reader [data-action="delete"]');
  await page.evaluate(() => { state.folders.push({ id: 'fE2E', name: 'E2E-Ordner', account: state.activeAccount }); persist(); });
  await createFilterViaUi(page, { name: 'Bleibt', value: 'x' });
  await page.reload();
  await page.waitForSelector('.mail-item');
  eq(await page.evaluate(i => findMail(i).folder, id), 'trash', 'Ordner nach Reload');
  eq(await page.evaluate(i => findMail(i).starred, starredId), true, 'Stern nach Reload');
  assert(await page.evaluate(() => state.filters.some(f => f.name === 'Bleibt')), 'Filter nach Reload');
  assert((await page.locator('#nav').innerText()).includes('E2E-Ordner'), 'Ordner nach Reload');
  noErrors(issues);
});

test('persistenz: kaputtes JSON im localStorage → Neustart mit Demo-Daten', async () => {
  const { page, issues } = await open({ storage: '{kaputt' });
  assert((await page.locator('.mail-item').count()) > 0, 'Keine Demo-Daten');
  noErrors(issues);
});

test('persistenz: falsche Typen (mails kein Array) → Neustart mit Demo-Daten', async () => {
  const { page, issues } = await open({ storage: JSON.stringify({ mails: 'x', accounts: [] }) });
  assert((await page.locator('.mail-item').count()) > 0, 'Keine Demo-Daten');
  noErrors(issues);
});

test('persistenz: localStorage gesperrt (wirft) → App läuft im Arbeitsspeicher', async () => {
  const { page, issues } = await open({
    init: () => {
      Storage.prototype.getItem = () => { throw new DOMException('blocked', 'SecurityError'); };
      Storage.prototype.setItem = () => { throw new DOMException('blocked', 'SecurityError'); };
    },
  });
  assert((await page.locator('.mail-item').count()) > 0, 'Keine Nachrichten');
  noErrors(issues);
});

known('persistenz: manipulierte Einträge (Nachricht ohne Felder) bringen die App nicht zum Absturz',
  'loadState prüft nur, ob mails/accounts Arrays sind; ein Eintrag wie {} führt beim Rendern zu einer Exception und leerer Oberfläche',
  async () => {
    const { page, issues } = await open({ storage: JSON.stringify({ mails: [{}], accounts: [{}] }), wait: false });
    await waitUntil(async () => issues.pageErrors.length || (await page.locator('#nav .nav-item').count()));
    await settle(page);
    eq(issues.pageErrors, [], 'Uncaught Exceptions');
    assert((await page.locator('#nav .nav-item').count()) > 0, 'Navigation leer');
  });

known('persistenz: Zustand mit fehlenden Sammlungen (folders/labels) bringt die App nicht zum Absturz',
  'loadState übernimmt einen Zustand ohne folders/labels/contacts; customFolders()/labelById() werfen dann',
  async () => {
    const minimal = { mails: [], accounts: [{ id: 'a1', name: 'A', email: 'a@example.org', label: 'A', ooo: { enabled: false } }], activeAccount: 'a1' };
    const { page, issues } = await open({ storage: JSON.stringify(minimal), wait: false });
    await waitUntil(async () => issues.pageErrors.length || (await page.locator('#nav .nav-item').count()));
    await settle(page);
    eq(issues.pageErrors, [], 'Uncaught Exceptions');
  });

/* ---- Sicherheit (XSS) ---- */
test('xss: statisch – innerHTML nur für Icons, Sanitizer-Ausgabe und eigene Elemente', async () => {
  const assigns = [...SOURCE.matchAll(/\.innerHTML\s*=(?!=)\s*([^;\n]+)/g)].map(m => m[1].trim());
  eq(assigns, ['ICONS[name]'], 'innerHTML-Zuweisungen');
  assert(!/insertAdjacentHTML|document\.write|\beval\(|new Function/.test(SOURCE), 'Gefährliche Senke gefunden');
});

test('xss: Betreff und Absendername einer gesendeten Nachricht werden als Text angezeigt', async () => {
  const { page, issues } = await open();
  const other = await page.evaluate(() => state.accounts.find(a => a.id !== state.activeAccount)?.email);
  assert(other, 'Kein zweites Konto für Selbstversand');
  await openCompose(page);
  await addRecipient(page, `${XSS} <${other}>`);
  await page.fill('#compose-subject', XSS);
  await page.click('.compose .editor');
  await page.keyboard.type(XSS);
  await composeSend(page);
  await page.click('#nav .nav-item[data-folder="sent"]');
  await page.locator('.mail-item').first().click();
  await page.waitForSelector('#reader .msg');
  assert((await page.locator('.mail-item .mi-subject').first().innerText()).includes('<img'), 'Betreff nicht als Text');
  assert((await page.locator('#reader').innerText()).includes('onerror'), 'Text nicht sichtbar');
  eq(await page.locator('#app img').count(), 0, 'img-Element erzeugt');
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: bösartiger Mail-Body (img/script/svg/javascript:-Link/Event-Attribute) wird bereinigt', async () => {
  const { page, issues } = await open();
  await page.evaluate(() => {
    const base = state.mails.find(m => m.folder === 'inbox' && m.account === state.activeAccount);
    state.mails.push({ ...structuredClone(base), id: 'evil', subject: 'Böse', date: Date.now() + 60e3, read: false,
      body: '<p onclick="window.__xss=1">x</p><img src=x onerror="window.__xss=1"><script>window.__xss=1<\/script><svg onload="window.__xss=1"></svg>'
        + '<a href="javascript:window.__xss=1">klick</a><a href=" JaVaScRiPt:alert(1)">b</a><iframe src="javascript:window.__xss=1"></iframe>'
        + '<div style="background:url(javascript:1)">s</div><math><mtext><table><mglyph><style><img src=x onerror=window.__xss=1>' });
    persist();
    render();
  });
  await page.click('.mail-item[data-id="evil"]');
  await page.waitForSelector('#reader .mail-body');
  await settle(page);
  const info = await page.evaluate(() => {
    const body = [...document.querySelectorAll('#reader .mail-body')].pop();
    return {
      bad: [...body.querySelectorAll('*')].filter(e => [...e.attributes].some(a => /^on|^style$|^src$/i.test(a.name)) || /^(IMG|SCRIPT|SVG|IFRAME|MATH|STYLE)$/i.test(e.tagName)).map(e => e.outerHTML),
      hrefs: [...body.querySelectorAll('a')].map(a => a.getAttribute('href')),
    };
  });
  eq(info.bad, [], 'Gefährliche Elemente/Attribute');
  assert(info.hrefs.every(h => h === null), `javascript:-Links übernommen: ${info.hrefs}`);
  await page.locator('#reader a', { hasText: 'klick' }).click().catch(() => {});
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Ordner- und Kategoriename', async () => {
  const { page, issues } = await open();
  await page.evaluate(() => { createFolder(); });
  await page.waitForSelector('#layer .dialog');
  await dialog(page).locator('input').fill(XSS);
  await clickDialogButton(page, 'Erstellen').catch(() => clickDialogButton(page, 'OK'));
  await page.evaluate(() => { createLabel(); });
  await page.waitForSelector('#layer .dialog');
  await dialog(page).locator('input').first().fill(`${XSS}2`);
  await dialog(page).locator('.dialog-actions .btn--accent').click();
  const nav = await page.locator('#nav').innerText();
  assert(nav.split(XSS).length >= 3, 'Namen nicht als Text in der Navigation');
  eq(await page.locator('#nav img').count(), 0, 'img in Navigation');
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Kontaktname im Adressbuch', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'contacts');
  await page.evaluate(() => { editPerson(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Name', XSS);
  await fillField(page, 'Notiz', XSS);
  await clickDialogButton(page, 'Speichern');
  assert((await page.locator('#space-view').innerText()).includes(XSS), 'Name nicht als Text');
  eq(await page.locator('#space-view img').count(), 0, 'img erzeugt');
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Filtername und Filterwert', async () => {
  const { page, issues } = await open();
  await createFilterViaUi(page, { name: XSS, value: XSS });
  const text = await dialog(page).innerText();
  assert(text.includes(XSS), 'Filter nicht als Text');
  eq(await page.locator('#layer img').count(), 0, 'img erzeugt');
  await page.locator('#layer button', { hasText: 'Filter jetzt ausführen' }).click();
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Chatnachricht, Termin- und Aufgabentitel', async () => {
  const { page, issues } = await open();
  await gotoSpace(page, 'chat');
  await page.locator('#space-view .contact-item').first().click();
  const input = page.locator('#space-view textarea[aria-label="Nachricht schreiben"]');
  await input.fill(XSS);
  await input.press('Enter');
  assert((await page.locator('#space-view .chat-log').innerText()).includes(XSS), 'Chat nicht als Text');
  await page.click('.space-tab[data-fk="space:calendar"]');
  await page.evaluate(() => { editEvent(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Titel', XSS);
  await clickDialogButton(page, 'Speichern');
  await page.click('.space-tab[data-fk="space:tasks"]');
  await page.evaluate(() => { editTask(); });
  await page.waitForSelector('#layer .dialog');
  await fillField(page, 'Titel', XSS);
  await clickDialogButton(page, 'Speichern');
  eq(await page.locator('#app img').count(), 0, 'img erzeugt');
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Suchbegriff wird in der Hervorhebung nicht als HTML interpretiert', async () => {
  const { page, issues } = await open();
  await page.fill('#search', XSS);
  await page.fill('#search', '<b>');
  eq(await page.locator('#message-list b').count(), 0, 'HTML aus Suche');
  eq(await xssFired(page), false, 'XSS ausgeführt');
  noErrors(issues);
});

test('xss: Link einfügen im Editor lehnt javascript:-URLs ab', async () => {
  const { page } = await open();
  eq(await page.evaluate(() => [safeHref('javascript:alert(1)'), safeHref(' data:text/html,x'), safeHref('https://ok.example')]), ['', '', 'https://ok.example'], 'safeHref');
});

/* ---- Barrierefreiheit ---- */
test('a11y: alle Buttons haben einen zugänglichen Namen (Liste, Lesebereich, Verfassen, Einstellungen)', async () => {
  const { page, issues } = await open();
  const found = [];
  found.push(...await unnamedButtons(page));
  await page.locator('.mail-item').first().click();
  found.push(...await unnamedButtons(page));
  await openCompose(page);
  found.push(...await unnamedButtons(page));
  await page.click('.compose button[aria-label^="Schließen"]');
  await page.click('#btn-settings');
  found.push(...await unnamedButtons(page));
  eq([...new Set(found)], [], 'Buttons ohne Namen');
  noErrors(issues);
});

test('a11y: Buttons in eingeblendeten Bereichen haben einen zugänglichen Namen', async () => {
  const { page } = await open();
  await enableSpaces(page);
  const found = [];
  for (const id of ['contacts', 'calendar', 'tasks', 'chat']) {
    await page.click(`.space-tab[data-fk="space:${id}"]`);
    found.push(...(await unnamedButtons(page)).map(b => `${id}: ${b}`));
  }
  eq(found, [], 'Buttons ohne Namen');
});

test('a11y: Tastaturfokus ist sichtbar', async () => {
  const { page } = await open();
  const missing = [];
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press('Tab');
    const r = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const s = getComputedStyle(el);
      const ring = (s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0) || (s.boxShadow && s.boxShadow !== 'none');
      const within = el.closest('.ring-within');
      const ringWithin = within && (getComputedStyle(within).boxShadow !== 'none' || getComputedStyle(within).outlineStyle !== 'none');
      return { ok: Boolean(ring || ringWithin), el: el.outerHTML.slice(0, 80) };
    });
    if (r && !r.ok) missing.push(r.el);
  }
  eq(missing, [], 'Elemente ohne sichtbaren Fokus');
});

test('a11y: Skip-Link springt zur Nachrichtenliste', async () => {
  const { page } = await open();
  await page.keyboard.press('Tab');
  eq(await page.evaluate(() => document.activeElement.className), 'skip-link', 'erstes Tab-Ziel');
});

test('a11y: Escape schließt Dialog und Menü, Fokus kehrt zurück', async () => {
  const { page, issues } = await open();
  await page.focus('#btn-settings');
  await page.keyboard.press('Enter');
  await page.waitForSelector('#layer .dialog');
  await page.keyboard.press('Escape');
  eq(await page.locator('#layer .dialog').count(), 0, 'Dialog noch offen');
  eq(await page.evaluate(() => document.activeElement.id), 'btn-settings', 'Fokus zurück');
  await page.focus('#btn-view');
  await page.keyboard.press('Enter');
  await page.waitForSelector('#layer [role="menu"]');
  await page.keyboard.press('Escape');
  eq(await page.locator('#layer [role="menu"]').count(), 0, 'Menü noch offen');
  noErrors(issues);
});

test('a11y: Fokus bleibt im Dialog gefangen (Tab-Falle)', async () => {
  const { page } = await open();
  await page.click('#btn-settings');
  for (let i = 0; i < 40; i++) await page.keyboard.press('Tab');
  eq(await page.evaluate(() => Boolean(document.activeElement.closest('#layer .dialog'))), true, 'Fokus außerhalb des Dialogs');
});

test('a11y: Hintergrund ist bei offenem Dialog inert', async () => {
  const { page } = await open();
  await page.click('#btn-settings');
  eq(await page.evaluate(() => document.querySelector('#app').inert || document.querySelector('#app').getAttribute('aria-hidden') === 'true'), true, '#app nicht inert');
});

/* ---- Responsiv ---- */
const MOBILE = { width: 375, height: 812 };
const hScroll = page => page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test('responsiv: 375×812 ohne horizontales Scrollen, Navigation per Schublade', async () => {
  const { page, issues } = await open({ viewport: MOBILE });
  eq(await hScroll(page), 0, 'Horizontaler Überlauf (Liste)');
  await page.locator('#head-default [data-action="drawer"]').click();
  assert(await page.locator('#nav .nav-item[data-folder="archive"]').isVisible(), 'Navigation nicht sichtbar');
  await page.locator('#nav .nav-item[data-folder="archive"]').click();
  eq(await page.evaluate(() => state.view.folder), 'archive', 'Ordnerwechsel');
  eq(await page.evaluate(() => ui.drawerOpen), false, 'Schublade nach Ordnerwahl offen');
  await page.locator('#head-default [data-action="drawer"]').click();
  await page.keyboard.press('Escape');
  eq(await page.evaluate(() => ui.drawerOpen), false, 'Escape schließt die Schublade nicht');
  eq(await hScroll(page), 0, 'Horizontaler Überlauf (nach Navigation)');
  noErrors(issues);
});

test('responsiv: 375×812 – Nachricht öffnen, lesen, zurück; Verfassen passt auf den Schirm', async () => {
  const { page, issues } = await open({ viewport: MOBILE });
  await page.locator('.mail-item').first().click();
  await page.waitForSelector('#reader .msg');
  assert(await page.locator('#reader [data-action="close-reader"]').isVisible(), 'Zurück-Button nicht sichtbar');
  eq(await hScroll(page), 0, 'Horizontaler Überlauf (Lesebereich)');
  const inside = b => b && b.x >= -1 && b.x + b.width <= MOBILE.width + 1;
  const box = await waitUntil(async () => { const b = await page.locator('#reader').boundingBox(); return inside(b) ? b : null; }) // Einblend-Animation
    || await page.locator('#reader').boundingBox();
  assert(box.x >= -1 && box.x + box.width <= MOBILE.width + 1, `Lesebereich außerhalb: ${JSON.stringify(box)}`);
  await page.click('#reader [data-action="close-reader"]');
  await page.locator('#head-default [data-action="drawer"]').click(); // „Neue Nachricht“ sitzt mobil in der Schublade
  await openCompose(page);
  const cb = await page.locator('.compose').boundingBox();
  assert(cb.width <= MOBILE.width + 1 && cb.x >= -1, `Verfassen-Fenster zu breit: ${JSON.stringify(cb)}`);
  eq(await hScroll(page), 0, 'Horizontaler Überlauf (Verfassen)');
  noErrors(issues);
});

test('responsiv: 375×812 – Einstellungen und eingeblendete Bereiche ohne Überlauf', async () => {
  const { page, issues } = await open({ viewport: MOBILE });
  await page.click('[data-action="drawer"] >> nth=0');
  await page.click('#btn-settings');
  const db = await page.locator('#layer .dialog').boundingBox();
  assert(db.width <= MOBILE.width + 1, `Dialog zu breit: ${db.width}`);
  await page.keyboard.press('Escape');
  await enableSpaces(page);
  for (const id of ['contacts', 'calendar', 'tasks', 'chat']) {
    await page.evaluate(i => setSpace(i), id);
    eq(await hScroll(page), 0, `Horizontaler Überlauf (${id})`);
  }
  noErrors(issues);
});

/* ---- Backend-Shim ---- */
test('backend: unbekannter Befehl wirft, bekannter liefert im lokalen Modus null', async () => {
  const { page } = await open();
  const r = await page.evaluate(async () => {
    let err = null;
    try { await backend.invoke('rm_rf', {}); } catch (e) { err = e.message; }
    return { err, mode: backend.mode, ok: await backend.invoke('save_contact', { contact: {} }) };
  });
  assert(r.err && r.err.includes('Unbekannter Befehl'), `Kein Fehler: ${r.err}`);
  eq(r.mode, 'local', 'Modus');
  eq(r.ok, null, 'Ergebnis lokal');
});

test('backend: im Tauri-Modus werden Befehl und Argumente unverändert weitergereicht', async () => {
  const { page } = await open({ init: () => { window.__calls = []; window.__TAURI__ = { core: { invoke: async (c, a) => { window.__calls.push([c, a]); return 'ok'; } } }; } });
  const r = await page.evaluate(async () => ({ mode: backend.mode, res: await backend.invoke('delete_task', { id: 't1' }), calls: window.__calls.filter(c => c[0] === 'delete_task') }));
  eq(r.mode, 'tauri', 'Modus');
  eq(r.res, 'ok', 'Rückgabe');
  eq(r.calls, [['delete_task', { id: 't1' }]], 'Aufruf');
});

test('backend: Fehler des Backends erzeugt Hinweis, App bleibt bedienbar', async () => {
  const { page, issues } = await open({ init: () => { window.__TAURI__ = { core: { invoke: async () => { throw new Error('intern: db locked at /var/x'); } } }; } });
  await createFilterViaUi(page, { name: 'B', value: 'b' });
  const t = (await waitUntil(async () => { const x = await toastText(page); return x.includes('Backend') ? x : null; })) || await toastText(page);
  assert(t.includes('Backend'), `Kein Hinweis: ${t}`);
  assert(!t.includes('/var/x'), 'Interne Details geleakt');
  noErrors(issues);
});

test('backend: alle per commit() genutzten Befehle stehen in BACKEND_COMMANDS', async () => {
  const listed = new Set([...SOURCE.match(/const BACKEND_COMMANDS = \[([\s\S]*?)\];/)[1].matchAll(/'([a-z_]+)'/g)].map(m => m[1]));
  const used = [...new Set([...SOURCE.matchAll(/(?:commit|invoke)\('([a-z_]+)'/g)].map(m => m[1]))];
  eq(used.filter(c => !listed.has(c)), [], 'Nicht gelistete Befehle');
});

known('backend: Mail-Aktionen (senden, verschieben, löschen) werden dem Backend gemeldet',
  'Senden/Verschieben/Löschen von Mails rufen backend.invoke nie auf; BACKEND_COMMANDS enthält keine Mail-Befehle',
  async () => {
    const { page } = await open({ init: () => { window.__calls = []; window.__TAURI__ = { core: { invoke: async c => { window.__calls.push(c); return null; } } }; } });
    await page.locator('.mail-item').first().click();
    await page.click('#reader [data-action="delete"]');
    const calls = (await waitUntil(async () => { const c = await page.evaluate(() => window.__calls); return c.length ? c : null; }, { timeout: 1500 })) || [];
    assert(calls.length > 0, 'Löschen einer Mail erreichte das Backend nicht');
  });

/* ------------------------------------------------------------------ Runner */
/** Unbehandelte Rejections werden dem gerade laufenden Test zugeordnet (→ FEHLER), außerhalb eines Tests gesammelt. */
let current = null;
const stray = [];
process.on('unhandledRejection', reason => {
  const msg = `Unbehandelte Rejection: ${reason?.message ?? reason}`.split('\n')[0];
  if (current) current.unhandled.push(msg); else stray.push(msg);
});
const TEST_TIMEOUT = 20000;

(async () => {
  const filter = process.argv[2];
  const selected = filter ? tests.filter(t => t.name.toLowerCase().includes(filter.toLowerCase())) : tests;
  if (!selected.length) { console.log(`Kein Test passt zu „${filter}“.`); process.exit(1); }
  browser = await chromium.launch();
  const counts = { ok: 0, fehler: 0, bekannt: 0, behoben: 0 };
  const knownList = [];
  for (const t of selected) {
    const started = Date.now();
    let error = null;
    let timer;
    current = { unhandled: [] };
    try {
      await Promise.race([t.fn(), new Promise((_, rej) => { timer = setTimeout(() => rej(new Error(`Zeitüberschreitung (${TEST_TIMEOUT / 1000} s)`)), TEST_TIMEOUT); })]);
    } catch (e) { error = e; } finally { clearTimeout(timer); }
    if (error && /^Zeitüberschreitung/.test(error.message)) {
      // Kontexte sofort schließen, damit der abgebrochene Test nicht in den nächsten hineinläuft
      for (const c of browser.contexts()) await c.close().catch(() => {});
    }
    await new Promise(r => setImmediate(r)); // späte Rejections noch diesem Test zuordnen
    const unhandled = current.unhandled;
    current = null;
    if (!error && unhandled.length) error = new Error(unhandled.join(' | '));
    const ms = Date.now() - started;
    // Nur echte Assertion-Fehlschläge zählen bei bekannten Fehlern; Timeouts, Selektor- und Laufzeitfehler bleiben FEHLER.
    const expectedFailure = error instanceof AssertionError && !unhandled.length;
    if (t.bug && (!error || expectedFailure)) {
      if (error) { counts.bekannt++; knownList.push(`${t.name}: ${t.bug}`); console.log(`BEKANNT   ${t.name}  (${ms} ms)\n          ${t.bug}\n          → ${error.message.split('\n')[0]}`); }
      else { counts.behoben++; console.log(`BEHOBEN?  ${t.name}  – Markierung „known“ entfernen`); }
    } else if (error) {
      counts.fehler++;
      console.log(`FEHLER    ${t.name}  (${ms} ms)${t.bug ? '  [bekannter Fehler, aber kein Assertion-Fehlschlag]' : ''}\n          ${error.message.split('\n')[0]}`);
      try {
        fs.mkdirSync(SHOTS, { recursive: true });
        const page = browser.contexts().at(-1)?.pages().at(-1);
        if (page) await page.screenshot({ path: path.join(SHOTS, `${t.name.replace(/[^a-z0-9]+/gi, '_').slice(0, 60)}.png`) });
      } catch { /* Screenshot optional */ }
    } else { counts.ok++; console.log(`ok        ${t.name}  (${ms} ms)`); }
    for (const c of browser.contexts()) await c.close().catch(() => {});
  }
  await browser.close();
  if (stray.length) { counts.fehler++; console.log(`FEHLER    außerhalb eines Tests: ${stray.join(' | ')}`); }
  console.log(`\n${selected.length} Tests: ${counts.ok} ok, ${counts.fehler} FEHLER, ${counts.bekannt} bekannte Fehler, ${counts.behoben} BEHOBEN?`);
  process.exit(counts.fehler ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
