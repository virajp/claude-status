---
type: vwf-plan-backlog
title: Backlog — claude-status
description: Ideas accepted but not yet planned. Each entry is a candidate
  slice, not a commitment, and carries what is already true so a plan does not
  re-derive it.
status: stable
---

# Backlog

Things worth doing, not yet cut into a cycle. An entry graduates by becoming a
plan doc in [`docs/plans/`](./index.md) — until then nothing here is a promise,
and the "already true" notes are the point: they are what stops a future plan
from re-discovering the same ground.

## Nothing pending

**Every entry has graduated.** The four that were here were cut into plans on
2026-08-23 and now live in the three folders under [`docs/plans/`](./index.md):

| Was                                 | Became                                                                       |
| ----------------------------------- | ---------------------------------------------------------------------------- |
| `$schema` in every generated config | [config-and-cli/04](./2026-08-23-config-and-cli/04-schema-and-validation.md) |
| Config validation in `--debug`      | [config-and-cli/04](./2026-08-23-config-and-cli/04-schema-and-validation.md) |
| A Homebrew formula                  | [distribution/02](./2026-08-23-distribution/02-homebrew-formula.md)          |
| Remove the npm installer            | [distribution/01](./2026-08-23-distribution/01-drop-npm.md)                  |

Planning turned up more than the entries asked for, and the extra plans are
worth knowing about because no backlog entry predicted them:

- **There were no Rust config types** to hang a schema or a validator off, so
  [config-and-cli/01](./2026-08-23-config-and-cli/01-typed-config.md) exists to
  create them.
- **Retiring the installer stopped being a port and became a deletion**, once
  `--configure` moved into the binary
  ([config-and-cli/03](./2026-08-23-config-and-cli/03-cli-surface.md)) and it
  was settled that nothing needs migrating.
- **A website was commissioned** — [website/](./2026-08-23-website/index.md) —
  which is now part of the install flow rather than marketing, since both the
  formula's caveats and `--help` point at it.

## Closed

Kept as a record of what was raised and where it went, so a later reader does
not re-open any of them.

- **Nothing seeded the repo-level config layer.** Shipped as the installer's
  `--configure` (`d97dc4b`), with the binary growing a matching autoseed on the
  render path (`60f5c0f`) so a repo gets its layer without anyone running a
  command. It answered its own open questions: the **installer** owns the flag,
  not the binary; an existing `claude-status.json` is kept and only gains a
  missing `projectName`; a lone `statusline.json` is rewritten rather than
  renamed (`c42aabc`) with its `$schema` repointed; the name is the directory
  basename, not the git remote; and it honours `--dry-run`. `projectName` became
  repo-level only with `autoConfigureRepo` defaulting on (`03f6773`).

- **An unresolvable `$HOME` had no defined meaning.** Raised by security review
  during `macos-only`, deferred, then fixed in that same cycle. The contract now
  says absent-never-relative and the four callers agree with it.

- **Path-derived segment text reached the row unfiltered.** Same origin, same
  outcome: filtering now sits at the single point every segment's text passes
  through.
