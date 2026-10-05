<!-- GSD:project-start source:PROJECT.md -->
## Project

**Чистовик (Chistovik)**

Чистовик — лёгкое Windows-приложение для работы с текстом через нейросеть в любом приложении. Пользователь выделяет текст (или просто стоит в поле ввода), жмёт глобальную горячую клавишу — появляется красивая быстрая палитра с вариантами: исправить орфографию/пунктуацию, переписать в моём стиле, улучшить, сократить, свой промпт и т.д. Результат вставляется обратно на место исходного текста. Аналог браузерных расширений MaxAI / Sider, но системный — работает везде в Windows. Open source, публикуется на GitHub.

**Core Value:** Выделил текст → хоткей → через секунду-две в том же месте лежит исправленный текст. Мгновенный отклик, без лишних кликов, в любом приложении.

### Constraints

- **Платформа**: Windows 10/11 x64 — целевая ОС пользователя
- **Стек**: Tauri 2 (Rust + React/TypeScript) — минимальный размер и RAM, переиспользование кода Handy
- **Производительность**: палитра появляется < 100 мс после хоткея; бинарник ~10 МБ; RAM в простое ~30–50 МБ; анимации ≤ 200 мс — пользователь требует «супербыстро, суперлегко, без лишней херни»
- **Безопасность**: API-ключи только в Windows Credential Manager; текст не логируется на диск, кроме истории (локально, с возможностью очистки)
- **Дизайн**: гибрид Apple и Windows 11, ближе к Apple — стекло (Acrylic/Mica), мягкие тени, пружинные анимации, крупные скругления; при этом системный акцент и тема Windows
- **Open source**: публичный репозиторий GitHub, лицензия MIT, чистая структура, CI, README RU/EN
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Rust (src-tauri)
| Crate | Version | Why |
|---|---|---|
| tauri | 2.12 | Core. Features: `tray-icon`, `image-png`; drop unused (no `devtools` in release) |
| tauri-plugin-global-shortcut | 2.4 | Handy uses 2.3.1; bump, same API. Re-register on the fly |
| tauri-plugin-store | 2.5 | settings.json (non-secret). Keys NOT here |
| tauri-plugin-autostart | 2.7 | Autostart (HKCU Run); launch with `--hidden` arg |
| tauri-plugin-updater | 2.13 | GitHub Releases `latest.json`, minisign signatures |
| tauri-plugin-process | 2.4 | `relaunch()` after update |
| tauri-plugin-single-instance | 2.5 | Second launch -> show settings. Register FIRST |
| tauri-plugin-clipboard-manager | 2.4 | NOT needed for paste (use Handy's paste_tx/win32). Only for the "Copy" button; or use arboard/win32 directly |
| keyring | 4.2 | Windows Credential Manager. 4.x moved stores into separate crates (`windows-native-keyring-store ^1` listed as dep) — LOW confidence on exact feature wiring; spike in phase 1, fallback pin `keyring = "3.6"` with `windows-native` feature. One entry per provider: service `chistovik`, user `provider:<id>` |
| window-vibrancy | 0.8 | `apply_acrylic` / `apply_mica` (Win10 1809+/Win11). Acrylic on palette, Mica on settings |
| windows | 0.62 | UIA (`Win32_UI_Accessibility`), `GetGUIThreadInfo`, `SetForegroundWindow`, `SendInput`, clipboard. Enable only needed features (compile time/size). Handy's pinned version may differ — keep Handy's if copying code verbatim, bump later |
| webview2-com | match tauri's | Only if keeping Handy's accelerator-key interception |
| reqwest | 0.12 (Handy) — 0.13 exists | Use `default-features=false, features=["json","stream","rustls-tls"]`. Stay 0.12 initially (code reuse), bump later |
| eventsource-stream | 0.2.3 | Parse SSE from `bytes_stream()`. Tiny; OpenAI SSE is simple enough that a manual `data: ` line parser (~30 lines) is also fine — **decide: manual parser** (no dep, handles `[DONE]`, tolerant of Ollama/proxies) |
| enigo | Handy's | Ctrl+V only if needed; prefer raw `SendInput` with explicit modifier release (see PITFALLS) |
| rusqlite (`bundled`) | latest | History DB — **decision: SQLite**, see ARCHITECTURE. (JSON rewrite on each entry + cap/search/clear is worse; +~1MB) |
| serde, serde_json, tokio, tokio-util (CancellationToken), thiserror, tracing | latest | standard |
## Frontend
| Lib | Version | Decision |
|---|---|---|
| React | 19.x | **Keep React** (19, not Solid/Svelte). Reason: reuse Handy components/stores/i18n, team knowledge; WebView2 cost dominates RAM/start, not framework (~45KB vs ~7KB gz irrelevant). Palette speed comes from pre-created hidden window, not framework. Upgrade 18->19 is low-risk for greenfield |
| Vite | 7 or 8 (8.3 latest) | Handy uses 6; use latest stable that `@vitejs/plugin-react` supports; two HTML entries (palette.html, settings.html) |
| TypeScript | 5.x | |
| Tailwind | 4.3 (`@tailwindcss/vite`) | CSS vars for accent/theme |
| Zustand | 5.0 | Small stores; palette store separate from settings store |
| i18next + react-i18next | 26 / 17 | RU+EN JSON; bundle both statically (no http backend) |
| lucide-react | 1.x | Import named icons only (tree-shaken). Custom-icon picker: need a dynamic list — use `lucide-react/dynamicIconImports` ONLY in settings window, never palette; or curate ~150 icons statically (**recommended**, keeps bundle small) |
| Animation | **CSS-only** (+ `linear()`/cubic-bezier spring approximation) | Palette is one component, ≤200ms. `motion` 14 adds ~15-30KB and JS-driven frames; use only in settings if ever. Use `@starting-style`/keyframes + `prefers-reduced-motion` |
| @tauri-apps/api | 2.12 | Match tauri crate minor |
| @tauri-apps/plugin-* | match Rust | |
| Biome or ESLint+Prettier | — | pick Biome (single binary, fast) |
| Vitest | latest | store/util tests |
## What NOT to use
- Electron / WPF / WinUI — rejected by constraints.
- tauri-plugin-clipboard-manager for paste — can't do delayed-render/multi-format restore.
- Plain-text keys in store/localStorage; `tauri-plugin-stronghold` (heavy, password UX).
- UIA-only without Ctrl+C fallback (see PITFALLS).
- Heavy UI kits (MUI, Mantine, Radix full) — use headless bits only if needed (cmdk-style list is ~80 lines by hand).
- framer-motion / big router / react-query; Redux.
- Tauri `decorations` + custom titlebars for palette — use `decorations:false, transparent:true, shadow` handled by acrylic; no CSS blur-backdrop as glass (WebView2 can't blur desktop).
- openai/ai SDKs on frontend — all LLM calls happen in Rust (keys never reach the webview).
- `unsafe` global keyboard hooks (WH_KEYBOARD_LL) — AV flag magnet; RegisterHotKey via global-shortcut plugin suffices.
## Install
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
