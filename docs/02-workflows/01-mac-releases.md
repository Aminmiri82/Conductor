---
title: Mac Releases
description: Bot-maintained release PRs, draft builds, and signed updates.
---

# Mac Releases

Release notes and version bumps are maintained by a bot. Merging a feature
PR prepares the next release; merging the release PR builds it. Publishing
remains a deliberate step after the build succeeds.

## Release PRs

The [Release PR workflow](../../.github/workflows/release-please.yml) runs on
pushes to `main`. [Release Please](https://github.com/googleapis/release-please)
opens or updates one release PR containing the accumulated changelog and
version changes. Its [configuration](../../release-please-config.json) covers
`package.json`, the Tauri config, `Cargo.toml`, and the app's `Cargo.lock`
entry. The [manifest](../../.release-please-manifest.json) tracks the version.
Do not manually maintain the generated `CHANGELOG.md` or release description.

Use conventional PR titles that describe the resulting behavior, such as
`fix(mac): keep agent verification isolated`. Squash-merge using that title.
Commit types group the generated notes into features, fixes, performance,
and maintenance categories. The bot formats commit messages; it does not use
an AI model to infer changes from code. Accurate titles are the source of
accurate notes. Direct commits also need conventional messages.

Before 1.0, features and fixes increment the patch version, and breaking
changes increment the minor version. The release PR shows the proposed
version and all notes for review. You can continue merging changes while it
is open; the bot updates it automatically.

The [checks workflow](../../.github/workflows/checks.yml) validates PR titles
and runs `pnpm check`. Bot-created PRs do not trigger ordinary workflows with
`GITHUB_TOKEN`, so the Release PR workflow explicitly dispatches checks for
the bot's branch. No personal access token is required. The repository must
allow GitHub Actions to create pull requests under Settings → Actions →
General → Workflow permissions. The workflow requests its own scoped write
permissions; the repository's default token permissions can remain read-only.

## Build And Publish

When the release PR is merged, the bot creates a tag and a **draft** release
with generated notes. The same workflow calls the
[Mac release workflow](../../.github/workflows/mac-release.yml), avoiding the
restriction on chaining bot-created tag events. It verifies that the tag
belongs to `main`, checks out that exact commit, runs the checks, and builds
for Apple Silicon. It attaches the signed updater archive, its signature,
and `latest.json`, preserving the generated notes.

Wait for **Mac release** to succeed, inspect the draft and its assets, then
publish it as the latest release. Do not publish an empty or failed draft.
Users can then select Check for Updates in the app menu or Settings → About,
and choose Install and restart. The app checks only on request.

If a build fails, run **Mac release** manually with the existing draft tag,
for example `app-v0.1.3`. It rebuilds the tagged commit even if `main` has
advanced. It refuses to overwrite a published release. An unchanged rerun
of Release Please does not guarantee another build, so use this explicit retry.

Tauri's update-signing key is separate from Apple Developer ID signing and
notarization. The public key is in [Tauri's config](../../src-tauri/tauri.conf.json).
The private key is outside the repository at `~/.tauri/conductor-updater.key`
and in the `TAURI_SIGNING_PRIVATE_KEY` Actions secret. Never print or commit
it. The current [release config](../../src-tauri/tauri.release.conf.json) uses
ad-hoc Apple signing, not Developer ID notarization. See
[Tauri's updater documentation](https://v2.tauri.app/plugin/updater/#signing-updates).

## App Data

Updates preserve the release data directory
described in [Storage](../01-architecture/03-storage.md). Replacing the app
does not migrate an incompatible pre-production database. Apps without updater
support must first be upgraded manually.
