# Decisions

The reasoning behind `claude-status`, kept because it is the half that no test
can hold.

Behaviour is pinned by tests, by goldens and by the code itself, and all three
are checkable. **Why** a thing is the way it is is not recoverable from any of
them — and the failure this file exists to prevent is a decision surviving only
as its conclusion, so that the next person to look at it sees an arbitrary
choice and re-takes it from scratch.

This record was harvested from the behaviour contract —
`docs/spec/statusline-behaviour.md`, deleted 2026-08-27 and kept in git history
— before that document was retired. That document had accumulated fourteen
sections and eleven amendment blocks, and had stopped being reliable as a
description of behaviour — the audit that justified retiring it found five
claims that were actively wrong. Its *decisions* were not wrong; they were the
only part with nowhere else to go.

## How to read this

- **Every entry carries its date and its reasoning**, not just its outcome. A
  decision compressed to its conclusion is the exact failure this file exists to
  fix.
- **A reversed decision keeps both halves.** The original argument is preserved
  under **Was**, the reversal under **Reversed** or **Superseded**, with what
  changed between them. The record of *why* a choice changed is the part with no
  substitute — and a reversal is easier to trust when the thing reversed is
  still legible.
- Dates are the date the decision was recorded, and the parenthesised name is
  the cycle that took it.
- Where a decision is about behaviour, the behaviour itself lives in the tests.
  This file does not restate it — that restatement is what drifted.

---

## 1. Foundations

### The whole binary is Rust

**Decided** at the outset, and it is the constraint every phase assumed. Not
TypeScript, not Go, not a Node script with a Rust helper.

The bar renders **on every turn and every few seconds** — `refreshInterval: 4`
in the shipped config — in every open session at once. Process startup is
therefore the dominant cost, and it is paid constantly. The implementation this
replaced was a single ~900-line CommonJS file run as
`node ~/.claude/scripts/statusline`; Node's own startup is roughly **30–50 ms**
before a line of it executes, against **1–2 ms** for a Rust binary. That is the
entire argument, and it is a good one: this is a program whose runtime is almost
entirely startup.

Two secondary wins follow. A single static binary has no runtime to install and
cannot break when the user's Node changes. And the spend subsystem does file
locking, atomic replacement and a network fetch in a detached child — things
Rust expresses more honestly than a script leaning on `try {} catch {}`.

`refreshInterval: 4` is therefore **load-bearing rather than decoration**: it is
what makes "the bar renders every turn and every few seconds" true, which is the
whole argument for this being a Rust binary. It belongs to `statusLine` alone.

### The JavaScript was a specification, not a design to imitate

**Decided** at the outset. The original was to be treated as the *specification*
of behaviour and not ported line by line: where the contract described
behaviour, match it; where it described structure, use judgement.

The corollary was also recorded, and is the reason several subtleties survived
the rewrite: **read the original before reimplementing anything subtle.** The
spend subsystem, the git resolution and the powerline seam logic each encode
decisions the contract summarised but did not fully justify.

### No crate was named in advance

**Decided** at the outset, deliberately: the contract named no JSON, HTTP or
timeout crate, on the grounds that the right choice as of the build date is for
the implementer to check against current documentation rather than for a plan
written earlier to assert.

### No async runtime

**Decided** at the outset. There is exactly **one** network call, it happens in
a detached child, and it may block freely. Pulling in a full async runtime for
it would work against the startup-time argument that motivates the whole rewrite
— the argument in [The whole binary is Rust](#the-whole-binary-is-rust) is spent
if the binary pays a runtime's initialisation to make one request that was
allowed to block anyway.

### Two rendering surfaces, selected by an explicit flag

**Decided 2026-08-19** (`main-bar`). `--statusline` renders the main bar;
`--subagent` renders the subagent panel as NDJSON.

**Was:** shape-detection on a `tasks` array in the payload.

**Reversed** because an explicit surface is diagnosable and a shape heuristic is
not: a payload that stopped carrying `tasks` would silently render the wrong
surface, **with no way to tell from the output**.

**The cost, accepted:** the installer must rewrite both `settings.json` keys on
every upgrade, and anyone who hand-swaps the binary without re-running it gets
the missing-flag line instead of a bar.

### Invoked with no flag, the binary discriminates on whether stdin is a TTY

**Decided 2026-08-19** (`main-bar`), as the other half of the flag decision — no
flag at all is what a stale `settings.json` produces after an upgrade.

A TTY means someone typed it, so it prints full help. A pipe means Claude Code
invoked it, so it prints **exactly one line** on stdout naming the fix.

One line fits the bar and names the fix. Twenty lines of usage would be
unreadable in a status line, and printing nothing would leave the user with a
silently blank bar and no clue.

---

## 2. The five cross-cutting invariants

These are not features. Each was written down centrally rather than beside the
feature it constrains **because more than one feature has to obey it** — and the
ones added later were added precisely because a rule kept in one place had been
applied in one place.

Their behaviour is held by tests. The reasoning is here because it has nowhere
else to live.

### 1 — stdout is the bar

Claude renders whatever arrives there. Diagnostics, warnings, errors — all
stderr, always. **A single stray byte on stdout is a corrupted status line.**

### 2 — a render never blocks

No network call, no unbounded subprocess, no waiting on a lock. Anything slow is
read from cache or skipped. The git subprocesses are the only exception, and the
whole set of them is hard-bounded at 250 ms.

**Amended 2026-08-19** (`main-bar`). The invariant originally said "the two git
subprocesses … both are hard-bounded at 250 ms". There are up to **four** — the
ahead count, `diff --numstat HEAD`, its `--cached` fallback, and the untracked
probe — and the old implementation ran them **sequentially at 250 ms each**, a
~1 s worst case. They now run on two threads under **one shared 250 ms
deadline**, so the budget is what the invariant always claimed it was.

The point of the correction is that per-subprocess timeouts do not compose: a
budget stated per call is not a budget.

### 3 — a render never fails visibly

Any panic or error must still produce a usable line. The implementation this
replaced caught everything and fell back to printing `⚡ Claude`; that was
reproduced deliberately — wrap the render in `std::panic::catch_unwind`, print
the fallback, put the real error on stderr.

This invariant **outranks** nearly everything else in this file. Where a later
decision could have made a failure visible to the user, it was resolved the
other way and the resolution says so.

### 4 — only the renderer emits escapes

**Added 2026-08-21** (`macos-only`), after review found the powerline separators
reaching the row unfiltered. Every dynamic value is stripped of control
characters before it is written, on **both** rendering surfaces and in
`--doctor`. See
[§6, Escapes and untrusted input](#6-escapes-and-untrusted-input).

**Amended 2026-08-28** (`colour`). Colour on the diagnostic surfaces is escapes,
and this invariant is why it was possible without weakening anything: text is
**filtered first and painted second**, so every escape in the result was put
there by `_shared::paint` and nothing a config value, a path or an argv token
carried can reach the terminal. The filter did not change by a character. See
[Colour goes on after the filter](#colour-goes-on-after-the-filter-never-before).

### 5 — an unresolvable `$HOME` means absent, never relative

**Added 2026-08-21** (`macos-only`), after review found four callers of the
home-directory helper had each invented their own answer and **one had invented
the wrong one**.

`$HOME` is the only source of the user's home directory. When it is unset or
empty, every path derived from it is **absent** — the feature that needed it
does nothing, and says so where there is somewhere to say it.

The failure mode this exists to prevent: a path that names the home directory
and cannot resolve one must **never** degrade to the unexpanded text. `~/x` and
`spend.json` taken literally are *relative* paths, so the process writes into
whatever directory Claude Code was launched from **believing it wrote into the
home one**. That is a stray file in the user's working tree, and a cache that
never hits because the next session starts somewhere else.

Concretely the spend cache path, the usage mirror directory and the credentials
*file* are each absent without a home. Invariant 3 still outranks this: the
render succeeds, the segment omits like any other, and `--doctor` names the
missing `$HOME` rather than reporting an empty result. A path that never asked
for the home directory is unaffected — an absolute `$CLAUDE_STATUS_SPEND_CACHE`
works with no `$HOME` at all.

#### The keychain is the exception, and the ordering is deliberate

The macOS keychain is **not** scoped by `$HOME`, so "no home" does not mean "no
credentials" — the fallback can still return a real token. The rule is that
**the cache path is resolved first, and no fetch is made when it is absent**:
with nowhere to write the result, a request would spend the account's rate limit
to produce nothing, on every render.

This was stated explicitly because it is the one case where a `$HOME`-derived
absence has to gate something that is not itself `$HOME`-derived, and it was
previously only implied by the order the code happened to run in.

#### The same asymmetry is why a fake `$HOME` does not make a test safe

Two rules follow, and they are **separate** — stating only the first is how
three harnesses came to invent three different answers to the second:

1. **Pin the endpoint.** Every test that can reach `http::fetch` sets
   `$CLAUDE_STATUS_SPEND_URL` itself rather than trusting its runner to have
   exported it.
2. **Neutralise *both* credential arms.** A fake `$HOME` removes the credentials
   *file*. The keychain arm is not `$HOME`-scoped: it shells out to `security`,
   so it is neutralised by pointing `PATH` at a directory that does not exist. A
   test that wants credentials seeds the file instead; a test that wants *none*
   must do both, or it is asserting whatever happens to be true of the machine
   it ran on.

**`PATH=""` is not the way to do the second.** An empty `PATH` is a single empty
entry, which POSIX resolves as the **current directory** — so a `security`
binary sitting in the package root would be run and its stdout parsed as an
OAuth document. Unsetting `PATH` is no better: the C library falls back to
`_PATH_DEFPATH`, which includes `/usr/bin`.

---

## 3. Input handling

### Every field on the payload is optional

**Decided** at the outset. Claude Code has changed the payload's shape before
and will again, so it is parsed defensively — **a missing or unexpected field
omits its segment, it does not fail the render.**

This is the input-side expression of invariant 3, and it is a decision rather
than a nicety: the alternative, validating the payload and failing on a shape
that does not match, hands the user a broken bar on the day an upstream field is
renamed.

### The subagent `type` is rendered as a glyph, never as text

**Decided** at the outset, from the original — recorded as a trap "learned the
hard way". `type` is almost always the generic `"local_agent"` regardless of the
actual subagent type, so falling back to showing it as the name displays the
same useless word on every row.

Neither `model` nor `effort` is a documented per-task field. A per-task value is
read if present — a future build may add one — else the panel-wide value, else
the segment omits.

---

## 4. Configuration

### Three layers, deep-merged low → high

**Decided** at the outset: shipped defaults, then the per-user config, then the
per-repo layer. What each layer *may* contain has moved twice since; see below.

A layer that is missing, unreadable, malformed, or **not a JSON object** is
ignored rather than fatal, and the render proceeds on the layers below it —
invariant 3 again.

### The defaults are embedded in the binary

**Decided 2026-08-19** (`main-bar`).

**Was:** two file layers only, which meant a machine with neither rendered
**blank**.

Embedding the defaults means a cold start draws a full bar. Output is
byte-identical for every install whose user file *is* the seeded defaults, which
was all of them; the visible change is that a user who deleted a key expecting
it gone now gets the default back.

The consequence that later became load-bearing: **with no config file anywhere
the bar renders from the embedded defaults, and that is a supported, tested
state rather than a degraded one.**

### The config file is renamed to `claude-status.json`

**Decided 2026-08-19** (`main-bar`), from `statusline.json`, for consistency
with the tool's identity.

The contract's own standing advice at the time was against exactly this, and the
migration was accepted as the price: `--install` moved the old file, preserving
the user's theming. The binary only ever knows the new name, so **no per-render
stat is spent on a legacy path**. Until the Phase 5 cutover both files existed
on purpose — the JS bar was still live and still read the old name, so neither
was stale.

The migration and everything around it was later deleted; see
[A render reads, it never writes](#a-render-reads-it-never-writes).

### A migration rewrites; it does not rename

**Decided 2026-08-22** (`repo-autoconfig`). Recorded although the migration it
governed no longer exists, because the reasoning is a general one about carrying
a file across a rename.

The legacy file pointed `$schema` at the `ai-plugins` repo, and one kept under
that URL **is validated against the wrong document for the rest of its life**.
So `$schema` was repointed and the file written under the new name, the old one
removed only once the new one was on disk.

Every other key was carried across untouched, with one exception: migrating the
**user** layer dropped `projectName`. The JS bar read that key from this same
file, but here it is repo-level only, and one kept at layer 2 would name every
repo the user opens after whichever one they set it in. It was dropped rather
than moved, because **nothing in the user layer records which repo it was meant
for**, and `--configure` derives the right name from the repo it runs in.

A legacy file that was **not a JSON object** had nothing to set `$schema` on and
was moved as-is. This applied at both levels and in both writers.

### `projectName` is repo-level only

**Decided 2026-08-22** (`repo-autoconfig`), and **still in force** — it survived
the reversal of the cycle that introduced it.

It ships in neither the embedded defaults nor a seeded user config, and the
shipped schema describes it *without* the asset carrying it — the one deliberate
asymmetry between the two, pinned by name in `defaults_integrity`.

**A key that identifies one repo has no meaningful value at a layer shared by
all of them.** Embedding the old `"Project-Name"` placeholder meant every
unconfigured repo rendered the same fictional name.

### `autoConfigureRepo` and the render-path write

**Decided 2026-08-22** (`repo-autoconfig`) — and **largely reversed 2026-08-23**
(`config-relocation`). Both halves are here because the reversal is the larger
decision and it is unreadable without what it reversed.

**Was:** `autoConfigureRepo`, a boolean defaulting to `true`, let a
`--statusline` render that found no layer 3 **write one** — migrating a
repo-level `statusline.json` if present, otherwise seeding `projectName` from
the repo directory's name. Writing `false` into layer 2 opted out.

Four constraints were designed to make a write on the render path safe, and they
are worth keeping as a record of what it takes:

- **Read from layers 1 and 2 only.** The flag was resolved before layer 3
  existed, so a repo could not enable its own creation. The accessor's fallback
  was `true`, matching the shipped default, so a config that failed to parse
  behaved like one that was never written.
- **`--statusline` only.** `--subagent` and the caps hook resolve a repo root
  too and stayed strictly read-only, so there was exactly one writer.
- **Silent on every failure.** A read-only checkout, a `.config` that is a file,
  a full disk: the render proceeds. Invariant 3 outranks seeding a convenience
  file, and invariant 1 leaves nowhere to complain to. An existing layer-3 file
  that did not parse was never overwritten — the create path re-checked for the
  *file*, not for a successful parse.
- **Costs one stat when off or already done.** `layers::load` already stats
  layer 3, and the create path was reached only when that stat came back empty.

The write was atomic (temp file, then rename), because two sessions can render
in the same repo at once, and indented rather than compact, because unlike the
spend cache this is a file a person opens.

**Reversed** — see
[A render reads, it never writes](#a-render-reads-it-never-writes).

### `--uninstall` removes; it does not restore a migrated file

**Decided 2026-08-22** (`repo-autoconfig`), withdrawing the earlier statement
that `--uninstall` would put the legacy file back.

A config the install migrated in is **this project's file** and is removed under
its own name, guarded by the digest the receipt recorded — an edit since the
install keeps it. The legacy `statusline.json` it came from is **not**
recreated: bringing it back would leave the user holding a config for a tool
they no longer have. The receipt therefore records no `movedFrom` for anything.

The restore discipline still governed `settings.json` **keys**, which are prior
state the installer overwrote, not files of its own. **The two are different
obligations** and conflating them is what produced the original claim.

There is now no installer and no uninstall at all; see
[The receipt discipline](#the-receipt-discipline-was-never-discharged).

### The merged config is deserialized into typed Rust structs

**Decided 2026-08-23** (`typed-config`). Once, after the merge and never before.
The merge itself is unchanged — it still operates on untyped JSON, and the
object-only filter and the forbidden-key strip still happen first.

The forgiveness rule gains a case: a merged tree that will not deserialize is
ignored too, and the **embedded defaults render**, with one diagnostic on
stderr. That fallback is a value in code rather than a second parse, **so it
cannot itself fail**.

**A mistyped value costs its own key and nothing else.** Every coercion the
untyped accessors performed is preserved per-field, so `"symbols": {"model": 5}`
still renders that one glyph as `""` and leaves the rest of the layer —
including the user's theme — applied. The whole-tree fallback is reached only by
a tree malformed *as a whole*, such as a root that is not an object. The
distinction matters: **the two outcomes differ by everything the user
configured.**

Types are what let the code **name a default**, which is what makes storing only
non-defaults possible. Nothing a user can see changed in that cycle.

### Merge semantics

**Decided** at the outset, and they must match exactly:

- Objects merge **key by key**, recursively.
- Arrays and scalars are **replaced wholesale**. A repo overriding `lines` means
  to replace the layout, not to append to it.
- Keys `__proto__`, `constructor`, `prototype` are skipped. In Rust this is moot
  — the *behaviour* of ignoring them is kept so a config written for the old
  implementation behaves identically.

### Keeping the file name, location and schema identical — retracted

**Was decided** at the outset: keep the config file name, location and schema
identical to the JS implementation's, so existing users could point Claude at
the new binary and see the same bar. Any schema change is a migration you have
to design, and there was no reason to take that on in v1.

**Retracted 2026-08-23** (`config-relocation`). The instruction was sound when
it was written and **its premise turned out to be false: there is no existing
user.** `claude-status` had never been released, so "point Claude at the new
binary and see the same bar" described nobody, and the migration the note warned
about is a migration from a state that never existed.

The file name, location and schema have all moved since, and **no fallback path
was written for any of them — for the same reason.**

This is the clearest instance in the record of a decision that was correct for
its input and wrong for the world.

### The user config lives in `~/.config/claude-status/config.json`

**Decided 2026-08-23** (`config-relocation`). A **directory** rather than a bare
file, because the tool will accumulate more than one thing to store, and because
a directory is one thing to delete.

**There is no fallback to `~/.config/claude-status.json`**, and a file left
there is ignored rather than migrated — per the retraction above.

### The cache does not move, and the split is the point

**Decided 2026-08-23** (`config-relocation`), and recorded explicitly **so it is
not later "tidied" into the config directory.**

`~/.cache/claude-status/` holds the spend cache and its lock, and belongs there
rather than beside the config: a spend figure derived from an account token is
**machine-local and regenerable**, while the config directory is the thing
people commit to a dotfiles repo and sync between machines. A cache that
followed the config would arrive on the second machine stale, keyed to the first
machine's account.

### A config written by the binary holds only non-defaults

**Decided 2026-08-23** (`config-relocation`). A key appears only where its value
differs from the binary's, so **an unset key follows the binary forward across
upgrades**. The output always carries `$schema` — not a setting, but the pointer
that makes the file editable.

**This property was not delivered end to end for two cycles, and the gap was on
purpose.** The npm installer seeded `assets/claude-status.defaults.json`
**verbatim**, pinned by a test asserting the seeded file was sha-identical to
the asset, so every install performed through it **froze every shipped value at
whatever version happened to be installed** — exactly the freeze this rule
exists to end. `config-relocation` changed the binary's writer and deliberately
left the installer alone; `distribution/01` then deleted the installer outright
and the seeding and its test went with it. The binary's writer is now the only
writer there is.

**Open maps are diffed entry by entry.** `palette`, `symbols`, `typeSymbols`,
`segments` and `subagent.statuses` all have non-empty defaults, so emitting a
whole map because one entry changed **would silently freeze the rest at today's
values while looking like it worked**.

### Layer 3 may set `projectName` and nothing else

**Decided 2026-08-23** (`config-relocation`). Any other key is **ignored** — not
merged, and not an error, because the never-fail rule still holds — and
`--doctor` names the keys it dropped, which is the only place a user can find
out why the file they wrote is doing nothing.

This is a **reduction** of the three-layer merge, not a clarification of it: the
layer used to be able to override anything. The layer existed to name the
project, and letting it override styling **made every repo a place where the bar
could look different for reasons nobody could find**. Nobody lost a working
setup — nothing had shipped.

**The narrowing reverses two earlier decisions, not one.** They are recorded
separately because they were taken for different reasons and cost different
amounts:

1. **Styling.** The layer could override `lines`, `palette`, `segments` and the
   rest. The capability goes; nobody was using it.
2. **Caps.** The `caps` cycle removed a tighten-only clamp **knowingly**, so a
   repo could raise its own limits — argued on the grounds that layer 3 is a
   file you commit and review in your own repository. `caps` is now not readable
   from layer 3 **at all**: it resolves embedded → user and stops.

   That argument **held for a repo you wrote and never held for one you
   cloned**, and the caps hook is not a rendering decision: a repo raising its
   own context cap does not draw an odd bar, it **suppresses the directive that
   stops an agent running past its budget**. This is the larger of the two
   reversals — it un-takes a tradeoff that was argued for on the record, rather
   than one that was merely inherited.

See [Caps become config](#caps-become-config) for the half being reversed.

### A render reads. It never writes

**Decided 2026-08-23** (`config-relocation`). `autoConfigureRepo` and the
render-path creation of layer 3 are both gone, together with the
`statusline.json` migration: the JS bar's config is another tool's file, and
with nothing released there is no user holding one this binary was ever going to
read. **A repo config exists only if a human writes one.**

The invariant that buys is worth naming: **a status line that redraws every four
seconds provably touches nothing on disk during a render**, which is easier to
reason about than any amount of care about *when* it writes.

Discoverability moves entirely to `--help` and the website as a result. The repo
layer being supported is not the same as anyone knowing it exists.

**Amended 2026-08-28** (`doctor-rename`). Of those two, the **website** now
carries it; `--help` only points there. See
[`--help` is an index, not the documentation](#--help-is-an-index-not-the-documentation--reversing-criterion-7).
The obligation this paragraph creates is unchanged — someone still has to tell
the user the layer exists — but the binary is no longer the one doing it.

### `--configure` seeds an empty user config

**Decided 2026-08-23** (`cli-surface`). With no file at layer 2 it creates
`~/.config/claude-status/config.json` containing `$schema` and nothing else — a
starting point an editor can complete against, which is what makes "hold only
what differs from layer 1" a thing a person can actually do by hand.

**Deliberately not the shipped defaults**: the npm installer wrote the whole
asset verbatim, so every install froze a full copy of layer 1 at the version
that happened to be current, and a later release changing a default reached
nobody. A pointer and no settings is the opposite of that.

**An existing config is never touched** — not merged, not topped up, not
reordered. A writer that round-tripped one would preserve a *degraded* config
rather than the configuration, **because degradation maps several inputs onto
one state and is lossy by construction.**

### `--configure` seeds from the shipped defaults, never from the loaded config

**Decided 2026-08-23** (`cli-surface`). `layers::load` merges the repo layer's
`projectName`, so seeding from what it returns would pin the name of whatever
repository the user happened to be standing in **into their global config**,
where it would then override every other repo's name.

### The JSON schema is generated output

**Decided 2026-08-24** (`schema-and-validation`).
`schemas/claude-status.schema.json` is produced from the Rust config types by
`mise run code:schema`, behind an off-by-default `schema` Cargo feature — **so
the released binary carries no `schemars`**. A pre-commit hook and a test both
run the `--check` mode, which means a field added to `Config` without
regenerating fails the commit and fails CI, naming the command that fixes it.

`$id` is injected from the same `SCHEMA_URL` constant `--configure` stamps into
every file it writes, **so the published schema and the pointer inside a user's
config cannot disagree.**

**`$schema` stays a declared property.** `Config` does not model it — it is a
pointer rather than a setting — and the root is `additionalProperties: false`,
so without an explicit declaration **every file `--configure` writes would fail
to validate against its own schema**.

**A colour accepts `null`**, which the hand-written schema got wrong. An
explicit `null` clears a colour the defaults set, so the segment falls through
to `defaultFg`, and `--configure` writes that key back verbatim — the old schema
**called the binary's own output invalid**. `bold` accepts `null` for the same
reason.

### Validation is advisory, in the strict sense

**Decided 2026-08-24** (`schema-and-validation`). Nothing validation finds
changes a byte of stdout or the exit code, in any mode. **Invariant 3 is not
renegotiated in a diagnostics cycle.**

In particular the render path keeps the **permissive** types:
`deny_unknown_fields` appears only in the `schemars` namespace, where it shapes
the schema, and never in `serde`, where one mistyped key inside `powerline`
would blank the block and draw a bar with no separators.

**Unknown-key detection covers closed objects only.** Five maps are open at the
key level — `palette`, `symbols`, `typeSymbols`, `segments` and
`subagent.statuses` — because their keys are names the user chooses, **so no key
in them can be *unknown***. Their **values** are still closed: `segments.foo` is
a legal segment id this build may not know, while `segments.foo.bge` is a typo
and is reported as one. Where the binary has a list to check an open key against
it says so as a note rather than a warning; for `typeSymbols` and
`subagent.statuses` it has none, because every key in both is read. **This is
the limit the original ask ran into and it cannot be removed.**

### Do not retype the glyphs

**Decided** at the outset, after the failure that motivated it. Almost every
symbol in the defaults is a Nerd Font **private-use codepoint**, which renders
as nothing or as a box in most editors, diffs and terminals — and is therefore
silently dropped by copy-paste, **and by any model transcribing it**. The
original contract inlined them and **lost all 28 that way**.

The rule: copy the JSON as bytes, and verify by **rendering** the bar, never by
reading a diff. The codepoint table that accompanied it existed so a lost glyph
is recoverable and a port is checkable.

### `typeSymbols._default` moved from U+F544 to U+F1B2

**Decided 2026-08-26** (site feedback). `U+F544` **is a codepoint no current
Nerd Font can draw.**

It was a Nerd Fonts **v2** Material Design Icons codepoint. v3 remapped that set
to `U+F0001`–`U+F1AF0` and left `U+F534`–`U+F560` **unpopulated**, so the
fallback glyph for an unrecognised subagent `type` rendered as tofu in every
Nerd Font 3.x — verified against Hack, FiraCode, Iosevka and Meslo at Nerd Fonts
3.5.1, all four missing it and all four carrying the other 24 codepoints this
product uses.

Replaced with `U+F1B2` (cube): present in v3, and **deliberately generic**, so
it does not read as any of the specific types beside it.

**Found while subsetting a glyph font for the documentation site, which is the
first thing that ever checked these codepoints against a font.** The
generalisation worth keeping is that a table of private-use codepoints is
unverifiable by reading — only rendering it against a real font checks it.

### The subagent status walk keeps three quirks on purpose

**Recorded 2026-08-21** (`subagent-panel`). Status matching is tried **in config
order**, first `match` regex to hit wins. Three details of the walk are
observable in the output and were unstated until the panel was built; each is
kept rather than corrected, because the panel had to match the original:

- An entry with an empty `match` is **recorded** as the fallback and the walk
  **continues**, so with two of them the **last** wins.
- The patterns are **unanchored substrings**, so `not_ok` matches `done` through
  its `ok` alternative. A wart, and the original's.
- A pattern the regex engine **rejects is skipped rather than fatal** —
  invariant 3 again, applied to a config value.

### The subagent description budget measures UTF-16 code units

**Decided** when the panel was built. Truncation is to `budget - 1` **UTF-16
code units** plus one U+2026, so a truncated description is exactly `budget`
units long. **UTF-16 because that is what JS `String.length` and `slice`
count**, and the panel had to match the original's output.

**The accepted flaw:** it measures units rather than terminal columns, so a
CJK-heavy description overruns its visual budget. Known, cosmetic, and shared
with the original — recorded as a tradeoff taken rather than a bug missed.

---

## 5. Rendering

### A segment with no data omits entirely

**Decided** at the outset. A segment builder returning "no data" **omits the
segment** — it does not render an empty box. A line whose segments all omit
renders as nothing, and an all-empty line is dropped rather than printed blank.

An unknown segment id in `lines` writes to **stderr** and omits the segment: it
must not fail the render and the exit code stays 0. **A segment builder that
panics costs only its own segment** — invariant 3 applied at the smallest unit
that can hold it.

### The `branch` segment's worktree prefix is conditional — and the docs mirrored the error

**Corrected 2026-08-24** (`website/01-site`). The prefix is pushed only when a
worktree subpath resolved, so **an ordinary checkout renders the branch with no
worktree glyph at all.** An earlier correction had fixed the *order* of the two
glyphs and listed only the ahead and dirty markers as conditional, **which reads
as though the prefix were always present.**

The part worth keeping is how it was found: while writing the user-facing docs,
which **had mirrored the error — as had the renderer's own doc comment.** The
site's hero screenshot showed the correct behaviour and therefore contradicted
the page describing it. **A wrong claim propagates into every document derived
from it, and the rendered artefact is the only thing that disagrees.**

### `project` falls back to the git root's directory name

**Decided 2026-08-26** (site feedback). A **deliberate behaviour change** rather
than a correction of a mis-record.

**Was:** the segment sat out when `projectName` was not set in config. That was
true when written.

The name is now the first of these that exists: `projectName` from the repo
layer, `projectName` from the user layer (which is **not** inert — it names
every repository that has not named itself), then **the git root's own directory
name**. The segment omits in exactly one case: there is no git root to take a
name from.

`projectName` therefore **stops being what turns the segment on** and becomes
only how a repository is called something other than its directory. The
repo-level narrowing is untouched, and so is the rule that the key ships in
neither set of defaults.

**On the security question:** the directory name is attacker-nameable, as
`projectName` already was — a clone lands in a directory the cloner chose. It
reaches the bar through the same `sanitize` every segment's text passes, which
already lists "a worktree directory" among its inputs, **so this widens no
surface.**

Requested by the owner, whose expectation was that the segment already behaved
this way.

---

## 6. Escapes and untrusted input

**Added 2026-08-21** (`macos-only`). Before this, nothing said which strings on
the row were trusted, and **the answer in the code turned out to be "the ones
somebody remembered"**.

### Treat every dynamic value as hostile

Not as a worst case — **as the normal case**. A branch name, a directory under a
worktree, a session name, a model string, and a task's `name` and `description`
(written by a model, and therefore steerable by indirect prompt injection) all
reach the bar unreviewed. So does the repo-level config file, which is read from
whatever repository the user changes into: **cloning a hostile repo is the
entire attack, with no further interaction.**

**Narrowed 2026-08-23** (`config-relocation`), and the rule is unchanged. Layer
3 may now set `projectName` and nothing else, so a cloned repository reaches
exactly one string on the bar rather than any key it likes — notably **not** the
powerline separators, which sit outside every segment's SGR bracket and were the
sharpest target.

**This narrows an input; it removes none.** `projectName` is still drawn, still
written by whoever wrote the repo, and every other source above never passed
through a config layer at all. The repo layer also gained a *new* surface in the
same cycle: `--doctor` reports the keys it ignored **by name**, and a JSON key
may contain a newline, so the report's row filter covers key names as well as
values.

### Two filters, not three

- **`Cc`** — the Unicode control category, which is `U+0000`–`U+001F`, `U+007F`
  **and** `U+0080`–`U+009F`. C1 needs no rule of its own: **it is already
  `Cc`.** It matters because a terminal in 8-bit mode reads `U+009B` as CSI with
  no `ESC` in front of it.
- **The invisibles that are not `Cc`** — bidi overrides and isolates
  (`U+202A`–`U+202E`, `U+2066`–`U+2069`), and `U+200B` / `U+FEFF`.

**Kept:** ZWJ, variation selectors, and the private-use codepoints — the bar is
built from Nerd Font glyphs, **so filtering those would erase it.**

### The filter belongs at the chokepoint, not in each producer

There are **five** such points, one per surface, and a sixth surface would need
its own: the main bar's `segments::build`; the subagent sweep ending `task_row`
(the panel builds its `Segment`s directly and **inherits none of the bar's
filtering**); `Powerline::from_config` (config-supplied and written **outside**
any segment's SGR bracket — the widest of the five); one sweep over `--doctor`'s
assembled report; and `_shared::diag` for all of stderr.

**The stderr surface was the last to be found, and for the usual reason: it is
not stdout, so it did not look like a rendering surface. It is one.** It was
first patched at two `{}` writes by hand — the per-write pattern this rule
exists to reject — then narrowed to `narrate`, which turned out to be **one of
six** writers. `_shared::diag` is now the only `eprintln!` in the crate, **which
makes the rule checkable with a grep rather than by reading every call site.**

`--doctor` earned a chokepoint rather than a call per write for the same reason
the cycle that added it found: filtering the paths first **missed** the layout
entries and the spend gate table, both of which reach the terminal by a
different route. Anything added to the report later is covered without anyone
having to remember.

### Two consequences of the `--doctor` sweep, both load-bearing

- **Newlines survive it.** The report is deliberately many lines, so it uses a
  variant of the filter that keeps `\n` and strips everything else.
- **The `SAMPLE RENDER` section is appended after it.** That section *is*
  renderer output: its SGR codes are meant to be there, and every dynamic value
  inside it already passed through `segments::build`. **Sweeping it would strip
  the colours the section exists to show.**

### stderr keeps newlines out, and pays for it

`_shared::diag` uses the row filter, so one call is one line — which collapses a
multi-line panic payload onto a single line rather than preserving its shape.

**Deliberate:** a panic message quotes whatever it panicked on, so allowing a
newline there would let a branch name or a config value **forge a second
`claude-status:` line**. A stack's shape is worth less than a diagnostic whose
boundaries a reader can trust.

### A dynamic value may never contribute a newline

**This is a rule, not a consequence of the one above**, and it is why
`--doctor`'s report-wide sweep is not its only defence: that sweep exempts `\n`
so the report can be many lines, and a value carrying one would forge a line, a
section header, or a whole `CLAUDE WIRING` block **in the diagnostic a user
reads *because* they are trying to work out what is wrong. No escape is needed
for that attack.** Every value in the report therefore also goes through the row
filter; only the report's own structure may add newlines.

The report may still *quote* a hostile value — that is it doing its job. **What
it may not do is let the value stop being a quoted value.**

### Known residual, accepted

A dynamic value may still contain a private-use separator glyph and so *look*
like a segment boundary. Accepted: **the same config layer can already set the
row's colours by design, and the line drawn here is between theming the bar and
escaping out of it.**

---

## 7. The CLI surface

### `--version` is checked first and prints nothing but the version

**Decided** at the outset. It is the one output of this binary a script may
parse, and **two release gates fail over its shape**: the release workflow
refuses to publish a built binary whose `--version` differs from the crate
version, and the `build:statusline` smoke test asserts the same thing before the
artifact leaves the machine.

### `--doctor` is both a mode and a modifier

**Decided 2026-08-19** (`main-bar`), absorbing an earlier `--info` idea. As a
mode its report is the output and goes to stdout; as a modifier it narrates to
stderr and **must not change stdout by a single byte**. `--version --doctor`
still prints a bare version.

It exists because the spend path is otherwise completely silent.

`--info` had been a flag on the `ai-plugins` installer rather than the script;
it belongs to whoever owns the binary, which is now this repo.

It was spelled `--debug` when this was decided — see the rename below. The
duality it records is unchanged; only the word is.

### `--debug` was renamed to `--doctor`, with no alias

**Decided 2026-08-28** (`doctor-rename`). Both of the flag's jobs moved
together: the report surface *and* the stderr-narration modifier. Splitting them
— `--doctor` for the report, `--debug` kept for narration — was considered and
rejected: the entry above records the duality as the point of the flag, and two
names for one behaviour is the drift this repo keeps deleting.

**No alias**, on the same terms as the refresh rename above but for a different
reason. That one had never shipped; this one has. The alias was rejected anyway
because a silent alias makes the old name permanent, and the mitigation is
better than compatibility: the old spelling now falls into the unrecognised-
argument arm, which **names it on stderr and prints the help after it**, and
that help names the old flag beside the new one. A user who kept `--debug` in
`settings.json` is told, once per render, in words.

The note started as a four-line `RENAMED:` block and was cut the same day to the
parenthetical `--doctor    (earlier flag was --debug)` — see the help-scope
entry below. The reader who needs it is scanning for `--doctor` and recognising
the old word next to it; the reasoning is the website's job.

The version bump is a **patch**, which understates it — this removes a flag.
Recorded as the maintainer's call rather than argued for.

### `--help` is an index, not the documentation — reversing criterion 7

**Decided 2026-08-28** (`doctor-rename`), reversing the criterion recorded in
[a render reads, it never writes](#a-render-reads-it-never-writes) and the two
tests that held it.

**What it was:** `--help` carried five sections and about seventy lines — every
surface including the three Claude Code invokes for you, the full shape
`--configure` writes into `settings.json`, both config paths, the repo layer's
one permitted key, and a `$schema` example. The reasoning was criterion 7: with
the npm installer deleted and the autoseed gone, `--help` was **the only
documentation that shipped**, so "vague about the repo layer" and "the repo
layer is undiscoverable" were the same statement. The tests asserted a floor —
`HELP.lines().count() > 40`.

**What it is now:** about twenty lines. The five flags a person types, the two
modifiers, one line on what an unrecognised argument does, and the website. The
tests assert a **ceiling**, `< 30`, and additionally assert that `--statusline`,
`--subagent` and `--caps-hook` are **absent**.

**Why it reversed.** The premise expired when the website shipped. Criterion 7
was written when there was nowhere else for a user to be sent; there is now, it
is linked from the help, and the same content lives there in a form that can
carry tables and examples. What is left is the cost — the first thing a new user
reads was seventy lines, most of it about files most users never create, since
the bar is useful out of the box with no config at all.

**The wired surfaces are absent, not merely deprioritised.** `--configure` sets
them up and a user never types them, so a line each spends the top of the help
on flags its reader cannot use. `MISSING_FLAG` still names `--statusline` and
`--subagent`, which is the one place naming them earns the space: it is what a
user sees when their `settings.json` has gone stale. A test asserts that too, so
cutting them from the help cannot quietly cut them from there.

**`--refresh` went with them**, on the same rule and a different caller: it is
**the bar** that spawns it, not Claude Code. `resolve_spend` re-invokes this
binary with it, detached, when the cache goes stale, then draws the cached
figure without waiting. Typing it yourself is the identical call —
`bypass_dedupe: false`, so a no-op on a fresh cache — and it prints nothing
either way, because `the_refresh_child_is_recognised_and_silent` pins the
silence the `/dev/null` child was designed around. **A listed flag that can do
nothing and say nothing teaches a user that the tool is broken.** `--doctor` is
what forces a fetch and shows the answer, and it stays listed.

The flag is not removed, only unadvertised: anything that already scripts it
keeps working, and `REFRESH_FLAG` is still the constant the spawn and the parser
share.

**Both guards were inverted rather than deleted**, on the same reasoning as
[layer 3 may set `projectName` and nothing else](#layer-3-may-set-projectname-and-nothing-else)'s
inverted pair: length is exactly what regresses here, because every future flag
will want three lines to explain itself. One test was genuinely deleted —
`the_help_examples_schema_url_is_the_one_the_writer_emits`, which guarded the
`$schema` example in `HELP` against drifting from `config::write::SCHEMA_URL`.
With no example left there is no second copy to drift; a `#[allow(dead_code)]`
note in `cli.rs` records the deletion so the constant is not found untested and
re-guarded.

### An unrecognised argument is named on stderr — reversing the silence

**Decided 2026-08-28** (`doctor-rename`), reversing half of
[`--configure` is the one mode that rejects an unrecognised argument](#--configure-is-the-one-mode-that-rejects-an-unrecognised-argument).

**What it was:** every surface but `--configure` ignored an argument it did not
recognise, *silently*. The reasoning was invariant 3 — Claude Code invokes the
render surfaces, and a stray token must never cost a user their bar — and the
silence was taken as the price of that.

**What it is now:** every mode names each unrecognised argument on stderr and
prints `HELP` after it. `--configure` still refuses with a non-zero exit; every
other surface still renders exactly what it rendered before, byte for byte, and
still exits 0.

**Why it reversed.** The silence was never actually load-bearing — invariant 3
is about stdout and the exit code, and stderr costs neither. What made the price
visible was the rename directly above: `--debug` became precisely an
unrecognised argument, so under the old rule a user who kept it got a bar with
narration quietly switched off, and a user who typed `claude-status --debug` got
the help text with **nothing anywhere saying why**. A rule whose first real
encounter with a live case produces that is the wrong rule.

`Mode::Help` is carved out of the `HELP`-on-stderr half alone: it is already
writing the same fifty lines to stdout, both streams land in one terminal, and
the motivating case — someone typing `claude-status --debug` at a prompt — is
exactly that mode. The argument is still named.

The multi-line write needed a second stderr entry point, `_shared::diag_report`,
because `diag` uses the *row* filter that strips newlines and would have
collapsed the help onto one line. It takes **static text only**; the argv tokens
that provoke it go through `diag` one line at a time, `{:?}`-escaped first,
because a token is the most directly attacker-nameable input this binary has.

### The refresh flag was renamed, with no alias

**Decided 2026-08-23** (`cli-surface`). The flag was renamed to `--refresh`, and
the longer name it was renamed from is not accepted as an alias. Nothing had
shipped, so an alias would be compatibility with a version that never existed.

The name is a **constant** (`cli::REFRESH_FLAG`) rather than a literal in two
places, because a render spawns a detached child with it and **that child's
stdio is `/dev/null`** — a caller and a parser that drifted apart would leave
the spend segment silently frozen, with nothing on any stream to say why.

### Colour goes on after the filter, never before

**Decided 2026-08-28** (`colour`). The human-facing surfaces — `--doctor`,
`--configure`, and the stderr diagnostics — carry green/yellow/red. Three things
made that safe rather than a hole in invariant 4.

**Order.** `sanitize` strips every control character, ESC included. Painting
before it would simply have the colour stripped; painting after it means every
escape in the output provably came from `paint`, because the sweep has already
removed all the others. This is not a new idiom — `--doctor`'s SAMPLE RENDER was
already appended *after* the sweep "because it is the one part whose escapes are
meant to be there". The rule generalised; the filter did not move.

**A side channel, not colour in the string.** `paint::Marked` accumulates text
plus `(line, health)` marks and implements `fmt::Write`, so the existing
`writeln!(out, …)` calls are unchanged and the five `&mut String` helpers needed
only a type swapped in their signature. Marks are applied after the sweep. This
rests on the filter preserving line count, which
`line_count_survives_the_report_filter` pins — if it could renumber a line,
every mark after that point would paint the wrong row, silently, in the one
surface a user reads to find out what is wrong.

**Health, not severity of prose.** Green is working, yellow is absent-but-fine
or written-and-ignored, red is actively failing. `Health::Note` — most lines —
is uncoloured, because a report where everything is coloured says no more than
one where nothing is.

**`Absent` config layers are NOT yellow**, though the first sketch of this had
them so. A machine with no config file anywhere is a *supported* state; the bar
renders from the embedded defaults and nothing is wrong. Painting the common
case as needing attention would tell every new user their install is off.
`Unusable` is red, because a file **is** there and contributed nothing.

**TTY-gated, and `NO_COLOR` is honoured** when present and non-empty, per
<https://no-color.org>. Each stream answers for itself: `--doctor > report.txt`
run in a terminal leaves stderr a tty and stdout a file. The bar is exempt from
the gate — Claude Code reads it through a pipe and still wants colour.

**Three surfaces are never painted**, and the reason is the same for all three:
a machine parses them. `--version` is read by the release workflow and the
`build:statusline` smoke test; `--caps-hook`'s stdout is injected verbatim into
an agent's context; `--subagent` is NDJSON. A decoration on any of them is a
broken build or a corrupted payload, not a nicer terminal. `--help` is not
painted either, for a different reason: an index of flags reports no state, so
there is nothing in it that is green or red.

**`diag` takes the health as a parameter rather than inferring it.** Inference
here means grepping our own prose for the word "error", which breaks the first
time a path contains it.

### `--configure` is the binary's whole setup story

**Decided 2026-08-23** (`cli-surface`), because the npm installer that used to
wire Claude Code was deleted by `distribution/01`. It writes the keys into
`~/.claude/settings.json`, merges rather than replaces `hooks.PostToolUse`,
preserves every unrelated key, and creates a user config with a `$schema`
pointer alone when there is none. `--dry-run` prints the same plan and writes
nothing.

Three decisions inside it, **none recoverable afterwards**:

- **It overwrites a `statusLine` belonging to another tool, without asking, and
  there is no receipt and no `--unconfigure`.** So it **prints what it
  replaced**, on stderr, before it writes. **That printing is the entire
  mitigation, not a nicety.**
- **A `settings.json` it cannot read is refused**, not overwritten: it names the
  file, changes nothing, and exits non-zero. **Absent is different from corrupt,
  and only the first is safe to write over.** The npm installer did not draw
  that line — it parsed inside a bare `catch`, fell back to `{}`, and **replaced
  the user's entire Claude Code configuration with three keys.**
- **The wired command is the bare name**, resolved from `PATH`, not an absolute
  path.

### The wired command is a bare name, resolved from `PATH`

**Decided 2026-08-23** (`cli-surface`).

**Was:** `${HOME}/.claude/bin/claude-status …`, while the npm installer placed
the binary and knew where it had put it.

Under Homebrew, `current_exe()` resolves the symlink to a **versioned Cellar
directory that `brew upgrade` deletes**, so an absolute path would leave the
wiring pointing at a binary that no longer exists — **silently, until the next
upgrade.**

**The accepted cost:** Claude Code's own `PATH` must contain Homebrew's `bin`,
which a GUI-launched application does not always inherit.

### Ownership is decided by the command's program name, not by a substring

**Decided 2026-08-23** (`cli-surface`). `contains("claude-status")` also matches
`claude-statusline`, `claude-status-pro` and `/opt/claude-statusbar` — none of
which are this tool, **all of which were being overwritten *without the warning*
that is the whole mitigation for having no undo.**

The legacy `context-caps.js` hook is likewise matched only under
`.claude/hooks/`, because **that is the one ownership test whose consequence is
deletion**, and another project's script of the same name is not ours to remove.

**The cost, taken deliberately:** a hook wired as
`/Users/me/bin/statusline --caps-hook` — a *renamed* copy of this binary — is
now foreign, so `--doctor` reports it unset and `--configure` adds ours beside
it, firing the actuator twice. No shipped install can produce that; it takes a
renamed binary or a hand-written line. **What it buys is that the report and the
writer share one definition**, which is what stops `--doctor` calling a hook
wired while `--configure` is about to duplicate it.

### `hooks.PostToolUse` keeps exactly one entry of ours

**Decided 2026-08-23** (`cli-surface`). Updated in place so the group's
`matcher` survives, with any later copies removed. It is a list Claude Code
iterates: **two identical entries are two invocations per tool call, so
normalising duplicates into copies of each other is not deduplication.**

### `--configure` is the one mode that rejects an unrecognised argument

**Decided 2026-08-23** (`cli-surface`). **The asymmetry is the decision.**

Every other surface carries on past what it does not know, and must: Claude Code
invokes the renderers, and invariant 3 says a stray token may never cost a user
their bar. But that same tolerance on the *writing* flag turns
`--configure
--dry-runn` — one mistyped character — into a **real, unundoable
overwrite of a file this tool does not own, performed by a user who believed
they had asked for a preview.**

**Amended 2026-08-28** (`doctor-rename`). As decided, "carries on past" also
meant *silently*; it no longer does. Every mode now names the argument on stderr
— see
[An unrecognised argument is named on stderr](#an-unrecognised-argument-is-named-on-stderr--reversing-the-silence).
The asymmetry this entry is about survives intact, because it was never about
who *speaks*: `--configure` is still the only mode that **refuses**, and the
only one that exits non-zero.

### Dotfiles: symlinks are followed, hardlinks cannot be

**Decided 2026-08-23** (`cli-surface`). The write is temp-then-rename, so a
**symlinked** `settings.json` is resolved first and written *through* — a rename
over a symlink would otherwise replace the link with a regular file and orphan
the real one, silently.

A **hardlinked** one cannot be followed, because a hardlink is a second name for
an inode rather than a pointer to a path: the other name keeps the old contents.
That is a known limitation and **the accepted cost of atomicity, which is not
negotiable here** — a `settings.json` seen half-written breaks Claude Code
outright, whereas a stale second name leaves the file Claude Code actually reads
correct.

**It is not left silent.** The rule that the destructive case must be visible
applies here more sharply than to the overwrite it was written for: an
overwritten `statusLine` at least gets quoted back, while **a stale hard link is
otherwise impossible for a user to find out about.** So a write that would break
one says so on stderr, naming the link count. It does **not** block the write or
change the exit code, and `--dry-run` reports it too — **a preview that stayed
quiet here would be silent about the one consequence it is uniquely useful
for.** The other name cannot be named: an inode does not know its own names,
which is the same fact that makes the limitation unfixable.

A **read-only** `settings.json` is rewritten too, and also says so. An atomic
replace needs write permission on the *directory*, not on the file, so a 0400
file is swapped out and its mode restored — **the mode is honoured, the intent
behind it is not.** Not a refusal: typing `--configure` is a clearer statement
of intent than a mode bit set at some point in the past.

### Concurrency is safe by construction, and was measured

**Decided 2026-08-23** (`cli-surface`). The merge is a pure function of the
bytes it read, so two racing runs starting from the same file compute identical
output and whichever rename lands last is byte-identical to the one it replaced;
a run starting after another finished reads a wired file and writes nothing at
all. **No interleaving produces a wrong file.**

The residual is a lost update against a *third-party* writer — Claude Code
itself, or an editor — landing inside the read→rename window, **roughly 1 ms of
a ~3 ms process**. The file always remains valid JSON.

### `--configure` is a deliberate repurposing of the name

**Decided 2026-08-23** (`cli-surface`). In the npm installer `--configure` meant
the **opposite** thing: it gave the repo you were standing in a repo-level
config layer, and advertised itself as the only command that wrote nothing under
`~`. This one writes **only** under `~`, and the repo layer is now written by
hand. `distribution/01` deleted that installer, so the two meanings no longer
coexist.

### A config layer has three states in `--doctor`, not two

**Decided** with the report. `loaded`, `using defaults`, and `UNREADABLE`.

**The middle one is the point:** with the defaults embedded, a machine with no
config file anywhere is *working*, and calling that `not found` **described a
supported state as a missing one.**

The third exists because the same word used to cover a file that is present and
will not parse — which is a real problem, **is silent everywhere else**
(invariant 3 leaves nowhere to complain to), and so is only visible here.

### `CONFIG LAYERS` reports validation findings per layer

**Decided 2026-08-24** (`schema-and-validation`). Findings print as continuation
rows under the layer that caused each, **because the answer to "why is the key I
wrote doing nothing" is which of three files it is written in, and after the
merge that is no longer answerable.**

Three kinds, and **the middle one is never a warning**:

- **⚠ unknown key** — a key in a closed object. A typo, reportable with
  confidence, with a did-you-mean when one is close enough to be worth the risk
  of being wrong.
- **· not a key this binary reads** — a key in an open map that nothing in this
  build asks for. Legal, and **warning about a working config would be worse
  than saying nothing.**
- **· coerced** — what the binary actually *did* with a value: a `0` width that
  renders ten, an empty gauge glyph, a `worktreePattern` that will not compile.
  **The one no schema can give you**, and the one a user staring at a
  wrong-looking bar actually needs.

`--doctor` is not a hot path — it already does git discovery and reads the spend
cache — so the walk's cost is not a consideration.

The embedded layer is not validated: it is this binary's own, and
`defaults_integrity` already holds it to the types. The repo layer is validated
over **what survived the narrowing**, because everything else is already named
on the `ignored` row above it.

---

## 8. Git resolution

### Filesystem first, subprocess only where unavoidable

**Decided** at the outset, because this is a hot path. Root and branch are read
from the filesystem, never from `git`.

Rust's `std::process::Command` has no built-in timeout, and the shape that works
was recorded because the obvious one does not: spawn, move the pipe into a
reader thread, and wait on a channel. **A solution that does not drain stdout
can deadlock on a full pipe before its timeout is ever consulted.**

### Both markers are gated on a resolved branch, not on a root

**Decided** at the outset: a repo whose `HEAD` is empty runs **no subprocesses
at all**.

### The broken-`.git` asymmetry is deliberate

**Recorded 2026-08-19** (`main-bar`). An `HEAD` that cannot be *read* does not
stop the upward walk — it continues to the parent, so a nested repo with an
unreadable HEAD reports the outer repo. An `HEAD` that reads but says nothing
useful, or a `gitdir:` pointer that will not parse, *does* stop it, with a root
but no branch.

**That asymmetry is faithful to the original's try-block scoping and is
load-bearing for submodules.**

### A change touching only binaries renders clean

**Recorded** at the outset as shipped behaviour rather than a bug. Each numstat
count is `\d+` or `-`, and the two sides are suppressed **independently**; git
reports `-` on both sides for a binary file. **That looks like a bug and is the
shipped behaviour** — recorded so it is not "fixed" into a divergence from the
original.

### If the untracked probe fails, the whole dirty marker is dropped

**Decided** at the outset, even though numstat succeeded. **A partial count
would be a quietly wrong number**, which is worse than no marker.

---

## 9. The spend subsystem

### A render must never fetch

**Decided** at the outset, and it is the design's central constraint.

The figure comes from the OAuth usage endpoint Claude Code's own `/usage` uses.
That endpoint **throttles on accumulated usage** — a tripped account stays 429
for half an hour or more — and the bar can render every few seconds.

So a render **reads a cache file and nothing else**. When that cache is older
than `refreshMinutes`, the render **spawns a detached child and draws the cached
value immediately**; it never waits for the child. `refreshMinutes: 0` disables
the refresh entirely.

### The cache is machine-global on purpose

**Decided** at the outset: one fetch per interval per machine, however many
sessions are open.

### No migration was written for the cache path

**Decided 2026-08-20** (`spend`), resolving an open question. The path and the
env var both took this repo's name, and **no migration was written**. An
existing `~/.cache/ai-plugins/spend.json` is left in place and ignored, so an
upgraded install **re-fetches exactly once — harmless by the contract's own
reasoning**, since the value is regenerable.

`$AI_PLUGINS_SPEND_CACHE` is **no longer read at all**. Anyone with it exported
in a shell profile will find it silently ignored.

**Only a leading `~` expands** in the cache path — unlike the usage-mirror
variable, which also expands `$HOME` and `${HOME}`. **The two are different
contracts** and the old document conflated them; they are recorded here as
deliberately different rather than as an inconsistency to tidy.

### Never clear a good value because one fetch failed

**Decided** at the outset. **429** sets
`backoffUntil = now + min(refreshMinutes ×
2^failures, 6h)` and keeps the last
good `data`. **401** — the token expired — increments failures and keeps the
last good data. A network error or missing credentials does the same.

### The lock is taken exclusively, and a stale one is taken over

**Decided** at the outset. Create `<cache>.lock` with `O_EXCL`; if it exists and
its mtime is under two minutes old, **another refresh is running: exit**. Older
than that, **the holder died**: take it over. A 60-second dedupe on the cache's
own age catches a sibling that just ran.

### Four gates hide the segment, and gate 4 catches most people

**Decided** at the outset, and recorded carefully **because a fully successful
refresh can still render nothing** — which is the subsystem's whole diagnostic
problem.

In order: `spend` absent from `lines` (the cache is never even read and no
refresh is ever spawned — **a user without the segment pays nothing**); no
cached `data`; `data.enabled == false` or no `limitMinor`; and `show == "auto"`
with a plan that is not `team` or `enterprise`.

**Gate 4 catches most people.** On a Max account the figure is fetched and
cached perfectly and then hidden here — and **before `--doctor` existed, that
was indistinguishable from a broken token.**

One further subtlety: under `show: "auto"` with an irrelevant plan the refresh
interval **stretches to 24 hours** rather than `refreshMinutes`, so a Pro/Max
machine re-checks its plan daily instead of every quarter-hour.

### `--doctor` as a mode performs a live, synchronous, foreground fetch

**Decided 2026-08-20** (`spend`). It does not merely report the cache, **and the
distinction is the whole reason the subsystem is diagnosable at all.**

On a fresh machine the cache **does not exist**. The first render reads nothing,
therefore draws nothing, and only *then* spawns the detached refresh child —
whose stdio is `/dev/null`. **So run one is guaranteed to show no budget, and
every diagnostic from that first fetch is discarded.** A passive `--doctor`
inspecting the cache at that moment could only say "no cache yet", which is
precisely the useless answer the user already had.

Three consequences, all deliberate:

- It **respects the lock and the backoff but reports them rather than silently
  obeying** — "a refresh is already running, holder started 14s ago", "in
  backoff, 28m left — fetching anyway to diagnose".
- It **bypasses the 60-second dedupe**, because a user typing `--doctor` twice
  wants two answers.
- It **writes the result to the cache** like any successful refresh, so a
  `--doctor` that works leaves the next render working too. **This is the
  supported fix for a first install that shows no budget.**

As a *modifier* this does not apply: a render still never fetches, and stdout
stays byte-identical.

### The token is never printed, in any branch

**Decided 2026-08-20** (`spend`). Not on stdout and not on stderr, in success,
401, 429, no credentials, refused connection, or keychain denial. **Only where
it was found** is reported.

### `--doctor` fetches even when the user's own gates hide the segment

**Decided 2026-08-21** (`macos-only`), after review pointed out the ordering was
real in the code and unwritten.

The four gates decide whether the figure is *drawn*; **they do not decide
whether it is *fetched***. So `--doctor` on a config with `spend` absent from
`lines` still performs the authenticated request, and then reports
`gate 1 ✗ HIDDEN`. **"You have it switched off" and "your token is rejected" are
different answers, and a passive `--doctor` could not tell them apart.**

**One thing does stop it, and the order matters:** the cache path is resolved
**first**, and no fetch happens when it is absent (invariant 5). With nowhere to
write the result there is nothing to diagnose and nothing to keep.

### Reproduce the verdict

**Decided** at the outset. Every gate above was a silent `return` in the
original. The report now narrates the cache path and endpoint, the prior cache's
age/plan/failure count, a held or stale lock, the dedupe, where credentials were
found, the HTTP status, what was extracted — and, at the end, a verdict:
`WILL RENDER` with the figure, or which gate stops it.

**It is the single most useful line the tool prints.**

### The spend fetch trusts the OS, not a baked root set

**Reversed 2026-08-27**, reported from an office network.

**Was:** ureq with rustls and its default `WebPkiRoots` — Mozilla's root set,
compiled into the binary. The argument was never *for* baked roots on their
merits; it was against `native-tls`, whose `openssl-sys` on linux-gnu wants
headers at build time and a versioned `libssl.so` at run time, which would end
the single-binary distribution story. Baked roots were what fell out of avoiding
that, and nobody weighed them separately.

**What it cost:** behind a TLS-intercepting corporate proxy, the certificate
chain terminates at a root that MDM installed in the login keychain. `curl`,
`git` and Claude Code itself all trust it, because they ask the OS. This binary
asked Mozilla, and returned
`FAILED after 137ms — io: invalid peer certificate: UnknownIssuer`. **There was
no flag, no environment variable and no config key that could have fixed it** —
the user's only options were to leave the network or stop using the spend
segment.

**Reversed to `RootCerts::PlatformVerifier`** (ureq's `platform-verifier`
feature, via `rustls-platform-verifier` → the macOS Security framework). **The
reason for the original ban is untouched**: this is still rustls and still links
no openssl, so the single-binary story survives — `cargo build` pulls
`security-framework` and `core-foundation`, not `openssl-sys`.

**The rule this is a case of:** a tool a user installs with `brew` should trust
what the rest of their machine trusts. A private root set is a claim to know
better than the operating system, and on a managed laptop that claim is simply
wrong.

**`--doctor` now names the store**, on the line above the failure, for `https`
endpoints only — `roots OS trust store (macOS keychain)`. Diagnosing the
original report meant reading `Cargo.toml`, because
`invalid peer certificate: UnknownIssuer` names no store and so cannot be told
apart from a genuinely bad certificate; the fix for one is to install a root
where this binary looks, and without knowing where that is the reader cannot
act. The string lives beside the `TlsConfig` in `http.rs` rather than in the
reporter, so the claim and the configuration are edited together.

**Two things worth knowing before touching this again:**

- **It is not a laxer check**, only a different set of roots. An expired,
  mismatched or self-signed certificate is rejected exactly as before.
- **The feature flag is not self-enforcing, and fails in the worst possible
  place.** With `platform-verifier` off, `Cargo.toml` still resolves, the code
  still compiles, and ureq panics *at run time* — "Rustls + PlatformVerifier
  requires feature: platform-verifier" — inside the detached refresh child, on
  the first real HTTPS request. Measured by commenting the feature out, at which
  point every pre-existing test in `spend::http` still passed, because they all
  speak plain `http://` to a closed port and never reach TLS.
  `https_is_negotiated_and_fails_closed` exists to close that gap and forces a
  real handshake.

---

## 10. The usage mirror — a contract with another repository

### The mirror exists because the figures arrive on no other channel

**Decided** at the outset. Context-window and rate-limit figures arrive **only
on the statusline payload** — never on hook stdin. So every main-bar render
mirrors them to a session-keyed file, which a `PostToolUse` hook then reads.
**This is not internal**: it is the reason vwf's context-cap hook can work at
all.

It is enabled only when the usage-dir variable is set, and is inert otherwise.
The write is atomic and **best-effort: a failure here must never affect the
rendered line.**

**The field names and the file layout are byte-compatible by contract.** A
consumer lives in another repo, so changing the format is a coordinated change
across both. **The env var name is part of the contract too** — renaming it
silently disables vwf's caps.

### The variable was migrated by honouring both names

**Decided 2026-08-21** (`caps-hook`). The variable was `$AI_PLUGINS_USAGE_DIR`
alone, and the contract said the name does not change.

It now migrates: **both** the writer and the reader try
`$CLAUDE_STATUS_USAGE_DIR` first and fall back, **so a machine still running the
JS hook — which only knows the old name — keeps working through the
transition.** Once the JS hook is gone this binary is both the writer and the
only reader, and the mirror stops being a cross-repo contract at all.

---

## 11. Distribution

This section is a record of a decision that changed four times. Each half is
kept, because a revisited channel should see **what was actually weighed**
rather than a tidied version of it.

### The options, as weighed on the day

| Option                               | For                                                                              | Against                                                               |
| ------------------------------------ | -------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| **GitHub Releases + install script** | Standard for Rust CLIs; no toolchain needed; `curl \| sh` is one line            | You own a shell installer, per-platform builds and a checksum story   |
| **npm with platform binaries**       | Keeps `pnpx`, which existing users already know; the pattern esbuild and swc use | A published package per platform; awkward for a repo with no other JS |
| **`cargo install`**                  | Trivial to publish                                                               | Requires a Rust toolchain — most users will not have one              |
| **Homebrew tap**                     | Great on macOS, one command                                                      | A second channel to maintain; Linux users still need another          |

**Read this table as of the day it was written.** Every row's reasoning assumed
the six-target set that was struck through on 2026-08-21 — most visibly the
Homebrew row, whose "Linux users still need another" **stopped being an argument
against anything on that date**. The table is kept unedited for exactly that
reason.

### npm was chosen against the recommendation

**Decided** by the distribution cycle. **The recommendation was** GitHub
Releases with prebuilt binaries plus a small install script that also merged the
`settings.json` keys. **The decision went the other way:** npm with platform
binaries — a wrapper whose `optionalDependencies` carried one package per target
— **so existing `ai-plugins` users kept the `pnpx` invocation they already
knew** and the repo did not have to own a shell installer, a build matrix *and*
a checksum story on day one. A Homebrew tap could still come later.

Every part of this was subsequently reversed. It is kept because the reversals
are only legible against it.

### Six targets → two

**Reversed 2026-08-21** (`macos-only`).

**Was:** six targets — macOS and Linux on both architectures, plus Windows,
which Claude Code runs on natively.

**Now:** two — macOS on both architectures, and nothing else. Three things the
six-target decision could not weigh at the time:

- **Four of the six were never verified.** `build:cross` proved architecture,
  not execution: **no Linux or Windows binary this repo produced was ever *run*
  by anyone.** Shipping them was shipping a claim nobody had checked.
- **The C toolchain problem arrived.** The first full build after the spend
  subsystem's TLS stack landed produced four of six, both Windows targets
  failing on a missing archiver. The mitigation was a preflight and a
  `--host-only` flag — **two features whose only purpose was making a partial
  build survivable.**
- **A platform costs more than a matrix row.** Windows alone was a `cargo-xwin`
  pin, an `llvm-lib` preflight, a `.exe` filename branch, a `USERPROFILE` home
  branch, a `chmod` guard, a parallel `cmd /C` test fixture module and two CI
  runners — **for a platform that cannot be tested here.**

Nothing had been published when this was decided, so **no user lost a
platform**. The narrowing was stated at three layers: the wrapper's
`"os": ["darwin"]` made npm refuse the install, the installer named the host it
would not serve before writing anything, and the readme led with it.

**The bar for adding a target back is set by this same decision and nothing
since has lowered it: a native runner that *builds and runs the suite* per
target.**

**One derived list is kept in step by hand.** `supported_targets()` is the
single source and nothing that can *derive* the list or its length may hard-code
it. Two things cannot derive it: the `build` **and** `test` matrices in the
release workflow — one runner per published target in each, because **a build
matrix that skips a target does not ship it, and a test matrix that skips one
ships it untested on its own architecture; both are failures and only the first
is caught** — and the crate, where a platform may need a `cfg` that macOS does
not.

### Two targets → one

**Decided 2026-08-22** (`release-fix`). The published set is **one** target:
`aarch64-apple-darwin`. Intel macOS is out.

**The reason is the ecosystem, not a preference.** pnpm — which this repo's own
task library depends on — publishes `darwin-arm64`, `linux-*` and `win32-*`
standalone binaries and **no macOS x64 build at any recent version**. CI could
not install its own tooling on an Intel runner, every binary backend failed
identically, and no backend swap fixes it. **A target whose build tooling has
abandoned it is a gap this repo would own indefinitely** — and it would be owned
to serve an architecture Apple stopped shipping in 2023.

### Linux was surveyed before being declined

**Decided 2026-08-22** (`release-fix`), and recorded at length **so that it
reads as a choice rather than an assumption.**

The survey came back **viable**:

- `from_keychain()` already guards on `cfg!(target_os = "macos")` and falls back
  to the credentials file, **so credentials degrade rather than break**.
- The crate's platform-specific spots are **Unix rather than Apple**, and are
  what blocks *Windows*, not Linux.
- `ureq` is pinned to rustls with baked roots **precisely so no `openssl-sys` is
  in the way.**

**What it costs** is two native runners, a glibc-versus-musl portability floor,
and **the end of local complete builds** — a Mac cross-compiles to another Apple
slice with a rustup target and cannot produce a Linux binary at all, so
`build:all` would stop being able to make a releasable set on a maintainer's
machine.

### The binary moved out of the npm package, then back in

Both halves, one day apart, because the second is only defensible against the
first.

**Decided 2026-08-22** (`github-artifacts`). The channel stayed npm; what moved
was the **bytes** — the binary became a GitHub Release asset that `--install`
downloaded, rather than the payload of one npm package per platform. **Three
published packages become one.**

The table's two rows were **not** alternatives after all: this took the artifact
half of the first and kept the entry point of the second, **which is why the
row's "you own a shell installer" cost did not apply** — there was no shell
installer, only the Node CLI that already existed.

**The standard objection did not apply either.** Fetching a binary from an npm
package is normally a `postinstall` hook, which `--ignore-scripts` suppresses
and a lockfile cannot vouch for. `--install` was a command the user typed, which
already wrote to `~/.claude` and `~/.config`; no package-manager setting
suppresses it and nothing about it was implicit.

**Integrity was anchored on npm, not on GitHub.** A release asset is mutable —
it can be deleted and re-uploaded at the same URL — **and an npm version is
not**. So `bin/checksums.json` shipped inside the package naming every target's
asset and its SHA-256, and the download was verified against it. A mismatch was
fatal and reported as itself, with an explicit instruction **not to retry: a
mismatch is not a flaky download.** The trust root did not move; GitHub was
reduced to a bytes-mover.

**Reversed the same day** (`release-fix`, second amendment). The binary travels
**inside** the npm package again, and the download path was **deleted, not
disabled**.

**The earlier reasoning was sound for the input it had.** With three published
packages, fetching bought one npm package instead of three, one Trusted
Publisher instead of three, and a first publish that did not have to reserve
three names by hand. **Every one of those is a benefit of not having
per-platform packages** — and cutting to one target delivered all of them for
free. So the download's entire upside evaporated and **only its costs
remained**: a required network call, air-gapped installs broken, `HTTPS_PROXY`
unhonoured by Node's `fetch`, a release that had to precede the npm publish, and
a digest manifest maintained against a mutable asset.

**The integrity argument inverts rather than weakens.** The download design's
central move was pinning digests in the immutable artifact because a release
asset can be re-uploaded at the same URL. **Embedding makes that problem not
exist**: there is no second artifact to distrust, and npm's own immutability is
the whole story.

**Two things from that cycle stayed.** One npm package — the per-platform
packages are gone and stay gone. And **the receipt records the binary's
digest**, which it never did before, so an uninstall applies to the binary the
same "edited since install" guard it already applied to the config.

**One version line, again.** `crate_version()` is "the single source of truth
for everything published … deliberately NOT duplicated into a package.json that
could drift". That stopped being true for one cycle, when the npm package
carried a hand-set `0.x` while the binary was `1.0.0` — **so one artifact
claimed two versions of itself.**

**Amended 2026-08-27** (`npm-installer`). The download path is live again, **on
the npm channel and nowhere else** — that channel is now an installer rather
than a carrier of bytes.

**Every one of the five costs listed above is real and every one is being
taken**: a required network call, air-gapped installs broken, `HTTPS_PROXY`
unhonoured by Node's `fetch`, a release that has to precede the npm publish, and
a digest pinned against a mutable asset. **What changed is the comparison, not
the costs.** The passage above weighs downloading against *embedding* and
concludes embedding is strictly better; that conclusion is not being contested,
because this is not a choice between the two. A third channel that embedded the
binary would be a fourth copy of the bytes with a fourth digest to keep true, so
there is no embedding option here to lose to.

**The integrity argument is taken up rather than dropped.** A release asset is
mutable and an npm version is not — which is precisely why the digest is pinned
**inside the published package**, written at publish time from that release's
own `SHA256SUMS`. The trust root is npm's immutability, the same shape as the
formula's `sha256`, which
[the standing credential entry](#the-standing-credential-is-a-github-app-and-what-that-does-and-does-not-buy)
calls the only thing standing between a user and substituted bytes.

**One of the five is paid by an ordering rather than by work.** "A release that
had to precede the npm publish" costs nothing to satisfy here: `publish-npm`
runs after the release job in the same workflow, which is where `bump-tap`
already runs for the same reason.

### Caps become config

**Decided 2026-08-23** (caps as config). The `--caps-hook` thresholds move into
the config file under `caps`, and gain a fourth: `spend`, a percent of the
account's monthly budget.

**Two things changed at once, and both are loosenings.** The caps used to be a
constant a repo could only *tighten*, **scraped out of `<cwd>/.config/vwf.yaml`
by a narrow line scan**. They now resolve through the ordinary layers, and the
YAML scrape is **deleted rather than kept as a second source** — one config
format, one resolution path, no second place a cap can come from.

The tighten-only rule existed so a repo could not raise its own limits, and the
code said so: reversing it *"would let a project silently disable its own safety
rail, which is the one failure mode of a config-driven cap that nobody would
notice."* **That remained true, and the tradeoff was taken anyway**: layer 3 is
a file you commit and review in your own repository, at the same trust level as
every other setting it already controls, and a caps key that behaved differently
from its neighbours was a surprise of its own. A user who wanted the old
guarantee got it by not writing `caps` into a repo config.

**Reversed 2026-08-23** (`config-relocation`), days later — `caps` is not
readable from layer 3 at all. See
[Layer 3 may set `projectName` and nothing else](#layer-3-may-set-projectname-and-nothing-else)
for the argument that undid it.

**`spend` is a percentage, not an amount**, because a budget is denominated in
the account's own currency and **only a percentage means the same thing on every
seat.** It is evaluated **before** the other three — level `4`, above the 7-day
`3` — because **a rate-limit window empties itself on a timer while an exhausted
budget needs somebody to act.**

Its figure does **not** come from the usage mirror, which a render writes from
the payload and which carries no spend data. It comes from the spend cache the
refresh child maintains, read as a local file: **the hook still never fetches,
exactly as a render never does.** A seat with no budget block yields `None`,
which never breaches — **not even against a cap of `0`.**

A key that is absent, negative, non-numeric or absurd **falls back to its
shipped default rather than being clamped**, so a config that failed to make
sense behaves like one that was never written. **`0` is a real cap** meaning
"breach on any usage at all", and is not mistaken for unset.

### npm is retired as a channel

**Decided 2026-08-23** (`drop-npm`), before it ever shipped a real version. The
release *is* the distribution: per target a `.tar.gz` with the binary at its
root, the raw binary beside it, and a `SHA256SUMS` covering both.

**The reason is the one the table's npm row understated.** "Awkward for a repo
with no other JS" turned out to mean **a second language in the tree for the
life of the project**: a TypeScript installer, its own test suite, `tsup`,
`pnpm`, a lockfile, a `tsconfig`, and node in the toolchain — **all of it to
deliver a Rust binary that needs none of it.** The thing npm bought was the
`pnpx` invocation existing `ai-plugins` users knew, and `--configure` had
already moved the wiring into the binary, **so installing from the tap and
running `--configure` replaces it with no Node anywhere.**

**The `ai-plugins` upgrade path is knowingly dropped.** Nothing now sweeps the
orphans a previous install left behind — the `statusline` script under
`~/.claude/scripts/`, the receipt under `~/.config/ai-plugins/receipts/`, and
`~/.claude/hooks/context-caps.js` — and any machine that ran the npm installer
keeps an orphaned receipt. **`--doctor` still reports a stale
`node …/context-caps.js` hook and `--configure` still replaces it, so the one
consequence that changes what renders is handled; the rest is litter.**

**Nothing gates the platform between that cycle and the tap.** The two gates —
the npm manifest's `os`/`cpu` and the installer's unsupported-platform message —
were deleted there, and the formula that replaces them came later. A
non-`darwin:arm64` user was told nothing in the meantime. **Accepted
deliberately rather than papered over with a runtime host check, because the
check would outlive the window and the binary would then be refusing to run on a
platform the formula had already refused to install.**

**Amended 2026-08-27** (`npm-installer`). npm is retired as *the* channel and
returns as a **third** one, beside the tap and mise:
`npx @virajp.dev/claude-status --install`. **The package carries no binary and
is never installed globally** — it is an installer and nothing else, and its
version, its tag and its digest all describe an artifact that lives on a GitHub
Release.

**What this section argued against was a build toolchain, and six of the seven
things it named do not come back.**

| Named above            | What returns                                                                                 |
| ---------------------- | -------------------------------------------------------------------------------------------- |
| a TypeScript installer | one `.mjs` — no types, no compile step                                                       |
| `tsup`                 | nothing; the published file is the tracked file                                              |
| `pnpm`                 | nothing                                                                                      |
| a lockfile             | nothing — there are zero runtime dependencies, so there is nothing to lock                   |
| a `tsconfig`           | nothing                                                                                      |
| node in the toolchain  | a **test-time** tool only, exactly as `tests/site.rs` already uses it                        |
| its own test suite     | **this one does come back** — as `tests/npm.rs`, inside the Rust suite rather than beside it |

**The seventh is the honest residual, and it is accepted rather than dismissed:
there is JavaScript in the tree again**, and it will need maintaining for the
life of the channel. What is bought against it is that the file has no build, no
dependency graph and no second `test` command — `mise run code:test` still runs
everything.

**What makes this defensible rather than a change of taste is that two of the
three things it rests on were delivered by this retirement itself, and the third
came two days after it.** `--configure` had already moved the wiring into the
binary — that is this section's own argument — so what is left for an installer
to do is *place a file*, not the work the retired installer was doing. The
release *is* the distribution, which this cycle made true, so the installer
consumes what already ships and adds no artifact of its own. And the tap proved
the pattern: `bump-tap` reads a digest out of a published release and pins it in
an immutable artifact, and `publish-npm` is that same job against a different
registry.

**The other half of the argument above is untouched.** What npm bought was the
`pnpx` invocation, and `--configure` did make it unnecessary. This is not a
replacement for the tap and nothing here argues that it should be; it is a route
for people who reach for `npx` before `brew`.

**The platform gate deleted here comes back on this channel.** The manifest's
`os`/`cpu` makes npm refuse with `EBADPLATFORM` before a line of the installer
runs, and the installer names the host it will not serve. The window recorded
just above as knowingly open closes for anyone arriving that way — **for that
way only**, which is why the formula, not this, is what closed it in general.

### The receipt discipline was never discharged

**Was decided** at the outset: whatever the channel, the installer must keep the
receipt discipline — record what was there before, so an uninstall *restores*
the `settings.json` keys it overwrote, **so the user's previous bar comes back
rather than being deleted and leaving them with none.** Files it wrote are
removed, not restored to some earlier name; **the two are different
obligations.** And replacing a statusline the installer did not write must
require explicit consent.

**Superseded 2026-08-23.** There is no installer, no receipt and no uninstall.
Consent for replacing a foreign statusline is **a warning on stderr rather than
a prompt**.

`--configure` shipped with no receipt and no `--unconfigure` **deliberately**: a
`statusLine` belonging to another tool is overwritten after the foreign value is
quoted back on stderr, which is the whole mitigation. **The obligation above is
recorded rather than deleted, because if a channel ever regains an uninstall the
reasoning is the reasoning.**

The verification this cost is worth naming: the distribution phase's check used
to end "then uninstall and confirm the tree is byte-identical to before".
**There is no uninstall to run, so that round trip is unverifiable rather than
merely unperformed.** The snapshot harness that would apply it survives in
`tests/e2e.rs`; what it has no counterpart for is the second half.

**Amended 2026-08-27** (`npm-installer`). A channel has regained an uninstall,
so the obligation kept above is claimed rather than merely recorded — **"the
reasoning is the reasoning" is being taken at its word.** The round trip called
unverifiable above is what makes it defensible to put `settings.json` editing in
a second language at all: it is pinned as a test, not asserted in prose. The
supersession stands everywhere else — the binary still ships no `--unconfigure`,
and a foreign `statusLine` is still a warning on stderr rather than a prompt.

### The Homebrew tap is the channel

**Decided 2026-08-25** (`distribution/02`). A formula in `virajp/homebrew-tap`
pinning the release's `.tar.gz`. **The options table's Homebrew row is the
answer, and its one argument against — "Linux users still need another" —
stopped applying on 2026-08-21.**

**The tap is a generated artefact and holds nothing authoritative.** The
formula's source is `.config/homebrew/claude-status.rb` in this repository; the
release workflow renders the **whole file** — substituting only `url` and
`sha256` — and overwrites the tap's copy every release. **So the tap cannot
drift, and the first release *creates* the formula rather than needing one
seeded by hand.**

That shape was chosen over the obvious alternative of hand-writing the first
formula and patching two fields thereafter. **The alternative leaves `desc`,
`homepage`, `caveats` and the `depends_on` pair living only in the tap, where
nothing in this repo's suite can see them**, and makes the first release depend
on a manual step somebody has to get right once. Neither cost is worth a patch
over a render.

### The install is three commands, and none is avoidable

**Corrected twice — 2026-08-26 and again 2026-08-27** — and the second
correction is the first one's own lesson landing a second time.

**Corrected 2026-08-27 to three commands**, in this order:

```sh
brew trust --formula virajp/tap/claude-status
brew tap virajp/tap
brew install --formula virajp/tap/claude-status
```

**Was (2026-08-26):** two commands, `brew trust --formula …` then
`brew install --formula …`, on the reasoning that a fully-qualified formula name
makes an explicit tap unnecessary.

**What the qualified name actually buys is `brew trust`, not `brew install`.**
`brew trust --formula virajp/tap/claude-status` succeeds against an untapped
repository — which is why the pair looked complete — and `brew install` then
reports it cannot find the formula. The tap has to be added between them.

**Found the same way as last time, and that is the point.** A colleague
installing on a clean machine hit it on 2026-08-27; nothing in the repo could
have. The owner's machine has `virajp/tap` tapped *and*
`virajp/tap/claude-status` in `~/.homebrew/trust.json`, so it cannot reproduce a
first install at all — and the previous correction had already written down why
that matters, in the paragraph below, without that being enough to prevent the
recurrence.

> **The generalisation, restated because it caught us twice: a claim about a
> first-run experience cannot be verified by anyone who has already had it — and
> knowing that does not help unless somebody with a clean machine actually runs
> it.**

The 2026-08-26 correction, kept in full because its shape is what repeated:

**Was:** "Only the fully-qualified form works. Homebrew 6.0.0 (2026-06-11)
requires explicit trust for non-official taps, so `brew tap virajp/tap` followed
by `brew install claude-status` **fails** until `brew trust`. Documentation must
give the one fully-qualified command and not the pair."

**Corrected:** the install is **two** commands —
`brew trust --formula virajp/tap/claude-status`, then
`brew install --formula virajp/tap/claude-status`.

The original had **the shape of the problem right and the conclusion wrong**.
Homebrew 6 does require explicit trust — but trust is required for the
**formula**, not merely for the tap, so qualifying the name fully does not avoid
it. **There is no one-command install.**

This was found by the owner installing it, not by anything in the repo. The
evidence is `~/.homebrew/trust.json`, which lists the formula under
`trustedformulae` — an entry only `brew trust --formula` writes. The original
claim came from **a recon agent that reasoned about tap trust without running
the install**, and nothing since checked it, **because the person who could
check it had already trusted the formula and never saw the failure.**

> **The generalisation worth keeping: a claim about a first-run experience
> cannot be verified by anyone who has already had it. The state that makes the
> install work is the state that hides the bug.**

### The formula's url and digest are read out of the published release

**Decided 2026-08-25** (`distribution/02`), never reconstructed from a version
string.

**A formula naming an asset that does not exist passes every gate there is** —
plain `brew audit` does not fetch the url, and `brew audit --strict` was
measured **exiting 0 against a url returning 404** — so it would surface first
as a failed `brew install` in front of the first user.

The bump job asks GitHub what the release actually carries and takes both values
from the answer, **which is why there is no assertion guarding the asset name:
there is no name to get wrong.** The digest is read from that release's
`SHA256SUMS`, matched on the whole asset name, and **a miss is fatal rather than
empty.**

### The standing credential is a GitHub App, and what that does and does not buy

**Decided 2026-08-25** (`distribution/02`). `distribution/01` had removed the
last standing credential by deleting OIDC's consumer; pushing a formula into
another repository needs one, because `GITHUB_TOKEN` cannot reach outside this
repo.

It is a **GitHub App** — `APP_ID` (the App's *Client ID*, not the numeric App
ID) and `APP_KEY` — and **what is at rest only authorises *minting***: the token
itself is scoped to the tap repository, narrowed to `contents: write`, valid for
an hour, and revoked when the job ends. **A PAT would have been account-wide and
a deploy key would have pushed to the tap forever.**

**That is a reduction in duration and breadth, not immunity**, and the residual
is stated rather than implied: the App's private key does not expire and nothing
prompts a rotation; and **a live compromise has exactly the scope needed to
publish a malicious formula, which is the scope that matters to someone running
`brew install`.** The formula's `sha256` is the only thing standing between a
user and substituted bytes, because a release asset is mutable.
`reproducible_tar` is what makes re-running a tag safe.

### The npm name is not reclaimed, and needs nothing

**Decided 2026-08-23** (`drop-npm`), **closed 2026-08-25** (`distribution/02`).
The package name stays as it is; unpublishing is neither possible nor wanted.

An authenticated fetch was made and returns 404, so **there is nothing on the
registry to deprecate** and the `npm deprecate` step was **cut rather than
deferred**. Whether the name ever carried a published version **is still not
decidable from outside** — an unpublished package 404s the same way — and
nothing turns on the difference. The name is unreserved; claiming it is a
separate decision nobody has made.

**Amended 2026-08-27** (`npm-installer`). **The name stays unreclaimed, and the
third channel publishes under a different one:** `@virajp.dev/claude-status`.

**An earlier draft of this amendment said the opposite** — that this cycle
claimed `@askviraj/claude-status` — and it is corrected rather than deleted,
because what changed the answer is worth keeping. The publish was attempted and
returned `E404` on the `PUT`; **npm answers an unauthorised publish of a scoped
name with 404 rather than 401**, deliberately, so that the response cannot be
used to probe whether a private package exists. The 404 said nothing about the
name. What it meant was that nobody was logged in.

**The scope is the constraint, not the name.** A scoped package may only be
published under a scope its publisher owns, so the choice was never between
strings — it was between accounts. `@virajp.dev` matches the domain the site
already serves from, which is the one identifier this project had already
committed to. Unscoped `claude-status` was measured **taken** on 2026-08-27 and
was never available to take.

**A dotted scope is not exotic**, which is worth recording because it looks like
it should be: `@socket.io/component-emitter` resolves, so npm permits a dot in
an org name and this is not a novel bet.

**The measurement above stands as measured.** The authenticated fetch did return
404, and whether `@askviraj/claude-status` ever carried a published version is
still not decidable from outside. Nothing here makes it more knowable — and
nothing now depends on it, which is the real change: **a name nobody publishes
to cannot block a release.**

**Two things this needs cannot be done by a commit in this repository**, and
both have to land before the next tag or `publish-npm` fails a release that is
otherwise fine: the scope has to exist on npmjs.com, and Trusted Publishing has
to be configured for that repo and that job. Whether npm will attach a trusted
publisher to a package that does not yet exist is **not established here** — if
it will not, the first publish is manual or token-authenticated and OIDC takes
over from the second. Whoever finds out records the answer in this section.

### The install receipt is state, and is neither config nor cache

**Decided 2026-08-27** (`npm-installer`). The npm installer's receipt lives at
`~/.local/state/claude-status/install-receipt.json` — a third directory, chosen
against both of the two this project already uses.

**Not `~/.config/claude-status/`.** That is the directory people commit to a
dotfiles repo and sync between machines —
[the cache does not move, and the split is the point](#the-cache-does-not-move-and-the-split-is-the-point)
— and a receipt names *this machine's* install path. Synced, it would arrive on
the second machine asserting a binary that is not there, which is worse than
arriving with no receipt at all: **the guard that refuses to touch a binary this
installer did not place would be reasoning from a fiction**, on the one machine
where nobody would think to check.

**Not `~/.cache/claude-status/` either.** A cache is regenerable by definition,
and clearing one is a supported thing for a user to do; a receipt is neither
regenerable nor safe to lose. **Clearing a cache must not strand the
uninstall.**

### A package runner's own shim is not an installed binary

**Fixed 2026-08-28** (`installer-shim`), reported from a real run.

`npx`, `pnpx` and `bunx` put the package's own `node_modules/.bin` at the front
of `PATH` before running it, and this package declares a bin named
`claude-status`. So `which claude-status` found **the shim belonging to the
process doing the looking**, `classifyExisting` called it `unknown`, and
`--install` refused to replace it:

```text
claude-status: /…/pnpm/dlx/3aa68349…/node_modules/.bin/claude-status was not
placed by this installer, or has changed since it was
```

**The npx route was broken for every user**, including one with nothing
installed at all, and the message named a path inside a cache directory they had
never heard of. It is the failure mode `--force` exists for, fired at a file
nobody asked to touch.

`locate` now scans `which -a` and takes the first hit **without** a
`node_modules` path segment. The rule is safe for a reason rather than by luck:
`chooseInstallDir` never selects a directory under `node_modules` — only
`~/.local/bin` or `~/bin` — so a `claude-status` found in one was never placed
by this installer and is never a destination it would choose. Both the raw
`which` hit and its realpath are tested, because the runners disagree about
which one lands in `node_modules`: npm symlinks into the package, pnpm writes a
real shim and leaves the realpath alone. Segment, not substring, on the same
terms as `Cellar`.

The test's **controls carry it**: a rule that answered "yes" to everything would
pass the shim cases and silently disable the protection over a Homebrew binary,
which is worse than the bug it fixes.

**`-a` is the second half, and 1.1.6 shipped without it.** The first attempt
read one line from `which` and returned null on finding a shim — but `which`
reports only the FIRST match, and under a runner the shim is always first. So on
a machine with a Homebrew install two entries further down, the installer
reported nothing installed and placed a **second** `claude-status` in
`~/.local/bin`, shadowed by the first: exactly the two-channels-fighting case
`classifyExisting` exists to prevent, reintroduced by the fix for the shim.

**Only running the real command found it.** The unit test passed, the suite
passed, and the published 1.1.6 was wrong.
`npx @virajp.dev/claude-status
--install` on a machine with a brew install is
the check that fails, and it is worth writing down that no amount of the tests
above substituted for it.

### The refusal says what was found, why, and what to type

**Decided 2026-08-28** (`installer-shim`). The wording above opened with an
absolute path and a passive clause that trailed off mid-sentence — "or has
changed since it was" — which reads as a fault in the tool rather than a
decision it took deliberately. It now names the file on its own line, says this
installer did not place it, and ends with the flag to re-run with.

### The installer's `--help` is an index too

**Decided 2026-08-28** (`installer-shim`), applying
[`--help` is an index, not the documentation](#--help-is-an-index-not-the-documentation--reversing-criterion-7)
to the surface it was missed on. The binary's help was cut the same day and the
installer's was left at fifty-eight lines, carrying `WHAT --install DOES` and
`WHAT --uninstall DOES`: where the binary lands, the digest check, the receipt
path, the three settings keys, what an uninstall leaves behind.

All of it is on the website, which can format a table and be corrected without a
release. **The receipt path was the one fact with no other home**, so it was
added to the site before it was cut from here — the same rule the binary's cut
followed.

Twenty-four lines now. The flag list stays complete because
`every_flag_the_help_lists_is_a_flag_the_parser_accepts` scans this text and
needs at least six flags to be sure it is reading the right thing; the six the
parser accepts are exactly the six listed.

### Consent to `--configure` has three states, and the third is a decline

**Decided 2026-08-27** (`npm-installer`). `--install --configure` runs it,
`--install --no-configure` skips it, and with neither flag the installer prompts
on a TTY and skips silently where there is none.

**`--no-configure` exists because a script must be able to decline as explicitly
as it consents.** Without it there are two ways to finish unwired — declining,
and running with no TTY — and **from outside the process they are
indistinguishable**. So a CI job that meant to decline has no way to say so, and
one that meant to consent cannot tell that it did not. The decline is a decline
and not a failure: it exits 0 and prints the one line the user would otherwise
have to be told.

**The two flags together are refused rather than ranked.** No precedence, no
last-one-wins.
[`--configure` is the one mode that rejects an unrecognised argument](#--configure-is-the-one-mode-that-rejects-an-unrecognised-argument)
already, for the reason that it writes to a file this tool does not own — and a
contradiction resolved silently is how a script ends up doing the opposite of
what it says on its own command line.

### GitHub raw is not an address this project publishes

**Decided 2026-08-30**, consolidating a rule that had been taken three times in
three places and written out at every one of them. The prose was removed from
those files on the same day: a rejected option explained six times over is six
copies of one argument, and five of them sit in files a user can open.

`raw.githubusercontent.com` serves the current file from `main`, sends
`access-control-allow-origin: *`, and costs nothing to use. It was therefore the
first answer every time this project needed a URL for a file in this repository,
and it has now been declined three times:

- **The npm readme's images.** npmjs.com renders the readme from the published
  tarball and cannot serve a file out of it, so the images have to be absolute.
  They named a release tag on GitHub raw and were rewritten per publish, on the
  argument that an npm version is immutable and its readme should be too.
- **The config generator's two inputs.** The form is built from the schema and
  the shipped defaults, and fetching them from `main` at runtime would have
  worked.
- **The agent install runbook.** `install.md` is written to be fetched rather
  than browsed; both readmes and the install page print a prompt whose entire
  payload is one URL, and an agent reads whatever is at it.

**The operational reason is the same one each time: that host has been going
down under load.** A readme that will not render is worse than one showing a
newer screenshot than its own release, a documentation page that stops working
when GitHub is down is worse than one that is a tag behind, and a prompt that
resolves to nothing is a user who concludes the project is broken before they
have installed it. The generator carried two further objections of its own — a
corporate proxy blocking the host, or a reader on a plane, against a site every
other page of which is readable offline once loaded.

**And in two of the three the address cannot be taken back.** A published npm
version is immutable: the readme inside `@virajp.dev/claude-status@1.2.3` names
its URLs forever, and nothing this repository does later can re-point them. That
is what turns a hosting choice into a permanent one.

**What replaced it settled into one shape.** The site serves all of them.
`site:assets` stages each file into `site/static/` as build output — gitignored
and excluded from dprint, because dprint's `includes` covers `**/*.json` and
`**/*.md` and would reformat a tracked copy into a file that no longer matches
its source with nothing to say so. None of them is fingerprinted: `site:build`
fingerprints an explicit list of names, so a stable address holds by
construction rather than by a rule someone remembers, and `_headers` leaves them
to `/*` and `must-revalidate`, which is what an address whose bytes may change
requires. The images live at `/media/`, the runbook at `/install.md`.

**The tradeoff travelled with them, and is taken knowingly.** This site deploys
on a `site-v*` tag, so everything it serves is pinned at the last tag while raw
served `main` live. Every published readme now shows the *current* artwork
rather than its own release's, and an edit to `install.md` does not reach the
deployed copy until the next site tag. Being uniformly one tag behind is a
smaller lie than being internally inconsistent, and both are smaller than an
address that does not answer.

**What still names raw, recorded as open rather than settled.** `SCHEMA_URL` in
`src/modules/config/write.rs` is a raw URL, and `--configure` writes it as
`"$schema"` into every `~/.config/claude-status/config.json` it creates; the
schema's own `$id`, the shipped defaults, and the config examples on three site
pages repeat it. A user's editor fetches it, so the outage argument applies
unchanged. Moving it has a cost the other three did not: a schema served from
this site is one tag behind the binary that just wrote the config being
validated against it, and the two disagreeing is a red squiggle under a key that
is correct. The site already stages and serves the schema, so the move is
available whenever that cost is judged worth paying.

### The installer's version split from the binary's

**Reversed 2026-08-30.** The npm package now carries its own version and ships
on its own tag line.

**Was:** one version, and it was the crate's. `npm/package.json` was stamped
from `crate_version()` at publish time, and a test pinned the tracked manifest
to it in between. The concrete failure behind that rule is recorded above: the
package once carried a hand-set `0.x` while the binary reported `1.0.0`, so
**one artifact claimed two versions of itself.**

**That argument was about an artifact that no longer exists.** It was correct
while the package *carried* the binary — one shipped thing, one number. Since
`npm-installer` the package carries no bytes: it downloads a release asset. The
installer and the binary are two artifacts with two changelogs, and pinning
their numbers together stopped describing anything true.

**The cost was measured, not argued.** `v1.1.6`, `v1.1.7` and `v1.1.8` each
changed **zero files under `src/`**. Three consecutive binary releases rebuilt,
re-uploaded, re-tapped and re-announced a binary whose source had not moved,
because the only way to ship an installer fix was to release the binary again.
Every `brew upgrade` user was told there was a new version of something that had
not changed.

**The split is enforced by derivation rather than by agreement.** `install.mjs`
no longer reads its own `package.json` at all — the version it reports, checks
the downloaded binary against, and writes into the receipt is `INSTALLS`, which
is `ASSET.tag` without its leading `v`. Every one of those uses always meant the
binary; they read the package's number only because the two were pinned equal. A
rule about what the file may *read* survives the two numbers happening to
coincide, which they do today and would pass either way, so
`the_installed_version_is_the_assets_and_not_the_packages` asserts the source
and not the values.

**Both tag lines live in one workflow file, and this is not a style choice.**
npm's trusted publishing binds a registration to a repository **and a workflow
filename**; the OIDC token is minted against that pair. A second workflow
running `npm publish` could not authenticate at all, however correct its YAML.
The first cut of this change was a separate `npm.yml` and would have failed on
its first tag. `npm_publishing_jobs` asserts there is exactly one such job
anywhere in `.github/workflows/`, so the mistake cannot be made twice.

**What a binary release costs now: two bumps rather than one.** npm cannot
republish a version, and a new binary is no use on the npx channel until the
package points at it — so `Cargo.toml` and `npm/package.json` both move for a
`v*` tag. Forgetting is a refused publish against a release that is already
complete, which is recoverable with an `npm-v` tag and loses nothing.

**A guard that could not fail was found by trying to make it fail.** The
dispatch guard on the publishing job asserted the body contained the string
`REF_TYPE` — which also appears in the step's own `env:` block, so gutting the
comparison to `if false` left the test green. It now matches the comparison and
the `exit 1`, verified by doing exactly that and watching it go red. The same
shape as the eleven-line comment above the failing step in `ci.yml`: a scan that
reads what a file *says* rather than what it *does* is not a guard.

### Still unowned: code signing and notarisation

**Recorded 2026-08-25** (`distribution/02`) as an open risk rather than a
decision. A brew-installed binary is downloaded rather than built, exactly as
npm's would have been, but **"Homebrew installed it" makes people assume it is
signed. It is not.**

**Amended 2026-08-27** (`npm-installer`). The risk is unchanged and now spans
**two channels rather than one**. `fetch` does not set the
`com.apple.quarantine` xattr, so a binary the npm installer places runs without
Gatekeeper stopping it — **which removes the symptom rather than the risk**, and
with it the one moment a user might have been told the binary is unsigned.

### Still unowned: the proxy and the air gap, on the npm channel

**Recorded 2026-08-27** (`npm-installer`) as open risks rather than decisions.
Both are inherent to a channel that downloads, and neither is fixed here.

**`HTTPS_PROXY` is not honoured.** Node's `fetch` ignores it. This is one of the
costs listed above as a reason the download path was deleted, and **it returns
with this channel**; a user behind a proxy installs from the tap.

**Air-gapped installs do not work on this channel.** Inherent — the package
carries no bytes. The tap does.

---

## 12. Testing

### Three layers, and the middle one is the one people skip

**Decided** at the outset. Unit tests for the pure logic; **golden renders** —
fixture payload in, exact expected ANSI string out, which is **what catches a
separator regression or an off-by-one in the gauge, and nothing else will**; and
end-to-end runs of the built binary as a subprocess with a fake `$HOME`, which
is how Claude Code invokes it.

### Assert that no diagnostic reaches stdout

**Decided** at the outset. Capture both streams separately and check stdout
holds only the bar. **This is the invariant most likely to regress and least
likely to be noticed** — invariant 1 has no other guard.

### Making a previously inert path live can turn a passing test into a live-fetch test, silently

**Recorded** at the outset as a hazard to design around. When a `match` arm
stops being a stub, **re-audit the *unit* tests too, not just the integration
ones** — an in-process test that calls the dispatcher is one arm away from a
real request, **and nothing about it will look different when it starts
fetching.**

The keychain half of this hazard is invariant 5's; see
[The same asymmetry](#the-same-asymmetry-is-why-a-fake-home-does-not-make-a-test-safe).

### The spend subsystem is exercised against a stub server

**Decided** at the outset: exercise 200 with a `spend` block, 200 with
`extra_usage`, 200 with neither, 401, 429, and a connection refusal — **against
a stub HTTP server, not the real endpoint.** Then one real `--doctor` to confirm
the credential path works on a live machine.

---

## 13. Scope

### `context-caps.js` moved into this binary

**Reversed 2026-08-21** (`caps-hook`).

**Was:** `context-caps.js` stays in `ai-plugins`. It is vwf policy, not
statusline behaviour, and this repo's obligation to it is the usage mirror and
nothing more.

**The ownership argument is sound and never addressed the performance one.** The
hook is wired as `node ${HOME}/.claude/hooks/context-caps.js` on `PostToolUse`,
so it paid Node's startup **after every tool call** — measured at **28.6 ms**
against this binary's **2.8 ms** for the same work. It is now `--caps-hook`, one
more mode on the same binary.

**vwf still owns the *policy*:** the caps, the thresholds and the directive
wording live in `vwf.yaml` and in the vwf skills, and **this binary only
actuates them.** The reversal moved the actuator, not the ownership.

### The four retired render targets were not ported

**Decided** at the outset. OpenCode, Oh-My-Pi and Cursor, and every trace of
them: **Cursor never had a status surface; the other two are discontinued.**

---

## 14. Amendments that carried no decision

The retired contract was maintained by amendment blocks, and not every block
recorded a decision — some corrected a claim the document had got wrong about
behaviour that never changed. Those are listed here rather than transcribed,
**so that nothing was dropped without saying so**, with where the behaviour
actually lives now.

- **Four segment rows were wrong** (corrected 2026-08-19, `main-bar`, against
  the reference builders): the space before the reset glyph on the rate-limit
  segments, `duration` omitting only when its field is *absent* so an explicit
  `0` renders, the order of the two glyphs on `branch`, and `context` rendering
  a full empty gauge with no data at all. Every one is held by the goldens —
  which is the point: **a table in prose was wrong for months and an exact-ANSI
  golden could not have been.**
- **The defaults were never "seeded" anywhere** (corrected 2026-08-23,
  `config-relocation`). The document had claimed the shipped defaults were
  written into a user config at a path that does not exist. Nothing in the
  binary seeds anything; the asset is the embedded lowest merge layer. The
  decision behind that is
  [The defaults are embedded in the binary](#the-defaults-are-embedded-in-the-binary),
  and the correction added nothing to it. The same block also withdrew the
  "byte-faithful copy" description of the defaults asset, which had stopped
  being literally true — a statement of fact, not a decision.

---

## 15. Continuous integration

**Not harvested.** Sections 1–14 came out of the retired behaviour contract;
this one did not, because CI did not exist when that document did. The workflows
carry their own reasoning at length in comments beside the steps — what belongs
here is the part a comment cannot hold, which is why a choice changed.

### The Node 20 deprecation was answered with a floor, not a pin

**Decided 2026-08-27.**

`actions/upload-artifact@v4`, `actions/download-artifact@v4` and
`cloudflare/wrangler-action@v3` all declared `runs.using: node20`. GitHub warns
on every step that uses one and will eventually stop running them — which would
break `release.yml` and `site.yml`, the two workflows that ship anything, and
neither of which runs on a pull request. Nothing would have surfaced it before a
tag.

**The floors were read out of each action's own `action.yml`, not out of its
release notes**, and the notes would have misled: `upload-artifact@v5` announces
Node 24 support while its `action.yml` still says `node20`. The first major that
actually runs on Node 24 is v6 for `upload-artifact`, v7 for `download-artifact`
and v4 for `wrangler-action`. The repository took the newest major of each — v7,
v8, v4 — rather than the oldest that clears the bar.

**`tests/workflows.rs` pins a floor rather than the exact major**, so a routine
upgrade does not have to argue with the test. The cost, taken knowingly: a *new*
action introduced on `node20` is not caught, because the table names actions
rather than enumerating them.

### The artifact layout is decided here, not by `download-artifact`

**Decided 2026-08-27**, after it broke the second `v1.1.0` attempt.

`publish` downloaded the build artifacts with `pattern: binary-*` and let the
action choose the directory structure. Under `download-artifact@v4` a pattern
always nested each artifact under its own name, so `_rust_reassemble` read
`artifacts/binary-<target>/claude-status`. **v8 changed that for a pattern
matching exactly one artifact** — its README says so plainly, "this change also
applies to patterns that only match a single artifact" — and this repository
publishes exactly one target, so the bump moved every binary up a directory.
`publish` died on `cp: cannot stat`, after `verify`, `test` and `build` had all
gone green and spent their minutes.

**The default was a trap rather than merely a breakage.** Matching today's shape
would have worked until a *second* target was added, at which point the layout
flips back to nested — breaking the release on the change `supported_targets`
exists to make a one-liner, and breaking it in the job that runs last.

**So the layout is now this repository's decision.** `build` stages each binary
as `claude-status-<target>` and `publish` passes `merge-multiple: true`, which
flattens unconditionally. The two halves are one decision: flattening is only
safe because the filenames are unique, and with the old
`claude-status`-for-everyone naming it would have silently overwritten one
target's binary with another's.

**What actually let this reach a tag** is that nothing ran `_rust_reassemble`.
It is the only step that creates the path every downstream asset is built from,
its input came from a third party, and the sole way to discover it disagreed
with that third party was to cut a release.
`reassembles_the_layout_the_release_workflow_downloads` now runs it against the
layout the workflow produces — confirmed to reproduce the exact `cp` failure
when pointed back at the v4 path.

### A fix to one workflow is not a fix, and v1.1.0 proved it

**Decided 2026-08-27**, after the release it cost.

`ci.yml` gained an explicit `rustup component add clippy` earlier the same day,
because dropping `install_args` had made rustup's `minimal`-profile resync rarer
without making it impossible. **`release.yml` has the same lint step and did not
get the same line.** The very next tag, `v1.1.0`, failed in its `test` job with
`'cargo-clippy' is not installed for the toolchain '1.98.0'`, having just logged
"downloading 3 components". `publish` and `bump-tap` were skipped, so no assets
were published and the tap was never rewritten.

**The exposure was worse in the workflow that was left unfixed**, which is the
part worth remembering. `ci.yml` already records the trade — a random failure on
a pull request costs a re-run, the same failure on a tag costs the release — and
the fix went to the cheap side first because that is where the flake had last
been *seen*. Where a bug is observed and where it is expensive are different
questions.

**`every_job_that_lints_installs_clippy_first` now holds it across every
workflow**, keyed on any job running `code:lint` or `code:all`. A per-file guard
would have reproduced the original mistake: the failure mode here is not "a
workflow is wrong", it is "the workflows disagree", and only a test that reads
all of them can see that. It is why `tests/workflows.rs` is organised by
invariant rather than by file.

**The eleven-line comment above the failing step named the exact symptom.** It
had described the `minimal`-profile resync, in that job, since before the run —
and the job still failed on it, because a comment is not a step. That is why the
guard strips comments before scanning.

### The download-artifact ignore rule was deleted, and its reasoning was wrong

**Reversed 2026-08-27**, the same day it was taken.

**Was:** `.config/grype.yaml` suppressed GHSA-cxww-7g56-2vh6 against
`actions/download-artifact`, on the grounds that `@v4` is a mutable major tag
resolving far past the 4.1.3 the advisory names, and that grype **"reads the
literal string `v4`, cannot order it against `4.1.3`, and reports the advisory
as unfixed"**. Its stated expiry was pinning the actions to exact versions or
SHAs.

**The conclusion was right and the mechanism was not.** grype orders the tag
perfectly well — it strips the `v` and compares `4` against `4.1.3`, which is
genuinely lower, and that is why it matched. Nothing was unorderable. The
finding was a false positive, but not for the reason the rule gave, and a rule
that misdescribes the tool it works around is a rule nobody can re-check.

**What retired it had nothing to do with its stated expiry.** Moving the
workflows to `@v8` for the Node 24 reason above left grype comparing `8` against
`4.1.3` and reporting nothing — **zero matches against the whole tree**,
measured before and after the bump rather than argued. The actions remain on
floating major tags; that choice was re-affirmed on 2026-08-27 and is unchanged,
so the pinning the rule named as its expiry never happened.

**The general point is the one worth keeping:** the rule's expiry condition was
written as though it were the only way the rule could become wrong, and the rule
was retired by a route nobody had listed. An ignore rule earns its place by
being re-checked, not by carrying a condition.

---

## Provenance

Harvested 2026-08-26 from the behaviour contract
(`docs/spec/statusline-behaviour.md`, since deleted; git history holds it),
working through its amendment blocks section by section rather than from memory,
as step 2 of the `retire-the-spec` cycle. The spec was deleted four steps later,
deliberately — **this file had to exist first, because reasoning is not
recoverable from tests the way behaviour is.** Both documents remain in git
history.
