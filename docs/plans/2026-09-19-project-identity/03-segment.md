# U3 — The `project` segment and `--doctor` draw the identity

- **Wave:** 2
- **Depends on:** U1, U2
- **Owns:** `src/modules/render/segments.rs`, `src/_runtime/app.rs`,
  `tests/e2e.rs`, `tests/golden.rs`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing; then
  `src/modules/git.rs` as U1 left it (the `Project`/`ProjectKind` types and
  `git::project`), and `src/modules/config/mod.rs` `default_symbols()` as U2
  left it (the four key names).
- **Lazy-load:** `tests/fixtures/*.json` (only for the payload shape the e2e
  helper feeds the binary).

## Ruling

> 6 — The render: `project()` returns `None` when `git.project` is `None`; else
> `format!("{} {name}", config.symbol(key))` with key
> `projectGit`/`projectRemote`/`projectGithub`/`projectGitlab` by kind and
> `name = config.project_name.clone().unwrap_or(project.name)`. Same `sanitize`
> path as today. Rejected: `projectName` also picking the glyph.

> 7 — `--doctor`: GIT section gains one row after `root:`:
> `project: <kind lower-cased> <name> (<identity root>)`, or `project: <none>`
> outside git; both values through `field()`. `build_bar` narration gains
> `project: …` beside `repo root:`. e2e asserts the row is present and names the
> kind, not the whole line. Rejected: no row.

> 8 (this unit's half) — segments.rs: one render per kind plus `projectName`
> replacing the name under a `Github` glyph, plus `None` → omit. e2e: real
> `git init`, `git remote add origin git@github.com:acme/widget.git`,
> `git worktree add`, and
> `git -c protocol.file.allow=always submodule add
> <local path>`, rendering
> the bar from the main checkout, the worktree and the submodule and asserting
> glyph + `acme/widget`; the existing `:1824` worktree test is left as-is.

> 1 (the constraint this unit inherits) — `GitFacts.project: Option<Project>` …
> computed by `git::project(root)` from the root `find_root_and_branch`
> returned.

The user's words: "Keep it, overrides the name only" (`projectName`). "Omit the
segment" (outside git). "Add `project:` row" (doctor). "Only the project
segment" resolves to main/superproject.

## Edits

1. **`src/modules/render/segments.rs`**
   - Rewrite `project()` (`:140-149`) per ruling 6. Map `ProjectKind::Git` →
     `"projectGit"`, `Remote` → `"projectRemote"`, `Github` → `"projectGithub"`,
     `Gitlab` → `"projectGitlab"`; put the match in a small private
     `fn glyph_key(kind: ProjectKind) -> &'static str` so the doctor row cannot
     use a different one.
   - Rewrite the doc comment (`:127-139`): what is drawn per kind, that
     `projectName` replaces the name only, that outside git there is no identity
     and the segment omits, and that both the URL path and the directory name
     are attacker-nameable and pass `sanitize` — cite the tests below by name.
   - Tests: replace `project_reads_the_config_not_the_payload` (`:364-369`) and
     `project_falls_back_to_the_repo_directory_name` (`:372-382`) with:
     - `project_draws_one_glyph_per_kind` — a
       `Config::new(json!({ "symbols":
       { "projectGit": "G", "projectRemote": "R", "projectGithub": "H",
       "projectGitlab": "L" } }))`;
       for each kind a
       `GitFacts { project:
       Some(Project { kind, name: "acme/widget".into(), root: "/x".into() }),
       ..Default::default() }`;
       expect `"G acme/widget"`, `"R acme/widget"`, `"H acme/widget"`,
       `"L acme/widget"`.
     - `project_name_replaces_the_text_but_not_the_glyph` — same config plus
       `"projectName": "renamed"`, a `Github` fact; expect `"H renamed"`.
     - `project_omits_without_an_identity` — `GitFacts::default()` → `None`; and
       a `GitFacts { root: Some("/x".into()), project: None, .. }` → `None` (a
       root alone no longer draws anything).
     - Other `GitFacts` literals in the module (`:270`, `:388`, `:398-408`,
       `:425`) use `..Default::default()` and need no change; confirm.
2. **`src/_runtime/app.rs`**
   - At both construction sites (`:290-295`, `:515-520`) add
     `project: root.as_deref().and_then(git::project)` — `root` is the
     `Option<PathBuf>` already in scope from `find_root_and_branch`.
   - Narration at `:275`: extend to
     `narrate("repo root: {root:?}, branch: {branch:?}, project: {project:?}")`
     or an adjacent line — one line, same style.
   - `--doctor` GIT section: after the `root:` row (`:512`) add
     `project: <kind> <name> (<identity root>)` where kind is the enum's name
     lower-cased, or `project: <none>` when the fact is `None`; every value
     through `field()` (`:363-365`). Match the column alignment of the
     neighbouring rows exactly.
3. **`tests/e2e.rs`**
   - Add
     `a_repository_is_named_by_its_origin_from_the_checkout_a_worktree_and_a_submodule`
     beside `:1824`, reusing its inner `git()` helper shape (`:1826-1836`):
     `git init -q -b main` in `<tmp>/acme/widget`, one commit,
     `git remote add origin git@github.com:acme/widget.git`;
     `git worktree add <tmp>/wt -b wt`; a second repo `<tmp>/lib` with one
     commit; `git -c protocol.file.allow=always submodule add <tmp>/lib lib`
     inside `widget`. Use a **user** config that sets no `projectName` (not
     `safe_config()` — it names every repo `e2e-fixture`) and sets
     `"symbols": { "projectGithub": "H" }` so the assertion is ASCII. Render the
     bar with `cwd` = `widget`, `wt`, `widget/lib`; assert each stdout contains
     `H acme/widget`. Assert the `wt` render's branch reads `wt` and the `lib`
     render's reads `main` — proof that only the identity moved. Skip
     (early-return with a note, as the existing test does) when `git` is not on
     PATH.
   - Add `a_repository_without_an_origin_is_named_parent_slash_base` — real
     `git init -q -b main` in `<tmp>/acme/plain`,
     `"symbols": { "projectGit":
     "G" }`; assert `G acme/plain`.
   - Add `doctor_names_how_the_project_was_identified` — using the github repo
     above, run `--doctor`; assert the GIT section contains a line starting
     `project:` that contains `github` and `acme/widget`. Do not pin the whole
     line.
   - Every existing assertion on `e2e-fixture` / `from-the-repo` still holds:
     `safe_config()`'s user-layer `projectName` still replaces the name. Do not
     edit those tests; if one fails, the cause is a real regression.
   - Leave `:1824` untouched.
4. **`tests/golden.rs`** — no golden carries a project segment (facts). Only if
   a `GitFacts` literal fails to compile, add `..Default::default()`; otherwise
   leave the file untouched and say so in `CHANGED:` as "no change needed".

## Verification

- `cargo test --features schema` green — `render::segments::`, `e2e`, `golden`,
  `app::` all pass.
- `cargo clippy --all-targets -D warnings` with and without `--features schema`
  clean.
- `grep -n 'symbol("project")' src/` → nothing.
- `mise run code:all` green.

## Guardrails

- Do not touch `src/modules/git.rs`, `src/modules/config/**`, `assets/**`,
  `schemas/**`, `site/**`, `tests/defaults_integrity.rs`, `tests/schema.rs`,
  `tests/site.rs` — other units own them.
- The e2e submodule add needs `-c protocol.file.allow=always` on git ≥ 2.38.3;
  without it the add is refused and the test would fail for a reason unrelated
  to the change.
- `safe_config()` sets a user-layer `projectName` — never use it in a test that
  asserts a derived name.
- Delete with `rm`, never `git rm`.

## Commit

`feat(render): the project segment names the repository by where it lives` —
written by the orchestrator after the wave gate, not by the unit.
