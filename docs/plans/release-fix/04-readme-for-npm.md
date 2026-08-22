---
type: vwf-plan
title: readme-for-npm — 2026-08-22
description: Rewrite readme.md for the npm listing page — the audience is
  someone deciding whether to install, not someone working on this repo.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/release-fix/02-embed-the-binary.md,
]
timestamp: 2026-08-22T17:30:00Z
tags: [ docs, npm ]
---

# Plan: readme-for-npm — 2026-08-22

## Slice

`readme.md` is what `build:installer` stages as the package's `README.md`, and
npm publishes it whatever `files` says. It is therefore the listing page — the
first and usually only thing a prospective user reads. It is currently written
for someone standing in the repository.

**The audience is a user deciding whether to install**, on a page with no
sidebar, no file tree and no way to click into `src/`. Everything that only
makes sense with the repo checked out belongs somewhere else.

## Current state (actual)

```
# Statusline for Claude Code
## Requirements          ← macOS statement, then unsupported-platform prose
## Install
### Running it elsewhere ← `cargo build --release`, Windows/Linux compile notes
## Uninstall
## Configuration
### Getting a repo layer
### Segments
### The spend segment, and why it is usually hidden
## Diagnosing
## Environment
## Building              ← `mise run setup:all`, build:statusline, build:installer
## Licence
```

Three passages are repo-developer content on a package listing:

- **`Running it elsewhere`** — `cargo build --release`, which Rust targets fail
  to compile, that `ring` needs a C toolchain, and a pointer at
  `supported_targets` in `.config/mise/tasks/_scripts/_rust`. None of this is
  actionable for someone who ran `npx`.
- **`Building`** — `mise run setup:all`, `code:test`, `build:statusline`,
  `build:installer`. Task names for a checkout the reader does not have.
- **Parts of `Requirements`** — the paragraph arguing that building from source
  is "genuinely unsupported — not a soft *we'd rather you didn't*" is an
  argument aimed at contributors.

There is no `CONTRIBUTING.md`, so today the readme is the only home this content
has. That is why it is there, and it is why this plan has to give it somewhere
to go rather than just deleting it.

## Target state (per contract)

`readme.md` reads as a package page: what it is, what it looks like, what it
needs, how to install, how to configure, how to diagnose, how to remove.

Developer content moves to **`CONTRIBUTING.md`** at the repo root — building,
the task library, the target table, and the from-source notes — linked once from
the readme in a single line near the end, so a contributor arriving via GitHub
is not stranded.

The readme keeps a short Requirements section, because "Apple Silicon Mac only"
is exactly the kind of thing a prospective user must learn *before* installing,
not after `EBADPLATFORM`.

## Delta — ordered steps

### 1. Create `CONTRIBUTING.md`

Move `Building` and `Running it elsewhere` there verbatim first, then edit in
place. Moving and rewriting in one step is how the reasoning in those paragraphs
gets quietly lost — the Windows compile notes in particular name three specific
places in the crate and are hard-won.

→ **verify:** every task name and file path that was in the readme is in the new
file; `mise run` commands there still exist in `mise tasks`.

### 2. Trim `Requirements` to what a user must know first

Three facts: Apple Silicon Mac only; Node 18+ to run the installer once, nothing
after; npm refuses other platforms with `EBADPLATFORM` rather than installing
something broken. The from-source argument goes to `CONTRIBUTING.md`.

→ **verify:** the section is short enough to read before the fold, and states
the Intel exclusion explicitly — after plan 1, npm's terse error is the only
other place a user learns it.

### 3. Drop `Building` from the readme

Replaced by one line: a pointer to `CONTRIBUTING.md` for anyone wanting to build
or contribute.

→ **verify:** no `mise run` invocation remains in `readme.md`.

### 4. Read the whole file as a listing page

With plan 2 landed there is no download, no network requirement and no proxy
note to remove — plan 2 removes them. What remains is a pass for repo-shaped
assumptions: links that resolve only on GitHub, references to files by path,
"see the contract" pointers that a package reader cannot follow.

Relative links are the concrete hazard. npm does not reliably rewrite them, and
a `CONTRIBUTING.md` link that 404s on npmjs.com is worse than no link. Use
absolute `https://github.com/virajp/claude-status/blob/main/…` URLs for anything
outside the package, exactly as the screenshot already does and for the same
reason.

→ **verify:** every link in `readme.md` is either absolute or an in-page anchor.

### 5. Keep the two audiences from diverging

`CONTRIBUTING.md` at the repo root is not just a filename: GitHub surfaces it in
the new-issue and pull-request flows, so a contributor is pointed at it without
having to go looking. That is a better outcome than `docs/development.md`, which
nothing links to automatically.

`readme.md` is now the package page **and** the repository landing page — GitHub
renders it too. That is fine; the two audiences want the same first 80% and
diverge only at "how do I work on this", which is now one link.

Note it in `CONTRIBUTING.md` so the next person editing the readme knows they
are editing an npm page.

→ **verify:** the note exists; `/vwf:docs-sync` over the cycle's commit range.

## Acceptance criteria (from contract)

1. `readme.md` contains no `cargo` invocation and no `mise run` invocation.
2. Every link in `readme.md` is absolute or an in-page anchor.
3. `CONTRIBUTING.md` exists and carries the build instructions, the task library
   and the from-source notes, with their reasoning intact.
4. `Requirements` states Apple Silicon only, in the first screen of the page.
5. The staged `README.md` is byte-identical to `readme.md`.

## Risks / drift

**Content moved is content that stops being read.** The Windows and Linux
compile notes are genuinely useful to the next person who considers adding a
target, and burying them in `CONTRIBUTING.md` makes them easier to miss.
Contract §9 is the durable home for the *decision*; `CONTRIBUTING.md` should
carry the mechanics and point at §9.

**Two files can now disagree** about what the supported platforms are. The
readme states them for users, `CONTRIBUTING.md` for builders, and
`supported_targets()` is the truth. Neither doc derives from it, and nothing
checks them — worth a follow-up, out of scope here.

## Out of scope for this cycle

- **A contribution *policy*.** `CONTRIBUTING.md` gets the build mechanics this
  cycle. How to propose a change, what review looks like, what a PR needs — that
  is a separate document's worth of decisions and is not being invented here.
- **Restructuring `Configuration`.** It is long, but it is the part of the page
  a user actually needs; length there is earned.

## Gaps surfaced during execution

*(filled in during execution)*
