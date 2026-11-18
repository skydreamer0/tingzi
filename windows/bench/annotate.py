"""Suggests the Zhuyin reading of each sentence, to help write cases for sentences.tsv.

    python windows/bench/annotate.py <folder with phrases.txt> < lines.txt

Each input line is "category<TAB>text". Each output line is a row for sentences.tsv. The reading
is the most common one in the word list, so check characters with several readings (了, 的, 行,
長, 重, 還 and the like) by hand before adding a row. Latin letters, digits and spaces are kept as
they are and pass through the benchmark unconverted.
"""
import sys
from pathlib import Path


def load(folder):
    best = {}
    with open(Path(folder) / "phrases.txt", encoding="utf-8") as lines:
        for line in lines:
            parts = line.split()
            if len(parts) != 3 or not "㄀" <= parts[0][0] <= "ㄯ":
                continue
            reading, text, score = parts[0], parts[1], float(parts[2])
            if len(text) != reading.count("-") + 1:
                continue
            if text not in best or score > best[text][1]:
                best[text] = (reading, score)
    return {text: reading for text, (reading, _) in best.items()}


def annotate(text, readings, longest=6):
    tokens, at = [], 0
    while at < len(text):
        if text[at].isascii():
            end = at
            while end < len(text) and text[end].isascii():
                end += 1
            word = text[at:end].strip()
            if word:
                tokens.append(word)
            at = end
            continue
        for size in range(min(longest, len(text) - at), 0, -1):
            piece = text[at:at + size]
            if piece in readings:
                tokens.extend(readings[piece].split("-"))
                at += size
                break
        else:
            tokens.append("?" + text[at])
            at += 1
    return " ".join(tokens)


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    readings = load(sys.argv[1])
    for line in sys.stdin:
        line = line.rstrip("\n")
        if not line.strip() or line.startswith("#"):
            continue
        category, text = line.split("\t", 1)
        print(f"{category}\t{text}\t{annotate(text, readings)}")


if __name__ == "__main__":
    main()
