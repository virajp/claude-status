# U7 — Gates and bump: 1.2.0 → 1.3.0

- **Wave:** 5
- **Depends on:** U6
- **Owns:** `Cargo.toml`, `Cargo.lock`, `npm/package.json`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing — the version
  lines are `Cargo.toml:4`, `Cargo.lock:100-101` (the `claude-status` package
  block), `npm/package.json:3`.
- **Lazy-load:** `git show 0fb6ce4 --stat` (the last bump, for the shape of the
  three-file edit).

## Ruling

> 13 — Version bump: minor, 1.2.0 → 1.3.0, all three files as `0fb6ce4` did.
> Rejected: patch; none.

Consent row: "Release claude-status publicly — minor — 1.2.0 → 1.3.0, by
hand-editing `Cargo.toml`, refreshing `Cargo.lock`
(`cargo update -p
claude-status`), and `npm/package.json` alongside".

## Edits

1. **`Cargo.toml:4`** — `version = "1.3.0"`.
2. **`Cargo.lock`** — run `cargo update -p claude-status --offline` (or
   `cargo build` once) so the `claude-status` package block reads `1.3.0`;
   confirm no other line of the lockfile changed.
3. **`npm/package.json:3`** — `"version": "1.3.0"`.

No generators: U2 already regenerated the schema, and `mise run code:all` checks
it.

## Verification

- `grep -n '"1.3.0"\|= "1.3.0"' Cargo.toml Cargo.lock npm/package.json` → one
  hit per file.
- `git diff --stat` shows exactly the three owned files.
- `mise run code:all` green.
- `mise run site:build` green.
- `mise run release:preflight` is **not** run here — it requires `main` and runs
  after landing.

## Guardrails

- Touch nothing but the three version files.
- Do not run `mise run release:tag` or `code:merge:main` — those are
  after-landing steps the orchestrator asks about.
- Delete with `rm`, never `git rm`.

## Commit

`ops: bump to 1.3.0` — written by the orchestrator after the wave gate, not by
the unit.
