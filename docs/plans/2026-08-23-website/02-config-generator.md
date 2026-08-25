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

> **Corrected during execution.** "From the schema's `default` values" is
> unimplementable — the schema has four of them, all under `caps`. The values
> live in `assets/claude-status.defaults.json`, which this plan never mentions,
> and the page loads both documents. See Gaps.

The page holds the defaults — from the schema's `default` values — and emits a
key only where the user's value differs. Same rule as the binary's serialiser,
and the same trap: a map whose default is non-empty must be diffed **entry by
entry**, or changing one palette colour emits the whole palette.

`$schema` is always emitted.

### 4. Port the renderer to JavaScript — **NOT DONE, and not what will be done**

> **Deferred, and superseded.** The preview will be the Rust renderer compiled
> to WebAssembly, which was proved to work and to reproduce the goldens byte for
> byte. There is no JavaScript port and there should not be one. See Gaps.

Enough of it to draw the main bar: segment assembly, the powerline separators
and caps, colour resolution through the palette, the gauge, and the same
truncation rules. Output as styled HTML rather than ANSI.

**This is a second implementation and it will drift.** Step 5 is what makes it
acceptable; without step 5 this step should not ship.

### 5. Gate the preview against the Rust goldens — **NOT DONE, and vacuous**

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
4. ~~Given the golden fixtures, when CI runs the JS renderer over them, then the
   output is byte-identical to `tests/golden/*.txt`.~~ **Deferred, and withdrawn
   as written** — see Gaps. There is no JS renderer and there will not be one;
   the preview is WebAssembly, so this criterion has no second implementation to
   measure.
5. ~~Given a Rust change that alters a golden, when CI runs, then the JS gate
   **fails** — the gate must be able to catch drift in both directions, not just
   in the JS.~~ **Deferred, and vacuous as written** — same reason. The
   follow-up cycle needs a build-reproducibility check on the committed `.wasm`,
   not a differential one.
6. Given a downloaded config, when it is placed at
   `~/.config/claude-status/config.json` and the bar is rendered, then the bar
   matches what the preview showed. **Deferred** with the preview.
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

### The cycle was cut: steps 4 and 5 are deferred, and so are criteria 4, 5 and 6

**Built:** steps 1, 2, 3, 6, 7 and 8 — the form, the non-defaults emitter, the
download and copy, the repo-config snippet, and the docs.

**Deferred:** step 4 (a JavaScript port of the renderer) and step 5 (a CI job
diffing it against `tests/golden/*.txt`), and with them acceptance criteria 4, 5
and 6, all three of which are about that port. This is a scope decision, not a
shortfall discovered late. The plan itself says step 4 "should not ship" without
step 5, and step 5 is the larger half.

Nothing half-built was left behind. There is no renderer, no partial one, and no
placeholder — the page says plainly that it does not draw a bar and sends the
reader to `--debug`.

### When the preview lands it is WebAssembly, not a JavaScript port — and that

### makes criteria 4 and 5 vacuous rather than hard

**This is a plan edit, not a footnote.** A recon pass proved it by building it,
and the result changes what the deferred cycle is:

- The render path is **already pure**.
  `render_main(&MainFacts, &GitFacts, &Config, Option<&str>) -> String`
  (`src/modules/render/main_bar.rs:18`) touches no IO; the only `std::fs` under
  `src/modules/render/` is inside a `#[cfg(test)]` block in `powerline.rs`.
- It **compiles to `wasm32-unknown-unknown` today**, with a raw ABI — no
  `wasm-bindgen`, no JavaScript toolchain, nothing that would put npm back in a
  tree that spent `distribution/01` taking it out.
- It reproduces `tests/golden/fixture.txt` **byte for byte** (669 bytes == 669
  bytes) from the browser-shaped build.
- **247 KB raw, 86 KB gzipped** — four times under the repository's 1024 KB
  commit limit.
- The blocking diff is small and none of it is in the render path: `ureq`
  appears in exactly one file (`src/modules/spend/http.rs:13-14`), and there are
  eleven Unix-trait errors across three files.

The consequence for the criteria: **criterion 4 ("CI proves the JS renderer
matches the goldens") and criterion 5 ("a Rust change that alters a golden fails
the gate") become vacuous.** They are the mitigation for a second
implementation, and with WebAssembly there is no second implementation to drift
— the bytes in the browser *are* the bytes the binary runs. The gate they
describe would be a test that the Rust renderer equals itself.

What the follow-up cycle needs instead is a much smaller thing: proof that the
committed `.wasm` was built from the committed source. That is a build
reproducibility check, not a differential one, and it should be written as such
rather than inherited from this plan's wording.

The Risks section's "the JS renderer is the one real duplication in the whole
project" is therefore **withdrawn**. It described a design that is not going to
be built.

### Criterion 1 could not be tested forwards, and is gated negatively instead

"Given the committed schema with a key added, the form shows that key with no
hand-edit" cannot be exercised against the committed schema: the drift check
(`tests/schema.rs::the_committed_schema_is_what_the_config_types_generate`) and
the `always_run` pre-commit hook both regenerate the file from the Rust types,
so there is no way to add a key to it and leave it added.

Split in two:

- **Forwards**, in `tests/js/generator.test.mjs`: the form builder is a pure
  function over a schema object, so the test feeds it a **synthetic** schema
  carrying an invented key and an invented block, and asserts both appear with
  the right widget, the right bounds and their descriptions.
- **Negatively**, in
  `tests/site.rs::no_config_key_is_hard_coded_into_the_pages
  _that_build_the_form`:
  no config key name may appear as a **string literal** in any tracked file
  under `site/templates/` or `site/static/`. This is the half that actually
  gates a regression — the day somebody writes `if (key === "palette")` to make
  one field look nicer, the form stops being a function of the schema and every
  other test stays green.

The negative scan reads string literals with comments stripped, not bare
identifiers: `width`, `name`, `id`, `match`, `head` and `bold` are all config
keys *and* ordinary words in CSS and JavaScript, so a word scan would need an
allowlist longer than the schema and would still fail on `input[type="number"]`.
The limit is recorded above the test, and the test carries a control proving the
scanner can fail.

### Step 3 was unimplementable as written — the defaults are a second document

The plan says *"the page holds the defaults — from the schema's `default`
values"*. The committed schema has **four** `default` values, all under `caps`,
against a default tree of about a hundred leaves. `config-and-cli/04` stripped
the rest deliberately (`config::schema::strip`) so the palette, the twenty
symbols and the two layout rows — which carry Nerd Font private-use codepoints —
stay out of a file dprint formats, and
`tests/schema.rs::the_only_defaults_in_the_schema_are_the_four_caps` pins it.

Built on the schema's defaults, the page would have shown an empty config and
emitted every key the user touched as a change against nothing.

So the page loads **two** documents: the schema for the shape, and
`assets/claude-status.defaults.json` — which this plan never mentions — for the
values.

### Where the two documents are served from, and the trade taken

**Staged into `site/static/` at build time by a new `site:assets` task**,
gitignored, dprint-excluded, and depended on by both `site:build` and
`site:serve`.

Not committed under `site/static/`: dprint's `includes` is `**/*.json` and its
exclusion of the defaults asset is written at that path *only*, so a tracked
copy would be reformatted on commit and would then differ, byte for byte, from
the file it is a copy of — the same formatter-versus-generator loop
`the_generated_schema_is_already_dprint_formatted` exists for.

Not fetched at runtime from the `$id` URL, although that works —
`raw.githubusercontent.com` serves the current file and sends
`access-control-allow-origin: *`. Two reasons: it would make a static
documentation page stop working offline, behind a proxy that blocks that host,
or during a GitHub outage; and it would make the **form** newer than the **prose
around it**, since the site deploys on a `site-v*` tag and everything else on
the page is pinned at that tag.

**The trade, stated:** a schema change does not reach the deployed page until
the next `site-v*` tag. That is the same trade every other sentence on this site
already takes, which is the argument for it — being uniformly one tag behind is
a smaller lie than being internally inconsistent.

`.github/workflows/site.yml`'s `pull_request.paths` was widened to `schemas/**`,
`assets/claude-status.defaults.json` and `.config/mise/tasks/site/**`. Without
that, a pull request renaming a config key would change what the generator page
renders while touching nothing under `site/`, and the build that would have
caught it would never have run.

### The plan's step 2 undercounted the widget shapes

Five defects, all found before implementation and all handled:

- **Five open maps, not four.** The fifth is `subagent.statuses`, whose values
  are *closed objects* (`match` / `symbol` / `bg`) rather than scalars — a
  different widget from the other four. `write.rs:286-288` says in its own
  comment that the previous cycle's inventory of these was short; this plan made
  the same undercount again.
- **`$defs/color` is a three-branch `oneOf`, not a triple.** The plan's "a
  picker that emits the `[r, g, b]` triple the schema requires" would have been
  actively harmful: the shipped defaults reference colours **by palette name**,
  so a triple-only picker converts every colour it touches into a literal that
  stops following the palette forward. And `null` is load-bearing —
  `write.rs::clearing_a_default_emits_an_explicit_null` pins it as the only way
  to clear a shipped colour. Built as a mode switcher over the schema's own
  branches, with the live palette names offered as completions on the string
  branch.
- **`lines` had no rule at all.** It is an array of arrays of `segmentEntry`,
  itself a `oneOf` of a bare id or a styled object, in which `name` and `id` are
  aliases. Covered by the generic array and `oneOf` rules.
- **`"type": ["boolean", "null"]`** (`bold`) is tri-state, not a checkbox.
- **`$schema` is a declared property with no Rust field** and had to be excluded
  from the form. It is excluded by the rule "properties whose name starts with
  `$`", so the page never names it — and the pointer it emits is derived from
  the document too: the key is that one `$`-prefixed property and the value is
  the schema's `$id`, which `config::schema` injects from `write::SCHEMA_URL`.
  Change that constant in Rust and the page follows with no edit.

### Two JavaScript-only traps in the emitter, and one relief

- **`JSON.stringify` comparison would have been wrong.** It is key-order
  sensitive; `serde_json::Value` compares its `IndexMap` by content. Reordering
  `subagent.statuses` would have emitted the entire map where the binary emits
  nothing (`write.rs:366`). The emitter walks instead.
- **Prototype pollution.** All five open maps take free-text keys.
  `src/_shared/json.rs:15` drops `__proto__`, `constructor` and `prototype` at
  every depth in the binary, where they are inert; in a browser they are not.
  The page drops them on input, in the diff, and at every depth of a wholesale
  emission.
- **The relief:** `json!(15) != json!(15.0)` — cycle 02's `f64` trap — does not
  exist in a browser, which has one number type.

**A third trap was found during implementation and is not in any brief.** The
binary compares two *serialized* `Config`s, in which every unset `Option` is an
explicit `null`; the shipped defaults JSON simply omits those keys. Without
handling it, clearing a colour that was never set would emit `"fg": null` where
the binary emits nothing. The emitter's equality therefore reads a **missing key
as `null`**, which reproduces the binary's trees rather than approximating them,
and both directions are pinned by the harness.

### "Remove" cannot mean remove

`deep_merge` has no delete operator (`src/_shared/json.rs:149`), so
`{"palette": {}}` merges as a no-op. Removing a row the defaults ship means
**revert to shipped**, and the button says exactly that; a row the user added is
genuinely removable, because removing it just stops it being emitted. The page's
prose carries the same rule, since it is the sort of thing a user discovers by
being surprised.

The same applies to `lines`: it is replaced wholesale, never diffed
(`write.rs:178-180`), so touching one segment emits both default rows. That is
correct behaviour that looks like the non-defaults promise breaking, so the
output pane says so whenever it happens — generically, for any array, rather
than by naming the key.

### Criterion 7 was fixed at the source: ten descriptions added to the Rust types

Eleven of forty-seven named schema properties had no `description`, and they
were exactly the ones the form's widgets render from: the four unlabelled
`subagent.segments` rows, and `bg` / `fg` / `bold` in both `$defs/style` and
`$defs/segmentEntry`'s object branch.

Fixed where the schema comes from — `#[schemars(description = …)]` on the config
types — rather than in the published file, then regenerated with
`mise run code:schema`. `PARENT_DESCRIPTION_COUNT` moved 39 → 49 and
`DESCRIPTION_DIGEST` moved with it; both constants moving together is the guard
working as designed, and every one of the 39 existing strings is byte-identical
(verified: the regenerated file's diff contains no removed `description` line).

`/properties/$schema` is the eleventh and stays bare. It is a pointer rather
than a setting, it is excluded from the form, and
`every_top_level_property_is_described` already names it as the one allowed
exception.

### The `<script>` guard was strengthened rather than deleted

`tests/site.rs` asserted no `<script>` in any tracked `site/**/*.html` or
`*.css`. This cycle necessarily adds JavaScript, and deleting the guard was the
easy move and the wrong one. It was replaced with three narrower ones, each
stronger than what it replaced:

1. **The nav assertion stays**, scoped to the `<nav>…</nav>` slice. Its stated
   reason — a hamburger behind a script — is untouched by anything here.
2. **Exactly one allowlisted path** may carry a script:
   `site/templates/generate.html`. A second script anywhere fails, and the
   allowlisted file must actually still have one, so a stale entry cannot leave
   a spare permission lying around.
3. **The scan now reads `site/content/*.md`**, which it did not. Zola passes raw
   HTML in markdown through verbatim, so a `<script>` written into a content
   page went straight past the old guard. That hole is closed.

Both new guards were verified by mutation: a `<script>` added to a content page
fails, and a config key written into the module fails.

`site/config.toml:19` read "NO THEME, NO CSS FRAMEWORK, NO JAVASCRIPT" and
became a lie in this commit. It now records what that line was always protecting
— the absence of a **build**, not of a `<script>` tag — and names the three
tests that hold it.

### Criterion 8: the docs are the page, and the real check is human

There is no headless browser here and adding one is the JavaScript toolchain
`website/01-site`'s criterion 1 forbids — the same trade already recorded above
`the_layout_carries_the_static_marks_of_a_readable_phone_page`.

So the page is built the way that makes the criterion true by construction. The
markdown carries the whole reference as ordinary static content — the target
path, the three emission rules, the four colour forms, the open-map table, the
forbidden key names, the repo-config snippet — and the script replaces one
element and touches nothing else. **A `<noscript>` block was deliberately not
used**: it is a second copy of the documentation that nothing checks, and the
copy that rots is always the one nobody reads.

`the_generator_page_reads_as_documentation_without_its_script` asserts the
construction, in the source and in the built HTML ahead of the `<script>` tag.
**The real check is a human one at the gate: open the page with JavaScript
disabled and read it.**

### Testing JavaScript without a JavaScript toolchain

`tests/js/generator.test.mjs` runs the generator's pure core — the emitter and
the form builder — against the **real** committed schema and the **real**
shipped defaults. 60 checks: criteria 2 and 3, all five open maps, the prototype
keys, the key-order and `null`-versus-absent rules, the colour branches, the
tri-state `bold`, the layout's nested shape, and criterion 1's forward direction
against a synthetic schema.

**Nothing is installed to run it.** No `package.json`, no lockfile, no
`node_modules` — `no_javascript_lockfile_or_node_modules_is_tracked` still
passes and `code:sec`'s grype scan still sees no npm ecosystem. `node` is
invoked as a bare binary, the way the suite already invokes `git`, `mise` and
`dprint`, and the Rust side skips **loudly** when it is absent, following
`the_generated_schema_is_already_dprint_formatted`.

That skip is the honest weakness: on a machine with no `node`, those 60 checks
do not run. It is mitigated by a `--self-check` mode that asserts something
false on purpose, which `tests/site.rs` runs and requires to fail — because a
harness invoked wrongly exits non-zero and is caught, while a harness whose
assertions never run exits **zero** and looks exactly like a clean pass.

The module is copied and renamed to `.mjs` for the run. Node decides a file's
module system from its extension and the browser file has to stay `.js`, or a
static host serves it as `application/octet-stream` and the browser rejects it.

The DOM half — roughly five hundred lines of widget code — is not covered by the
suite. It was exercised during execution under a scratch fake DOM (4583 elements
built across every widget kind, then a mode switch, an add, a revert, a reorder
and a download), and that probe was verified to fail on an introduced typo. It
is not committed: a hand-rolled DOM stub is a second implementation of the
browser, which is the kind of thing this plan is otherwise about not building.
**The DOM path's real check is a human one at the gate.**

### The version-skew mitigation had no source

The Risks section says "the page should name the version it is generating for".
There is no version to name: `Cargo.toml`'s is served nowhere, and the
repository has **zero tags and zero releases**.

Adding `[extra] version` to `site/config.toml` with a test cross-checking
`Cargo.toml` was considered and rejected — it would be a second copy of a number
that names nothing a user can install, earning its keep only once releases
exist.

Instead the page names its **provenance**, derived rather than copied: it
displays the loaded schema's own `$id`, which is a `.../main/schemas/...` URL,
and says in as many words that there are no releases yet so it is the schema on
`main` at build time rather than one pinned to an installed version. Revisit
once a `v*` tag exists.

### Smaller corrections

- **Step 6's repo-config example is three lines, not two.** It carries `$schema`
  (`layers.rs`'s `SCHEMA_KEY`), matching the worked example in
  `site/content/configure.md`. Cited by symbol rather than by line: the two line
  numbers this entry originally carried were both wrong by the time anyone read
  them, one of them because this very cycle moved the line.
- **`projectName` appears in the form** because it is in the schema, and setting
  it there writes it into the *user* config where it applies to **every repo
  that has not named itself** — the user layer merges whole, and only the repo
  layer is narrowed to this key (`layers.rs:220` calls `narrow`, the user layer
  at `:198` does not). The trade below still stands, but it was first accepted
  on the false premise that a user-set value was inert. Excluding it would have
  meant naming a key in the page — the coupling criterion 1 forbids. The
  schema's own description already says "**Repo-level only**", the form shows
  that description, and the page says it again in prose. This is the right trade
  but it is a real rough edge.
- **An unrecognised schema shape becomes a raw JSON box**, not nothing. This is
  what makes criterion 1 true for *shapes* rather than only for names: a
  construct nobody anticipated stays editable — badly, but honestly — instead of
  vanishing from a form that claims to be complete. The harness asserts that no
  property of the committed schema falls back to it today, so the valve cannot
  quietly become a resting place.
- **Page weights renumbered.** `generate` is 3, and `repo-config`, `segments`
  and `diagnosing` moved to 4, 5 and 6 so weight keeps mirroring nav order.
  Nothing on this site reads a weight today; the ordering is for whoever adds a
  section listing later.

### Test count

551 → **557** under `mise x -- cargo test --features schema` (549 → 555 bare),
556 from the generator work itself, plus one added in the Phase 4 fix round
pinning that a user-layer `projectName` reaches every unnamed repo, plus 60
JavaScript checks inside
`the_generators_pure_core_holds_against_the_real_schema`.
[38;2;131;148;150m─────┬──────────────────────────────────────────────────────────────────────────[0m
[38;2;131;148;150m│ [0m[1mSTDIN[0m
[38;2;131;148;150m─────┼──────────────────────────────────────────────────────────────────────────[0m
[38;2;131;148;150m 1[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 2[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m### Gaps from the Phase 3 review
(recorded in Phase 4)[0m [38;2;131;148;150m 3[0m [38;2;131;148;150m│[0m
[38;2;131;148;150m 4[0m [38;2;131;148;150m│[0m [38;2;248;248;242mThree
reviewers ran across four reports; two runs stalled mid-work and were[0m
[38;2;131;148;150m 5[0m [38;2;131;148;150m│[0m [38;2;248;248;242mrecovered
from context. One blocker and eleven should-fix/nit findings came back.[0m
[38;2;131;148;150m 6[0m [38;2;131;148;150m│[0m [38;2;248;248;242mNine
should-fixes plus the blocker and two of the late findings were fixed in[0m
[38;2;131;148;150m 7[0m [38;2;131;148;150m│[0m [38;2;248;248;242mthis
cycle. What follows is what was **not** fixed, and why.[0m [38;2;131;148;150m
8[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 9[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m#### Why criteria 4 and 5 are
vacuous rather than deferred[0m [38;2;131;148;150m 10[0m
[38;2;131;148;150m│[0m [38;2;131;148;150m 11[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mThe preview was scoped out of this cycle and will be
**WebAssembly, not a[0m [38;2;131;148;150m 12[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mJavaScript port**. That is not merely a postponement — it
changes what criteria[0m [38;2;131;148;150m 13[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m4 and 5 can ever mean.[0m [38;2;131;148;150m 14[0m
[38;2;131;148;150m│[0m [38;2;131;148;150m 15[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mBoth criteria are drift guards: they assume a *second
implementation* of the[0m [38;2;131;148;150m 16[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mrender path, in another language, that could disagree with
the Rust one. A recon[0m [38;2;131;148;150m 17[0m [38;2;131;148;150m│[0m
[38;2;248;248;242magent built the WASM path rather than estimating it —
`wasm32-unknown-unknown`,[0m [38;2;131;148;150m 18[0m
[38;2;131;148;150m│[0m [38;2;248;248;242mraw ABI, no `wasm-bindgen` and no JS
toolchain — and reproduced[0m [38;2;131;148;150m 19[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m`tests/golden/fixture.txt` byte for
byte (669 == 669) at 247 KB raw / 86 KB[0m [38;2;131;148;150m 20[0m
[38;2;131;148;150m│[0m [38;2;248;248;242mgzipped. The render path is already
pure[0m [38;2;131;148;150m 21[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m(`render_main(&MainFacts, &GitFacts, &Config, Option<&str>) -> String`,[0m
[38;2;131;148;150m 22[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m`src/modules/render/main_bar.rs:18`), and the only `std::fs`
under[0m [38;2;131;148;150m 23[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m`src/modules/render/` is inside `#[cfg(test)]`.[0m
[38;2;131;148;150m 24[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 25[0m
[38;2;131;148;150m│[0m [38;2;248;248;242mSo the preview will run **the same
compiled Rust** the binary runs. There is no[0m [38;2;131;148;150m 26[0m
[38;2;131;148;150m│[0m [38;2;248;248;242msecond implementation, and therefore
nothing to drift: criteria 4 and 5 have no[0m [38;2;131;148;150m 27[0m
[38;2;131;148;150m│[0m [38;2;248;248;242mreferent, rather than an unmet one.
The follow-up cycle needs a[0m [38;2;131;148;150m 28[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m**build-reproducibility check on the
`.wasm`** in their place — that the shipped[0m [38;2;131;148;150m 29[0m
[38;2;131;148;150m│[0m [38;2;248;248;242martifact is the one the current
source produces. Rewriting those two criteria is[0m [38;2;131;148;150m 30[0m
[38;2;131;148;150m│[0m [38;2;248;248;242ma plan edit for that cycle, not a
footnote here.[0m [38;2;131;148;150m 31[0m [38;2;131;148;150m│[0m
[38;2;131;148;150m 32[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m`rustup target add wasm32-unknown-unknown` was left installed
on the 1.98.0[0m [38;2;131;148;150m 33[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mtoolchain (additive; `rustup target remove` reverses it).[0m
[38;2;131;148;150m 34[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 35[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m#### Not fixed — carried forward[0m
[38;2;131;148;150m 36[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 37[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m- **The built-page assertion is
developer-only, knowingly.**[0m [38;2;131;148;150m 38[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m `tests/site.rs`'s built-page check
needs `site/public/`, which only a full[0m [38;2;131;148;150m 39[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m `zola` build produces. `code:test`
now stages the two JSON assets (through a[0m [38;2;131;148;150m 40[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m `site:assets` dependency) but
deliberately does not pull `zola` into the Rust[0m [38;2;131;148;150m 41[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m test path, and `site.yml` — the one
workflow that builds the site — never runs[0m [38;2;131;148;150m 42[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m this suite. Making it CI-strict
would couple the two. The comment now says so[0m [38;2;131;148;150m 43[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m instead of implying a gate. **The
byte comparison beside it *was* fixed** and[0m [38;2;131;148;150m 44[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m now fails under CI.[0m
[38;2;131;148;150m 45[0m [38;2;131;148;150m│[0m [38;2;248;248;242m-
**`node` is still an ambient dependency.** It is declared in none of the
three[0m [38;2;131;148;150m 46[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m mise configs and works because the runner images preinstall
it. The guard now[0m [38;2;131;148;150m 47[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m **fails under CI** when `node` is absent instead of skipping
silently, so the[0m [38;2;131;148;150m 48[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m suite can no longer report green with the 458-line JS
harness unrun — but[0m [38;2;131;148;150m 49[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m pinning `node` in `mise.toml` remains the real fix and was
left out as[0m [38;2;131;148;150m 50[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m out-of-scope tooling work.[0m [38;2;131;148;150m 51[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m- **The wholesale-array warning only
scans top-level keys**[0m [38;2;131;148;150m 52[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m (`config-generator.js:687`), while its comment claims it
"fires for any array[0m [38;2;131;148;150m 53[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m the user touched". No live impact: the only non-leaf array
is `lines`[0m [38;2;131;148;150m 54[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m (top-level, caught); every other array is a palette RGB
triple, which is a[0m [38;2;131;148;150m 55[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m leaf. Left as a latent trap for whoever adds a nested
list.[0m [38;2;131;148;150m 56[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m- **The `file:` claim in `config-generator.js`'s comment is
false.** Browsers[0m [38;2;131;148;150m 57[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m block `fetch()` on `file://`, so opening
`site/public/generate/index.html`[0m [38;2;131;148;150m 58[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m directly takes the error branch.
Comment defect only, and the error branch is[0m [38;2;131;148;150m 59[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m correct and loud. Argued from the
fetch spec, never measured — no browser was[0m [38;2;131;148;150m 60[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m drivable.[0m [38;2;131;148;150m
61[0m [38;2;131;148;150m│[0m [38;2;248;248;242m- **A theoretical Rust/JS
divergence, unresolved.** Rust's `diff`[0m [38;2;131;148;150m 62[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m (`write.rs:195-205`) emits a key
wholesale when `shipped.get(key)` is `None`;[0m [38;2;131;148;150m 63[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m the JS calls
`deepEqual(current[key], undefined)` first and reads it as[0m
[38;2;131;148;150m 64[0m [38;2;131;148;150m│[0m [38;2;248;248;242m
UNCHANGED. A user-added open-map entry whose value is `null` would be
emitted[0m [38;2;131;148;150m 65[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m by Rust and dropped by the JS. No reachable case was
constructed, and[0m [38;2;131;148;150m 66[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m `blankValue` never produces `null` for a colour entry.[0m
[38;2;131;148;150m 67[0m [38;2;131;148;150m│[0m [38;2;248;248;242m-
**`code:sec`'s grype scoping was fixed here but is not this cycle's bug.**
It[0m [38;2;131;148;150m 68[0m [38;2;131;148;150m│[0m [38;2;248;248;242m
had neither `#MISE dir` nor a `cd`, so `grype .` resolved against the
caller's[0m [38;2;131;148;150m 69[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m cwd and picked up sibling worktrees — reporting an
`esbuild`/npm finding that[0m [38;2;131;148;150m 70[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m reads exactly like this repo
reacquiring a JavaScript toolchain. It had not.[0m [38;2;131;148;150m 71[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m Both scanners now take an absolute
path. The two real findings[0m [38;2;131;148;150m 72[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m (`actions/download-artifact v4`,
`release.yml:175` and `site.yml:160`) predate[0m [38;2;131;148;150m 73[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m this cycle and are still open.[0m
[38;2;131;148;150m 74[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 75[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m#### Never verified by anyone — the
human gate covers these[0m [38;2;131;148;150m 76[0m [38;2;131;148;150m│[0m
[38;2;131;148;150m 77[0m [38;2;131;148;150m│[0m [38;2;248;248;242m- **No
real browser was ever driven, by any of the three reviewers.** Only[0m
[38;2;131;148;150m 78[0m [38;2;131;148;150m│[0m [38;2;248;248;242m Safari
is installed; Computer Use is denied on Accessibility. Every DOM,[0m
[38;2;131;148;150m 79[0m [38;2;131;148;150m│[0m [38;2;248;248;242m layout,
wrapping and accessibility statement in this cycle is a static reading[0m
[38;2;131;148;150m 80[0m [38;2;131;148;150m│[0m [38;2;248;248;242m of
markup and CSS, or a hand-written stub — never an observation.[0m
[38;2;131;148;150m 81[0m [38;2;131;148;150m│[0m [38;2;248;248;242m
**Unverified:** layout, focus order, keyboard access, screen-reader output,[0m
[38;2;131;148;150m 82[0m [38;2;131;148;150m│[0m [38;2;248;248;242m and the
rendering of `<datalist>` and `<input type="color">`.[0m [38;2;131;148;150m
83[0m [38;2;131;148;150m│[0m [38;2;248;248;242m- **Criterion 8's
JavaScript-off reading** was checked only as a static reading[0m
[38;2;131;148;150m 84[0m [38;2;131;148;150m│[0m [38;2;248;248;242m of the
built HTML. The three "the form above" sentences it turned up were[0m
[38;2;131;148;150m 85[0m [38;2;131;148;150m│[0m [38;2;248;248;242m fixed;
the lived experience was not observed.[0m [38;2;131;148;150m 86[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m- **The RGB widget's accessible
names** were fixed (three of four controls had[0m [38;2;131;148;150m 87[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m none) but the fix is unconfirmed
against a real accessibility tree.[0m [38;2;131;148;150m 88[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m- **`config-generator.js` was never
reviewed end to end.** Reviewer A covered[0m [38;2;131;148;150m 89[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m the emitter and the widget region;
nobody read the download path or the[0m [38;2;131;148;150m 90[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m sanitising in full.[0m
[38;2;131;148;150m─────┴──────────────────────────────────────────────────────────────────────────[0m
[38;2;131;148;150m─────┬──────────────────────────────────────────────────────────────────────────[0m
[38;2;131;148;150m│ [0m[1mSTDIN[0m
[38;2;131;148;150m─────┼──────────────────────────────────────────────────────────────────────────[0m
[38;2;131;148;150m 1[0m [38;2;131;148;150m│[0m [38;2;131;148;150m 2[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m#### Late findings, recovered after
the fix rounds — not fixed[0m [38;2;131;148;150m 3[0m
[38;2;131;148;150m│[0m [38;2;131;148;150m 4[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mTwo stalled reviewers were recovered from context after their
runs dropped.[0m [38;2;131;148;150m 5[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mMost of what they held was already fixed; these two were
not.[0m [38;2;131;148;150m 6[0m [38;2;131;148;150m│[0m [38;2;131;148;150m
7[0m [38;2;131;148;150m│[0m [38;2;248;248;242m- **Nothing prunes
`site/static/`.** `site:assets` only ever copies in — it[0m [38;2;131;148;150m
8[0m [38;2;131;148;150m│[0m [38;2;248;248;242m never removes a file it no
longer stages. A local `site/static/stale.json`[0m [38;2;131;148;150m 9[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m left by an earlier build survives,
is copied into `site/public/` by zola, and[0m [38;2;131;148;150m 10[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m shows as `??` (neither gitignored
nor dprint-excluded, since both name the[0m [38;2;131;148;150m 11[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m staged paths exactly). **The deploy
half is contained**: CI always builds from[0m [38;2;131;148;150m 12[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m a fresh `actions/checkout`, so
nothing stale can reach production — which is[0m [38;2;131;148;150m 13[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m why this is a nit rather than the
severity of the staging drift fixed above.[0m [38;2;131;148;150m 14[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m A `rm` of the staged set before
copying would close it.[0m [38;2;131;148;150m 15[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m- **`.config/grype.yaml` does not exist**, and `code:sec`
guards on its presence[0m [38;2;131;148;150m 16[0m [38;2;131;148;150m│[0m
[38;2;248;248;242m — so grype runs with no config and **no severity
threshold**. Whether that is[0m [38;2;131;148;150m 17[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m deliberate or a gap left by an
earlier cycle could not be determined, and it[0m [38;2;131;148;150m 18[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m is outside this cycle's blast
radius either way. Worth settling deliberately,[0m [38;2;131;148;150m 19[0m
[38;2;131;148;150m│[0m [38;2;248;248;242m because `code:sec` currently cannot
fail on severity alone.[0m [38;2;131;148;150m 20[0m [38;2;131;148;150m│[0m
[38;2;131;148;150m 21[0m [38;2;131;148;150m│[0m [38;2;248;248;242mBoth were
raised by the blast-radius reviewer, whose verification table also[0m
[38;2;131;148;150m 22[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mre-confirmed every prose fix in this cycle by re-running the
thing each comment[0m [38;2;131;148;150m 23[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mcites — the citation in `site/templates/generate.html` is now
correct in the[0m [38;2;131;148;150m 24[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mdemonstrable sense: breaking its `<script>` turns the test it
names red, and[0m [38;2;131;148;150m 25[0m [38;2;131;148;150m│[0m
[38;2;248;248;242mleaves the test it used to name green.[0m
[38;2;131;148;150m─────┴──────────────────────────────────────────────────────────────────────────[0m
