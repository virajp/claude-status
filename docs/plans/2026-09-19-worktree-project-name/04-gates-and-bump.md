# U4 — Gates and bump: 1.2.0 → 1.2.1

- **Wave:** 4
- **Depends on:** U3
- **Owns:** `Cargo.toml`, `Cargo.lock`, `npm/package.json`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing.
- **Lazy-load:** `.config/mise/tasks/release/preflight` (only if the gate
  complains about the version).

## Ruling

> Consent — Release claude-status publicly: patch — 1.2.0 → 1.2.1, by
> hand-editing `Cargo.toml`, refreshing `Cargo.lock`, and moving
> `npm/package.json` alongside.

> 9 — Version bump: patch, 1.2.0 → 1.2.1, all three files (`Cargo.toml`,
> `Cargo.lock`, `npm/package.json`) as `0fb6ce4` did. Rejected: binary only; no
> bump.

## Edits

1. **`Cargo.toml`** line 4: `version = "1.2.1"`, keeping the trailing comment.
2. **`Cargo.lock`**: run `cargo update -p claude-status --offline` so the two
   `version = "1.2.0"` lines for this crate (101 and 814 today) follow; verify
   no other line moved.
3. **`npm/package.json`** line 3: `"version": "1.2.1"`.
4. No generators: `mise run code:schema --check` must already pass and the
   schema carries no version.

## Verification

- `mise run code:all` green.
- `mise run site:build` green.
- `grep -c '1\.2\.1' Cargo.toml Cargo.lock npm/package.json` reads 1, 2, 1.
- `grep -rn '1\.2\.0' Cargo.toml Cargo.lock npm/package.json` is empty.

## Guardrails

- Touch nothing outside the three owned files.
- Do not tag and do not run `release:tag` — both after-landing steps are `ask`
  and belong to the orchestrator.
- Delete with `rm`, never `git rm`.

## Commit

`ops: cut v1.2.1 for the worktree project name` — written by the orchestrator
after the wave gate, not by the unit.
