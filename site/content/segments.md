+++
title = "Segments"
description = "The eleven pieces the bar is built from, and when each one sits out."
weight = 4
+++

The bar is rows of segments, and the rows are yours. `lines` in your config is
an ordered list of rows; each row is an ordered list of segment ids. The shipped
layout is two rows:

```json
{
  "lines": [
    ["model", "context", "rl5h", "rl7d", "spend", "cost"],
    ["project", "worktree", "branch"]
  ]
}
```

Mix and match freely. A row that ends up with no visible segments is dropped
rather than drawn empty, which is why the second row disappears entirely outside
a git repository.

## The catalogue

Eleven segments. `{sym.x}` below is the glyph configured under `symbols`, and
every `·` is one literal space — the renderer adds one more on each side of the
whole text.

| id         | Draws                                    | Sits out when                           |
| ---------- | ---------------------------------------- | --------------------------------------- |
| `model`    | `{sym.model}·Opus 5·[high]`              | never — falls back to `Claude`          |
| `context`  | `{sym.context}·▰▰▰▱▱▱▱▱▱▱·259k/1M·(26%)` | never                                   |
| `rl5h`     | `{sym.win5h}·7.0%·{sym.reset}·4h36m`     | the payload carries no percentage       |
| `rl7d`     | `{sym.win7d}·1.0%·{sym.reset}·5d2h`      | the payload carries no percentage       |
| `session`  | `{sym.session}·users-and-groups`         | the session name is absent **or empty** |
| `cost`     | `{sym.cost}·$46.51`                      | never — an absent cost renders `$0.00`  |
| `spend`    | `{sym.spend}·$75.93/$150·(51%)`          | any of four gates — see below           |
| `duration` | `{sym.duration}·9hr 19m`                 | the duration is **absent**; `0` renders |
| `project`  | `{sym.project}·my-repo`                  | `projectName` is not set **in config**  |
| `worktree` | `{sym.worktree}·{sym.folder}·sub/path`   | you are not inside a worktree           |
| `branch`   | `{sym.branch}·main·↑·±`                  | no branch could be resolved             |

Three of those have a detail worth knowing:

- **The rate-limit segments** show the reset half only when the reset time is
  known; the percentage alone renders otherwise.
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

`project` reads `projectName`, and `projectName` is a **repo-level** key — it
does not come from the session payload and it is not in the shipped defaults. A
repository that has not been named omits the segment. See
[Per-repo](@/repo-config.md).

## Why `spend` is missing

Four gates hide it, checked in this order:

1. `spend` is not in your `lines`.
2. There is no usable cached figure yet.
3. The budget is unusable — either disabled, or a limit of zero. (A *missing*
   budget block is gate 2, not this one.)
4. The seat is one that `show: "auto"` hides. `auto` shows the segment only for
   `team` and `enterprise` seats, so every other plan — and any cache with no
   plan recorded at all — is hidden here. Much the most common answer.

[`--debug`](@/diagnosing.md) reports all four and tells you which one applied.
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
