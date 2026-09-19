# U2 — Four per-kind project glyphs replace `symbols.project` and `symbols.repo`

- **Wave:** 1
- **Depends on:** —
- **Owns:** `src/modules/config/mod.rs`, `assets/claude-status.defaults.json`,
  `tests/defaults_integrity.rs`, `schemas/claude-status.schema.json`,
  `tests/schema.rs`, `.config/claude-status.json`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing.
- **Lazy-load:** `src/bin/schema.rs` (only to confirm the output path),
  `.config/mise/tasks/code/schema` (only to see what `mise run code:schema`
  runs), `docs/decisions.md:607-617` ("Do not retype the glyphs").

## Ruling

> 5 — Symbol keys: Flat keys in `symbols`: `projectGit` `\u{e702}`,
> `projectRemote` `\u{f401}`, `projectGithub` `\u{f09b}`, `projectGitlab`
> `\u{f0ba0}`. Remove `project` and `repo` from `default_symbols()`, the
> defaults asset, and the `GLYPHS` table (39 → 41 rows). Update the schemars
> strings at `mod.rs:99` (drop the `symbols.project` mention and the "omitted
> when unset" claim) and `:123` (drop `project`, add the four). Regenerate the
> schema; update `DESCRIPTION_COUNT`/`DESCRIPTION_DIGEST` in `tests/schema.rs`
> to what the regenerated file yields. Rejected: a `projectSymbols` map
> mirroring `typeSymbols`; keep `symbols.repo`.

> 10 — This repo's repo layer: Remove `projectName` from
> `.config/claude-status.json`; if only `$schema` remains, `rm` the file — the
> GitHub rule derives `virajp/claude-status`. Rejected: leave it.

The user's words: "Flat keys in `symbols` (your earlier pick)" — the option text
read "Delete `symbols.repo` and `symbols.project`." Key names: "projectGit /
projectRemote / projectGithub / projectGitlab". Codepoints: "U+E702 nf-dev-git",
"U+F09B nf-fa-github", "U+F401 nf-oct-repo", GitLab U+F0BA0 (`nf-md-gitlab`, the
one glyph that survived transport).

## Edits

1. **`src/modules/config/mod.rs`**
   - `default_symbols()` (`:795-819`): remove the `("project", "\u{f401}")`
     (`:806`) and `("repo", "\u{f401}")` (`:807`) entries; add, in the map's
     existing alphabetical position, `("projectGit", "\u{e702}")`,
     `("projectGithub", "\u{f09b}")`, `("projectGitlab", "\u{f0ba0}")`,
     `("projectRemote", "\u{f401}")`. Write them as `\u{…}` escapes exactly as
     the neighbours are — never a pasted glyph.
   - Schemars description on `project_name` (`:99`): rewrite to "The name the
     `project` segment draws in place of the derived one — the repository's
     `owner/repo` from its `origin`, or its `parent/base` directory. Repo-level
     only: it ships in neither the embedded defaults nor a seeded user config.
     The glyph is chosen by how the repository was identified, never by this
     key." (Keep the sentence about the repo layer if it is there.)
   - Schemars description on `symbols` (`:123`): in the consumed-keys list,
     replace `project` with
     `projectGit, projectGithub, projectGitlab,
     projectRemote`; `repo` is
     not listed today, so nothing to drop there.
   - Nothing else in this file changes; `Config::symbol` and `glyph_table` stay
     as they are.
2. **`assets/claude-status.defaults.json`** — in the `symbols` block
   (`:186-207`): delete the `"project"` (`:198`) and `"repo"` (`:199`) lines;
   insert the four new keys in the block's existing key order with their glyphs.
   **Byte-copy the codepoints** — write the file with a script that emits the
   characters from their hex values (`python3 -c
   'print(chr(0xE702))'` etc.),
   never by typing a glyph into an editor. The file is `-text -diff` and
   dprint-excluded; do not run a formatter over it. Keep the trailing-newline
   and indentation conventions of the neighbours.
3. **`tests/defaults_integrity.rs`** — `GLYPHS` table (`:33-74`): remove the
   `symbols.project` (`:56`) and `symbols.repo` (`:57`) rows; add four rows for
   the new keys in the table's key order, each carrying its codepoint the way
   the neighbours do. Change the length assertion (`~:103`) from `39` to `41`.
   `the_asset_carries_no_symbol_the_table_does_not_cover` must pass unchanged.
4. **`schemas/claude-status.schema.json`** — regenerate with
   `mise run code:schema` (no `--check`). Do not hand-edit. Confirm the diff
   touches only the two description strings and nothing structural.
5. **`tests/schema.rs`** — set `DESCRIPTION_COUNT` (`:46`) and
   `DESCRIPTION_DIGEST` (`:67`) to what the regenerated schema yields; the
   test's own failure message prints the new values. The count stays 49 unless a
   description was added — it should not have been.
6. **`.config/claude-status.json`** — remove the `projectName` key. If the
   remaining object holds only `$schema`, `rm` the file.

## Verification

- `mise run code:schema --check` green.
- `cargo test --features schema` green — `defaults_integrity`, `schema`,
  `config::` all pass; `the_embedded_defaults_deserialize_to_the_default_config`
  (`mod.rs:903`) proves the asset and `default_symbols()` agree.
- `python3 -c "import json;s=json.load(open('assets/claude-status.defaults.json'))['symbols'];print(sorted(k for k in s if k.startswith('project')), 'project' in s, 'repo' in s)"`
  prints the four keys, `False False`.
- `grep -c 'symbols.project"' assets/claude-status.defaults.json` → 0.
- `mise run code:all` green.

## Guardrails

- Do not touch `src/modules/render/segments.rs`, `src/_runtime/app.rs`,
  `src/modules/git.rs`, `tests/e2e.rs`, `tests/golden.rs`, `site/**` — U1, U3
  and U4 own them.
- Never retype a glyph; emit from hex. `docs/decisions.md:607-617`.
- The asset is `-text -diff` and excluded from dprint — no formatter.
- `schemas/claude-status.schema.json` is generated; regenerate, never edit.
- Delete with `rm`, never `git rm`.

## Commit

`feat(config): per-kind project glyphs replace symbols.project and symbols.repo`
— written by the orchestrator after the wave gate, not by the unit.
