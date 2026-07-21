# Postman Compatibility — Feature Gaps

What Conductor does **not** yet do compared to Postman. Grouped by impact.

## Scripts
- **Pre-request and test scripts are not executed.** They're stored on import, but only a regex
  scrapes `pm.environment.set(...)` / `pm.collectionVariables.set(...)` / `postman.setEnvironmentVariable(...)`
  calls. No real `pm.*` sandbox, no assertions, no test results.

## Auth
- Only `bearer`, `basic`, `apikey`, `noauth` are supported.
- **Missing:** OAuth 2.0, OAuth 1.0, AWS SigV4, Digest, NTLM, Hawk — dropped on import.

## Collections
- **Saved example responses (`item.response[]`) are ignored on import.**
- No export back to Postman format (import-only — no round-trip).
- Folder/collection-level pre-request & test scripts (events) aren't run, only request-level
  ones are even stored.

## Sending
- No cookie jar (send/store/display cookies).
- No collection runner (run a folder/collection in sequence, CSV/data-file iteration).
- Limited per-request settings: no follow-redirect / SSL-verify / timeout / proxy toggles.

## Smaller gaps
- `description` fields not imported.
- Response cookies not surfaced.
- formdata file uploads don't carry content-type.

---

## Upstream references (open source, `postmanlabs`, Apache-2.0)
- `postman-collection` — SDK + large fixture corpus; the collection format.
- `postman-runtime` — reference for auth inheritance, variable resolution order, cookies.
- `postmanlabs/schemas` — v2.1.0 JSON Schema (the field-level spec).
- Postman Echo (`postman-echo.com`) — public live endpoints for auth/cookie/redirect testing.
