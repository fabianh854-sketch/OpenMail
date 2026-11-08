#!/usr/bin/env python3
"""Erzeugt die Platzhalter-Icons für die OpenMail-Desktop-Shell.

Motiv: gerundetes Quadrat in der Akzentfarbe des Prototyps (--accent, helles Theme,
#5B6670) mit einem weißen Ring („O“) in der Mitte; außerhalb der Rundung transparent.
Nur Python-3-Standardbibliothek (zlib/struct). Antialiasing per Supersampling.

Aufruf:  python3 src-tauri/icons/generate_icons.py
Ausgabe: icon.png, 32x32.png, 128x128.png, 128x128@2x.png, icon.ico, icon.icns
         (im Ordner dieses Skripts)
"""
import struct
import sys
import zlib
from pathlib import Path

ACCENT = (0x5B, 0x66, 0x70)
WHITE = (255, 255, 255)
SS = 4  # Supersampling-Faktor je Achse


def coverage(x, y, size):
    """Deckung (0..1) von Rundung bzw. Ring im Pixel (x, y) per Supersampling."""
    s = float(size)
    radius = 0.22 * s
    c = s / 2.0
    r_out, r_in = 0.30 * s, 0.17 * s
    box = ring = 0
    for j in range(SS):
        for i in range(SS):
            px = x + (i + 0.5) / SS
            py = y + (j + 0.5) / SS
            # Rundes Quadrat: Abstand zur inneren Box mit Eckenradius
            dx = max(abs(px - c) - (c - radius), 0.0)
            dy = max(abs(py - c) - (c - radius), 0.0)
            if dx * dx + dy * dy <= radius * radius:
                box += 1
                d2 = (px - c) ** 2 + (py - c) ** 2
                if r_in * r_in <= d2 <= r_out * r_out:
                    ring += 1
    n = SS * SS
    return box / n, ring / n


def render(size):
    """Liefert RGBA-Rohdaten (Zeilen mit Filter-Byte 0) für ein size x size Bild."""
    raw = bytearray()
    for y in range(size):
        raw.append(0)
        for x in range(size):
            a, ring = coverage(x, y, size)
            if a == 0:
                raw += b"\x00\x00\x00\x00"
                continue
            # Ring-Anteil innerhalb der Box mischen (nicht-vormultipliziert)
            t = ring / a
            rgb = [round(ACCENT[k] * (1 - t) + WHITE[k] * t) for k in range(3)]
            raw += bytes(rgb) + bytes([round(255 * a)])
    return bytes(raw)


def chunk(tag, data):
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)


def png(size):
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(render(size), 9)) + chunk(b"IEND", b""))


def ico(images):
    """ICO mit eingebetteten PNGs; images = [(größe, png_bytes), ...]."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    return head + entries + blobs


def icns(items):
    """ICNS mit eingebetteten PNGs; items = [(typ, png_bytes), ...]."""
    body = b"".join(t + struct.pack(">I", 8 + len(d)) + d for t, d in items)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


def main():
    out = Path(__file__).resolve().parent
    cache = {s: png(s) for s in (16, 32, 48, 64, 128, 256, 512)}
    (out / "icon.png").write_bytes(cache[512])
    (out / "32x32.png").write_bytes(cache[32])
    (out / "128x128.png").write_bytes(cache[128])
    (out / "128x128@2x.png").write_bytes(cache[256])
    (out / "icon.ico").write_bytes(ico([(s, cache[s]) for s in (16, 32, 48, 256)]))
    # ic07 = 128, ic08 = 256, ic09 = 512 (je PNG)
    (out / "icon.icns").write_bytes(icns([(b"ic07", cache[128]), (b"ic08", cache[256]),
                                          (b"ic09", cache[512])]))
    print("Icons geschrieben nach", out)


def selftest(out):
    """Parst die erzeugten Dateien wieder und prüft Signatur, CRC, Maße, Container."""
    def parse_png(data):
        assert data[:8] == b"\x89PNG\r\n\x1a\n"
        pos, w, h, idat = 8, 0, 0, b""
        while pos < len(data):
            n, tag = struct.unpack(">I4s", data[pos:pos + 8])
            body = data[pos + 8:pos + 8 + n]
            crc = struct.unpack(">I", data[pos + 8 + n:pos + 12 + n])[0]
            assert crc == zlib.crc32(tag + body) & 0xFFFFFFFF, "CRC"
            if tag == b"IHDR":
                w, h, bd, ct = struct.unpack(">IIBB", body[:10])
                assert (bd, ct) == (8, 6)
            elif tag == b"IDAT":
                idat += body
            pos += 12 + n
        assert len(zlib.decompress(idat)) == h * (1 + 4 * w)
        return w, h
    for name, s in (("icon.png", 512), ("32x32.png", 32), ("128x128.png", 128),
                    ("128x128@2x.png", 256)):
        assert parse_png((out / name).read_bytes()) == (s, s), name
    d = (out / "icon.ico").read_bytes()
    _, typ, cnt = struct.unpack("<HHH", d[:6])
    assert typ == 1 and cnt == 4
    sizes = []
    for k in range(cnt):
        w, _h, _c, _r, _p, _b, ln, off = struct.unpack("<BBBBHHII", d[6 + 16 * k:22 + 16 * k])
        sizes.append(parse_png(d[off:off + ln])[0])
        assert (w or 256) == sizes[-1]
    assert sizes == [16, 32, 48, 256], sizes
    d = (out / "icon.icns").read_bytes()
    assert d[:4] == b"icns" and struct.unpack(">I", d[4:8])[0] == len(d)
    pos, found = 8, []
    while pos < len(d):
        tag, ln = struct.unpack(">4sI", d[pos:pos + 8])
        parse_png(d[pos + 8:pos + ln])
        found.append(tag.decode())
        pos += ln
    assert found == ["ic07", "ic08", "ic09"], found
    print("Selbsttest ok")


if __name__ == "__main__":
    main()
    if "--no-test" not in sys.argv:
        selftest(Path(__file__).resolve().parent)
