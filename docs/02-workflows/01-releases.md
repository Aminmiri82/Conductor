---
title: Releases
description: Bot-maintained release PRs, Mac, Windows, and Linux draft builds, and signed updates.
---

# Releases

A bot maintains release notes and versions. Merging a feature PR updates the
open release PR; merging the release PR builds a draft. Publishing that draft
is a deliberate, manual step. The [releasing skill](../../.agents/skills/releasing/SKILL.md)
has the step-by-step commands.

## Release Notes Come From PR Titles

[Release Please](https://github.com/googleapis/release-please) builds the
changelog from conventional commit messages, with no AI reading the code, so
accurate titles are the only source of accurate notes. Use titles that describe
the resulting behavior, such as `fix(mac): keep agent verification isolated`,
and squash-merge with that title. Direct commits need conventional messages
too. Do not edit the generated `CHANGELOG.md` or release description by hand.

Before 1.0, features and fixes bump the patch version, and breaking changes
bump the minor version.

## Publishing

A merged release PR runs the [Release workflow](../../.github/workflows/release.yml),
which builds a **draft** release with the Mac DMG, the Windows `-setup.exe`,
the Linux `.deb` and `.AppImage`, and the signed updater files. The three
builds run in parallel and do not repeat `pnpm check`, since the same code
already passed it on its PR and on `main`. None of them writes `latest.json`:
parallel jobs rewriting one file would drop a platform. A final `updater` job
writes it once from the draft's assets and signatures with
[`scripts/updater-json.mjs`](../../scripts/updater-json.mjs), and fails if any
platform is missing. A draft without a platform's `latest.json` entry would
leave those users without updates, so a failed job means the draft is not
ready to publish.

The release PR itself gets a light check: the dispatched
[Checks](../../.github/workflows/checks.yml) run confirms it only changes
version files and that every version agrees. The bot's own `pull_request` run
skips all jobs, so there is one run to wait on.

On Linux, only the AppImage updates in place. A `.deb` install is updated by
installing the next release's `.deb`. The Linux build runs on the oldest
supported Ubuntu (22.04), because a binary built against a newer glibc does not
start on older systems. Inspect the draft and its assets before publishing it as the
latest release; never publish an empty or failed draft. The app only sees an
update once the release is published, and it checks only when the user asks.

If a build fails, run the **Release** workflow manually with the existing
draft tag (for example `app-v0.1.3`). It rebuilds that tag's commit and refuses
to overwrite a published release.

## Signing

Tauri's update-signing key is separate from Apple and Windows code signing. Its private key
lives outside the repository and in the `TAURI_SIGNING_PRIVATE_KEY` Actions
secret; never print or commit it. Apple signing is ad hoc, not notarized, so
macOS blocks a downloaded DMG's first launch, while in-app updates are not
blocked. The Windows installer is not code signed either, so SmartScreen
warns on its first run. See [Tauri's updater documentation](https://v2.tauri.app/plugin/updater/#signing-updates).

Updates keep the release data directory, and the app upgrades its database on
launch (see [Storage](../01-architecture/03-storage.md)).
