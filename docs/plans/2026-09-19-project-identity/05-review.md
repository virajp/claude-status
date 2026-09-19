# U5 — Review: the identity resolver, the symbol change, the render and the font

- **Wave:** 3
- **Depends on:** U1, U2, U3, U4
- **Owns:** —
- **Model:** opus
- **Kind:** review

## Scope

Covers U1, U2, U3 and U4 — every unit that lands binary source or the assets the
binary embeds — and reviews the whole branch delta since the branch base, this
being the first review row. Decision 11 records why the row exists: the change
lands runnable code.

Points the reviewers are asked to weigh, beyond the engines' defaults:

- The identity resolver reads `.git`, `commondir`, a `modules/` path and a
  `config` file that a cloned repository controls. Every string that reaches the
  bar from them must pass `render::sanitize`; no path they name may be read
  except `commondir`, `HEAD` and `config` under the resolved git dir.
- The resolver adds no subprocess and no `canonicalize`; it walks at most the
  gitdir path and reads at most three small files, on a bar with a 250 ms
  budget.
- `find_root_and_branch`, `probe`, `resolve_markers` and the repo-config lookup
  are behaviourally unchanged (`git.rs`'s existing tests are the proof; they
  must be unedited).
- The four new symbol keys are present in the asset, `default_symbols()`, the
  `GLYPHS` table and the schema description, and `project`/`repo` in none of
  them.
- The e2e test's `git submodule add` uses `-c protocol.file.allow=always` and
  skips cleanly without `git` on PATH.
