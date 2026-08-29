#!/usr/bin/env python3
"""Turn a real Linux keymap into the table Thalyx loads into the kernel.

Why this script exists rather than a table somebody typed:

A keyboard layout is data about the world — which physical key carries `ñ` on a
Latin American keyboard — and rule 6 of `CLAUDE.md` says a fixture invented by
the person who needs it proves only that it matches their model. Nobody on this
project has memorised 12 modifier maps of 256 keycodes, and a layout that is
subtly wrong is worse than one that is absent: it is discovered one key at a
time, months later, by somebody who assumes they typed it wrong.

So the table comes from `kbd`, which owns the format, resolved by `loadkeys`
itself:

    loadkeys --mktable /usr/share/keymaps/i386/qwerty/la-latin1.kmap.gz \
        | dev/keymap-table.py la-latin1 > crates/thalyx-term/src/keymap/la_latin1.rs

`la-latin1.kmap` is a *diff* against two includes — 40 lines that mean nothing
without `qwerty-layout` and `linux-with-alt-and-altgr` — so reading the file
directly is exactly the mistake this avoids. `--mktable` resolves the includes
and prints what the kernel would actually hold.
"""

import re
import sys

MAP_NAMES = [
    "plain_map", "shift_map", "altgr_map", None,
    "ctrl_map", "shift_ctrl_map", "altgr_ctrl_map", None,
    "alt_map", "shift_alt_map", "altgr_alt_map", None,
    "ctrl_alt_map", "shift_ctrl_alt_map", "altgr_ctrl_alt_map", None,
]


def parse(text):
    maps = {}
    for name in filter(None, MAP_NAMES):
        match = re.search(
            r"unsigned short %s\[NR_KEYS\]\s*=\s*\{(.*?)\};" % name, text, re.S
        )
        if not match:
            continue
        values = [int(v, 0) for v in re.findall(r"0x[0-9a-fA-F]+", match.group(1))]
        if len(values) != 256:
            raise SystemExit(f"{name} has {len(values)} entries, not 256")
        maps[name] = values

    accents = []
    match = re.search(r"accent_table\[MAX_DIACR\]\s*=\s*\{(.*?)\n\};", text, re.S)
    if match:
        # The three columns are C character literals and two of them are octal
        # escapes: `{'\'', 'a', '\341'}` is «acute, then a, makes \u{e1}». Read
        # as literals rather than as numbers, because `\341` is 225 and `341`
        # is not — a keymap that got that wrong would put the wrong letter under
        # every accented key and look like a font problem.
        for cell in re.findall(r"\{([^}]*)\}", match.group(1)):
            parts = re.findall(r"'(\\?.{1,3}?)'", cell)
            if len(parts) != 3:
                raise SystemExit("an accent entry did not have three columns: %r" % cell)
            accents.append(tuple(unquote(part) for part in parts))
    return maps, accents


def unquote(literal):
    """One C character literal, as the number the kernel stores."""
    if not literal.startswith("\\"):
        return ord(literal)
    rest = literal[1:]
    if rest and all(character in "01234567" for character in rest):
        return int(rest, 8)
    # `\'` and `\\` — the escape is only there for C's benefit.
    return ord(rest[0])


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "layout"
    maps, accents = parse(sys.stdin.read())

    out = sys.stdout.write
    out("// GENERATED — do not edit by hand.\n")
    out("//\n")
    out("// Produced by `dev/keymap-table.py` from the output of\n")
    out("// `loadkeys --mktable`, which is `kbd`'s own resolution of\n")
    out(f"// `/usr/share/keymaps/i386/qwerty/{name}.kmap.gz`. The reason it is\n")
    out("// generated rather than written is in that script's docstring.\n\n")
    out("use super::{Accent, Layout, MapEntries};\n\n")

    for index, map_name in enumerate(MAP_NAMES):
        if map_name is None or map_name not in maps:
            continue
        out(f"/// Modifier table {index} — `{map_name}`.\n")
        out(f"static {map_name.upper()}: MapEntries = [\n")
        values = maps[map_name]
        for row in range(0, 256, 8):
            out("    " + " ".join(f"0x{v:04x}," for v in values[row : row + 8]) + "\n")
        out("];\n\n")

    out(f"/// The layout as the kernel would hold it.\n")
    out(f"pub static LAYOUT: Layout = Layout {{\n")
    out(f'    name: "{name}",\n')
    out("    tables: &[\n")
    for index, map_name in enumerate(MAP_NAMES):
        if map_name is None or map_name not in maps:
            continue
        out(f"        ({index}, &{map_name.upper()}),\n")
    out("    ],\n")
    out("    accents: &[\n")
    for diacr, base, result in accents:
        out(f"        Accent {{ dead: 0x{diacr:02x}, base: 0x{base:02x}, made: 0x{result:04x} }},\n")
    out("    ],\n")
    out("};\n")


if __name__ == "__main__":
    main()
