#!/usr/bin/env python3
"""The scorer's own checks, named after #1749's scenarios: python3 test_score.py"""
import json
import tempfile
from pathlib import Path

from score import edit_distance, missed_terms, score, words


def test_dicteval_1_wer_on_normalized_words():
    assert words("Abre o PR, por favor!") == ["abre", "o", "pr", "por", "favor"]
    assert words("Ação") == ["acao"]
    assert edit_distance(words("merge it into dev"), words("Merge it in to dev.")) == 2
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        (root / "clips").mkdir()
        (root / "results" / "apple-pt").mkdir(parents=True)
        (root / "clips" / "pt-01.txt").write_text("abre o pull request")
        (root / "clips" / "pt-02.txt").write_text("fecha a tarefa")
        (root / "results" / "apple-pt" / "pt-01.json").write_text(json.dumps({"text": "Abre o pull request.", "seconds": 1.0}))
        # pt-02 failed in this engine: all three words count as wrong
        [(engine, group, g)] = score(root)
        assert (engine, group) == ("apple-pt", "pt")
        assert (g["errors"], g["words"]) == (3, 7)


def test_dicteval_4_terms_missed():
    terms = ["Deskree", "Claude Code", "oby"]
    assert missed_terms("open Deskree in Claude Code", "open desk tree in Claude Code", terms) == ["Deskree"]
    assert missed_terms("open oby", "open Oby", terms) == []


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
            print("ok", name)
