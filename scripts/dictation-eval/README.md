# Dictation eval (#1749)

Measures which on-device engine writes the owner's Oby requests best, before dictation is built on
one (#1748). Nothing leaves the Mac, and the recordings stay out of this repository.

## Record

Put about 30 real requests to Oby in `~/oby-dictation-eval/clips/`, 10 of each group:

- `pt-01.wav` … Portuguese, `en-01.wav` … English, `mix-01.wav` … Portuguese with English words.
- Beside each, `pt-01.txt` with exactly what you said.
- Any format macOS reads (`.wav`, `.m4a` renamed is fine); up to 60 s each. QuickTime Player's
  New Audio Recording works; so does `ffmpeg -f avfoundation -i ":0" -ac 1 -ar 16000 pt-01.wav`.

Optionally `~/oby-dictation-eval/terms.txt`: the words that matter, one per line (repo names,
agents, people, branch names). They go to each engine as hints and are scored on their own.

## Run

```bash
scripts/dictation-eval/run.sh
```

It prints word error rate, terms missed and seconds per clip, for each engine and group:

- `apple-pt`, `apple-en`: Apple's SpeechTranscriber in one locale (what the iPhone uses today).
- `apple-best`: both locales on the same clip, keeping the more confident.
- `parakeet`: NVIDIA Parakeet TDT 0.6B v3 (25 European languages, Portuguese among them, language
  detected) through FluidAudio's Core ML build. The first run builds it and downloads the model from
  Hugging Face; after that it runs offline. No hints yet.

Whisper isn't here: Parakeet is smaller and faster at similar accuracy (#1749). It comes back only if
neither engine handles mixed speech.

The scorer's checks: `cd scripts/dictation-eval && python3 test_score.py`.
