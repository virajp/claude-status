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

Two folders, independent of each other. Nothing in one blocks anything in the
other.

### [config-ergonomics](./2026-08-23-config-ergonomics/index.md)

| # | Plan                                                                                | Requires | Status |
| - | ----------------------------------------------------------------------------------- | -------- | ------ |
| 1 | [typed-config](./2026-08-23-config-ergonomics/01-typed-config.md)                   | —        | active |
| 2 | [schema-and-validation](./2026-08-23-config-ergonomics/02-schema-and-validation.md) | 1        | active |

Gives the config real Rust types, then makes those types the single source for
the JSON schema and for a `--debug` validation section.

### [distribution](./2026-08-23-distribution/index.md)

| # | Plan                                                                 | Requires | Status      |
| - | -------------------------------------------------------------------- | -------- | ----------- |
| 1 | [release](./2026-08-23-distribution/01-release.md)                   | —        | active      |
| 2 | [homebrew-formula](./2026-08-23-distribution/02-homebrew-formula.md) | 1        | active      |
| 3 | [retire-installer](./2026-08-23-distribution/03-retire-installer.md) | 2        | **blocked** |

Ships the first release, adds a Homebrew tap, and — once the tap is proven —
moves the wiring into the binary and retires the npm installer.

**Plan 3 is gated on a person.** It runs when the maintainer confirms the tap is
proven in real use, not when plan 2 merges. It removes the only install path
that has ever worked.

**Execution order across both folders** — `distribution/01-release` is the only
irreversible cycle in the tree and the only one anything external depends on. If
capacity is limited, it goes first.

Accepted but not yet cut into a cycle: [`backlog.md`](./backlog.md) — currently
empty, every entry having graduated into the two folders above.

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

Filename timestamps record when each plan was written, not when it ran:
`caps-hook` was added after `distribution` and preceded it, and `distribution`
ran partly ahead of its `requires:` on request.

**Three acceptance criteria were archived open**, all deliberately, and all
carried forward into
[`distribution/01-release`](./2026-08-23-distribution/01-release.md): one binary
per platform from a real publish and a no-Node-no-Rust machine running it (from
`distribution`), and the `EBADPLATFORM` proof (from `macos-only`, where npm 11
had no way to simulate a foreign host).

**`github-artifacts` executed in full and was then deliberately reversed** by
`release-fix`, which moved the binary back inside the npm package. It is
archived as executed rather than as abandoned — every step landed.

## Superseded, never executed

`2026-08-21-2122-release.md` was **deleted** in this round rather than archived.
It was written for three npm packages and two targets, a shape three later
cycles removed, and it never ran — so it belonged neither in the archive (which
is for executed cycles) nor in the active set. Its substance and its three open
criteria are carried into
[`distribution/01-release`](./2026-08-23-distribution/01-release.md); git
history holds the original.

## Nothing has shipped

The repo carries no release tag, and the only thing on the registry is a `0.0.1`
placeholder reserving `@askviraj/claude-status`. The release pipeline —
`verify`, `test`, `build`, `publish` — is complete and has never run on a tag.
The first publish also cannot go through CI unassisted;
[npm requires a package to exist before a Trusted Publisher can be configured](https://github.com/npm/cli/issues/8544).
[`distribution/01-release`](./2026-08-23-distribution/01-release.md) owns all of
it.

**One name, one target.** After `release-fix`, a complete release is a single
npm package carrying a single `aarch64-apple-darwin` binary — no wrapper, no
platform packages, no download.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` — the JS
bar and `context-caps.js` both live there — from `virajp/ai-plugins`, retiring
the `cli/src/statusline.ts` install path that deploys them to
`~/.claude/scripts/statusline` and `~/.claude/hooks/context-caps.js`, and
pointing that repo's docs at this one. A change to a different repository, gated
on a shipped release.

It also owns the one simplification this repo cannot make until it lands:
retiring `$AI_PLUGINS_USAGE_DIR` together with §8's frozen mirror field names —
once the JS hook is gone, this binary is both the writer and the only reader of
that file. The transitional `.config/statusline.json` is already gone, deleted
in `16c3ac3`.

**Code signing and notarisation.** Deferred by §9 and still unowned. It becomes
more visible with a Homebrew tap — people expect a brew-installed binary to be
signed — but no plan in the tree claims it.
