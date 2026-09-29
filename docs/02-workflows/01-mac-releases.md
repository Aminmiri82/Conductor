---
title: Mac Releases
description: Local builds, GitHub release drafts, signed updates, and Apple notarization.
---

# Mac Releases

Local installation and published updates use the same app identifier, so both
keep their data in the release directory described in [Storage](../01-architecture/03-storage.md).
Replacing the app bundle does not migrate an incompatible pre-production database.

## Local Builds

Run `pnpm install:mac` to build the current checkout and replace
`/Applications/Conductor.app`. The command does not fetch Git changes.

The first updater-capable version must be installed manually. An older app
cannot gain a Check for updates button through an update it cannot request.

## Publish A Release

The [Mac release workflow](../../.github/workflows/mac-release.yml) is started
manually from GitHub Actions. It builds for Apple Silicon, creates a **draft**
GitHub Release, and uploads Tauri's signed updater archive and `latest.json`.
Inspect the draft, then publish it to make it visible to the
app's Check for updates button. The app checks only when that button is pressed.
GitHub [hosts release assets without a bandwidth limit](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases#storage-and-bandwidth-quotas).

Before each release, increase the version in `src-tauri/tauri.conf.json`,
`src-tauri/Cargo.toml`, and `package.json` together. The updater compares that
version with the installed version; reusing a version will not offer an update.

Tauri requires its own update signing key, separate from Apple code signing.
The public key is in [Tauri's config](../../src-tauri/tauri.conf.json). The
private key is stored outside this repository at
`~/.tauri/conductor-updater.key` and in the GitHub Actions secret
`TAURI_SIGNING_PRIVATE_KEY`. Back it up securely: losing it prevents signing
updates for apps that already trust its public key. Never commit or print it.
See [Tauri's updater guide](https://v2.tauri.app/plugin/updater/#signing-updates).
