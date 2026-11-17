"""Count which words follow which in a text corpus, and write the pairs the Zhuyin engine uses.

The engine scores each word by how common it is on its own. That cannot tell 長文本 from 常文本:
both split into a character and the word 文本. This script reads running text, cuts it into the
words of the word list, and records word pairs that occur together far more often than chance.

Input: a Wikipedia XML dump (.bz2) or any UTF-8 text file. Output: one pair per line,
`first<TAB>second<TAB>bonus`, where the bonus is how many powers of ten more often the two words
meet than their separate frequencies predict. Only the statistics are kept, never the text.

    python build_pairs.py phrases.txt corpus.xml.bz2 pairs.txt
"""
import argparse
import bz2
import collections
import ctypes
import math
import re
import sys

LONGEST = 8
KEEP = 150_000
FEWEST = 8
LOWEST_BONUS = 0.4
HIGHEST_BONUS = 3.0

CJK = re.compile(r'[一-鿿]+')
MARKUP = [
    (re.compile(r'<ref[^>]*?/>|<ref.*?</ref>|<!--.*?-->|<[^>]+>', re.S), ' '),
    (re.compile(r'\{\{[^{}]*\}\}'), ' '),
    (re.compile(r'\{\|.*?\|\}', re.S), ' '),
    (re.compile(r'\[\[(?:File|Image|Category|檔案|文件|分類|分类):[^\]]*\]\]', re.I), ' '),
    (re.compile(r'\[\[[^\]|]*\|([^\]]*)\]\]'), r'\1'),
    (re.compile(r'\[\[([^\]]*)\]\]'), r'\1'),
    (re.compile(r"'{2,}|={2,}"), ' '),
    # Variant markup holds both scripts side by side; keep neither.
    (re.compile(r'-\{.*?\}-', re.S), ' '),
]


def traditional(text):
    """Character-level conversion by Windows, the same call the engine uses."""
    if not text:
        return text
    size = len(text) * 2 + 1
    out = ctypes.create_unicode_buffer(size)
    written = ctypes.windll.kernel32.LCMapStringEx(None, 0x04000000, text, len(text), out, size, None, None, 0)
    return out.value[:written] if written > 0 else text


def word_scores(path):
    """The best score of each word in the word list, whatever its reading."""
    scores = {}
    with open(path, encoding='utf-8') as lines:
        for line in lines:
            parts = line.split()
            if len(parts) < 3 or ord(parts[0][0]) < 0x80:
                continue
            try:
                score = float(parts[2])
            except ValueError:
                continue
            word = parts[1]
            if len(word) <= LONGEST and score > scores.get(word, -99.0):
                scores[word] = score
    return scores


def segment(run, scores):
    """Cuts a run of characters into the most likely sequence of listed words."""
    size = len(run)
    best = [0.0] * (size + 1)
    step = [1] * (size + 1)
    for at in range(size - 1, -1, -1):
        top = -1e9
        for length in range(1, min(LONGEST, size - at) + 1):
            score = scores.get(run[at:at + length])
            if score is None:
                if length > 1:
                    continue
                score = -12.0
            total = score + best[at + length]
            if total > top:
                top, step[at] = total, length
        best[at] = top
    words, at = [], 0
    while at < size:
        words.append(run[at:at + step[at]])
        at += step[at]
    return words


def pages(path):
    """Yields the text of each article in a Wikipedia dump, or the whole of a plain text file."""
    if not path.endswith('.bz2'):
        with open(path, encoding='utf-8', errors='ignore') as plain:
            for line in plain:
                yield line
        return
    page, inside = [], False
    with bz2.open(path, 'rt', encoding='utf-8', errors='ignore') as dump:
        for line in dump:
            if '<text' in line:
                inside, line = True, line.split('>', 1)[-1]
            if inside:
                if '</text>' in line:
                    page.append(line.split('</text>')[0])
                    yield ''.join(page)
                    page, inside = [], False
                else:
                    page.append(line)


def count(corpus, scores, limit):
    singles, pairs, seen = collections.Counter(), collections.Counter(), 0
    for number, page in enumerate(pages(corpus)):
        # Templates nest, and each pass removes the innermost ones.
        for pattern, replacement in MARKUP + MARKUP[1:2] * 2:
            page = pattern.sub(replacement, page)
        for run in CJK.findall(traditional(page)):
            if len(run) < 2:
                continue
            words = segment(run, scores)
            singles.update(words)
            pairs.update(zip(words, words[1:]))
            seen += len(run)
        if number % 2000 == 0:
            print(f'{number} pages, {seen:,} characters, {len(pairs):,} pairs', file=sys.stderr, flush=True)
        if seen >= limit:
            break
    return singles, pairs


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('word_list')
    parser.add_argument('corpus')
    parser.add_argument('output')
    parser.add_argument('--characters', type=int, default=80_000_000, help='stop after this many Chinese characters')
    args = parser.parse_args()
    singles, pairs = count(args.corpus, word_scores(args.word_list), args.characters)
    total = sum(singles.values())
    kept = []
    for (first, second), together in pairs.items():
        if together < FEWEST:
            continue
        bonus = math.log10(together * total / (singles[first] * singles[second]))
        if bonus >= LOWEST_BONUS:
            # Pairs seen often matter more than rare ones with a high ratio.
            kept.append((together * min(bonus, HIGHEST_BONUS), first, second, min(bonus, HIGHEST_BONUS)))
    kept.sort(reverse=True)
    with open(args.output, 'w', encoding='utf-8', newline='\n') as out:
        for _, first, second, bonus in sorted(kept[:KEEP], key=lambda item: (item[1], item[2])):
            out.write(f'{first}\t{second}\t{bonus:.2f}\n')
    print(f'{total:,} words, {len(pairs):,} distinct pairs, {min(len(kept), KEEP):,} written', file=sys.stderr)


if __name__ == '__main__':
    main()
