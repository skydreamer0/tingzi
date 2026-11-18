"""Writes the design tokens for every window from one file.

    python assets/design/build_tokens.py          rewrites app/ui/tokens.css and windows/palette.h
    python assets/design/build_tokens.py --check  exits 1 if either file is out of date

The voice window and settings read tokens.css; the candidate window and composition line,
which Windows draws without a browser, read palette.h.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "assets" / "design" / "tokens.json"
CSS = ROOT / "app" / "ui" / "tokens.css"
HEADER = ROOT / "windows" / "palette.h"


def rgb(colour):
    colour = colour.lstrip("#")
    return tuple(int(colour[i:i + 2], 16) for i in (0, 2, 4))


def mix(first, second, percent_first):
    """Like CSS color-mix(in srgb, first percent, second)."""
    a, b = rgb(first), rgb(second)
    share = percent_first / 100
    return "#" + "".join(f"{round(x * share + y * (1 - share)):02x}" for x, y in zip(a, b))


def theme(tokens, name):
    colours = dict(tokens["color"][name])
    derived = tokens["color"]["derived"]
    colours["tint"] = mix(colours["accent"], colours["surface"], derived["tint"][name])
    colours["accent-text"] = mix("#ffffff", colours["accent"], derived["accent-text"][name])
    return colours


def css_block(tokens, name):
    derived = tokens["color"]["derived"]
    lines = [f"  --{key}: {value};" for key, value in tokens["color"][name].items()]
    # Written as color-mix so they follow the accent when the user picks Windows' accent colour.
    lines.append(f"  --tint: color-mix(in srgb, var(--accent) {derived['tint'][name]}%, var(--surface));")
    lines.append(f"  --accent-text: color-mix(in srgb, #ffffff {derived['accent-text'][name]}%, var(--accent));")
    return "\n".join(lines)


def build_css(tokens):
    shared = [f"  --{key}: {value};" for key, value in tokens["color"]["brand"].items()]
    shared += [f"  --radius-{key}: {value}px;" for key, value in tokens["radius"].items()]
    shared += [f"  --font-{key}: {value}px;" for key, value in tokens["font"].items()]
    shared += [f"  --size-{key}: {value}px;" for key, value in tokens["size"].items()]
    shared += [f"  --motion-{key}: {value}ms;" for key, value in tokens["motion"].items() if key != "about"]
    dark = css_block(tokens, "dark")
    return (
        "/* Written by assets/design/build_tokens.py from assets/design/tokens.json; edit that file instead. */\n"
        ":root {\n  color-scheme: light;\n"
        + css_block(tokens, "light") + "\n" + "\n".join(shared) + "\n  --preview-size: 20px;\n}\n"
        "@media (prefers-color-scheme: dark) {\n  :root:not([data-theme=\"light\"]) {\n    color-scheme: dark;\n"
        + "\n".join("  " + line for line in dark.splitlines()) + "\n  }\n}\n"
        ":root[data-theme=\"dark\"] {\n  color-scheme: dark;\n" + dark + "\n}\n"
    )


def build_header(tokens):
    def palette(name):
        colours = theme(tokens, name)
        picked = [colours[key] for key in ("surface", "text", "muted", "border", "tint", "accent-text")]
        return "Palette{" + ",".join("RGB({},{},{})".format(*rgb(colour)) for colour in picked) + "}"

    return (
        "// Written by assets/design/build_tokens.py from assets/design/tokens.json; edit that file instead.\n"
        "#pragma once\n#include <windows.h>\n\n"
        "namespace tingzi {\n"
        "// The window's background, its text and quieter text, its edge, the chosen item's background and text.\n"
        "struct Palette { COLORREF back, text, muted, frame, active, activeText; };\n"
        f"inline constexpr Palette lightPalette={palette('light')};\n"
        f"inline constexpr Palette darkPalette={palette('dark')};\n"
        f"inline constexpr int cornerRadius={tokens['radius']['lg']};\n"
        f"inline constexpr int chipRadius={tokens['radius']['sm']};\n"
        "}\n"
    )


def main():
    tokens = json.loads(SOURCE.read_text(encoding="utf-8"))
    outputs = {CSS: build_css(tokens), HEADER: build_header(tokens)}
    if "--check" in sys.argv[1:]:
        stale = [path for path, text in outputs.items() if not path.exists() or path.read_text(encoding="utf-8") != text]
        for path in stale:
            print(f"out of date: {path.relative_to(ROOT)}")
        return 1 if stale else 0
    for path, text in outputs.items():
        path.write_text(text, encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
