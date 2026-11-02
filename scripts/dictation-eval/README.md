# Dictation eval (#1749)

Measures which on-device engine writes the owner's Oby requests best, before dictation is built on
one (#1748). Nothing leaves the Mac, and the recordings stay out of this repository.

## Record

Put about 30 real requests to Oby in `~/oby-dictation-eval/clips/`, in English (Oby is English only,
the owner's decision of 2026-11-01):

- `en-01.wav`, `en-02.wav` …
- Beside each, `en-01.txt` with exactly what you said.
- Any format macOS reads (`.wav`, `.m4a` renamed is fine); up to 60 s each. QuickTime Player's
  New Audio Recording works; so does `ffmpeg -f avfoundation -i ":0" -ac 1 -ar 16000 en-01.wav`.

Optionally `~/oby-dictation-eval/terms.txt`: the words that matter, one per line (repo names,
agents, people, branch names). They go to each engine as hints and are scored on their own.

## Run

```bash
scripts/dictation-eval/run.sh
```

It prints word error rate, terms missed and seconds per clip, for each engine:

- `apple-en`: Apple's SpeechTranscriber in en-US, what the iPhone uses. It ships nothing with Oby,
  so it's the pick unless Parakeet clearly beats it on your recordings.
- `parakeet`: NVIDIA Parakeet TDT 0.6B v3 through FluidAudio's Core ML build. The first run builds
  it and downloads the model (471 MB) from Hugging Face; after that it runs offline. No hints yet.

The scorer's checks: `cd scripts/dictation-eval && python3 test_score.py`.
