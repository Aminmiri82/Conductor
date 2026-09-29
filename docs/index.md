---
title: Conductor Docs
description: Documentation for AI agents and contributors working on Conductor.
---

These pages explain the decisions and rules behind Conductor. They point to
the source rather than copying it. Read the page that matches your task;
reading every page is not required.

## Architecture

- [Performance](./01-architecture/01-performance.md): Why speed and low memory
  outrank features, and the rules that keep Conductor fast.
- [IPC And Generated Types](./01-architecture/02-ipc-and-types.md): How the
  frontend calls Rust, how the TypeScript types are generated, and how to add
  a command.
- [Storage](./01-architecture/03-storage.md): Where data lives, how the schema
  changes before 1.0, and how to run against a throwaway database.

Testing rules live in the
[writing-tests skill](../.agents/skills/writing-tests/SKILL.md).

## Workflows

- [Mac Releases](./02-workflows/01-mac-releases.md): Bot-maintained release PRs, draft
  builds and signed updates.
