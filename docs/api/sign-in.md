### obid only: sign-in reasons

A `reason` that isn't about starting: `sameAccount` (above, "The same login twice"), with `sameAs`.

Why a sign-in didn't start, as one stable word beside the `error` of `GET` and `POST /v1/accounts/{id}/sign-in`, `POST /v1/github/sign-in` (and the reviewer's) and `POST /v1/tetrix/sign-in` (#1579, docs/contracts/sign-in.md): `{"error": "…", "reason": "notInstalled"}`. The apps show their own words for it under the row, never `error`, which can name a folder. Reasons are add-only: never renamed or removed, and an app shows its plain sentence for one it doesn't know.

| `reason` | When |
|---|---|
| `notInstalled` | the tool's CLI, or gh, isn't found by an absolute path (`503`) |
| `ownLogin` | the tool's own login, signed in with the tool itself (`409`) |
| `folderUnsafe` | the account's login file, its lock or its folder is a link or not the person's (`409`), or its folder isn't one or can't be made (`502`) |
| `keychainOff` | Claude Code on macOS without `claudeKeychain` (`409`) |
| `busy` | a job of the account is queued or running, it's being removed, or GitHub's sign-in is starting (`409`) |
| `didNotStart` | the login, or gh, gave no link or code, or couldn't start; Tetrix's sign-in server refused to register Oby or couldn't be reached to (`502`) |
| `codeRefused` | Claude Code didn't sign in with the code sent (`422`) |
| `badAddress` | a Tetrix URL that breaks a connection's rules, or an unknown preset (`422`) |
| `noSignIn` | that Tetrix's metadata, sign-in server or client can't be found or used (`502`) |

