+++
title = "Segments"
description = "The eleven pieces the bar is built from, and when each one sits out."
weight = 5
+++

The bar is rows of segments, and the rows are yours. `lines` in your config is
an ordered list of rows; each row is an ordered list of segment ids. The shipped
layout is two rows:

```json
{
  "lines": [
    ["model", "context", "rl5h", "rl7d", "rl7dm", "spend"],
    ["project", "worktree", "branch", "cost"]
  ]
}
```

Mix and match freely. A row that ends up with no visible segments is dropped
rather than drawn empty, which is why the second row disappears entirely outside
a git repository.

## The catalogue

Twelve segments. `{sym.x}` below is the glyph configured under `symbols`, and
every `·` is one literal space — the renderer adds one more on each side of the
whole text.

| id         | Draws                                    | Sits out when                           |
| ---------- | ---------------------------------------- | --------------------------------------- |
| `model`    | `{sym.model}·Opus 5·[high]`              | never — falls back to `Claude`          |
| `context`  | `{sym.context}·▰▰▰▱▱▱▱▱▱▱·259k/1M·(26%)` | never                                   |
| `rl5h`     | `{sym.win5h}·7%·{sym.reset}·4h36m`       | the payload carries no percentage       |
| `rl7d`     | `{sym.win7d}·1%·{sym.reset}·5d2h`        | the payload carries no percentage       |
| `rl7dm`    | `Fable·12%`                              | the seat has no per-model 7-day window  |
| `session`  | `{sym.session}·users-and-groups`         | the session name is absent **or empty** |
| `cost`     | `{sym.cost}·$46.51`                      | never — an absent cost renders `$0.00`  |
| `spend`    | `{sym.spend}·$75.93/$150·(51%)`          | any of four gates — see below           |
| `duration` | `{sym.duration}·9hr 19m`                 | the duration is **absent**; `0` renders |
| `project`  | `{sym.projectGithub}·acme/widget`        | you are not inside a git repository     |
| `worktree` | `{sym.worktree}·{sym.folder}·sub/path`   | you are not inside a worktree           |
| `branch`   | `{sym.branch}·main·↑·±`                  | no branch could be resolved             |

Three of those have a detail worth knowing:

- **The rate-limit segments** show the reset half only when the reset time is
  known; the percentage alone renders otherwise.
- **`rl7dm`** is the 7-day window broken down per model, for seats where one
  model has its own weekly limit beside the shared one. It does not come from
  the session payload: it is read from the same cached fetch `spend` uses, so it
  appears on every plan, refreshes on the `spend.refreshMinutes` timer, and
  omits until the first fetch has landed. Several models are joined with `·` — a
  real middle dot, one space each side. No reset time is shown — it is the one
  `rl7d` already carries.
- **`context` always renders**, even with no data at all — as
  `{sym.context}·▱▱▱▱▱▱▱▱▱▱·?/?·(0%)`.
- **`branch`** carries the branch glyph, then the name. Three things around it
  are conditional: the *worktree* glyph appears **before** the branch glyph only
  when you are inside a worktree, and the `↑` (ahead of upstream) and `±`
  (dirty) markers follow it. Each is preceded by exactly one space. Dirty is `+`
  for additions only, `-` for deletions only, `±` for both. So an ordinary
  checkout renders `{sym.branch}·main`, and the same branch inside a worktree
  renders `{sym.worktree}·{sym.branch}·main`.

## Where `project` comes from

Never from the session payload. The segment names the repository by **where it
lives**, and the glyph says how it found out:

| You are in                                | Glyph                   | Name                                              |
| ----------------------------------------- | ----------------------- | ------------------------------------------------- |
| no git repository                         | —                       | segment omitted                                   |
| a repository with no `origin` remote      | `symbols.projectGit`    | `parent/base` of the identity root, `src/my-repo` |
| one whose `origin` host contains `github` | `symbols.projectGithub` | the URL's path, `acme/widget`                     |
| one whose `origin` host contains `gitlab` | `symbols.projectGitlab` | the URL's path, subgroups kept, `group/sub/repo`  |
| one whose `origin` is on any other host   | `symbols.projectRemote` | `parent/base` of the identity root                |

The **identity root** is the checkout itself in an ordinary repository, the
**main checkout** when you are in a linked worktree, and the **outermost
superproject** when you are in a submodule — so a worktree and a submodule are
named for the repository they belong to, while `branch`, the dirty and ahead
markers and the `worktree` segment still describe the checkout you are standing
in.

The remote is `origin` and only `origin`, read from the repository's own git
config the way git reads it — no other remote, no `insteadOf` rewrite, no
`[include]`. Host matching is by substring, so `github.example.com` counts as
GitHub. Two shapes are named differently from what you might expect: a linked
worktree of a hidden bare store that no `.git` pointer names (the dotfiles
pattern, `~/.cfg`) or of a `--separate-git-dir` store still gets the right glyph
from that store's `origin`, but when there is no `origin`, or it is on some
other host, the `parent/base` name is the store's (`user/.cfg`, `store/repo`)
rather than the checkout's. And an ssh alias in place of the host
(`gh:acme/widget`) is not a GitHub host, so it draws `projectRemote` and
`parent/base`.

`projectName` — repo layer first, then user layer — replaces the **name** and
only the name; the glyph stays with how the repository was identified. Set
nowhere, the name above is what draws. See [Per-repo](@/repo-config.md).

The behaviour is pinned by three end-to-end tests in `tests/e2e.rs`:
`a_repository_is_named_by_its_origin_from_the_checkout_a_worktree_and_a_submodule`,
`a_repository_without_an_origin_is_named_parent_slash_base` and
`doctor_names_how_the_project_was_identified`.

## Why `spend` is missing

Four gates hide it, checked in this order:

1. `spend` is not in your `lines`. (The fetch behind it still runs if `rl7dm`
   is.)
2. There is no usable cached figure yet.
3. The budget is unusable — either disabled, or a limit of zero. (A *missing*
   budget block is gate 2, not this one.)
4. The seat is one that `show: "auto"` hides. `auto` shows the segment only for
   `team` and `enterprise` seats, so every other plan — and any cache with no
   plan recorded at all — is hidden here. Much the most common answer.

[`--doctor`](@/diagnosing.md) reports all four and tells you which one applied.
More on the segment in [Configure](@/configure.md).

## Getting an id wrong

Typo a segment id and you get a note on **stderr** and a bar without it. The
render does not fail, the exit code stays 0, and stdout is only ever the bar:

```text
statusline: unknown segment "brnach"
```

## Styling one

Every segment takes `bg`, `fg` and `bold`, resolved **inline override →
`segments.<id>` → the built-in fallback**:

```json
{
  "segments": {
    "branch": { "bg": "aqua" },
    "cost": { "bg": "green", "fg": "white", "bold": true }
  }
}
```

Or, for one position only, put an object in the row instead of a bare id:

```json
{
  "lines": [
    ["model", { "name": "cost", "bg": "red" }]
  ]
}
```

Colours are covered in [Configure](@/configure.md).
