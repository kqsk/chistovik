# Chistovik

[Русская версия](README.md)

**Select text → press a shortcut → a second or two later the improved text is in the same place.** In any Windows app: browser, Word, Telegram, Notepad, mail.

Chistovik ("clean copy" in Russian) is a lightweight Windows 10/11 app that works with text through an AI provider (OpenAI, OpenRouter, DeepSeek, Groq, Ollama or any OpenAI-compatible service). Like the MaxAI or Sider browser extensions, but system-wide.

> ⚠️ **Early version (0.1).** The core works, rough edges are possible. Please report problems in [Issues](../../issues).

![Command palette](docs/screenshots/palette.png)

## Features

- **Command palette** on `Ctrl+Alt+Space`: fix spelling, improve, shorten, translate RU↔EN, business/friendly tone, as a list and more — 12 built-in commands.
- **Your own commands:** name, icon, prompt text, a shortcut and even a model of their own.
- **One-off instruction:** type what to do with the text in the palette and press Enter.
- **Preview:** look at the result first and press "Accept" — or replace right away.
- **Several providers** and a model per command.
- **History:** before and after, search, copy. Stored only on your computer.
- **Undo the last replacement** with `Ctrl+Alt+Z`.
- Russian and English UI, autostart, auto-update.

## Install

1. Open the [latest release](../../releases/latest) and download `Chistovik_…_x64-setup.exe`.
2. Run it. No administrator rights needed.
3. Windows may show a blue "Windows protected your PC" window — the app has no paid code-signing certificate yet. Click **"More info" → "Run anyway"**.

## First setup

1. After installation the settings open on the **Providers** tab.
2. Click **Add** and pick a service (e.g. OpenAI) — the address is filled in for you.
3. Paste the **API key** and click **Check** — the model list loads. Pick a model and **Save**.

The key is stored in **Windows Credential Manager**, never in files. Ollama on the same computer needs no key.

## Usage

| Keys | Action |
|---|---|
| `Ctrl+Alt+Space` | Open the command palette |
| `Ctrl+Alt+F` | Fix spelling and punctuation right away |
| `Ctrl+Alt+Z` | Put back the text from before the last replacement |
| `↑` `↓` `Enter`, digits `1`–`9` | Pick a command in the palette |
| `Esc` | Close the palette or cancel the request |

Select text (or just put the caret in a field — then the whole field is processed) and press the shortcut. Command shortcuts and the undo shortcut can be changed in Settings.

**Your own prompt.** On the Prompts tab click "+ New". Write `{{text}}` where the selected text should go; without it the text is added at the end.

## Privacy

- Text is sent only to the AI provider you chose.
- Text never goes to the app logs. Locally it is kept only in the history, which you can turn off or clear.
- Keys live only in Windows Credential Manager.

## Build from source

You need Node.js 22+, Rust (stable) and the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/).

```bash
npm ci
npm run tauri dev     # development run
npm run tauri build   # installer in src-tauri/target/release/bundle/nsis
```

Checks: `npm test`, `npm run lint`, `cargo test --manifest-path src-tauri/Cargo.toml`. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE). Parts of the code are based on [Handy](https://github.com/cjpais/Handy) (MIT), see [NOTICE](NOTICE).
