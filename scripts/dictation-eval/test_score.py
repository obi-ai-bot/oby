#!/usr/bin/env python3
"""The scorer's own checks, named after #1749's scenarios: python3 test_score.py"""
import json
import tempfile
from pathlib import Path

from score import edit_distance, missed_terms, score, words


def test_dicteval_1_wer_on_normalized_words():
    assert words("Open the PR, please!") == ["open", "the", "pr", "please"]
    assert words("Café") == ["cafe"]
    assert edit_distance(words("merge it into dev"), words("Merge it in to dev.")) == 2
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        (root / "clips").mkdir()
        (root / "results" / "apple-en").mkdir(parents=True)
        (root / "clips" / "en-01.txt").write_text("open the pull request")
        (root / "clips" / "en-02.txt").write_text("close the task")
        (root / "results" / "apple-en" / "en-01.json").write_text(json.dumps({"text": "Open the pull request.", "seconds": 1.0}))
        # en-02 failed in this engine: all three words count as wrong
        [(engine, group, g)] = score(root)
        assert (engine, group) == ("apple-en", "en")
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
