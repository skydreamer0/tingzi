"""Combine a locally supplied ordered CIN with the Tingzi fallback candidates."""

from pathlib import Path
from tempfile import NamedTemporaryFile
import argparse


def read_cin(path):
    text = path.read_text(encoding="utf-8-sig")
    header, section, body = text.partition("%chardef begin\n")
    rows, end, trailer = body.partition("%chardef end")
    if not section or not end or trailer.strip():
        raise ValueError(f"Invalid chardef section in {path}")
    keys = header.partition("%keyname begin\n")[2].partition("%keyname end")[0]
    layout = dict(line.split() for line in keys.splitlines() if line.strip())
    if not layout:
        raise ValueError(f"Missing key names in {path}")
    entries = {}
    for line in rows.splitlines():
        if not line.strip():
            continue
        code, value = line.split()
        if any(key not in layout for key in code):
            raise ValueError(f"Undefined key in {path}")
        entries.setdefault(code, {})[value] = None
    if not entries:
        raise ValueError(f"No candidates in {path}")
    return header, layout, entries


def merge(base, ordered, output):
    header, layout, fallback = read_cin(base)
    _, other_layout, candidates = read_cin(ordered)
    if layout != other_layout:
        raise ValueError("The two input tables use different key mappings")
    if output.resolve() in (base.resolve(), ordered.resolve()):
        raise ValueError("Choose an output separate from both inputs")
    for code, values in fallback.items():
        candidates.setdefault(code, {}).update(values)
    with NamedTemporaryFile("w", encoding="utf-8", newline="\n",
                            dir=output.parent, delete=False) as stream:
        tmp = Path(stream.name)
        try:
            stream.write(header + "%chardef begin\n")
            for code, values in candidates.items():
                for value in values:
                    stream.write(f"{code} {value}\n")
            stream.write("%chardef end\n")
        except BaseException:
            stream.close()
            tmp.unlink(missing_ok=True)
            raise
    try:
        tmp.replace(output)
    finally:
        tmp.unlink(missing_ok=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("base", "ordered", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    merge(args.base, args.ordered, args.output)
