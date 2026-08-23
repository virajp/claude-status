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

| Plan                                                    | Target repo     | Requires     | Landed | Status                       |
| ------------------------------------------------------- | --------------- | ------------ | ------ | ---------------------------- |
| [2026-08-21-2122-release](./2026-08-21-2122-release.md) | `claude-status` | `macos-only` | —      | needs re-planning, see below |

Accepted but not yet cut into a cycle: [`backlog.md`](./backlog.md).

### `release` is unexecuted and out of date

It is the only plan left, and it is the one thing that has never happened: the
first publish. Its **substance** still stands — npm requires a manual publish
before a Trusted Publisher can be configured, and the two criteria carried
forward from `distribution` are still open.

Its **steps** do not. It was written against a three-package, two-target world.
Since then `macos-only` cut six targets to two, `github-artifacts` collapsed
three npm packages to one, and `release-fix` cut two targets to one and put the
binary back inside that package. So its "reserve three names", its "register
OIDC for the three packages" and its `optionalDependencies` reasoning all
describe a shape the repo no longer has.

**Re-plan it before running it.** The one-package, one-target version is a
materially shorter cycle than the doc describes.

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

**`distribution` was archived with two acceptance criteria open**, deliberately
— one binary per platform from a real publish, and a machine with no Node and no
Rust running it. Both are carried forward verbatim into
[`release`](./2026-08-21-2122-release.md), which is where they can be closed.

**`macos-only` was archived with one open**, on the same terms: npm 11 has no
way to simulate a foreign host, so the `EBADPLATFORM` proof also moves to
`release`. The field is in place and correct; only the proof is outstanding.

**`github-artifacts` executed in full and was then deliberately reversed** by
`release-fix`, which moved the binary back inside the npm package. It is
archived as executed rather than as abandoned — every step landed — and the
reversal is reasoned out in its own `release-fix/index.md`.

## Nothing has shipped

The repo carries no release tag, and the only thing on the registry is a `0.0.1`
placeholder reserving `@askviraj/claude-status`. The first real release also
cannot go through CI —
[npm requires a package to exist before a Trusted Publisher can be configured](https://github.com/npm/cli/issues/8544),
so the name needs one manual publish first.
[`release`](./2026-08-21-2122-release.md) owns that, once re-planned.

**One name, one target.** After `release-fix`, a complete release is a single
npm package carrying a single `aarch64-apple-darwin` binary — no wrapper, no
platform packages, no download.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` — the JS
bar and `context-caps.js` both live there — from `virajp/ai-plugins`, retiring
the `cli/src/statusline.ts` install path that deploys them to
`~/.claude/scripts/statusline` and `~/.claude/hooks/context-caps.js`, and
pointing that repo's docs at this one. A change to a different repository, gated
on [`release`](./2026-08-21-2122-release.md) having shipped.

It also owns the one simplification this repo cannot make until it lands:
retiring `$AI_PLUGINS_USAGE_DIR` together with §8's frozen mirror field names —
once the JS hook is gone, this binary is both the writer and the only reader of
that file. The transitional `.config/statusline.json` is already gone, deleted
in `16c3ac3`.
