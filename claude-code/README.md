# mh-secrets

A Claude Code plugin for [Magic Hotkey](https://github.com/Towmeykaw/magic-hotkey)'s `mh` command-line tool. It lets Claude use the API keys, tokens and passwords in your OS keychain without their values ever entering the conversation.

## What it includes

- **Skill `mh-secrets`.** It teaches Claude to:
  - find secrets with `mh secret list` (names only)
  - pass them to commands with `mh exec`, which masks their values in the output
  - store a temporary secret from the clipboard, so the value never appears in the chat
- **A guard on Claude's Bash tool.** It refuses `mh secret get`, `mh exec --no-mask`, and `mh run` on a pipeline that has a secret step, and tells Claude to use `mh exec` instead. Commands that don't mention `mh` aren't affected.

## What it reads, runs and sends

- **Environment variables:** reads `APPDATA`, `HOME` and `XDG_CONFIG_HOME` to find Magic Hotkey's config folder.
- **Pipeline config:** reads `magic-hotkey/commands.json` in that folder, only when a command contains `mh run`, to check whether the pipeline reads a secret. The file holds pipeline names and steps, never secret values.
- **Nothing else:** the guard runs no programs and makes no network requests. The plugin stores and sends no data.

## Requirements

Magic Hotkey 0.4.0 or later, with `mh` on your PATH. The Windows installer adds it to PATH. On macOS, see the main README.

## Install

At the prompt of a Claude Code terminal session:

```
/plugin install mh-secrets --marketplace Towmeykaw/magic-hotkey
```

## Limits

The guard prevents accidental leaks. It isn't a sandbox: Claude runs as you, so it could still reach the keychain in other ways.

## License

MIT
