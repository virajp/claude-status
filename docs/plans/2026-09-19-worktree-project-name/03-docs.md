# U3 — Docs: reconcile the fallback prose and amend the decision

- **Wave:** 3
- **Depends on:** U1, U2
- **Owns:** `site/content/**`, `docs/decisions.md`, `readme.md`, `CLAUDE.md`,
  `docs/*.md`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file the findings name, top to bottom, before
  editing.
- **Lazy-load:** `src/modules/render/segments.rs` (to quote the test names the
  decision entry cites).

## Ruling

> Goal: This **amends** the standing decision "`project` falls back to the git
> root's directory name" (`docs/decisions.md:693`, decided 2026-08-26). Its
> third rung read "the git root's own directory name"; it now reads "the main
> worktree's directory name — in an ordinary checkout, the same thing". Both
> halves are kept, dated.

> The user confirmed on 2026-09-19: "Confirm amendment — the docs unit appends a
> dated amendment under that heading: was 'the git root's directory name', now
> 'the main worktree's directory name; in an ordinary checkout these are the
> same'."

> 10 — Repo config from a worktree: Untouched. Do not document a fallback for
> `.config/claude-status.json` that does not exist.

## Edits

1. Run `vwf:docs-sync` over the run's branch delta and apply its findings, plus
   every `DOCS FALSIFIED:` line U1 returned, plus this list from the survey:
   - **`docs/decisions.md`** under `### \`project\` falls back to the git root's
     directory
     name`(line 693): append an **Amended 2026-09-19**
     paragraph. Was: the third rung is the git root's own directory name — true
     in an ordinary checkout, and in a linked worktree that root is the
     worktree's folder, so`.claude/worktrees/feature`rendered`feature`. Now:
     the third rung is the **main worktree's** directory name, read by
     following the`.git`pointer file to its gitdir and that gitdir's`commondir`— filesystem only, per the module's standing rule. A`.git`file without a`commondir`
     (a submodule) keeps its own name. Cite the tests U1 added by name. Keep the
     original text above it intact.
   - **`site/content/repo-config.md`** lines 10, 14, 51-56: where the prose says
     the git root's directory name, say the repository's — the main checkout's —
     directory name, and add one sentence that a linked worktree renders the
     main checkout's name, not the worktree folder's.
   - **`site/content/segments.md`** lines 70-74: the same precedence list; same
     edit.
   - `readme.md:21`, `site/content/diagnosing.md`, `configure.md:79-80`,
     `generate.md:124-134`: edit only if docs-sync finds a passage the change
     falsified; the survey expects none.
2. Follow the `vwf:documentation-standards` skill for any Markdown edit.

## Verification

- `mise run site:build` green (zola build + internal link check).
- `mise run code:all` green (dprint covers Markdown).
- `grep -n 'Amended 2026-09-19' docs/decisions.md` shows one hit under the
  amended heading.
- `grep -rn "git root's own directory name" site/content/` shows no hit that
  still describes the fallback without the worktree qualification.

## Guardrails

- Do not touch `src/**`, `tests/**`, `Cargo.*`, `npm/**`.
- Do not create `docs/memory/decisions/` entries — this repo records decisions
  in `docs/decisions.md` (CLAUDE.md, "Where a fact belongs").
- Do not restate what the tests hold; cite them.
- Delete with `rm`, never `git rm`.

## Commit

`docs(decisions): the project segment names the main worktree` — written by the
orchestrator after the wave gate, not by the unit.
