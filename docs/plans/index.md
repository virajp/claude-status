---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, what it requires, what has
  landed, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline. Each plan is a diff against
[the behaviour contract](../spec/statusline-behaviour.md), and each ends at
something runnable.

## Active

Nine plans in three folders.

### [config-and-cli](./2026-08-23-config-and-cli/index.md)

| # | Plan                                                                             | Requires |
| - | -------------------------------------------------------------------------------- | -------- |
| 1 | [typed-config](./2026-08-23-config-and-cli/01-typed-config.md)                   | —        |
| 2 | [config-relocation](./2026-08-23-config-and-cli/02-config-relocation.md)         | 1        |
| 3 | [cli-surface](./2026-08-23-config-and-cli/03-cli-surface.md)                     | 2        |
| 4 | [schema-and-validation](./2026-08-23-config-and-cli/04-schema-and-validation.md) | 3        |

Types for the config, a move to `~/.config/claude-status/` storing only
non-defaults, `--configure` and the reshaped flag set, then a generated schema.

### [distribution](./2026-08-23-distribution/index.md)

| # | Plan                                                                 | Requires         |
| - | -------------------------------------------------------------------- | ---------------- |
| 1 | [drop-npm](./2026-08-23-distribution/01-drop-npm.md)                 | config-and-cli 3 |
| 2 | [homebrew-formula](./2026-08-23-distribution/02-homebrew-formula.md) | 1                |
| 3 | [release](./2026-08-23-distribution/03-release.md)                   | 2                |

Delete the npm installer, ship a Homebrew tap, cut `v0.1.0`.

### [website](./2026-08-23-website/index.md)

| # | Plan                                                            | Requires            |
| - | --------------------------------------------------------------- | ------------------- |
| 1 | [site](./2026-08-23-website/01-site.md)                         | —                   |
| 2 | [config-generator](./2026-08-23-website/02-config-generator.md) | 1, config-and-cli 4 |

`claude-status.virajp.dev` — Zola on Cloudflare Pages, then a schema-driven
config generator with a fixture-gated live preview.

### Cross-folder dependencies

Two, and both are easy to miss because the folders read as independent:

- **`distribution/01` requires `config-and-cli/03`.** It deletes the npm
  installer, which is the only thing that can wire Claude Code until
  `--configure` exists.
- **`website/02` requires `config-and-cli/04`.** The form is built from the
  generated schema.

One soft ordering, not enforced by `requires:`: **`website/01` should land
before `distribution/02`**, or the formula's caveats print a link to a site that
does not exist yet.

Accepted but not yet cut into a cycle: [`backlog.md`](./backlog.md) — currently
empty.

## The direction these replace

An earlier set of five plans, written the same day, was scrapped rather than
amended. It assumed npm stayed the channel, the installer was ported to Rust
rather than deleted, the config kept its current shape and paths, and there was
no website. Enough of that changed at once that editing would have left five
documents arguing with themselves.

**Their findings were harvested, not lost** — the accessor coercion table, the
`settings.json` key asymmetry, the tap and formula reasoning, the
strict/permissive validation split, and the release pipeline's shape all carried
into the new set. Git history holds the originals.

## Archived

Twelve cycles, all executed. They live on disk under `docs/plans/archived/` and
are **gitignored** — the record is kept locally, out of the repo. Nothing below
links, because in a fresh clone there is nothing to link to; the merge commit is
the durable pointer.

| Plan                                | Requires                         | Landed                    |
| ----------------------------------- | -------------------------------- | ------------------------- |
| `2026-08-19-1400-main-bar`          | —                                | merged in `7cb6247`       |
| `2026-08-19-1401-subagent-panel`    | main-bar                         | merged in `8728158`       |
| `2026-08-19-1402-spend`             | main-bar                         | merged in `d68baf6`       |
| `2026-08-19-1404-caps-hook`         | main-bar                         | merged in `61a5445`       |
| `2026-08-19-1403-distribution`      | subagent-panel, spend, caps-hook | merged in `33d6abc`       |
| `2026-08-21-2121-macos-only`        | —                                | merged in `e1d6125`       |
| `2026-08-22-2147-github-artifacts`  | —                                | `68c2c21`, since reversed |
| `release-fix/01-apple-silicon-only` | github-artifacts                 | `2474d51`                 |
| `release-fix/02-embed-the-binary`   | 01                               | `4f9dc93`                 |
| `release-fix/03-mise-consolidation` | —                                | `ed93a7c`                 |
| `release-fix/04-readme-for-npm`     | 02                               | `3288e9c`                 |

Filename timestamps record when each plan was written, not when it ran.

**Three acceptance criteria were archived open.** Two — one binary per platform
from a real publish, and a machine with no toolchain running it — are carried
into [`distribution/03`](./2026-08-23-distribution/03-release.md), restated for
Homebrew. The third, `macos-only`'s `EBADPLATFORM` proof, **dies with npm** and
is closed as no longer applicable.

## Nothing has shipped

No release tag, no GitHub Release. The registry holds a `0.0.1` placeholder
reserving `@askviraj/claude-status`, which will be deprecated rather than
published to. [`distribution/03`](./2026-08-23-distribution/03-release.md) owns
the first release.

**One target.** `supported_targets()` has one row, `aarch64-apple-darwin`. Linux
was evaluated for this round and **deferred, not rejected** — the technical cost
is low, but nobody has confirmed Claude Code on Linux writes
`~/.claude/.credentials.json`, without which the spend segment silently never
renders. The evaluation is recorded in the
[distribution index](./2026-08-23-distribution/index.md); do not re-derive it.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` from
`virajp/ai-plugins` and pointing that repo's docs at this one. A different
repository, gated on a shipped release.

Contract §8's usage mirror and `$AI_PLUGINS_USAGE_DIR` are **explicitly
unchanged** by every plan in the tree — it is a live contract with that repo.

**Code signing and notarisation.** Deferred by §9 and still unowned. More
visible with a Homebrew tap, since people expect a brew-installed binary to be
signed, and it is not.

**Linux.** See above.
