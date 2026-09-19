# U1 — Resolve the project identity: root, kind and name

- **Wave:** 1
- **Depends on:** —
- **Owns:** `src/modules/git.rs`, `src/_shared/text.rs`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing.
- **Lazy-load:** `src/_runtime/app.rs:290-295,515-520` (only to confirm the
  `..Default::default()` construction sites compile untouched — you do not edit
  them), `src/modules/render/segments.rs:140-149` (only to see the caller U3
  will rewrite — you do not edit it).

## Ruling

> 1 — Where the identity lives: New `GitFacts.project: Option<Project>` where
> `pub struct Project { pub kind: ProjectKind, pub name: String, pub root:
> PathBuf }`
> and `pub enum ProjectKind { Git, Remote, Github, Gitlab }`, computed by a new
> `pub fn project(root: &Path) -> Option<Project>` in `git.rs` from the root
> `find_root_and_branch` returned. `root`, `branch`, the markers,
> `worktree_subpath` and the repo-config lookup are **unchanged** — they
> describe the checkout the session is in. Rejected: redefine `root` as the
> main/superproject; resolve branch and markers there too.

> 2 — The identity root: `<root>/.git` is a directory → `root`. It is a file
> with `gitdir: <p>` (parsed as `probe()` does, normalised against `root`): if
> `<p>/commondir` reads, the identity root is the **parent** of
> `normalise(<p>/<commondir contents, trimmed>)`, and a parent whose file name
> ends in `.git` has that suffix stripped for naming (bare main); else if the
> normalised `<p>` has a `.git` path component followed by `modules`, the
> identity root is the directory holding the **topmost** such `.git` component
> (outermost superproject); else `root`. Lexical only — no subprocess, no
> symlink resolution. Rejected: `git rev-parse --git-common-dir` /
> `--show-superproject-working-tree`; immediate superproject; treat any `.git`
> file as a worktree.

> 3 — Where the remote is read: The git-common config file:
> `<identity root>/.git/config` for a directory `.git`, or
> `<p>/<commondir>/config` for a linked worktree, `<topmost .git dir>/config`
> for a submodule (the superproject's). Parse **`[remote "origin"]`** only; take
> the first `url =` in that section; ignore every other remote, `pushurl`, and
> `url.<x>.insteadOf`. No file, no section, no url → kind `Git`. Rejected: first
> remote in file order; the branch's upstream remote; `git remote get-url`.

> 4 — URL parsing and kind: Accept `scp-like` (`[user@]host:path`),
> `ssh://[user@]host[:port]/path`, `https?://[user@]host[:port]/path`,
> `git://host/path`, `file://` and bare paths (→ kind `Remote`, no path use).
> Strip a leading `/`, a trailing `/`, and a trailing `.git`. Host lower-cased:
> contains `github` → `Github`; contains `gitlab` → `Gitlab`; else `Remote`. A
> URL that yields no host or an empty path → `Remote`. `Github`/`Gitlab` name =
> the **full** path (every segment, subgroups kept). `Git`/`Remote` name =
> `parent/base` of the identity root; a root with no parent component → `base`
> alone. Rejected: exact-host match; last two path segments; `Option` on
> unparseable.

> 8 (this unit's half) — git.rs: one unit per rule with hand-written layouts
> under `tempfile` — plain repo (control: kind `Git`, `parent/base`);
> `.git/config` with an `origin` on `github.com` (scp-like), `gitlab.com`
> (https, three-segment path), `codeberg.org` (→ `Remote`, `parent/base`); no
> `origin` but an `upstream` (→ `Git`); linked worktree via `gitdir:` +
> `commondir` `../..` (→ main's name, and `find_root_and_branch` still returns
> the worktree); bare main (`repo.git`, suffix stripped); submodule via
> `gitdir: ../.git/modules/lib` (→ superproject name and superproject's origin);
> nested submodule (`modules/a/modules/b` → outermost).

The user's words: "Use the current logic for main worktree. Use the folder name
from main worktree when the context switches to another worktree." "Treat as B
but use as glyph" (other hosts). "Substring" (host match). "Full path"
(subgroups). "Parse .git/config" (no subprocess). "Strip .git suffix" (bare
main). "Parent repo" / "Outermost" (submodules). "origin only".

## Edits

1. **`src/modules/git.rs`**
   - Add `ProjectKind` (`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`, four
     variants) and `Project` (`#[derive(Debug, Clone, PartialEq, Eq)]`) above
     `GitFacts`, each with a doc comment stating what it is and citing the test
     that pins it by name.
   - Add `pub project: Option<Project>` to `GitFacts` (`:23-31`). `Default`
     still derives. Doc-comment: "the repository's identity — main checkout in a
     linked worktree, outermost superproject in a submodule — for the `project`
     segment; every other field describes the checkout `cwd` is in".
   - Add `pub fn project(root: &Path) -> Option<Project>`. Steps, each a small
     private fn with its own test:
     - `identity_root(root) -> (PathBuf /*identity*/, PathBuf /*common git dir*/)`:
       implement ruling 2 exactly. Reuse the `gitdir:` parsing at `:122`
       (extract it into a private
       `fn gitdir_pointer(dot_git: &Path) ->
       Option<PathBuf>` used by
       both `probe()` and this, so the two cannot drift) and `normalise()`
       (`:166`). For the worktree branch, the common git dir is
       `normalise(<p>/<commondir>)`; for the submodule branch it is the topmost
       `.git` directory itself; for a directory `.git` it is `<root>/.git`. The
       `.git` suffix strip applies to the **name** only; the returned identity
       root path keeps its real name.
     - `origin_url(common_git_dir: &Path) -> Option<String>`: read
       `<common_git_dir>/config`; a minimal INI walk — a line matching
       `[remote "origin"]` (after trim) opens the section, any other `[` line
       closes it, and inside it the first line whose trimmed key before `=` is
       `url` yields the trimmed value. No `regex-lite` needed; do not add a
       crate.
     - `classify(url: &str) -> (ProjectKind, Option<String> /*path*/)`:
       ruling 4. Host extraction: if the string contains `://`, host is between
       `://` (after an optional `user@`) and the next `/` (port stripped); else
       if it contains `:` before any `/`, it is scp-like and host is before the
       first `:` (after `user@`); else no host. Path is everything after the
       host separator, with leading `/`, trailing `/` and trailing `.git`
       stripped. `file://` and no-host inputs → `Remote` with `None` path.
     - `parent_base(identity_root: &Path) -> String`: `file_name()` of the root,
       with a trailing `.git` stripped, joined after the parent's `file_name()`
       with `/`; `to_str` (not lossy — a non-UTF-8 component yields `None` and
       the whole `project()` returns `None`, matching today's stance at
       `segments.rs:143-146`). Root with no parent file name → base alone.
     - Assemble: `Github`/`Gitlab` with a path → that path; otherwise
       `parent_base`.
   - In `resolve()` (`:38-46`) set `project: root.as_deref().and_then(project)`
     in the struct literal. Do **not** touch `find_root_and_branch`, `probe()`'s
     behaviour, `resolve_markers`, or any subprocess.
   - Update the module doc (`:1-10`) with one sentence: the project identity is
     read from `.git`, `commondir`, `modules/` and the common `config` — still
     never from `git`.
   - Tests, in the existing `#[cfg(test)]` block, using `tempfile` and
     `fs::write` as `:318-346` do. Each test name is a sentence; each fabricates
     only what it needs:
     - `a_plain_repo_is_kind_git_and_named_parent_slash_base` — the control:
       `<tmp>/acme/widget/.git/HEAD`; expect `Git`, `"acme/widget"`, identity
       root `<tmp>/acme/widget`.
     - `an_origin_on_github_names_the_url_path` — `.git/config` with
       `[remote "origin"]\n\turl = git@github.com:acme/widget.git\n`; expect
       `Github`, `"acme/widget"`.
     - `an_origin_on_gitlab_keeps_every_path_segment` —
       `url = https://gitlab.com/group/sub/repo.git`; expect `Gitlab`,
       `"group/sub/repo"`.
     - `a_self_hosted_gitlab_matches_by_substring` —
       `url = ssh://git@gitlab.example.com:2222/team/repo`; expect `Gitlab`,
       `"team/repo"`.
     - `an_origin_on_another_host_is_kind_remote_named_parent_slash_base` —
       `url = https://codeberg.org/someone/thing.git`, repo at
       `<tmp>/src/thing`; expect `Remote`, `"src/thing"`.
     - `a_config_without_origin_is_kind_git` — only `[remote "upstream"]`;
       expect `Git`.
     - `a_linked_worktree_names_its_main_checkout` — `<tmp>/main/.git/HEAD`,
       `<tmp>/main/.git/config` (origin on github, `acme/widget`),
       `<tmp>/main/.git/worktrees/wt/HEAD`,
       `<tmp>/main/.git/worktrees/wt/commondir` = `../..\n`, `<tmp>/wt/.git` =
       `gitdir: <tmp>/main/.git/worktrees/wt\n`; expect `project(&wt)` =
       `Github`, `"acme/widget"`, identity root `<tmp>/main`; **and**
       `find_root_and_branch(&wt).0 == Some(wt)`.
     - `a_linked_worktree_of_a_bare_main_strips_the_dot_git_suffix` —
       `<tmp>/src/repo.git/{HEAD,config(no origin),worktrees/wt/{HEAD,commondir=../..}}`,
       `<tmp>/wt/.git` pointer; expect `Git`, `"src/repo"`.
     - `a_submodule_names_its_superproject` —
       `<tmp>/acme/super/.git/{HEAD,config(origin github acme/super)}`,
       `<tmp>/acme/super/.git/modules/lib/HEAD`, `<tmp>/acme/super/lib/.git` =
       `gitdir: ../.git/modules/lib\n`; expect `project(&lib)` = `Github`,
       `"acme/super"`; and `find_root_and_branch(&lib).0 == Some(lib)`.
     - `a_nested_submodule_names_the_outermost_superproject` —
       `.git/modules/a/modules/b/HEAD`, `<super>/a/b/.git` =
       `gitdir: ../../.git/modules/a/modules/b\n`; expect the outer name.
     - `a_pointer_with_neither_commondir_nor_modules_is_its_own_project` —
       `.git` = `gitdir: ../store/gitdir` with only `HEAD` there; expect `Git`,
       `parent/base` of the checkout.
     - `a_url_with_no_host_is_kind_remote` — `url = /srv/git/thing.git` and
       `url = file:///srv/git/thing.git`; expect `Remote`, `parent/base`.
     - `classify_handles_every_accepted_url_shape` — table test over the forms
       in ruling 4 asserting `(kind, path)`.
     - Keep every existing test green and unedited.
2. **`src/_shared/text.rs`** — in the `sanitize` doc comment (`:10-56`), amend
   the paragraph at `:19-29`: a cloned repository now reaches the bar through
   **two** strings it controls — `projectName` from its repo layer, and the
   `origin` URL's path from its `.git/config` — both drawn only after this
   function. Add "a remote URL" to the attacker-nameable list at `:13-17`. No
   code change.

## Verification

- `cargo test --features schema git::` green, including every new test above.
- `cargo clippy --all-targets -D warnings` with and without `--features schema`
  clean.
- `grep -n 'Command' src/modules/git.rs` shows no new subprocess.
- `grep -n 'canonicalize' src/modules/git.rs` → nothing.
- `mise run code:all` green.

## Guardrails

- Do not touch `src/_runtime/app.rs`, `src/modules/render/segments.rs`,
  `src/modules/config/**`, `tests/**` — U2 and U3 own them. The new field
  defaults, so the construction sites compile untouched.
- No new crate. No `git` subprocess for identity or remote. No `canonicalize` —
  lexical like `normalise()`.
- Delete with `rm`, never `git rm`.
- The `.git` suffix strip applies to the bare-main **name**; never to a path you
  then read files from.

## Commit

`feat(git): resolve the project identity — main checkout, superproject, origin`
— written by the orchestrator after the wave gate, not by the unit.
