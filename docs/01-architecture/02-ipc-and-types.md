---
title: IPC And Generated Types
description: How the frontend calls Rust, why the TypeScript types are generated, and how to add a command.
---

The frontend talks to Rust only through Tauri commands. Command names,
argument names, and payload types are defined once, in Rust, and generated
into [`src/bindings.ts`](../../src/bindings.ts) with
[tauri-specta](https://github.com/specta-rs/tauri-specta). If Rust and the
frontend disagree, `pnpm check` fails instead of the app failing at runtime.

## How It Fits Together

- `specta_builder()` in [`lib.rs`](../../src-tauri/src/lib.rs) serves both the
  runtime invoke handler and the generator, so a command cannot be callable
  without being typed. It must stay in `lib.rs` because Tauri's command macros
  are exported at the crate root.
- [`bindings.rs`](../../src-tauri/src/bindings.rs) holds
  `bindings_are_up_to_date`, which fails when the committed file differs. Fix
  it with `pnpm bindings`, not by editing the test.
- [`src/features/types.ts`](../../src/features/types.ts) re-exports the
  generated types and defines frontend-only ones.

Commands reject with their error string (`ErrorHandlingMode::Throw`), exactly
like a plain `invoke`, so callers use `try`/`catch`.

## Adding Or Changing A Command

1. Write the function with `#[tauri::command]` and `#[specta::specta]`, and
   derive `specta::Type` on any new input or output struct.
2. Register it in `specta_builder()` in `lib.rs`.
3. Run `pnpm bindings` to regenerate `src/bindings.ts`.
4. Call it through `api` in [`src/lib/tauri.ts`](../../src/lib/tauri.ts) or
   through `commands` directly.

## Known Quirks

- **`#[serde(default)]` fields are optional in TypeScript**, even on types
  Rust always fills in (for example `RequestBody.formData`). Specta uses one
  shape for both directions, and the default only applies when reading. The
  defaults are needed: Postman import stores partial JSON that is read back
  through them. Handle these with `?? []` at the read site rather than removing
  the default.
- **`serde_json::Value` cannot be exported.** This specta release overflows
  the stack on it. Wrap it (see `FrontendJson` in `commands/storage.rs`) or
  override the field with `#[specta(type = specta_typescript::Unknown)]`.
- **Integers are exported as `number`.** `i64`/`u128`/`usize` would be
  `bigint`; we opt out because positions, byte counts, and durations never
  approach 2^53. Do not send IDs or values that can exceed that as integers.
- **tauri-specta is a release candidate**, pinned to an exact version in
  `Cargo.toml`. When upgrading, bump `specta`, `specta-typescript`, and
  `tauri-specta` together, run `pnpm bindings`, and review the diff of
  `src/bindings.ts`.
