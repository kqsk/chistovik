# Contributing / Как помочь

Спасибо за интерес к Чистовику! / Thanks for your interest in Chistovik!

## Before you start / Перед началом

- Bugs and ideas: open an [issue](../../issues/new/choose) first. / Ошибки и идеи — сначала issue.
- Small, focused pull requests are easiest to review. / Небольшие PR по одной теме.

## Setup / Окружение

Windows 10/11, Node.js 22+, Rust stable, [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm ci
npm run tauri dev
```

## Checks (CI runs the same) / Проверки (то же, что в CI)

```bash
npm run lint
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## Rules / Правила

- API keys only in Windows Credential Manager; never in settings files, logs or the webview.
- User text never goes to logs; on disk only in the history database.
- All LLM requests are made in Rust, never from the webview.
- UI strings live in `src/shared/locales/ru.ts` and `en.ts` (both languages).
- Keep it light: no heavy UI kits, CSS-only animations ≤ 200 ms.

## Releases / Выпуск

Maintainers: bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, then push a tag `vX.Y.Z`. The Release workflow builds the installer, signs the update and publishes `latest.json`.
