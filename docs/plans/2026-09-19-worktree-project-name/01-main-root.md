# U1 — Resolve the main worktree's root and let `project` draw it

- **Wave:** 1
- **Depends on:** —
- **Owns:** `src/modules/git.rs`, `src/modules/render/segments.rs`,
  `src/_runtime/app.rs`, `tests/e2e.rs`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing.
- **Lazy-load:** `src/modules/config/layers.rs` (only to confirm
  `repo_config_path` is untouched), `tests/golden.rs` (only if a `GitFacts`
  literal there fails to compile — it uses `..Default::default()` and should
  not).

## Ruling

> 1 — Find the main worktree: Follow the `.git` file's `gitdir:` (as `probe()`
> already does), read `<gitdir>/commondir`, resolve it relative to `<gitdir>`,
> take the **parent** of the resulting common dir. Rejected: strip
> `/.git/worktrees/<name>` from the gitdir path (assumes layout);
> `git rev-parse --git-common-dir` (subprocess, forbidden by the module doc).

> 2 — Where it lives: New field `GitFacts.main_root: Option<PathBuf>`, `Some`
> only for a linked worktree; `root` keeps its meaning so dirty/ahead,
> `worktree_subpath` and the repo-config lookup stay in the worktree. Rejected:
> redefine `root` as the main checkout.

> 3 — How it reaches the struct: A new
> `pub fn main_root(root: &Path) -> Option<PathBuf>` in `git.rs` that re-reads
> `<root>/.git`; the three construction sites set
> `main_root: root.as_deref().and_then(git::main_root)`. Rejected: widen
> `find_root_and_branch`'s tuple to three values.

> 4 — Submodule / no `commondir`: `main_root` is `None` — a `.git` file whose
> gitdir has no readable `commondir` is not a linked worktree; the segment keeps
> the checkout's own name. Rejected: treat any `.git` file as a worktree.

> 5 — Bare main repository: Accepted as-is: the common dir's parent is the bare
> repo directory and its name is what renders. No special case.

> 6 — Tests: git.rs unit: pointer + `commondir` → `main_root`; plain repo and
> pointer-without-`commondir` → `None`. segments.rs unit: `project()` prefers
> `main_root` over `root`. e2e: one real `git worktree add` test beside
> `e2e.rs:1824` asserting the bar shows the main checkout's name from inside the
> worktree, with the main checkout rendered as the control. Rejected: unit tests
> only.

> 8 — `--debug` output: Unchanged — no new `main root:` line.

The user's words: "Use the current logic for main worktree. Use the folder name
from main worktree when the context switches to another worktree. This doesn't
change the overall logic of first giving priority to statusline config in the
repo."

## Edits

1. **`src/modules/git.rs`**
   - Add `pub main_root: Option<PathBuf>` to `GitFacts` (line 24-31), with a
     doc-comment: the main checkout's root when `root` is a linked worktree,
     `None` otherwise; `project` draws it first. Cite the new test by name.
   - Add `pub fn main_root(root: &Path) -> Option<PathBuf>`: read `<root>/.git`;
     return `None` unless it is a file whose content parses as `gitdir: <path>`
     (reuse the same parsing `probe()` does at line 122, and `normalise()` for
     the relative case — relative to `root`, as the existing pointer handling
     does). Read `<gitdir>/commondir`; `None` if absent or unreadable. Trim it;
     if relative, join onto `<gitdir>` and `normalise()`. Return that common
     dir's `parent()` as an owned `PathBuf`. No subprocess, no symlink
     resolution — lexical, like `normalise()`.
   - In `resolve()` (line 38-46) set
     `main_root: root.as_deref().and_then(main_root)` in the struct literal.
   - Tests in the existing `#[cfg(test)]` block, using the local `repo()` helper
     and `fs::write` as `a_worktree_pointer_file_is_followed` (line 318) does:
     - `a_linked_worktree_reports_the_main_checkout_as_main_root`: fabricate
       `<main>/.git/HEAD`, `<main>/.git/worktrees/wt/HEAD`,
       `<main>/.git/worktrees/wt/commondir` containing `../..\n`, and
       `<wt>/.git` containing `gitdir: <main>/.git/worktrees/wt\n`; assert
       `main_root(&wt) == Some(main)` and
       `find_root_and_branch(&wt).0 == Some(wt)`.
     - `a_plain_checkout_has_no_main_root`: `.git` directory → `None`.
     - `a_pointer_without_commondir_has_no_main_root`: `.git` file whose gitdir
       has `HEAD` but no `commondir` (the submodule shape) → `None`.
2. **`src/modules/render/segments.rs`**
   - `project()` (line 140-149): the fallback rung becomes
     `git.main_root.as_ref().or(git.root.as_ref())?.file_name()?...`. Update the
     doc-comment at 127-139: the third rung is the main worktree's directory
     name; in an ordinary checkout that is the git root's. Name the tests.
   - Add test `project_prefers_the_main_worktree_over_a_linked_one` beside
     `project_falls_back_to_the_repo_directory_name` (line 372):
     `GitFacts {
     root: Some("/src/my-repo/.claude/worktrees/feature"), main_root:
     Some("/src/my-repo"), .. }`
     renders `my-repo`; with `main_root: None` it renders `feature`; a
     configured `projectName` still wins over both.
3. **`src/_runtime/app.rs`** — at both `GitFacts` literals (lines 290 and 515)
   add `main_root: root.as_deref().and_then(git::main_root),` before `root,`.
   Nothing else in this file changes; no new `--debug` line.
4. **`tests/e2e.rs`** — add `a_linked_worktree_renders_the_main_checkouts_name`,
   modelled on `a_dirty_linked_worktree_renders_its_own_dirty_marker`
   (line 1824) and reusing its local `git()` helper shape: `git init` a temp
   repo whose directory is named e.g. `main-checkout`, commit once,
   `git worktree add
   <tmp>/.claude/worktrees/feature`, render with `cwd` =
   the worktree and no `projectName` in any config layer; assert the `project`
   segment text is `main-checkout` and not `feature`. Render the main checkout
   too as the control and assert it also reads `main-checkout`. Update the
   doc-comment at line 1812 that says the dirty test is "the only case in the
   suite that runs a real `git worktree add`" — there are now two.

## Verification

- `mise run code:all` green (format, both clippy passes,
  `cargo test
  --features schema`, sec, schema).
- `cargo test --features schema main_root` runs the three new git.rs tests and
  they pass; `cargo test --features schema project_` runs the segments tests;
  `cargo test --features schema --test e2e linked_worktree` runs both e2e
  worktree tests.
- `grep -n 'main_root' src/_runtime/app.rs` shows exactly two hits.
- `grep -c 'rev-parse' src/modules/git.rs` is 0.

## Guardrails

- Do not touch `src/modules/config/layers.rs` — the repo-config lookup stays on
  `root` by ruling 10.
- Do not touch any `site/content/**`, `docs/**` or `readme.md` — report the
  passages as `DOCS FALSIFIED:` instead. The known ones:
  `site/content/repo-config.md:10,14,51-56`, `site/content/segments.md:70-74`,
  `docs/decisions.md:693-715`.
- `dprint` formats Rust here; run `mise run code:format --fix` before returning
  rather than hand-wrapping.
- Keep the repo and its worktree both under the same `TempDir`, as the dirty
  test does, so nothing outlives the test.
- Delete with `rm`, never `git rm`.

## Commit

`fix(git): name the main checkout from inside a linked worktree` — written by
the orchestrator after the wave gate, not by the unit.
