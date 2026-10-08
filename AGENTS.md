# Conductor

An ultra-lightweight, ultra-fast Postman alternative.
It's a Tauri 2 desktop app with a Rust + SQLite backend and a React + Zustand frontend.

**Performance is the product.** Speed and low memory outrank features. A change that makes startup, typing, or memory worse is wrong until Yara signs off, even if every test passes. Read [Performance](docs/01-architecture/01-performance.md) before touching hot paths, IPC, stores, or dependencies.

**Pre-production.** No backwards compatibility, including the database schema: edit `001_initial.sql` in place instead of writing migrations. See [Storage](docs/01-architecture/03-storage.md).

**Git.** Do not commit, push, or stage without Yara's explicit approval.

## Relevant Context

Use [the docs index](docs/index.md) to find the page for your task; unrelated pages are not prerequisites. Setup is in [README.md](README.md).
- Adding or revising tests: the [writing-tests skill](.agents/skills/writing-tests/SKILL.md).
- Confirming a visual change, or any change when user asks: the [verify-in-app skill](.agents/skills/verify-in-app/SKILL.md).

## Where Code Lives

- `src-tauri/src/commands/`: Tauri commands. `models.rs` holds the IPC types; `requests/` holds request storage, variables, body/auth building, and sending; `collections.rs` holds Postman import and the tree.
- `src-tauri/src/storage/`: the SQLite `Database` (one writer, pooled readers) and `schema/001_initial.sql`.
- `src-tauri/src/lib.rs`: app setup, the menu, and `specta_builder()`, which registers every command.
- `src/bindings.ts`: **generated** from Rust by `pnpm bindings`. Never edit it by hand.
- `src/lib/tauri.ts`: `api`, thin conveniences over the generated `commands`. Never call `invoke` with a string.
- `src/features/`: UI by area (`collections`, `requests`, `variables`, `settings`, `shell`) and the Zustand stores in `workspace/`.
- `src/components/ui/`: shadcn primitives. `src/app/`: app root and hotkeys.

## Verifying

- `pnpm check` is the definition of green: `tsc`, Vitest, `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test` (which includes the bindings check). Run it before calling a change done, and fix failures your change caused without asking.
- After changing a command or IPC type, run `pnpm bindings` and include the regenerated `src/bindings.ts`.
- Use the smallest proof that works: a unit test for logic. After a visual change (layout, styling, focus, what shows when), check your own work with the verify-in-app skill before reporting done.
- Never run the app against the real data directory, and never send requests to a real environment. Run the app with `pnpm agent:start`/`pnpm agent:stop` and send to local servers. Before computer use, `pnpm agent:status` must verify the tracked app; follow the [identity checks](.agents/skills/verify-in-app/SKILL.md). Never target the installed app by name.
- Never `pkill`/`kill` by name or pattern; Yara runs other dev servers and her own Conductor. Stop only PIDs you started.

## Taste

- Complexity belongs in Rust. Commands parse, resolve, store, and send; components render and edit.
- Parse untrusted input (Postman files, user JSON) once at the boundary into typed structs. Past that point, trust the type; no repeated coercions or just-in-case branches for states the types cannot produce.
- Types come from Rust. Do not hand-write a TypeScript copy of an IPC type; re-export it from `@/bindings`.
- Inferred types over annotations. `any` is the enemy.
- Comments explain how something is used or why it is unusual. Do not narrate every line.
- If a rule here fights the task in front of you, say so loudly and get Yara's sign-off before breaking it.
