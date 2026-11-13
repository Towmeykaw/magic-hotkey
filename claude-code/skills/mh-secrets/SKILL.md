---
name: mh-secrets
description: Use API keys, tokens and passwords stored in Magic Hotkey (the `mh` CLI) without their values ever entering the conversation. TRIGGER when a command needs a secret, token, API key, password or credential; when the user says a secret is "in mh", "in Magic Hotkey" or "in the keychain"; or when the user offers to give you a secret. Never ask the user to paste a secret into the chat.
---

# Secrets through mh

`mh` keeps secrets in the OS keychain. You never need to see a value to use it: `mh exec` hands it to the command that needs it and masks it in everything that command prints.

## Find what exists

```bash
mh secret list
```

Prints names only, never values. Temporary secrets show their expiry.

## Use a secret

```bash
mh exec -e GH_TOKEN=github_pat -- gh api /user
mh exec -e T=api_token -- sh -c 'curl -sS -H "Authorization: Bearer $T" https://api.example.com/me'
mh exec -e DB_PASSWORD=prod_db -e DB_USER=prod_db_user -- npm run migrate
mh exec --env-file .mhenv -- npm start
```

- `-e VAR=secret_name` puts the secret in the command's environment as `VAR`. Repeat it for several.
- When the command needs the value in an argument, run it through `sh -c '...'` with **single quotes**, so `$VAR` is expanded inside the command and not by the outer shell (which doesn't have it).
- `.mhenv` holds `VAR=secret_name` lines (names only, safe to commit).
- In the output, a secret shows up as `<masked:NAME>`. Its URL-encoded, hex and base64 forms are masked too, including inside a base64 Basic-auth header.
- The exit code is the command's own.

## Never

- `mh secret get`: it prints the value.
- `mh exec --no-mask`: it turns masking off.
- `mh run` on a pipeline that has a `secret` step: it prints the result.
- Anything that disguises a value so masking can't recognise it, such as reversing it, slicing it, hashing it or encrypting it, or writing it to a file you then read.

A hook refuses the first three. Treat a refusal as a reason to switch to `mh exec`, not to find another route.

## When the secret isn't stored yet

Ask the user to store it, and offer the two ways. Neither puts the value in the chat:

1. **Temporary, for this task:** the user copies the value to the clipboard and tells you. Then run:
   ```bash
   mh secret set <name> --ttl 1h --from-clipboard
   ```
   This reads the clipboard, stores the value, then clears the clipboard. Pick a TTL that covers the task (`30m`, `2h`, `1d`). The secret is deleted when it expires, and on Windows also at sign-out.
2. **Permanent:** the user adds it themselves, in the Magic Hotkey app (Settings → Secrets) or in their own terminal with `mh secret set <name>`.

When you're done with a temporary secret, delete it with `mh secret rm <name>`, or leave it to expire.

## If `mh` isn't found

On Windows it's installed at `C:\Program Files\Magic Hotkey\mh.exe`. Recent installers add that folder to PATH, but a session started before the install doesn't see the change. Until this session restarts, call it by full path:
`"/c/Program Files/Magic Hotkey/mh.exe" secret list`.

On macOS it's inside the app bundle: `"/Applications/Magic Hotkey.app/Contents/MacOS/mh"`.
