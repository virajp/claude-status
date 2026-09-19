# U6 — Docs: the site, the readmes, and the three amended decisions

- **Wave:** 4
- **Depends on:** U5
- **Owns:** `site/content/**`, `site/templates/index.html`, `docs/decisions.md`,
  `readme.md`, `npm/readme.md`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file that the list below names, top to bottom,
  before editing; `CLAUDE.md` "Where a fact belongs" (a doc never restates a
  behaviour a test holds — cite the test).
- **Lazy-load:** `src/modules/render/segments.rs` and `src/modules/git.rs` as
  U1/U3 left them (for the test names to cite), `tests/e2e.rs` (the e2e test
  names), the `vwf:docs-sync` skill.

## Ruling

> 14 — Decision record: The three amendments and the retirement go into
> `docs/decisions.md`, each under the section it amends with today's date and
> both halves kept — this repo records decisions there, not under
> `docs/memory/decisions/` (CLAUDE.md, "Where a fact belongs"). Rejected: a
> `docs/memory/decisions/` doc.

The Goal section of `index.md`, verbatim, is the behaviour to describe:

> The `project` segment identifies a repository by **where it lives** … The
> **identity root** is the main checkout when the session is in a linked
> worktree, the outermost superproject when it is in a submodule, and the
> checkout itself otherwise. `projectName` — repo layer, then user layer — still
> wins, but it replaces the **name text only**; the glyph stays kind-driven.
> Branch, dirty/ahead markers, the `worktree` segment and the repo-config lookup
> are unchanged.

## Edits

Run `vwf:docs-sync` over the run's branch delta first; apply its findings and
every `DOCS FALSIFIED:` line U1–U4 returned, plus this list from the survey:

1. **`site/content/segments.md`**
   - `:41` catalogue row: replace `{sym.project}·my-repo` with a rendering that
     shows the kind-driven glyph and an `owner/repo` name; the "omits when" cell
     stays "you are not inside a git repository".
   - `:66-78` "Where `project` comes from": rewrite as the five-row table from
     the Goal (situation → glyph key → name), then the identity-root paragraph
     (worktree → main checkout, submodule → outermost superproject), then
     `projectName`: repo layer, then user layer, replaces the name only. Say the
     remote is `origin`, read from the repository's own git config, and that
     host matching is by substring (`github`, `gitlab`). Cite the e2e tests by
     name rather than restating their assertions.
2. **`site/content/repo-config.md`**
   - `:9-15`: the bar names a repository without this file — from its `origin`
     (`owner/repo`) or, without one, its `parent/base` directory; the file
     exists to call it something else.
   - `:26-28`: `<repo-root>` stays "the checkout you are in — the directory that
     contains `.git`" and gains one sentence: in a linked worktree that is the
     worktree, and the name the bar derives comes from the main checkout — the
     two are different on purpose.
   - `:45-47`: drop the `symbols.project` mention; the glyph is chosen by how
     the repository was identified (`symbols.projectGit` / `projectRemote` /
     `projectGithub` / `projectGitlab`).
   - `:49-56`: the three-rung list's third rung becomes the derived identity
     (link to `segments.md`'s table); rungs 1–2 unchanged.
3. **`site/content/diagnosing.md`**
   - `:73-77` GIT block sample: add the `project:` row in the position U3 put
     it, with a realistic value
     (`github virajp/claude-status
     (/Users/…/claude-status)`).
   - `:136` "No project name" row: rewrite — the segment omits only outside git;
     a wrong name means `projectName` is set somewhere (see Per-repo) or
     `origin` points where you do not expect (`project:` in the GIT block says
     which).
4. **`site/content/configure.md`** — `:74` `symbols` row: if it enumerates keys,
   replace `project` with the four; if generic, leave it.
5. **`site/content/_index.md`** — `:25-26` feature bullet: add "and the
   repository's identity from its `origin`" or equivalent, one clause.
6. **`readme.md:18-22`**, **`npm/readme.md:20-24`**,
   **`site/templates/index.html:44`** — alt text: "the project name" → "the
   repository (`owner/repo` from its origin)". The PNG itself is retaken after
   landing (After landing, step 1) — do not touch it.
7. **`docs/decisions.md`** — append, dated 2026-09-19, under each section named;
   keep both halves, never rewrite the earlier text:
   - `:693-719` "`project` falls back to the git root's directory name" —
     **Amended 2026-09-19** (`project-identity`): the third rung is now the
     repository's identity — `owner/repo` from `origin` on a GitHub or GitLab
     host, else `parent/base` of the identity root, which is the main checkout
     in a linked worktree and the outermost superproject in a submodule. A bare
     directory name is never drawn. `projectName` still replaces the name and
     only the name. Why: the segment answered "which folder am I in", which the
     `worktree` segment already does; the question it exists for is "which
     repository". Cite the e2e test names.
   - `:729-749` "Treat every dynamic value as hostile" — **Amended 2026-09-19**:
     a cloned repository now reaches the bar through two strings it controls,
     `projectName` and the `origin` URL's path from `.git/config`; both pass
     `sanitize`; the resolver reads only `HEAD`, `commondir` and `config` under
     the git dir it resolved, lexically.
   - `:1222-1231` "The broken-`.git` asymmetry is deliberate" — **Amended
     2026-09-19**: the walk is unchanged; a submodule's *root* is still the
     submodule, so branch and markers are its own, and its *identity* is
     resolved separately from the `gitdir:` path (`.git/modules/`). The two
     answer different questions and were kept apart on purpose.
   - `:619-637` — the "other 24 codepoints" count: add one sentence dated
     2026-09-19 giving the new count (read it from U4's `style.css` comment),
     not a rewrite.
   - A new short entry after `:693-719`'s section: **`symbols.project` and
     `symbols.repo` retired (2026-09-19)** — the glyph is now one of four
     per-kind keys; `repo` had been read by nothing since it shipped.
   - `:1205-1215` "Filesystem first" — one sentence: the project identity and
     `origin` are read the same way; still no `git` for anything the filesystem
     already says.
8. Anything `vwf:docs-sync` finds that this list does not name — apply it, and
   report it in `DECIDED:`.

## Verification

- `mise run site:build` green (link check included).
- `grep -rn 'symbols.project\b\|sym.project\b' site/content readme.md
  npm/readme.md`
  → nothing (the `.project` followed by `Git|Remote|Github|Gitlab` is fine).
- `grep -rn "git root's own directory name\|git root's directory name"
  site/content`
  → nothing outside `docs/decisions.md`'s preserved history.
- `grep -n '2026-09-19' docs/decisions.md` shows every amendment above.
- `mise run code:all` green (dprint covers the markdown).

## Guardrails

- Do not touch `src/**`, `tests/**`, `assets/**`, `schemas/**`,
  `site/static/**`, the version files, `CLAUDE.md`, `docs/plans/**`.
- Never delete or rewrite an earlier decision's text; append dated amendments.
- Do not restate a test's assertions as prose — cite the test.
- Delete with `rm`, never `git rm`.

## Commit

`docs: the project segment names the repository by where it lives` — written by
the orchestrator after the wave gate, not by the unit.
