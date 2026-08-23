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

## Config ergonomics

The two entries below are one theme — making the config file writable by hand
without guessing — and would likely make one slice rather than two.

### Pin the `$schema` URL, and publish the schema

**Ask.** Editors should resolve the schema that matches the *installed* binary,
and ideally find it without the key being present at all.

**Already true — every generated config carries the key, and a plan should not
redo that.** `schemas/claude-status.schema.json` exists, is a draft-2020-12
schema with `additionalProperties: false`, and declares `$schema` as a permitted
key. All three writers emit it: `assets/claude-status.defaults.json` (what
`--install` seeds the user layer from, byte-for-byte),
`installer/src/_runtime/configure.ts` and `src/modules/config/autoseed.rs` (the
repo layer, seeded and migrated alike — both from a `SCHEMA_URL` constant).

**What is actually left.** The `$id`/`$schema` URL points at the mutable `main`
ref, so an editor resolves whatever `main` says today rather than the schema
matching the installed binary — worth deciding whether to pin to a tag once
[`release`](./2026-08-21-2122-release.md) ships. And whether to publish to
SchemaStore, so editors find it without the key at all.

### Config validation in `--debug`

**Ask.** `--debug` should validate the config and report what is wrong.

**Why.** Every layer is currently *silently* forgiving: a layer that is missing,
malformed, or not a JSON object is ignored rather than fatal
([contract §2](../spec/statusline-behaviour.md)), which is correct for a bar
that redraws every four seconds and must never fail to render — but it means a
typo'd key is indistinguishable from a key that does nothing. The only feedback
today is the unknown-`lines`-id warning on stderr.

**Already true.** `--debug` already reports the three config layers and which
resolved, so the reporting surface exists and this is a new section in it, not a
new command. `additionalProperties: false` in the schema means an unknown key is
already *defined* as an error — the definition just is not enforced anywhere.

**Open questions a plan must answer.** Whether to validate against the JSON
schema (a dependency, and a second source of truth to keep in step with the Rust
types) or to have the deserializer report unknown fields (`serde`'s
`deny_unknown_fields` on a shadow type, no dependency, but duplicates the
schema's job). Whether validation stays `--debug`-only — it must never make a
render fail. Whether it exits non-zero, which would let a setup script check a
config.

## Distribution

The two entries below are **ordered, and the second is gated on a human**. The
tap has to exist and be proven in real use before anything is taken away, and
the maintainer says when that has happened — see the entry.

### A Homebrew formula

**Ask.** Ship the binary through a Homebrew tap, so `brew install` is a way in
alongside `npx @askviraj/claude-status --install`.

**Why.** The product is Apple Silicon macOS only, and Homebrew is how macOS
developers install a CLI. npm is a strange front door for a Rust binary — it
asks for a Node toolchain to deliver something that does not use one.

**Already true — a plan should not redo any of this.**

- [Contract §9](../spec/statusline-behaviour.md) **parked a tap explicitly**, as
  "can still come later", and chose npm for day one. The one argument recorded
  against it — the options table's "Linux users still need another" — stopped
  being an argument when `macos-only` and `release-fix` cut the set to Apple
  Silicon alone, and §9 already carries a note saying that row is no longer
  live. So this is resuming a deferred decision, not reopening a settled one.
- **The release already publishes what a formula consumes.** The `release.yml`
  job collects one asset per `supported_targets()` row, writes `SHA256SUMS`
  beside them, and creates the GitHub Release with `--clobber` for idempotence.
  Its own comment at `.github/workflows/release.yml:214` says this exists "for
  people who want the binary directly, and for a Homebrew tap later" — the `url`
  and `sha256` a formula needs are already produced on every tag.
- **One target means one formula and no bottle matrix.** `supported_targets()`
  has a single row, `aarch64-apple-darwin`.

**Open questions a plan must answer.** Whether the tap is its own repo
(`virajp/homebrew-tap`) or a homebrew-core submission — core wants a notability
bar and a release history this project does not have yet, so the tap is the
likely answer and the plan should say so rather than leave it open. Whether the
formula points at the **raw binary** the release currently uploads or at a
tarball, which is the shape formulae normally expect and which nothing produces
today. Whether CI bumps the formula on tag or it is hand-edited — an unbumped
tap is a silently stale install path. And whether Gatekeeper needs answering
here: §9 deferred code signing and notarisation, and this changes who downloads
the binary but not whether it is signed.

**The thing that does not carry over.** A formula installs a binary into the
brew prefix and **cannot do the wiring** — Homebrew does not permit a formula to
write outside its prefix, and `caveats` can only print text. Everything
`--install` does beyond placing the binary has no home in a formula. That is the
whole substance of the next entry, and it is why these are two entries and not
one.

### Remove the npm installer, once the tap is proven

**Ask.** Retire the npm installer package once the Homebrew formula has been
**fully tested and confirmed by the maintainer**.

**This entry does not graduate on a schedule.** It graduates when the maintainer
says the tap is proven, and not before. A plan that cuts the only working
install path because the replacement looks finished is the failure mode this
sentence exists to prevent.

**Already true — and this is the part that makes it more than a deletion.** The
installer is not a binary-copier. `--install` also writes
`~/.claude/settings.json` (`statusLine`, `subagentStatusLine`, and the
`PostToolUse` caps hook), seeds `~/.config/claude-status.json` — migrating a
`statusline.json` if it finds one — and records a **receipt** at
`~/.config/claude-status/` of what was there beforehand, which is what lets
`--uninstall` restore prior state rather than infer it. That inference argument
is written out at the top of `installer/src/modules/receipt.ts` and is worth
reading before proposing anything simpler.

The binary has none of this: its flags are `--statusline`, `--subagent`,
`--refresh-spend`, `--caps-hook`, `--debug`, `--version`, `--help`. It renders
and it hooks; it has never installed anything.

**Open questions a plan must answer.** Where the wiring goes — into the binary
as a subcommand, or out to the user as printed `caveats` and manual steps. What
replaces the receipt, since `brew uninstall` removes a binary and cannot restore
a `settings.json` key. Whether npm is **deprecated or removed** — a published
name cannot be unpublished after 72 hours, so `npm deprecate` pointing at the
tap is the realistic answer. Whether npm survives as a second channel rather
than being retired at all, which would make this entry moot and is a legitimate
outcome. `--configure` is the least affected: `src/modules/config/autoseed.rs`
already seeds the repo layer from the render path, so the repo config survives
the installer going away.

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
