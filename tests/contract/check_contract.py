#!/usr/bin/env python3
"""Statischer Vertragstest Oberfläche (fluent-mail.html) ↔ Backend (src-tauri + openmail-core).

Nur Python-Standardbibliothek. Liest die Quellen, führt nichts davon aus.

Prüft:
  a) BACKEND_COMMANDS im Prototyp ↔ #[tauri::command]-Funktionen ↔ generate_handler![…]
  b) Argumentnamen der Aufrufe commit('befehl', {…}) / backend.invoke('befehl', {…})
     ↔ Parameternamen der Rust-Commands (Tauri v2: JS camelCase → Rust snake_case)
  c) Feldnamen der Objekte, die der Prototyp für Adressbuch/Kontakt/Termin/Aufgabe/Filter erzeugt,
     ↔ Felder der Rust-Structs in types.rs (deny_unknown_fields, Pflichtfelder, Enum-Werte, Id-Format)
  d) Beispiel-Nutzlasten in fixtures/*.json ↔ Validierungsregeln aus types.rs

Ausgabe: Bericht auf stdout. Exit-Code 0 = nur offene/erwartete Lücken, 1 = Widersprüche
in implementierten Commands, 2 = Quelle nicht lesbar/Parser findet erwartete Stellen nicht.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HTML = ROOT / "fluent-mail.html"
MAIN_RS = ROOT / "src-tauri" / "src" / "main.rs"
TYPES_RS = ROOT / "crates" / "openmail-core" / "src" / "types.rs"
TAURI_CONF = ROOT / "src-tauri" / "tauri.conf.json"
FIXTURES = Path(__file__).resolve().parent / "fixtures"

# Tauri-Parameter, die nicht aus den JS-Argumenten kommen.
INJECTED_TYPES = ("State<", "AppHandle", "Window", "WebviewWindow", "Webview", "Request<", "Channel<")



def rust_trim(text: str) -> str:
    """Wie Rusts `str::trim`: entfernt Unicode-White_Space. Pythons `strip()` entfernt zusätzlich
    die Trennzeichen U+001C–U+001F, die Rust nicht als Leerraum zählt."""
    keep = "\x1c\x1d\x1e\x1f"
    start, end = 0, len(text)
    while start < end and text[start].isspace() and text[start] not in keep:
        start += 1
    while end > start and text[end - 1].isspace() and text[end - 1] not in keep:
        end -= 1
    return text[start:end]

class ParseError(Exception):
    """Eine erwartete Stelle wurde in den Quellen nicht gefunden."""


# --------------------------------------------------------------------------- Bericht


@dataclass
class Report:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    open_items: list[str] = field(default_factory=list)
    proto_bugs: list[str] = field(default_factory=list)  # zählen nicht als Widerspruch
    ok: list[str] = field(default_factory=list)

    def print(self, verbose: bool) -> None:
        def block(title: str, items: list[str]) -> None:
            print(f"\n== {title} ({len(items)}) ==")
            for item in items:
                print(f"  - {item}")

        if verbose:
            block("OK", self.ok)
        block("OFFEN (noch nicht implementiert, erwartet)", self.open_items)
        block("PROTOTYP-BUG (Rust lehnt zu Recht ab, kein Widerspruch)", self.proto_bugs)
        block("WARNUNG", self.warnings)
        block("WIDERSPRUCH (Fehler)", self.errors)
        print()
        print(
            f"Ergebnis: {len(self.errors)} Widersprüche, {len(self.warnings)} Warnungen, "
            f"{len(self.proto_bugs)} Prototyp-Bugs, {len(self.open_items)} offen, {len(self.ok)} ok"
        )


# --------------------------------------------------------------------------- Hilfen


def line_of(src: str, index: int) -> int:
    return src.count("\n", 0, index) + 1


def snake_to_camel(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(p[:1].upper() + p[1:] for p in rest)


def split_top_level(text: str, rust: bool = False) -> list[str]:
    """Trennt an Kommas auf oberster Ebene. JS: Strings beachten; Rust: auch `<…>` als Klammer."""
    opening, closing = ("([{<", ")]}>") if rust else ("([{", ")]}")
    quotes = '"' if rust else "'\"`"  # in Rust ist ' eine Lebensdauer (`'_`)
    parts, depth, buf, i, quote = [], 0, [], 0, None
    while i < len(text):
        c = text[i]
        if quote:
            buf.append(c)
            if c == "\\":
                buf.append(text[i + 1] if i + 1 < len(text) else "")
                i += 2
                continue
            if c == quote:
                quote = None
        elif c in quotes:
            quote = c
            buf.append(c)
        elif c in opening:
            depth += 1
            buf.append(c)
        elif c in closing:
            depth -= 1
            buf.append(c)
        elif c == "," and depth == 0:
            parts.append("".join(buf))
            buf = []
        else:
            buf.append(c)
        i += 1
    if "".join(buf).strip():
        parts.append("".join(buf))
    return [p.strip() for p in parts if p.strip()]


def match_bracket(src: str, start: int, rust: bool = False) -> int:
    """Index der schließenden Klammer zu src[start].

    JS: Strings und Template-Literale werden beachtet. Rust: nur "…"-Strings und //-Kommentare
    (' ist dort meist eine Lebensdauer).
    """
    pairs = {"{": "}", "(": ")", "[": "]"}
    quotes = '"' if rust else "'\"`"
    stack = [pairs[src[start]]]
    i, quote = start + 1, None
    tmpl_stack: list[int] = []  # Klammertiefe beim Eintritt in `${`
    while i < len(src):
        c = src[i]
        if quote:
            if c == "\\":
                i += 2
                continue
            if quote == "`" and c == "$" and src[i + 1 : i + 2] == "{":
                tmpl_stack.append(len(stack))
                stack.append("}")
                quote = None
                i += 2
                continue
            if c == quote:
                quote = None
        elif rust and src.startswith("//", i):
            nl = src.find("\n", i)
            i = len(src) if nl < 0 else nl
            continue
        elif c in quotes:
            quote = c
        elif c in pairs:
            stack.append(pairs[c])
        elif c in ")]}":
            if not stack or stack[-1] != c:
                raise ParseError(f"Klammerfehler bei Zeile {line_of(src, i)}")
            stack.pop()
            if tmpl_stack and len(stack) == tmpl_stack[-1]:
                tmpl_stack.pop()
                quote = "`"
            elif not stack:
                return i
        i += 1
    raise ParseError(f"Keine schließende Klammer ab Zeile {line_of(src, start)}")


@dataclass
class JsProp:
    key: str | None  # None bei Spread
    value: str  # Quelltext des Werts bzw. des Spread-Ausdrucks


def js_object_props(src: str, brace: int) -> list[JsProp]:
    """Eigenschaften eines JS-Objektliterals ab src[brace] == '{' (nur oberste Ebene)."""
    end = match_bracket(src, brace)
    props = []
    for part in split_top_level(src[brace + 1 : end]):
        if part.startswith("..."):
            props.append(JsProp(None, part[3:].strip()))
            continue
        m = re.match(r"""^(?:([A-Za-z_$][\w$]*)|'([^']*)'|"([^"]*)")\s*:\s*(.*)$""", part, re.S)
        if m:
            props.append(JsProp(m.group(1) or m.group(2) or m.group(3), m.group(4).strip()))
        elif re.fullmatch(r"[A-Za-z_$][\w$]*", part):
            props.append(JsProp(part, part))  # Kurzschreibweise { contact }
        else:
            props.append(JsProp("?" + part[:30], part))
    return props


def js_string_literal(value: str) -> str | None:
    m = re.fullmatch(r"""'([^'\\]*)'|"([^"\\]*)\"""", value.strip())
    return None if not m else (m.group(1) if m.group(1) is not None else m.group(2))


def js_number_literal(value: str) -> int | None:
    m = re.fullmatch(r"-?\d+", value.strip())
    return int(m.group(0)) if m else None


def js_function_body(src: str, name: str) -> tuple[int, str]:
    m = re.search(rf"(?:async\s+)?function\s+{re.escape(name)}\s*\([^)]*\)\s*\{{", src)
    if not m:
        raise ParseError(f"Funktion {name} nicht gefunden")
    end = match_bracket(src, m.end() - 1)
    return m.start(), src[m.start() : end + 1]


def form_field_keys(body: str) -> list[str]:
    """Schlüssel aus formDialog({ fields: [{ key: '…' }, …] })."""
    return [m.group(2) for m in re.finditer(r"""\{\s*key\s*:\s*(['"])([^'"]+)\1""", body)]


# --------------------------------------------------------------------------- Rust


@dataclass
class RustField:
    rust_name: str
    json_name: str
    ty: str
    optional: bool  # fehlt im JSON erlaubt (serde(default) oder Option)


@dataclass
class RustStruct:
    name: str
    deny_unknown: bool
    fields: dict[str, RustField]  # json_name -> Feld


def strip_rust_comments(src: str) -> str:
    """Entfernt //- und /* */-Kommentare (Doc-Kommentare eingeschlossen), Strings bleiben erhalten."""
    out, i, n = [], 0, len(src)
    while i < n:
        if src[i] == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            out.append(src[i : j + 1])
            i = j + 1
        elif src.startswith("//", i):
            nl = src.find("\n", i)
            i = n if nl < 0 else nl
        elif src.startswith("/*", i):
            close = src.find("*/", i + 2)
            i = n if close < 0 else close + 2
        else:
            out.append(src[i])
            i += 1
    return "".join(out)


def serde_args(attrs: str) -> str:
    """Inhalt aller `#[serde(...)]`-Attribute, durch Kommas verbunden (andere Attribute zählen nicht)."""
    found = []
    for m in re.finditer(r"#\[\s*serde\s*\(", attrs):
        open_paren = m.end() - 1
        found.append(attrs[open_paren + 1 : match_bracket(attrs, open_paren, rust=True)])
    return ",".join(found)


def serde_flag(args: str, flag: str) -> bool:
    return any(part.strip() == flag for part in split_top_level(args, rust=True))


def serde_value(args: str, key: str) -> str | None:
    for part in split_top_level(args, rust=True):
        m = re.fullmatch(rf'{key}\s*=\s*"([^"]*)"', part.strip())
        if m:
            return m.group(1)
    return None


def split_attrs(part: str) -> tuple[str, str]:
    """Trennt führende `#[...]`-Attribute von der Deklaration."""
    rest, attrs = part.strip(), []
    while rest.startswith("#["):
        close = match_bracket(rest, 1, rust=True)
        attrs.append(rest[: close + 1])
        rest = rest[close + 1 :].strip()
    return " ".join(attrs), rest


def parse_types(src: str) -> tuple[dict[str, RustStruct], dict[str, list[str]], dict[str, dict[str, list[str]]], dict[str, int], list[str]]:
    """Structs, einfache Enums (Varianten in JSON-Form), getaggte Enums, Konstanten und Hinweise.

    Hinweise nennen Konstrukte, die der Test nicht abbildet (Tupel-Varianten, Unit-Varianten in
    getaggten Enums, Struct-Varianten in einfachen Enums) – sie werden gemeldet, nicht verworfen.
    """
    src = strip_rust_comments(src)
    consts = {m.group(1): int(m.group(2).replace("_", "")) for m in re.finditer(r"pub const (\w+): \w+ = (-?[\d_]+);", src)}
    structs: dict[str, RustStruct] = {}
    simple_enums: dict[str, list[str]] = {}
    tagged_enums: dict[str, dict[str, list[str]]] = {}
    notes: list[str] = []
    item_re = re.compile(r"((?:#\[[^\]]*\]\s*)*)pub (struct|enum) (\w+)\s*(?:<[^>{]*>)?\s*\{", re.S)
    for m in item_re.finditer(src):
        kind, name = m.group(2), m.group(3)
        args = serde_args(m.group(1))
        body = src[m.end() : match_bracket(src, m.end() - 1, rust=True)]
        rename_all = serde_value(args, "rename_all")

        def jname(v: str) -> str:
            if rename_all == "lowercase":
                return v.lower()
            if rename_all == "camelCase":
                return snake_to_camel(v)
            return v

        if kind == "struct":
            fields = {}
            for part in split_top_level(body, rust=True):
                fattrs, decl = split_attrs(part)
                fm = re.fullmatch(r"(?:pub(?:\([^)]*\))?\s+)?(\w+)\s*:\s*(.+)", decl.strip(), re.S)
                if not fm:
                    notes.append(f"types.rs {name}: Feld nicht verstanden: {decl.strip()[:40]!r}")
                    continue
                fname, fty = fm.group(1), re.sub(r"\s+", " ", fm.group(2).strip())
                fargs = serde_args(fattrs)
                if serde_flag(fargs, "skip") or serde_flag(fargs, "flatten"):
                    notes.append(f"types.rs {name}.{fname}: skip/flatten wird nicht abgebildet")
                rn = serde_value(fargs, "rename")
                json_name = rn or (snake_to_camel(fname) if rename_all == "camelCase" else fname)
                optional = serde_flag(fargs, "default") or serde_value(fargs, "default") is not None or fty.startswith("Option<")
                fields[json_name] = RustField(fname, json_name, fty, optional)
            structs[name] = RustStruct(name, serde_flag(args, "deny_unknown_fields"), fields)
            continue
        tagged = serde_value(args, "tag") is not None
        variants: dict[str, list[str]] = {}
        simple: list[str] = []
        for part in split_top_level(body, rust=True):
            vattrs, decl = split_attrs(part)
            vm = re.fullmatch(r"(\w+)\s*(\{.*\}|\(.*\))?\s*(?:=\s*-?\d+)?", decl, re.S)
            if not vm:
                notes.append(f"types.rs {name}: Variante nicht verstanden: {decl[:40]!r}")
                continue
            vname = serde_value(serde_args(vattrs), "rename") or jname(vm.group(1))
            shape = vm.group(2)
            if shape and shape.startswith("("):
                notes.append(f"types.rs {name}::{vm.group(1)}: Tupel-Variante wird nicht abgebildet")
                continue
            if tagged:
                if shape is None:
                    notes.append(f"types.rs {name}::{vm.group(1)}: Unit-Variante in getaggtem Enum (Serde ignoriert dann Zusatzfelder nicht streng)")
                    variants[vname] = []
                else:
                    variants[vname] = [
                        re.sub(r"^pub\s+", "", f.split(":")[0].strip()) for f in split_top_level(shape[1:-1], rust=True)
                    ]
            elif shape is not None:
                notes.append(f"types.rs {name}::{vm.group(1)}: Struct-Variante in ungetaggtem Enum wird nicht abgebildet")
            else:
                simple.append(vname)
        if tagged:
            tagged_enums[name] = variants
        else:
            simple_enums[name] = simple
    return structs, simple_enums, tagged_enums, consts, notes


@dataclass
class RustCommand:
    name: str
    line: int
    params: list[str]  # nur Parameter, die aus den JS-Argumenten kommen
    param_types: dict[str, str]


def parse_commands(src: str) -> tuple[dict[str, RustCommand], list[str]]:
    commands = {}
    for m in re.finditer(r"#\[tauri::command[^\]]*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(", src):
        end = match_bracket(src, m.end() - 1, rust=True)
        params, types = [], {}
        for p in split_top_level(src[m.end() : end], rust=True):
            pname, _, pty = p.partition(":")
            pname, pty = pname.strip().removeprefix("mut "), pty.strip()
            if any(t in pty for t in INJECTED_TYPES):
                continue
            params.append(pname)
            types[pname] = pty
        commands[m.group(1)] = RustCommand(m.group(1), line_of(src, m.start()), params, types)
    gh = re.search(r"generate_handler!\s*\[([^\]]*)\]", src)
    handler = [x.strip().split("::")[-1] for x in gh.group(1).split(",") if x.strip()] if gh else []
    return commands, handler


# --------------------------------------------------------------------------- Prototyp


@dataclass
class JsCall:
    command: str
    line: int
    keys: list[str] | None  # None = keine Argumente
    props: list[JsProp]


def parse_backend_commands(src: str) -> list[str]:
    m = re.search(r"const BACKEND_COMMANDS\s*=\s*\[", src)
    if not m:
        raise ParseError("BACKEND_COMMANDS nicht gefunden")
    end = match_bracket(src, m.end() - 1)
    found = [m.group(2) for m in re.finditer(r"""(['"])([a-z_]+)\1""", src[m.end() : end])]
    if not found:
        raise ParseError("BACKEND_COMMANDS ist leer oder nicht lesbar")
    return found


def parse_calls(src: str) -> list[JsCall]:
    calls = []
    for m in re.finditer(r"""\b(?:commit|backend\.invoke|invoke)\(\s*(['"])([a-z_]+)\1\s*(,\s*)?""", src):
        line = line_of(src, m.start())
        props: list[JsProp] = []
        keys = None
        if m.group(3):
            rest = m.end()
            if src[rest] == "{":
                props = js_object_props(src, rest)
                keys = [p.key or "..." + p.value for p in props]
            else:
                keys = ["?" + src[rest : rest + 20]]
        calls.append(JsCall(m.group(2), line, keys, props))
    if not calls:
        raise ParseError("keine commit()/backend.invoke()-Aufrufe mit Befehlsnamen gefunden")
    return calls


@dataclass
class Shape:
    """Ein Objekt, wie es der Prototyp erzeugt: Feldnamen und – wo literal – Werte."""

    entity: str
    origin: str
    keys: dict[str, str | None]  # Feld -> Quelltext des Werts (None = aus Formular/Spread)
    unresolved: list[str] = field(default_factory=list)
    func: str = ""  # umgebende JS-Funktion


ENTITY_BY_PREFIX = {"b": "book", "p": "contact", "e": "event", "t": "task", "fl": "filter"}
STRUCT_BY_ENTITY = {"book": "AddressBook", "contact": "Contact", "event": "CalEvent", "task": "Task", "filter": "MailFilter"}
# Platzhalter in Entwürfen, die der Dialog vor dem Speichern überschreibt und prüft
# (editFilter: `draft.name = nameInput.value.trim()`, leerer Name wird abgewiesen).
DRAFT_PLACEHOLDERS = {("editFilter", "name")}


def collect_shapes(src: str) -> list[Shape]:
    """Alle Objektliterale mit `id: uid('<präfix>')` plus aufgelöste Spreads."""
    # Spread-Auflösung: (Funktion, Ausdruck) -> Feldnamen. Formulare werden aus formDialog gelesen.
    _, person_body = js_function_body(src, "editPerson")
    _, event_body = js_function_body(src, "editEvent")
    _, task_body = js_function_body(src, "editTask")
    _, vcf_body = js_function_body(src, "parseVcf")
    vcf_ret = re.search(r"map\(.*?return\s*\{", vcf_body, re.S)
    if not vcf_ret:
        raise ParseError("parseVcf: Rückgabeobjekt nicht gefunden")
    vcf_obj = vcf_ret.end() - 1
    vcf_keys = [p.key for p in js_object_props(vcf_body, vcf_obj) if p.key]
    _, extras_body = js_function_body(src, "ensureExtras")

    def call_literal_keys(body: str, helper: str) -> dict[str, str]:
        keys: dict[str, str] = {}
        for cm in re.finditer(rf"\b{helper}\(\s*\{{", body):
            for p in js_object_props(body, cm.end() - 1):
                if p.key:
                    keys.setdefault(p.key, p.value)
                elif re.match(r"(allDay|fixed)\(", p.value) or p.value.startswith("{"):
                    for k in ("allDay", "start", "end"):
                        keys.setdefault(k, None)
        return keys

    resolvers = {
        "result@editPerson": dict.fromkeys(form_field_keys(person_body)),
        "data@editEvent": dict.fromkeys(form_field_keys(event_body) + ["start", "end", "remind"]),
        "data@editTask": dict.fromkeys(form_field_keys(task_body) + ["due"]),
        "c@openImportExport": dict.fromkeys(vcf_keys),
        "o@ensureExtras/ev": call_literal_keys(extras_body, "ev"),
        "o@ensureExtras/task": call_literal_keys(extras_body, "task"),
    }
    for rkey, keys in resolvers.items():
        if not keys:
            raise ParseError(f"Spread-Auflösung {rkey}: keine Felder gefunden")
    func_starts = sorted(
        (m.start(), m.group(1)) for m in re.finditer(r"(?:async\s+)?function\s+(\w+)\s*\(", src)
    )

    def enclosing(index: int) -> str:
        name = "?"
        for start, fname in func_starts:
            if start > index:
                break
            name = fname
        return name

    shapes = []
    for m in re.finditer(r"""\{\s*id\s*:\s*uid\(\s*(['"])(\w+)\1\s*\)""", src):
        entity = ENTITY_BY_PREFIX.get(m.group(2))
        if not entity:
            continue
        brace = m.start()
        func = enclosing(brace)
        helper = re.search(r"const (\w+)\s*=\s*\(?o\)?\s*=>\s*\(\s*$", src[max(0, brace - 40) : brace])
        origin = f"fluent-mail.html:{line_of(src, brace)} ({func}{'/' + helper.group(1) if helper else ''})"
        shape = Shape(entity, origin, {}, func=func)
        for p in js_object_props(src, brace):
            if p.key:
                shape.keys[p.key] = p.value
                continue
            rkey = f"{p.value}@{func}" + (f"/{helper.group(1)}" if helper else "")
            if rkey in resolvers:
                for k, v in resolvers[rkey].items():
                    shape.keys.setdefault(k, v)
            else:
                shape.unresolved.append(p.value)
        shapes.append(shape)
    missing = sorted(set(ENTITY_BY_PREFIX.values()) - {sh.entity for sh in shapes})
    if missing:
        raise ParseError(f"keine erzeugten Objekte gefunden für: {missing}")
    return shapes


def nested_literals(value: str) -> list[list[JsProp]]:
    """Objektliterale in einem Array-Literal `[ {…}, {…} ]`."""
    out = []
    if not value.startswith("["):
        return out
    for part in split_top_level(value[1 : match_bracket(value, 0)]):
        if part.startswith("{"):
            out.append(js_object_props(part, 0))
    return out


def option_values(src: str, const: str) -> list[str]:
    m = re.search(rf"const {const}\s*=\s*(?:\(\)\s*=>\s*)?\[", src)
    if not m:
        raise ParseError(f"{const} nicht gefunden")
    end = match_bracket(src, m.end() - 1)
    found = [m.group(2) for m in re.finditer(r"""\[\s*(['"])([^'"]*)\1\s*,""", src[m.end() : end])]
    if not found:
        raise ParseError(f"{const}: keine Optionen gefunden")
    return found


# --------------------------------------------------------------------------- Werteprüfung


class Validator:
    """Bildet die Parse-Regeln aus types.rs nach (nur für Fixtures und Literale)."""

    def __init__(self, structs, enums, tagged, consts):
        self.structs, self.enums, self.tagged, self.c = structs, enums, tagged, consts

    def id(self, v) -> str | None:
        ok = isinstance(v, str) and 1 <= len(v) <= self.c["MAX_ID_LEN"] and re.fullmatch(r"[A-Za-z0-9_-]+", v)
        return None if ok else f"ungültige Id {v!r}"

    def value(self, ty: str, v, path: str) -> list[str]:
        if ty.startswith("Option<"):
            return [] if v is None else self.value(ty[7:-1], v, path)
        if ty.startswith("Vec<"):
            if not isinstance(v, list):
                return [f"{path}: Liste erwartet"]
            return [e for i, x in enumerate(v) for e in self.value(ty[4:-1], x, f"{path}[{i}]")]
        if ty == "Id":
            err = self.id(v)
            return [f"{path}: {err}"] if err else []
        if ty == "Name":
            ok = isinstance(v, str) and v.strip() and len(v.strip()) <= self.c["MAX_NAME_LEN"] and not any(ord(ch) < 32 or 127 <= ord(ch) < 160 for ch in v.strip())
            return [] if ok else [f"{path}: ungültiger Name {v!r}"]
        if ty == "Text":
            ok = isinstance(v, str) and len(v.encode()) <= self.c["MAX_TEXT_LEN"] and all(ch in "\n\r\t" or not (ord(ch) < 32 or 127 <= ord(ch) < 160) for ch in v)
            return [] if ok else [f"{path}: ungültiger Text"]
        if ty == "Email":
            return [] if isinstance(v, str) and self.email(v) else [f"{path}: ungültige E-Mail {v!r}"]
        if ty == "bool":
            return [] if isinstance(v, bool) else [f"{path}: bool erwartet, {v!r}"]
        if ty in ("i64", "i32"):
            return [] if isinstance(v, int) and not isinstance(v, bool) else [f"{path}: Ganzzahl erwartet, {v!r}"]
        if ty in self.enums:
            return [] if v in self.enums[ty] else [f"{path}: {v!r} nicht in {ty} {self.enums[ty]}"]
        if ty in self.tagged:
            if not isinstance(v, dict) or v.get("type") not in self.tagged[ty]:
                return [f"{path}: unbekannter Typ {v!r}"]
            allowed = self.tagged[ty][v["type"]]
            errs = [f"{path}: unbekanntes Feld {k!r} (deny_unknown_fields)" for k in v if k not in allowed and k != "type"]
            errs += [f"{path}: Pflichtfeld {k!r} fehlt" for k in allowed if k not in v]
            errs += [e for k in allowed if k in v for e in self.value("Id", v[k], f"{path}.{k}")]
            return errs
        if ty in self.structs:
            return self.struct(ty, v, path)
        return [f"{path}: Typ {ty} unbekannt (Validator erweitern)"]

    @staticmethod
    def email(v: str) -> bool:
        v = v.strip()
        if len(v) > 254 or any(ch.isspace() or ord(ch) < 32 for ch in v) or "@" not in v:
            return False
        local, domain = v.split("@", 1)
        labels_ok = all(
            1 <= len(lbl.encode()) <= 63 and not lbl.startswith("-") and not lbl.endswith("-") and all(ch.isalnum() or ch == "-" for ch in lbl)
            for lbl in domain.split(".")
        )
        tld = domain.rsplit(".", 1)[-1]
        return bool(local) and len(local) <= 64 and "." in domain and labels_ok and any(ch.isalpha() for ch in tld)

    def struct(self, name: str, obj, path: str) -> list[str]:
        st = self.structs[name]
        if not isinstance(obj, dict):
            return [f"{path}: Objekt erwartet"]
        errs = []
        if st.deny_unknown:
            errs += [f"{path}.{k}: unbekanntes Feld (deny_unknown_fields)" for k in obj if k not in st.fields]
        for jn, f in st.fields.items():
            if jn not in obj:
                if not f.optional:
                    errs.append(f"{path}.{jn}: Pflichtfeld fehlt")
                continue
            if name == "Contact" and jn == "email" and (obj[jn] is None or (isinstance(obj[jn], str) and not rust_trim(obj[jn]))):
                continue  # deserialize_with: leer/Whitespace gilt als keine Adresse (None)
            errs += self.value(f.ty, obj[jn], f"{path}.{jn}")
        if not errs:
            errs += self.cross_field(name, obj, path)
        return errs

    def cross_field(self, name: str, o: dict, path: str) -> list[str]:
        """Nachbildung von CalEvent/Task/MailFilter::validate()."""
        lo, hi = self.c["MIN_TS"], self.c["MAX_TS"]
        if name == "CalEvent":
            if not (lo <= o["start"] <= hi and lo <= o["end"] <= hi):
                return [f"{path}: zeitpunkt außerhalb"]
            if o["end"] < o["start"]:
                return [f"{path}: ende liegt vor beginn"]
            if not -1 <= o["remind"] <= self.c["MAX_REMIND_MINUTES"]:
                return [f"{path}: erinnerung außerhalb"]
        if name == "Task" and o.get("due") is not None and not lo <= o["due"] <= hi:
            return [f"{path}: faelligkeit außerhalb"]
        if name == "MailFilter":
            if not 1 <= len(o["rules"]) <= self.c["MAX_RULES"]:
                return [f"{path}: bedingungen (1..{self.c['MAX_RULES']})"]
            if not 1 <= len(o["actions"]) <= self.c["MAX_ACTIONS"]:
                return [f"{path}: aktionen (1..{self.c['MAX_ACTIONS']})"]
            if any(not r["value"].strip() or len(r["value"].encode()) > self.c["MAX_RULE_VALUE_LEN"] for r in o["rules"]):
                return [f"{path}: bedingungswert leer oder zu lang"]
        return []


# --------------------------------------------------------------------------- Prüfungen


def check_commands(rep: Report, proto: list[str], cmds: dict[str, RustCommand], handler: list[str]) -> None:
    for c in proto:
        if c in cmds:
            rep.ok.append(f"Command {c}: im Prototyp und in main.rs")
        else:
            rep.open_items.append(f"Command {c}: im Prototyp, in main.rs noch nicht implementiert")
    for c, rc in cmds.items():
        if c not in proto:
            rep.errors.append(f"src-tauri/src/main.rs:{rc.line}: Command {c} fehlt in BACKEND_COMMANDS (Prototyp würde ihn ablehnen)")
        if c not in handler:
            rep.errors.append(f"src-tauri/src/main.rs:{rc.line}: Command {c} definiert, aber nicht in generate_handler! registriert")
    for c in handler:
        if c not in cmds:
            rep.errors.append(f"src-tauri/src/main.rs: generate_handler! nennt {c}, aber keine #[tauri::command]-Funktion")
    dupes = {c for c in handler if handler.count(c) > 1}
    for c in sorted(dupes):
        rep.errors.append(f"src-tauri/src/main.rs: {c} doppelt in generate_handler!")


def check_calls(rep: Report, calls: list[JsCall], proto: list[str], cmds: dict[str, RustCommand]) -> None:
    by_cmd: dict[str, set[tuple[str, ...]]] = {}
    for call in calls:
        where = f"fluent-mail.html:{call.line}"
        if call.command not in proto:
            rep.errors.append(f"{where}: Aufruf von {call.command!r}, nicht in BACKEND_COMMANDS (wirft 'Unbekannter Befehl')")
            continue
        sent = tuple(sorted(call.keys or []))
        by_cmd.setdefault(call.command, set()).add(sent)
        rc = cmds.get(call.command)
        if not rc:
            continue
        expected = sorted(snake_to_camel(p) for p in rc.params)
        if list(sent) == expected:
            rep.ok.append(f"{where}: {call.command} Argumente {list(sent)} passen")
        else:
            missing = [k for k in expected if k not in sent]
            extra = [k for k in sent if k not in expected]
            rep.errors.append(
                f"{where}: {call.command} sendet {list(sent)}, Rust erwartet {expected}"
                + (f" – fehlt {missing}" if missing else "")
                + (f" – überzählig {extra}" if extra else "")
            )
    for c, variants in sorted(by_cmd.items()):
        if c not in cmds and len(variants) > 1:
            rep.warnings.append(f"{c} (noch offen) wird mit uneinheitlichen Argumenten aufgerufen: {sorted(map(list, variants))}")
    for c, rc in cmds.items():
        if c not in by_cmd and rc.params:
            rep.warnings.append(f"src-tauri/src/main.rs:{rc.line}: {c} wird vom Prototyp nie aufgerufen (Argumente ungeprüft)")


def check_shapes(rep: Report, shapes: list[Shape], val: Validator, src: str) -> None:
    for sh in shapes:
        st = val.structs[STRUCT_BY_ENTITY[sh.entity]]
        for u in sh.unresolved:
            rep.warnings.append(f"{sh.origin}: Spread '...{u}' nicht auflösbar – Felder ungeprüft")
        extra = [k for k in sh.keys if k not in st.fields]
        missing = [jn for jn, f in st.fields.items() if not f.optional and jn not in sh.keys]
        if extra:
            rep.errors.append(f"{sh.origin}: {sh.entity} hat Felder {extra}, die {st.name} ablehnt (deny_unknown_fields)")
        if missing:
            rep.errors.append(f"{sh.origin}: {sh.entity} ohne Pflichtfelder {missing} für {st.name}")
        if not extra and not missing:
            rep.ok.append(f"{sh.origin}: {sh.entity}-Felder passen zu {st.name}")
        # Literale Werte prüfen (Strings/Zahlen), soweit statisch erkennbar.
        for key, raw in sh.keys.items():
            f = st.fields.get(key)
            if f is None or raw is None or (sh.func, key) in DRAFT_PLACEHOLDERS:
                continue
            for e in literal_errors(val, f.ty, raw, f"{sh.entity}.{key}"):
                rep.errors.append(f"{sh.origin}: {e}")
            if f.ty == "Vec<Rule>" or f.ty == "Vec<Action>":
                inner = f.ty[4:-1]
                for obj in nested_literals(raw):
                    obj_keys = {p.key: p.value for p in obj if p.key}
                    if inner == "Rule":
                        rs = val.structs["Rule"]
                        for k in obj_keys:
                            if k not in rs.fields:
                                rep.errors.append(f"{sh.origin}: Bedingung hat Feld {k!r}, das Rule ablehnt")
                        for k, v in obj_keys.items():
                            if k in rs.fields:
                                for e in literal_errors(val, rs.fields[k].ty, v, f"rule.{k}"):
                                    rep.errors.append(f"{sh.origin}: {e}")
                    else:
                        t = js_string_literal(obj_keys.get("type", ""))
                        allowed = val.tagged["Action"].get(t or "")
                        if allowed is None:
                            rep.errors.append(f"{sh.origin}: Aktion mit unbekanntem type {obj_keys.get('type')!r}")
                            continue
                        for k in obj_keys:
                            if k != "type" and k not in allowed:
                                rep.errors.append(f"{sh.origin}: Aktion {t} hat Feld {k!r}, das Rust ablehnt")
                        for k in allowed:
                            if k not in obj_keys:
                                rep.errors.append(f"{sh.origin}: Aktion {t} ohne Pflichtfeld {k!r}")
                            else:
                                for e in literal_errors(val, "Id", obj_keys[k], f"action.{k}"):
                                    rep.errors.append(f"{sh.origin}: {e}")


def literal_errors(val: Validator, ty: str, raw: str, path: str) -> list[str]:
    base = ty[7:-1] if ty.startswith("Option<") else ty
    s, n = js_string_literal(raw), js_number_literal(raw)
    if raw.strip() == "null":
        return [] if ty.startswith("Option<") else [f"{path}: null, Rust erwartet {ty}"]
    if s is not None:
        if base in ("i64", "i32", "bool"):
            return [f"{path}: String {s!r}, Rust erwartet {base}"]
        return val.value(base, s, path)
    if n is not None:
        return val.value(base, n, path)
    if raw.strip() in ("true", "false"):
        return val.value(base, raw.strip() == "true", path)
    return []  # Ausdruck: statisch nicht bestimmbar


def check_options(rep: Report, src: str, val: Validator) -> None:
    """Auswahllisten im Prototyp ↔ Rust-Enums und -Grenzen."""
    pairs = [("REPEATS", "Repeat"), ("PRIOS", "Prio"), ("FILTER_FIELDS", "RuleField"), ("FILTER_OPS", "RuleOp")]
    for const, enum in pairs:
        js = option_values(src, const)
        rust = val.enums[enum]
        if sorted(js) == sorted(rust):
            rep.ok.append(f"{const} = {enum} {rust}")
        else:
            rep.errors.append(f"fluent-mail.html {const} {js} ≠ Rust {enum} {rust}")
    js_actions = option_values(src, "FILTER_ACTIONS")
    if sorted(js_actions) != sorted(val.tagged["Action"]):
        rep.errors.append(f"FILTER_ACTIONS {js_actions} ≠ Rust Action {sorted(val.tagged['Action'])}")
    else:
        rep.ok.append(f"FILTER_ACTIONS = Action {sorted(val.tagged['Action'])}")
    match_opts = re.search(r"""selectEl\(\s*\[\s*\[\s*(['"])any\1[^\]]*\]\s*,\s*\[\s*(['"])all\2""", src)
    if not match_opts or sorted(val.enums["MatchMode"]) != ["all", "any"]:
        rep.errors.append("Filter-Verknüpfung any/all ≠ Rust MatchMode")
    else:
        rep.ok.append("Filter-Verknüpfung any/all = MatchMode")
    for v in option_values(src, "REMINDERS"):
        if not -1 <= int(v) <= val.c["MAX_REMIND_MINUTES"]:
            rep.errors.append(f"REMINDERS-Wert {v} außerhalb -1..{val.c['MAX_REMIND_MINUTES']}")
    for v in option_values(src, "moveOptions"):
        if val.id(v):
            rep.errors.append(f"moveOptions-Ziel {v!r} ist keine gültige Id")
    # Id-Erzeugung im Prototyp
    m = re.search(r"const uid = prefix => `\$\{prefix\}\$\{Date\.now\(\)\.toString\(36\)\}\$\{\(idCounter\+\+\)\.toString\(36\)\}`;", src)
    if not m:
        rep.warnings.append("uid()-Definition hat sich geändert – Id-Format manuell prüfen")
    else:
        bad = sorted({p for p in (m.group(2) for m in re.finditer(r"""uid\(\s*(['"])([^'"]*)\1\s*\)""", src)) if not re.fullmatch(r"[A-Za-z0-9_-]{0,40}", p)})
        if bad:
            rep.errors.append(f"uid()-Präfixe ergeben ungültige Ids: {bad}")
        else:
            rep.ok.append("uid(): Präfix + base36(Date.now) + base36(Zähler) ⊂ [A-Za-z0-9_-], < 64 Zeichen")


def check_fixtures(rep: Report, val: Validator, cmds: dict[str, RustCommand]) -> None:
    for path in sorted(FIXTURES.glob("*.json")):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            rep.errors.append(f"{path.name}: nicht lesbar ({exc})")
            continue
        cmd, args = data.get("command"), data.get("args", {})
        rc = cmds.get(cmd)
        label = f"fixtures/{path.name} ({data.get('source', '?')})"
        if not rc:
            rep.open_items.append(f"{label}: Command {cmd} noch nicht implementiert")
            continue
        expected = sorted(snake_to_camel(p) for p in rc.params)
        if sorted(args) != expected:
            rep.errors.append(f"{label}: Argumente {sorted(args)} ≠ {expected}")
            continue
        expect = data.get("expect")
        if expect not in ("accept", "reject"):
            rep.errors.append(f"{label}: Feld \"expect\" fehlt oder ist nicht \"accept\"/\"reject\"")
            continue
        errs = []
        for p in rc.params:
            errs += val.value(rc.param_types[p], args[snake_to_camel(p)], snake_to_camel(p))
        sev = data.get("severity", "-")
        if expect == "accept" and errs:
            rep.errors.append(f"{label}: soll angenommen werden, Rust lehnt ab [{sev}] – " + "; ".join(errs))
        elif expect == "accept":
            rep.ok.append(f"{label}: von Rust akzeptiert")
        elif errs:
            # Der Prototyp erzeugt eine ungültige Nutzlast; Rusts Ablehnung ist gewollt.
            rep.proto_bugs.append(f"{label}: Rust lehnt erwartungsgemäß ab [{sev}] – " + "; ".join(errs))
        else:
            rep.errors.append(f"{label}: soll abgelehnt werden, Rust würde annehmen")


def check_config(rep: Report, html: str) -> None:
    try:
        conf = json.loads(TAURI_CONF.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        rep.warnings.append(f"tauri.conf.json nicht lesbar: {exc}")
        return
    uses_global = "window.__TAURI__" in html
    if uses_global and not conf.get("app", {}).get("withGlobalTauri", False):
        rep.warnings.append(
            "src-tauri/tauri.conf.json: withGlobalTauri=false, der Prototyp erkennt Tauri aber über window.__TAURI__ "
            "→ backend.mode bleibt 'local', kein Aufruf erreicht Rust (bewusst, da UI später Leptos? sonst @tauri-apps/api bzw. withGlobalTauri)"
        )
    icons = ROOT / "src-tauri" / "icons" / "icon.png"
    if not icons.exists() and not conf.get("bundle", {}).get("icon"):
        rep.warnings.append("src-tauri/icons/icon.png fehlt → tauri::generate_context!() bricht beim Kompilieren ab")


# --------------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("-v", "--verbose", action="store_true", help="auch erfolgreiche Prüfungen zeigen")
    ap.add_argument("--report-only", action="store_true", help="immer Exit-Code 0 (nur Bericht)")
    args = ap.parse_args()
    try:
        html = HTML.read_text(encoding="utf-8")
        main_rs = MAIN_RS.read_text(encoding="utf-8")
        types_rs = TYPES_RS.read_text(encoding="utf-8")
        structs, enums, tagged, consts, type_notes = parse_types(types_rs)
        val = Validator(structs, enums, tagged, consts)
        proto = parse_backend_commands(html)
        cmds, handler = parse_commands(main_rs)
        calls = parse_calls(html)
        shapes = collect_shapes(html)
        for needed in ("AddressBook", "Contact", "CalEvent", "Task", "MailFilter", "Rule"):
            if needed not in structs:
                raise ParseError(f"Struct {needed} nicht in types.rs gefunden")
        if not cmds or not handler:
            raise ParseError("keine Tauri-Commands oder kein generate_handler! gefunden")
    except (OSError, ParseError) as exc:
        print(f"Abbruch: {exc}", file=sys.stderr)
        return 2

    rep = Report()
    print(f"Vertragstest: {len(proto)} Befehle im Prototyp, {len(cmds)} Tauri-Commands, "
          f"{len(calls)} Aufrufe, {len(shapes)} erzeugte Objekte")
    rep.warnings.extend(type_notes)
    check_commands(rep, proto, cmds, handler)
    check_calls(rep, calls, proto, cmds)
    check_shapes(rep, shapes, val, html)
    check_options(rep, html, val)
    check_fixtures(rep, val, cmds)
    check_config(rep, html)
    rep.print(args.verbose)
    if args.report_only:
        return 0
    return 1 if rep.errors else 0


if __name__ == "__main__":
    sys.exit(main())
