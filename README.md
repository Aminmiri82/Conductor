# Conductor

A lightweight, fast, performance-focused alternative to Postman.

Conductor is a desktop API client built with Tauri, React, and Rust. It aims to stay snappy on large collections, keep memory usage low, and feel native, without the bloat of an Electron-based app.



> Conductor is in **active development**. Expect rough edges and breaking changes

## Current Features

- Native desktop app 
- Workspaces, collections, and requests
- Environment and variable management
- Keyboard-first UX with hotkeys
- UI Themes
- Postman collection import

## Getting started (development)

Prerequisites:

- [Node.js](https://nodejs.org/) 22.12+ on the 22.x line, 24.x, or 26+ (required by the development tools)
- [pnpm](https://pnpm.io/)
- [Rust toolchain](https://www.rust-lang.org/tools/install) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform

Then:

```bash
pnpm install
pnpm tauri dev
```

Development runs keep their data in `~/Library/Application Support/com.yaramiri.conductor.dev/` on macOS. The installed release app uses `~/Library/Application Support/com.yaramiri.conductor/`.

## Checks

```bash
pnpm check
```

Runs Oxlint, Prettier's formatting check, the TypeScript typecheck, frontend tests, `cargo fmt --check`, `cargo clippy`, and the Rust tests. See [docs/](./docs/index.md) for architecture notes.

Use `pnpm lint` to check JavaScript and TypeScript throughout the repo, or `pnpm lint:fix` to apply safe fixes. Oxlint checks the migrated JavaScript/TypeScript rules and React hook ordering and dependencies, with warnings and unused suppression comments treated as failures. Rules, environments, and generated-file exclusions live in `.oxlintrc.json`.

Use `pnpm format` to format frontend source and root JavaScript, TypeScript, JSON, and HTML files, or `pnpm format:check` to check them without writing. Prettier handles formatting separately from Oxlint. Generated bindings and build output are excluded; Rust formatting stays with `pnpm format:rust`.

## Install a local Mac app

With the development prerequisites installed, build the current checkout and
replace `/Applications/Conductor.app` in one step:

```bash
pnpm install:mac
```

This asks the installed app to quit if it is running, checks the bundle identifier, and replaces only the app bundle. Collections and settings stay in `~/Library/Application Support/com.yaramiri.conductor/`. The command does not fetch changes from Git; update your checkout first if you want newer source. It needs write access to `/Applications`.

Release builds can be published through GitHub Actions, and the app checks for
updates from Settings → About. See [Mac Releases](./docs/02-workflows/01-mac-releases.md)
for the release steps and Apple notarization requirements.

## Contributing

Contributions are welcome, as long as they're good. Open an issue to discuss anything non-trivial before opening a PR, keep changes focused, and match the existing style.

## License

[MIT](./LICENSE) © Yara Miri
