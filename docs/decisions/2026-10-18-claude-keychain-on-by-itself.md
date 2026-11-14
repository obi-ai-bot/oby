# `claudeKeychain` turns on by itself on macOS

- **Date:** 2026-10-18
- **Status:** In force
- **Replaced:** Nothing AGENTS.md names
- **Changed by:** Nothing
- **Contracts and scenarios:** None named in AGENTS.md

## The owner's words

> (the owner's decision of 2026-10-18: obid turns it on by itself on macOS when it's left out of `config.json` and Claude Code's default login is only in the Keychain, asking `security` only whether the item exists; `false` keeps it off; once on, set or by itself, it covers every Claude Code account, those with a `configDir` too, each with its own Keychain item)

AGENTS.md's wording, not a quote.

## Where AGENTS.md states it

- Rules Oby never breaks

## Notes

Amends [2026-10-17-claude-keychain-login-copy.md](2026-10-17-claude-keychain-login-copy.md): it says when `claudeKeychain`, the setting that decision introduced, is on.
