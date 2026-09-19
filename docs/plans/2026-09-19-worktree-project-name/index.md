---
type: vwf-change-plan
title: The project segment names the main worktree from inside a linked one
requires: []
backlog: []
---

# Plan — The project segment names the main worktree from inside a linked one (2026-09-19)

## Status

**APPROVED**

APPROVED 2026-09-19 by the user

## Consent

| Action                                            | Granted                                                                                                               |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Merge to the integration branch and push on green | yes                                                                                                                   |
| After landing: `mise run code:merge:main`         | ask                                                                                                                   |
| After landing: `mise run release:tag`             | ask                                                                                                                   |
| Release claude-status publicly                    | patch — 1.2.0 → 1.2.1, by hand-editing `Cargo.toml`, refreshing `Cargo.lock`, and moving `npm/package.json` alongside |

**The mode recorded here is the consent.** A `run` step runs on a green landing
without a prompt; an `ask` step stops the run once before it, reports what it
would do, and waits. Both after-landing steps are `ask`, so the release row is
intent, not authorisation — the executor stops once before the merge to `main`
and once before the tag.

## Goal

After this lands, a session running inside a linked git worktree whose repo has
not set `projectName` sees the `project` segment draw the **main checkout's**
directory name, not the worktree folder's. An ordinary checkout is unchanged,
and `projectName` from the repo layer, then the user layer, still wins over
either.

This **amends** the standing decision "`project` falls back to the git root's
directory name" (`docs/decisions.md:693`, decided 2026-08-26). Its third rung
read "the git root's own directory name"; it now reads "the main worktree's
directory name — in an ordinary checkout, the same thing". Both halves are kept,
dated.

## Facts the survey established

- **Root resolution is filesystem-only.** `find_root_and_branch()` at
  `src/modules/git.rs:94-108` walks up from `cwd` (max 40 levels) via `probe()`
  at `git.rs:110-146`. A `.git` *directory* is the root and `HEAD` is read
  there; a `.git` *file* is parsed for `gitdir: <path>` (`git.rs:122-135`),
  `normalise()` at `git.rs:166` resolves it lexically, and `HEAD` is read at the
  target. **The returned root is the directory holding the `.git` entry** — in a
  linked worktree, the worktree itself. No `git rev-parse`, no git crate
  (`Cargo.toml` has `regex-lite`, `serde`, `serde_json`, `ureq`; nothing git).
  The module doc (`git.rs:1-10`) says root and branch are "never from `git`".
- **Two construction sites.** `GitFacts` (`git.rs:24-31`: `root`, `branch`,
  `worktree_subpath`, ahead/dirty markers) is built with `..Default::default()`
  at `src/_runtime/app.rs:290` (render), `app.rs:515` (`--debug`), and in
  `git::resolve()` at `git.rs:38-46`. All three take `(root, branch)` from
  `find_root_and_branch`.
- **The segment.** `project()` at `src/modules/render/segments.rs:140-149`:
  `config.project_name` (repo layer already merged over user layer by
  `layers.rs:217-260`), else `git.root.file_name()`, else the segment omits.
  Doc-comment at `segments.rs:127-139`.
- **Tests today.** `segments.rs:364` `project_reads_the_config_not_the_payload`
  and `segments.rs:372` `project_falls_back_to_the_repo_directory_name` use a
  bare `PathBuf` root. `git.rs:283` `a_plain_repo_reports_its_root_and_branch`,
  `git.rs:318` `a_worktree_pointer_file_is_followed`, `git.rs:334`
  `a_relative_gitdir_resolves_against_the_directory_holding_dot_git` hand-write
  `.git` with `fs::write` — no `git init`. `tests/e2e.rs:1824`
  `a_dirty_linked_worktree_renders_its_own_dirty_marker` is the one test that
  runs a real `git worktree add` (its doc-comment at `e2e.rs:1812` says so) and
  asserts the dirty marker is computed in the worktree, not the main checkout.
  **Nothing pins the project name inside a worktree.**
- **Repo-layer config** is read from `<root>/.config/claude-status.json`
  (`layers.rs:175-178`) using the same root — from a worktree, the worktree's
  copy. Out of scope; see below.
- **Gates.** `mise run code:all` = format (dprint), lint
  (`cargo clippy
  --all-targets -D warnings`, twice, with and without
  `--features schema`), test (`cargo test --features schema`, depends on
  `site:assets`), sec (grype + gitleaks), schema check. It is what pre-commit
  and `ci.yml` run on push to `develop`. `mise run site:build` = `zola build` +
  internal link check; `site.yml` runs it on PRs touching `site/**`. No
  `harness:` stamp in `.config/vwf.yaml`.
- **Release.** `Cargo.toml:4` `version = "1.2.0"`; `Cargo.lock:101,814` and
  `npm/package.json:3` carry the same string. No bump task: the last cut,
  `0fb6ce4`, hand-edited all three. `mise run release:tag` runs preflight,
  requires `main`, tags `v<Cargo version>`, pushes the tag; `release.yml` builds
  and publishes. No CHANGELOG file.
- **Commit convention** (`.config/git-conventional-commits.yaml`): types
  `feat fix perf refactor test docs ops blueprint merge wip`; scopes
  `cli
  config render subagent spend git usage caps installer plans site spec
  decisions`.
- **Docs the change falsifies.** `site/content/repo-config.md:10,14,51-56`
  (fallback prose and the three-rung precedence list),
  `site/content/segments.md:70-74` (same list), `docs/decisions.md:693-715` (the
  decision being amended). `readme.md:21`, `site/content/diagnosing.md`,
  `site/content/configure.md:79-80`, `site/content/generate.md:124-134` mention
  the segment or `projectName` without stating the fallback rung; docs-sync
  confirms.
- **Backlog:** unreadable — no GitHub Project titled `claude-status` exists
  under `virajp` (only `claude-plugins`). Nothing recalled; `backlog:` is empty.
- **Plan index:** `docs/plans/index.md` is the older prose index with
  `Plan | Requires | Landed` tables and no vwf table; `plan-management add`
  writes the header row and this row.
- **Site** is Zola, not Astro: pages under `site/content/*.md`.

## Assumed decisions — confirm or override at review

| #  | Decision                    | Ruling                                                                                                                                                                                                                                                                                                                                                     | Rejected                                                                                                                                         | Unit         |
| -- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------ |
| 1  | Find the main worktree      | Follow the `.git` file's `gitdir:` (as `probe()` already does), read `<gitdir>/commondir`, resolve it relative to `<gitdir>`, take the **parent** of the resulting common dir                                                                                                                                                                              | Strip `/.git/worktrees/<name>` from the gitdir path (assumes layout); `git rev-parse --git-common-dir` (subprocess, forbidden by the module doc) | U1           |
| 2  | Where it lives              | New field `GitFacts.main_root: Option<PathBuf>`, `Some` only for a linked worktree; `root` keeps its meaning so dirty/ahead, `worktree_subpath` and the repo-config lookup stay in the worktree                                                                                                                                                            | Redefine `root` as the main checkout                                                                                                             | U1           |
| 3  | How it reaches the struct   | A new `pub fn main_root(root: &Path) -> Option<PathBuf>` in `git.rs` that re-reads `<root>/.git`; the three construction sites set `main_root: root.as_deref().and_then(git::main_root)`                                                                                                                                                                   | Widen `find_root_and_branch`'s tuple to three values (touches every caller and its tests)                                                        | U1           |
| 4  | Submodule / no `commondir`  | `main_root` is `None` — a `.git` file whose gitdir has no readable `commondir` is not a linked worktree; the segment keeps the checkout's own name                                                                                                                                                                                                         | Treat any `.git` file as a worktree                                                                                                              | U1           |
| 5  | Bare main repository        | Accepted as-is: the common dir's parent is the bare repo directory and its name is what renders. No special case                                                                                                                                                                                                                                           | Detect bare and fall back to the worktree name                                                                                                   | U1           |
| 6  | Tests                       | git.rs unit: pointer + `commondir` → `main_root`; plain repo and pointer-without-`commondir` → `None`. segments.rs unit: `project()` prefers `main_root` over `root`. e2e: one real `git worktree add` test beside `e2e.rs:1824` asserting the bar shows the main checkout's name from inside the worktree, with the main checkout rendered as the control | Unit tests only                                                                                                                                  | U1           |
| 7  | Review row                  | Present (U2) — the change lands binary source, which executes                                                                                                                                                                                                                                                                                              | None                                                                                                                                             | U2           |
| 8  | `--debug` output            | Unchanged — no new `main root:` line; the doctor report is pinned by tests and the request did not ask for it                                                                                                                                                                                                                                              | Add a line                                                                                                                                       | U1           |
| 9  | Version bump                | patch, 1.2.0 → 1.2.1, all three files (`Cargo.toml`, `Cargo.lock`, `npm/package.json`) as `0fb6ce4` did                                                                                                                                                                                                                                                    | Binary only; no bump                                                                                                                             | U4           |
| 10 | Repo config from a worktree | Untouched — see Out of scope                                                                                                                                                                                                                                                                                                                               | Fall back to the main checkout's `.config/claude-status.json`                                                                                    | out of scope |

## New dependencies

none

## Units

| Id | Wave | Unit file                                    | Kind   | Owns                                                                                          | Depends on | Status  | Commit |
| -- | ---- | -------------------------------------------- | ------ | --------------------------------------------------------------------------------------------- | ---------- | ------- | ------ |
| U1 | 1    | [01-main-root.md](01-main-root.md)           | edit   | `src/modules/git.rs`, `src/modules/render/segments.rs`, `src/_runtime/app.rs`, `tests/e2e.rs` | —          | pending |        |
| U2 | 2    | [02-review.md](02-review.md)                 | review | —                                                                                             | U1         | pending |        |
| U3 | 3    | [03-docs.md](03-docs.md)                     | edit   | `site/content/**`, `docs/decisions.md`, `readme.md`, `CLAUDE.md`, `docs/*.md`                 | U1, U2     | pending |        |
| U4 | 4    | [04-gates-and-bump.md](04-gates-and-bump.md) | edit   | `Cargo.toml`, `Cargo.lock`, `npm/package.json`                                                | U3         | pending |        |

## Shared-file rule

| File                                                  | Why it collides                       | Owner   |
| ----------------------------------------------------- | ------------------------------------- | ------- |
| `Cargo.toml`, `Cargo.lock`, `npm/package.json`        | version files                         | U4 only |
| `site/content/*.md`, `docs/decisions.md`, `readme.md` | human-facing docs                     | U3 only |
| `src/_runtime/app.rs`                                 | the two `GitFacts` construction sites | U1 only |

## Waves

- Wave 1: U1 alone.
- Wave 2: U2 alone — reviews the delta since the branch base.
- Wave 3: U3 alone.
- Wave 4: U4 alone.

## Wave gate

```
mise run code:all
mise run site:build
```

plus the wave review, plus every report read for `UNRESOLVED:`. Every line must
be green before wave 1.

## After landing

| Step                       | Mode | Notes                                                                                                                      |
| -------------------------- | ---- | -------------------------------------------------------------------------------------------------------------------------- |
| `mise run code:merge:main` | ask  | lands `develop` on `main` `--no-ff` and pushes; refuses a dirty or unpushed branch                                         |
| `mise run release:tag`     | ask  | from `main`: runs `release:preflight`, tags `v1.2.1` from `Cargo.toml`, pushes the tag; `release.yml` builds and publishes |

## Gates the orchestrator keeps

- **Scratch worktree run**, after wave 1 is green: `cargo build --release`;
  `git worktree add /tmp/cs-scratch-wt -b scratch/worktree-project-name` from
  this checkout; feed the binary a payload whose `cwd` is `/tmp/cs-scratch-wt`
  (the shape `tests/e2e.rs` uses). Pass = the rendered `project` segment reads
  `claude-status`, not `cs-scratch-wt`. Then
  `git worktree remove /tmp/cs-scratch-wt` and
  `git branch -D
  scratch/worktree-project-name`.

## Unit contract

Every unit prompt carries, in order: its ruling quoted from this file, its owned
paths plus "touch nothing outside this list", the facts section, the shared-file
rule, and the return block below. A unit never bumps a version, never runs a
generator, never edits a doc, never adds a dependency this file does not list,
never commits. A unit deletes with plain `rm`, never `git rm` — it stages
nothing.

A unit returns exactly this block and nothing else — no file contents, no diff:

    CHANGED: <path> — <one line>            (one per file)
    DECIDED: <what> — <why>                 (choices made inside scope, or none)
    DOCS FALSIFIED: <path> — <passage>      (reported, never edited; or none)
    GAP: <what the plan left unspecified and the assumption taken>   (or none)
    UNRESOLVED: <the ruling needed>         (or none)

## Out of scope

- **Repo-layer config from a linked worktree.** `layers.rs:175` reads
  `<root>/.config/claude-status.json` from the worktree's own root. A worktree
  is a full checkout, so a committed config is already there; only a gitignored
  one would differ. Declined by the user 2026-09-19: leave it.
- **A `main root:` line in `--debug`.** Not asked for; the doctor report is
  test-pinned.

## Parked

none

## Run log

| Wave | Unit | Model | Round | Outcome | Detail | Commit |
| ---- | ---- | ----- | ----- | ------- | ------ | ------ |

## Launch

This folder is already committed and pushed on the branch it was planned on, so
the fresh session's worktree — cut from the integration branch — can see it.

Run in a fresh session:

/vwf:execute docs/plans/2026-09-19-worktree-project-name

or let the queue pick it, by priority:

/vwf:execute next
