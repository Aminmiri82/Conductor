# Conductor

A lightweight, fast, performance-focused alternative to Postman.

Conductor is a desktop API client built with Tauri, React, and Rust. It aims to stay snappy on large collections, keep memory usage low, and feel native, without the bloat of an Electron-based app.

## Current Features

- Workspaces, collections, and requests
- Environment and variable management
- Keyboard-first UX with hotkeys
- UI Themes
- Postman collection import

## Install

Download Conductor from the
[latest release](https://github.com/Aminmiri82/Conductor/releases/latest).

**macOS** (Apple Silicon only): download the `.dmg`, open it, and drag
Conductor into Applications. The app is not notarized, so macOS blocks the
first launch. Try to open it once, then choose **Open Anyway** in System
Settings → Privacy & Security. After that, updates install from the app menu or
Settings → About.

**Windows** (x64): download and run the `-setup.exe`. It installs for your user
only, so it needs no administrator rights. The installer is not code signed, so
SmartScreen may warn on first run; choose **More info** → **Run anyway**. After
that, updates install from Settings → About (Ctrl+,).

**Linux** (x64, Ubuntu 22.04 or newer): on Ubuntu or Debian, download the
`.deb` and install it with `sudo apt install ./Conductor_*.deb`. On other
distros, download the `.AppImage`, make it executable (`chmod +x`), and run it.
Either way, updates install from Settings → About (Ctrl+,); a `.deb` update
asks for your password.

## Getting started (development)

You need Node.js 24.22.1+, [pnpm](https://pnpm.io/), and the [Rust toolchain and Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri dev
```

See [docs/](./docs/index.md) for architecture notes.

## Contributing

Contributions are welcome, as long as they're good. Open an issue to discuss anything non-trivial before opening a PR, keep changes focused, and match the existing style.

## License

[MIT](./LICENSE) © Yara Miri
