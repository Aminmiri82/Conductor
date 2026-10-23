---
name: releasing
description: Commit finished work, merge it to main, and publish a Mac and Windows release the app can update to over the air. Only when Yara explicitly asks to commit or release.
---

# Releasing

Yara's request is the approval for these steps, nothing more. Stop and report
on any failure; never force-push, skip checks, or edit a published release.

1. `pnpm check` is green. Branch off `main`, commit with a conventional
   message (`fix:`/`feat:` drive the version bump), push, and `gh pr create`.
2. When the PR's checks pass, `gh pr merge --squash --delete-branch`.
3. The push to `main` makes release-please open `chore(main): release app X.Y.Z`.
   Its checks run as a dispatched `checks.yml` run on the bot's branch, not
   as PR checks; wait for that run, then squash-merge the release PR.
4. That merge runs the `Release PR` workflow, which builds a **draft** on Mac
   and then Windows. Wait for both jobs to succeed and confirm the draft has
   the `.dmg`, the `.app.tar.gz` and its `.sig`, the `-setup.exe` and its
   `.sig`, and a `latest.json` with the new version and both `darwin-aarch64`
   and `windows-x86_64` platforms.
5. Publish: `gh release edit app-vX.Y.Z --draft=false --latest`. The updater
   reads `releases/latest/download/latest.json`, so nothing is OTA-able until
   this step. Confirm that URL now reports the new version.
