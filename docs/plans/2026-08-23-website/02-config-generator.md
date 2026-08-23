---
type: vwf-plan
title: config-generator — 2026-08-23
description: Cycle plan (a diff) adding a schema-driven config form to the site
  with a live powerline preview, gated in CI against the same golden fixtures
  the Rust renderer is tested with.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-website/01-site.md,
  docs/plans/2026-08-23-config-and-cli/04-schema-and-validation.md,
]
timestamp: 2026-08-23T14:22:00Z
tags: [ website, config-generator, schema, preview, golden, ci ]
---

# Plan: config-generator — 2026-08-23

## Slice

A page on the site that builds a `claude-status` config: a form generated from
the committed JSON schema, a live preview of the resulting bar, and a download
that emits **only non-defaults**.

Plus the thing that keeps the preview honest: a CI job that renders the golden
fixtures through the JavaScript renderer and diffs against the Rust goldens.

## Current state (actual)

**After
[config-and-cli/04](../2026-08-23-config-and-cli/04-schema-and-validation.md)**,
`schemas/claude-status.schema.json` is generated from the Rust types, checked
for drift on every commit, and proven well-formed draft-2020-12. It is fetchable
from `main` at a stable URL, which is what `$schema` points at.

**Configs are non-defaults-only** after
[config-and-cli/02](../2026-08-23-config-and-cli/02-config-relocation.md). A
generator that emitted a full config would undo the whole point of that cycle —
it would hand every user a frozen copy of the defaults, which is exactly the
problem being solved.

**Four schema keys are open maps** — `palette`, `symbols`, `typeSymbols`,
`segments` — with user-chosen keys. The rest are closed objects. A form
generator has to handle both, and they need different widgets.

**The repo layer takes `projectName` and nothing else**, so its "generator" is a
two-line file — worth a copy button, not a form.

**The Rust renderer's output is pinned by goldens.** `tests/golden.rs` compares
exact ANSI strings against `tests/golden/*.txt`, regenerated only under
`UPDATE_GOLDEN=1`. Its own header says the goldens are generated and never
hand-written, because they contain Nerd Font private-use codepoints that do not
survive being retyped. **This is the fixture set the JS preview is measured
against**, and it already exists.

**§12 carries reference payloads** — the main-bar JSON, the subagent NDJSON — as
shell examples.

## Target state (per contract)

A user picks options, sees their bar update, and downloads a config containing
only what they changed. The preview is trustworthy because CI proves it renders
the reference fixtures identically to the binary.

## Delta — ordered steps

### 1. Serve the schema with the site

Copy `schemas/claude-status.schema.json` into the Zola build at a stable path.
The page loads it at runtime and builds the form from it.

**Built from the schema, not hand-written**, so a new config key appears in the
form without anyone remembering to add it. That is the entire reason
`config-and-cli/04` made the schema generated.

### 2. Render the form

Closed objects become fieldsets; primitives become inputs typed from the schema
(`enum` → select, `boolean` → checkbox, `integer` with `minimum`/`maximum` →
number). Open maps get add/remove row widgets. Colours get a picker that emits
the `[r, g, b]` triple the schema requires.

Every field shows its schema `description`. This is what `config-and-cli/04`'s
criterion 3 was protecting: a form built from a schema with no descriptions is a
form of unlabelled boxes.

### 3. Emit only non-defaults

The page holds the defaults — from the schema's `default` values — and emits a
key only where the user's value differs. Same rule as the binary's serialiser,
and the same trap: a map whose default is non-empty must be diffed **entry by
entry**, or changing one palette colour emits the whole palette.

`$schema` is always emitted.

### 4. Port the renderer to JavaScript

Enough of it to draw the main bar: segment assembly, the powerline separators
and caps, colour resolution through the palette, the gauge, and the same
truncation rules. Output as styled HTML rather than ANSI.

**This is a second implementation and it will drift.** Step 5 is what makes it
acceptable; without step 5 this step should not ship.

### 5. Gate the preview against the Rust goldens

A CI job that feeds the golden fixtures' inputs through the JS renderer and
compares against `tests/golden/*.txt`.

Since the JS renderer emits HTML and the goldens are ANSI, one side needs a
converter — write it as a **test-only ANSI emitter in the JS renderer**, so the
comparison is byte-for-byte against the real goldens rather than against a
second set of expectations that could themselves be wrong.

**The goldens are never regenerated to make this pass.** If the JS output
differs, the JS is wrong. If the Rust output genuinely changed, that is a Rust
cycle's business and its own golden update.

### 6. Add the repo-config snippet

A small section: the path, the one supported key, a two-line example, and a copy
button. Not a form.

### 7. Make it usable without a download

A copy-to-clipboard alongside the download, and the target path shown beside it
(`~/.config/claude-status/config.json`). Most people will paste into an editor.

### 8. Docs

The site's config reference links to the generator. `--help` and the formula
caveats already point at the site root
([config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md),
[distribution/02](../2026-08-23-distribution/02-homebrew-formula.md)); no change
needed there.

## Acceptance criteria (from contract)

1. Given the committed schema with a key added, when the site is rebuilt, then
   the form shows that key **without any hand-edit to the page**.
2. Given a form where the user changed exactly one value, when the config is
   downloaded, then it contains `$schema` and that one key.
3. Given a change to one `palette` entry, then the output contains that entry
   alone, not the whole palette.
4. Given the golden fixtures, when CI runs the JS renderer over them, then the
   output is byte-identical to `tests/golden/*.txt`.
5. Given a Rust change that alters a golden, when CI runs, then the JS gate
   **fails** — the gate must be able to catch drift in both directions, not just
   in the JS.
6. Given a downloaded config, when it is placed at
   `~/.config/claude-status/config.json` and the bar is rendered, then the bar
   matches what the preview showed.
7. Given every field in the form, then each shows the schema's description for
   that key.
8. Given the page with JavaScript disabled, then it degrades to readable
   documentation rather than a blank area.

## Risks / drift

**The JS renderer is the one real duplication in the whole project.** Everything
else has a single source of truth. This does not, and it is being accepted for
demo value. Criterion 4 is the mitigation and criterion 5 is what makes it a
real gate rather than a snapshot — a gate that only fires when the JS changes
would let a Rust change silently invalidate the preview.

**A partial port is worse than an obvious one.** If the JS handles the common
path and quietly diverges on truncation or an unusual palette, the preview is
wrong exactly where a user is doing something unusual — which is when they are
most likely to be using the generator. Where the port is incomplete, the page
should say so rather than render something plausible.

**Emitting only non-defaults is easy to get subtly wrong here too.** The page's
notion of a default comes from the schema's `default` values; the binary's comes
from `Config::default()`. `config-and-cli/01`'s criterion 2 ties the Rust
default to the embedded JSON, and `04` ties the schema to the Rust types — so
the chain holds, but it is a three-link chain and criterion 6 is the only thing
that tests it end to end. Do not skip it.

**Schema-driven forms are ugly by default.** A generated form is a stack of
unstyled inputs, and the site is also a marketing surface. Expect real CSS work
beyond the generation, and resist the temptation to hand-write the form for
looks — that would break criterion 1, which is the reason to do this at all.

**The site fetches a schema from `main`, not from a tag.** A user on `0.1.0`
gets a form built from whatever `main` says today, which can offer a key their
binary ignores. This is the same trade `config-and-cli/04` took deliberately for
the `$schema` URL, and it is consistent — but the generator makes it more
visible, so the page should name the version it is generating for.

## Out of scope for this cycle

- **Previewing the subagent panel.** The main bar is the demo. The panel is
  NDJSON consumed by Claude Code and has no equivalent moment.
- **Validating a pasted config in the browser.** `--debug` does that against the
  real binary, which is the honest answer.
- **Persisting configs, accounts, sharing links.** A static site with no backend
  stays a static site with no backend.
- **Theme galleries or community presets.** Worth wanting; not this cycle.
- **Regenerating the landing-page screenshot from the JS renderer.** Tempting
  once the renderer exists, and it would fix `01-site`'s stale-screenshot risk —
  but it makes the marketing image depend on the duplicated implementation.
  Revisit once criterion 4 has held through a few releases.

## Gaps surfaced during execution

*(filled in during execution)*
