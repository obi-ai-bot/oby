#!/usr/bin/env python3
"""Scores dictation engines against the owner's reference text (DICTEVAL-1, DICTEVAL-4).

    score.py <eval dir>

<eval dir>/clips/<name>.txt are the references; <eval dir>/results/<engine>/<name>.json are what
run.sh wrote. A clip's group is its name's prefix before the first "-" (en). An optional
<eval dir>/terms.txt lists the words that matter (repos, agents, people), one per line.
"""
import json
import statistics
import sys
import unicodedata
from pathlib import Path


def words(text):
    """Lowercase, accents and punctuation folded, so only the words count."""
    text = unicodedata.normalize("NFKD", text.lower())
    text = "".join(c for c in text if not unicodedata.combining(c))
    text = "".join(c if c.isalnum() else " " for c in text)
    return text.split()


def edit_distance(ref, hyp):
    row = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        prev, row[0] = row[0], i
        for j, h in enumerate(hyp, 1):
            prev, row[j] = row[j], min(row[j] + 1, row[j - 1] + 1, prev + (r != h))
    return row[-1]


def missed_terms(ref, hyp, terms):
    """Terms the reference says that the hypothesis doesn't (a term may be several words)."""
    def has(text, term):
        return f" {' '.join(words(term))} " in f" {' '.join(words(text))} "
    return [t for t in terms if has(ref, t) and not has(hyp, t)]


def score(root):
    root = Path(root)
    terms_file = root / "terms.txt"
    terms = [t for t in terms_file.read_text().splitlines() if t.strip()] if terms_file.exists() else []
    refs = {p.stem: p.read_text() for p in sorted((root / "clips").glob("*.txt"))}
    table = []
    for engine_dir in sorted(p for p in (root / "results").iterdir() if p.is_dir()):
        groups = {}
        for name, ref in refs.items():
            result_file = engine_dir / f"{name}.json"
            result = json.loads(result_file.read_text()) if result_file.exists() else None
            hyp = result["text"] if result else ""  # a failed clip counts as all words wrong
            g = groups.setdefault(name.split("-")[0], {"errors": 0, "words": 0, "missed": 0, "terms": 0, "secs": []})
            g["errors"] += edit_distance(words(ref), words(hyp)) if result else len(words(ref))
            g["words"] += len(words(ref))
            g["missed"] += len(missed_terms(ref, hyp, terms))
            g["terms"] += sum(1 for t in terms if f" {' '.join(words(t))} " in f" {' '.join(words(ref))} ")
            if result:
                g["secs"].append(result["seconds"])
        for group, g in sorted(groups.items()):
            table.append((engine_dir.name, group, g))
    return table


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    print(f"{'engine':<16} {'group':<6} {'WER':>6} {'terms missed':>13} {'median s':>9} {'worst s':>8}")
    for engine, group, g in score(sys.argv[1]):
        wer = g["errors"] / g["words"] if g["words"] else 0
        terms = f"{g['missed']}/{g['terms']}"
        med = statistics.median(g["secs"]) if g["secs"] else float("nan")
        worst = max(g["secs"]) if g["secs"] else float("nan")
        print(f"{engine:<16} {group:<6} {wer:>6.1%} {terms:>13} {med:>9.2f} {worst:>8.2f}")


if __name__ == "__main__":
    main()
