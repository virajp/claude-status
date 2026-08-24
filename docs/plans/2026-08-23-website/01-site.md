---
type: vwf-plan
title: site — 2026-08-23
description: Cycle plan (a diff) building claude-status.virajp.dev as a Zola
  static site in site/, deployed to Cloudflare Pages from Actions on a site-v*
  tag, with the readme shrinking to a pointer.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: []
timestamp: 2026-08-23T14:21:00Z
tags: [ website, zola, cloudflare, docs, readme, ci ]
---

# Plan: site — 2026-08-23

## Slice

A new surface: `claude-status.virajp.dev`. Source in `site/`, built by Zola,
deployed to Cloudflare Pages by `wrangler` from GitHub Actions on a `site-v*`
tag.

It carries the landing page, the docs, and — after
[plan 2](./02-config-generator.md) — the config generator.

## Current state (actual)

**There is no site and no `site/` directory.**

**`readme.md` was rewritten as an npm package listing page** by the archived
`release-fix/04-readme-for-npm` cycle: Requirements, Install, Uninstall,
Configuration, Diagnosing, Environment, Licence, with contributor material split
out to `CONTRIBUTING.md`. Its audience — someone reading a registry page — stops
existing when [distribution/01](../2026-08-23-distribution/01-drop-npm.md)
deletes npm.

**The docs that exist are `docs/spec/statusline-behaviour.md`** — a 1,300-line
behaviour contract written for implementers, not users. It is the source of
truth and is not user documentation.

**The repo is about to become single-language.**
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) removes Node with
the npm installer. Any site build that needs `node_modules` puts it straight
back.

**Two things will point here before it exists**: the formula's caveats
([distribution/02](../2026-08-23-distribution/02-homebrew-formula.md)) and
`--help` ([config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md)).

## Target state (per contract)

A static site at `claude-status.virajp.dev` that a user reaching it from
`brew install` can read end to end: what the bar shows, how to wire it, how to
configure it, and how to write a repo-level config. `readme.md` becomes a
pointer at it.

## Delta — ordered steps

### 1. Scaffold `site/` with Zola

`zola` pinned in `.config/mise.toml` beside the Rust toolchain.
`site/config.toml`, `site/content/`, `site/templates/`, `site/static/`.

**No CSS framework, no JS framework, no bundler.** Hand-written CSS in one
stylesheet. The constraint is not aesthetic minimalism — it is that this repo
just removed its only non-Rust toolchain and should not acquire another.

### 2. Build the landing page

What the bar is, a screenshot, the two install commands, and the reason to care.
It is the first thing a `brew info` reader sees.

**The screenshot is the honest problem here.** A powerline bar with Nerd Font
glyphs does not survive being described in prose, and the reference render in
`tests/golden/fixture.txt` is raw ANSI. Ship a real terminal screenshot, and
record how it was produced so it can be regenerated when the defaults change —
an out-of-date screenshot on a landing page is a slow lie.

### 3. Write the docs pages

Install, configure, the config reference, diagnosing, and the repo-level layer.

The **repo layer needs real prose**: after
[config-and-cli/02](../2026-08-23-config-and-cli/02-config-relocation.md)
deletes the autoseed, a repo config exists only if a human writes one, and its
only discovery routes are `--help` and this page. Give the exact path, the fact
that `projectName` is the only supported key, an example file, and what happens
to other keys (ignored, reported by `--debug`).

**Do not restate the behaviour contract.** Link to it for implementers. A user
page that tries to be complete about a 1,300-line spec ends up wrong in a way
nobody notices.

### 4. Ship the two credentials-free things first

The site must be useful before the generator exists. Pages 2 and 3 plus a `404`,
a `CNAME`-equivalent custom-domain setup, and nothing dynamic.

### 5. Deploy from Actions with wrangler

`.github/workflows/site.yml`, triggered on `site-v*`:

```text
site-v* tag → zola build → wrangler pages deploy site/public
```

`CLOUDFLARE_API_TOKEN` and the account id as repository secrets, scoped to the
Pages project alone.

**Not Cloudflare's git integration**, which deploys on branch pushes and cannot
be gated on a tag. The separate tag line is what lets a docs typo ship without
cutting a binary release.

### 6. Add a build check on PRs

`zola build` on every PR touching `site/`, without deploying. A site that only
builds at tag time fails at the worst moment — when you are trying to ship a
fix.

### 7. Shrink `readme.md` to a pointer

What it is, the screenshot, the two install commands, links to the site for docs
and configuration, and the licence. Everything else moves to the site.

**One canonical copy.** The alternative — a complete readme mirrored by a
complete site — is two documents that will disagree within a month, and the
disagreement will be discovered by a user.

`CONTRIBUTING.md` stays as it is; it is for people in the repo, who are not the
site's audience.

### 8. Docs

The behaviour contract gains a line naming the site as the user-facing
documentation and `readme.md` as a pointer, so a later reader knows where user
docs are supposed to live.

## Acceptance criteria (from contract)

1. Given a clean checkout with mise, when `mise run site:build` runs, then the
   site builds with **no `node_modules` and no lockfile** anywhere in the tree.
2. Given a `site-v*` tag, when the workflow runs, then the site is live at
   `claude-status.virajp.dev` over HTTPS.
3. Given a `v*` binary tag, then **no** site deploy is triggered.
4. Given a PR touching `site/`, then the site build runs and no deploy happens.
5. Given the docs pages, then they state the repo-config path, that
   `projectName` is its only key, and show an example.
6. Given the site, when every internal link is crawled, then none 404.
7. Given `readme.md`, then it is a pointer — install commands and links — and
   contains no configuration reference that the site also carries.
8. Given the site on a phone, then it is readable and the nav works.

## Risks / drift

**The formula's caveats may point here before this ships.**
[distribution/02](../2026-08-23-distribution/02-homebrew-formula.md) has no
dependency on this plan, so the tap can land first and print a dead link as a
user's first impression. Sequence them, or land a single-page placeholder at the
domain early. This is the most likely thing to actually go wrong.

**Two docs that both look canonical.** Step 7 is the mitigation and it needs to
be done properly — a readme that keeps "just a short configuration section" is
how the split starts. If it belongs on the site, it is not in the readme.

**A screenshot is a doc that never fails a test.** When the defaults change, the
build stays green and the landing page quietly shows a bar the tool no longer
renders. Recording how it was produced (step 2) is the minimum; regenerating it
in the release checklist would be better and is not planned here.

**Cloudflare credentials are the second long-lived secret in the repo.**
`distribution/01` removes the last one, `distribution/02` adds the tap token,
and this adds two more values. Scope the API token to the Pages project only —
an account-wide Cloudflare token is a much larger thing to leak than a tap push
token.

**`site-v*` versioning has no obvious meaning.** The site does not have features
to version. Decide during execution whether it is a date, a counter, or mirrors
the binary version — and write it down, because "what do I tag to ship a typo
fix" is a question that recurs monthly.

## Out of scope for this cycle

- **The config generator and the bar preview.**
  [Plan 2](./02-config-generator.md), which also owns the CI fixture gate.
- **Analytics, search, a blog, versioned docs.** None is needed for a
  single-binary tool with one supported platform.
- **Generating the readme from the site's markdown.** Considered and rejected:
  step 7 makes the readme small enough that keeping it by hand is cheaper than a
  generation step and a drift check.
- **Translating or hosting the behaviour contract.** It stays in the repo and is
  linked, not mirrored.

## Gaps surfaced during execution

### The cycle did not deploy, and criterion 2 is deferred

**Scope was narrowed before execution: build the site and the workflow, deploy
nothing.** `.github/workflows/site.yml` is authored in full, deploy job
included, and has never run. No Cloudflare resource was created, no repository
secret was added, no tag was pushed.

**Criterion 2 — "live at `claude-status.virajp.dev` over HTTPS" — is deferred**,
in the way `distribution/03` inherited its open criteria. The evidence, taken at
execution time:

- `curl` exits 6 (could not resolve host) on both `claude-status.virajp.dev` and
  `virajp.dev`
- `dig +short A` is empty for both
- `dig NS virajp.dev` does resolve — the zone **is** on Cloudflare
  (`celine.ns.cloudflare.com`, `harvey.ns.cloudflare.com`)
- `gh api repos/virajp/claude-status/actions/secrets` returns `total_count: 0`

So the DNS zone exists and nothing has been pointed at anything. The four steps
that close it are written into the workflow's header under **BEFORE THE FIRST
TAG**.

### `site-v*` is a plain counter

`site-v1`, `site-v2`. Nothing is encoded in the number. Recorded here and in the
workflow's header comment, because "what do I tag to ship a typo fix" is a
monthly question.

Rejected: **a date**, which collides the second time you ship twice in one day —
exactly the day you are fixing something you just broke. Rejected: **mirroring
`Cargo.toml`**, which would mean no documentation change can ship without a
binary version bump, contradicting the entire reason this tag line is separate
from `v*`.

`v*` and `site-v*` cannot collide: GitHub tag globs anchor at the start of the
ref, so `v*` never matches `site-v1`. That is criterion 3, and it closes
statically.

### Four criteria were not checkable as written, and are restated

Each restatement is carried in a doc comment above the test that enforces it in
`tests/site.rs`.

- **C1** — "no `node_modules` and no lockfile anywhere in the tree" fails
  literally on `Cargo.lock`. Restated as **no JS lockfile and no `node_modules`
  among tracked files**, checked with `git ls-files` (plus the `> 100` vacuity
  guard) rather than a filesystem walk. The distinction is live: the main
  checkout has stale untracked `node_modules/` and `npm/` on disk, so a walk
  would answer differently there than in a worktree.
- **C6** — `zola check` catches a broken `@/` link **in content** and nothing
  else. Root-relative (`/nope/`), parent-relative (`../gone/`) and every `href`
  in a Tera template were probed against the pinned 0.23.4 and all pass
  silently. Rather than pin a second crawler, the site holds two conventions —
  `@/<page>.md` in content, `get_url(path="@/<page>.md")` in templates — which
  together make `zola build` itself the check (verified by breaking a template
  link on purpose: the build hard-fails). Two tests keep the conventions true.
- **C7** — restated mechanically: `readme.md` carries no fenced `json`/`jsonc`
  block, names no config key, is under 100 lines, and links to the site.
- **C8** — no headless browser exists and adding one violates C1. Restated as a
  static proxy: a `<meta name="viewport">`, at least one `@media` breakpoint,
  and a nav of real `<a>` elements with no `<script>` anywhere under `site/`.
  **The real check is a human one at the gate** — the proxy can only say the
  page is not built in the way that guarantees it fails.

**C4 also introduces the repository's first PR CI.** `code:lint` and `code:test`
run on tags and on `workflow_dispatch` today and have never run on a pull
request. Widening that to the Rust suite was left as a separate decision.

### Blockers fixed on the way through, none of them the site's

- **`.config/gitleaks.toml` had no `[extend]`**, so `--config` *replaced*
  gitleaks' ~170 default rules with this repo's two. Proven: an AWS access key
  the default ruleset flags scanned clean under the repo config. Both `code:sec`
  and the pre-commit hook use that config, so the repository's secret scanning
  has been running on two rules. Fixed with `[extend] useDefault = true`; the
  same key is now caught, and the repository still scans clean.
- **`schemas/claude-status.schema.json` documented a path the code abandoned.**
  `spend.description` named `~/.cache/ai-plugins/spend.json` and
  `$AI_PLUGINS_SPEND_CACHE`; the code reads `CLAUDE_STATUS_SPEND_CACHE` and
  defaults to `~/.cache/claude-status/spend.json`. The site documents the real
  path, so the published schema had to stop contradicting it. Fixed at source
  (`src/modules/config/mod.rs`) and regenerated, which moved
  `DESCRIPTION_DIGEST` in `tests/schema.rs` — that guard firing is the guard
  working.
- **Zola 0.23's config schema is not what any pre-0.23 example shows.**
  Highlighting moved out of `[markdown]` into `[markdown.highlighting]` **and
  the keys were renamed**: `highlight_code`/`highlight_theme` are rejected
  wherever they are put, and the replacements are `style` and `theme`. Moving
  the section without renaming the keys still fails. The theme *set* changed too
  — `base16-ocean-dark`, `gruvbox`, `one-dark`, `zenburn`, `kronuz`,
  `agola-dark`, `inspired-github` and `visual-studio-dark` were all probed
  against 0.23.4 and none exists; the modern set (`nord`, `catppuccin-mocha`,
  `tokyo-night`, `github-dark`, `rose-pine`, `everforest-dark`, `solarized-*`,
  `ayu-*`, `dracula`, `monokai`) replaces it. The version is pinned exactly for
  this reason.
- **`wrangler` must never be pinned.** `mise registry wrangler` resolves to
  `npm:wrangler` and nothing else, with
  `allow_builds=["esbuild","sharp",
  "workerd"]`.
  `cloudflare/wrangler-action@v3` keeps it on the runner.
- **`dprint` had to be told to leave two trees alone.** Its `includes` cover
  `**/*.html` and `**/*.css` and excluded neither: it rewrites generated HTML
  that the next `zola build` undoes — the same forever-loop the pre-commit
  config already documents for `graphify-out/` — and `markup_fmt` does not
  understand Tera, joining `{% endfor %}` onto the following line. Content `.md`
  with `+++` frontmatter formats cleanly and is **not** excluded.
- **`site` was not a valid commit scope.** `commitScopes` in
  `.config/git-conventional-commits.yaml` is a closed list and the
  `conventional-commits` hook runs on every commit. Added. `installer` is now a
  dead scope — noted in place, left listed so existing history stays valid.

### Left alone: `actions/download-artifact@v4` is a standing grype finding

`code:sec` reports GHSA-cxww-7g56-2vh6 (High, fixed in 4.1.3) against
`actions/download-artifact@v4`. **It pre-dates this cycle** — `release.yml`
alone reproduces it, verified by scanning that file on its own — and `site.yml`
follows the same house style rather than diverging from it.

Not fixed here, deliberately. The advisory is about downloading an artifact
produced by a *different* workflow run; both uses in this repository download an
artifact the same run just uploaded. And `@v4` is a floating major tag that
GitHub resolves to the current v4 release at run time, so what actually executes
is long past 4.1.3 — the finding is grype reading the literal string. Pinning it
is worth doing, but as one change across both workflows, not as half a fix that
leaves the two files disagreeing.

### Premises above that were already false when execution started

The plan's **Current state (actual)** was written before the cycles ahead of it
landed, and is left as written — a plan is a record of a proposal, not a
description of the tree. What it says, and what was true at `fd8d4de`:

- **"`readme.md` was rewritten as an npm package listing page"** with sections
  Requirements/Install/Uninstall/Configuration/Diagnosing/Environment/Licence.
  That readme no longer existed: `distribution/01` had replaced it one commit
  earlier with a 297-line *project* readme. Step 7 was executed against that
  one, knowingly deleting ~250 lines written a commit before — three fenced
  config examples and four tables — after checking every fact in them onto the
  site.
- **"a 1,300-line behaviour contract"** — it is **1,812** lines.
- **"the schema"** is 306 lines, not the 301 the website blueprint records
  (`website/index.md:35`).
- **The future tense throughout** — "is about to become single-language",
  "removes Node", "will point here". All of it had already happened. `--help`
  was already shipping `https://claude-status.virajp.dev`
  (`src/_runtime/cli.rs:82`, pinned by `cli.rs:292` and `tests/e2e.rs:1306`), so
  the dead-link risk the plan lists under Risks was live, not hypothetical, for
  the whole gap between `config-and-cli/03` and this cycle.

### The screenshot is a placeholder, and cannot be regenerated automatically

`site/static/statusline.png` is a copy of `assets/statusline.png`, standing in
until a fresh render arrives. A test asserts it exists and is a real PNG; no
test can assert it is *current*, which is the risk the plan already named.

**Regeneration is a manual recipe, and there is no way around that** — the bar's
glyphs are Nerd Font private-use codepoints and no headless renderer available
here carries the font:

1. A terminal with a Nerd Font, 24-bit colour and a dark background.
2. `cd` into a repository whose `.config/claude-status.json` sets a sensible
   `projectName`.
3. Run `claude-status --debug`.
4. Screenshot the two `SAMPLE RENDER` lines.

The figures in that render come from `sample_facts()` and are fixed; the git
facts and the project name are real, which is why step 2 matters.

`.config/claude-status.json` in this repository said `@askviraj/claude-status` —
the npm name, retired by `distribution/01` — and now says
`virajp/claude-status`.

### Fix round — what adversarial review found

Two reviewers ran in **separate worktrees**. Between them: nine confirmed
defects, 25 of 27 mutations killed, and a 30-item verified-correct list.

**Six of the nine were user-visible falsehoods on a documentation site**, which
is the failure mode this cycle exists to avoid — nothing fails when prose
drifts.

- **The performance claim was a startup figure sold as a per-render figure.**
  The readme said "~1-2 ms per render" and the landing page leaned the
  four-second-refresh argument on it. Measured: `--version` sits about a
  millisecond above bare process spawn, so the number is right *for startup* —
  but `--statusline` inside a git repository costs tens of milliseconds, because
  `git.rs` shells out to up to four `git` subprocesses per render. The origin is
  `statusline-behaviour.md:17-19`, where the number is explicitly a **startup**
  comparison against Node. Both pages now say what is actually true, and say why
  the repository case is slower.
- **"Drawing the bar never writes to disk" was false**, and the same page
  contradicted it 110 lines later. A render writes the usage mirror
  synchronously, in-process, on every render when `CLAUDE_STATUS_USAGE_DIR` is
  set. Inherited verbatim from the old readme — **the shrink moved a false claim
  onto a user-facing site rather than introducing one**. Deleting 250 lines did
  not audit the 67 that survived.
- **The caps hook is inert unless `CLAUDE_STATUS_USAGE_DIR` is set, and nothing
  said so.** `--configure` wires the hook; the actuator reads that variable;
  there is no default. Unset, it exits silently and successfully every time, so
  a broken setup is indistinguishable from a quiet one. Three pages presented
  caps as a working shipped feature. Now documented, including that `--debug`
  will not tell you either.
- **The `branch` segment's worktree glyph is conditional**, and the site, the
  contract, and `segments.rs`'s **own doc comment** all said or implied
  otherwise. The comment read "every part after the branch glyph is
  conditional", which is exactly wrong about the one part that comes *before*
  it. The site's own hero screenshot shows the correct behaviour and therefore
  contradicted the page describing it. All three fixed; the contract gets a
  dated amendment rather than a rewrite, per that section's convention.
- **The `--debug` block was framed as a capture and was not one** — shortened
  paths, a reflowed `VERDICT`, and an `ignored` row that only appears when a
  repo config carries a key beyond `projectName`. Reframed as illustrative, with
  the difference stated.
- Plus three wording defects: the subagent status colours one segment rather
  than the row, gate 3 is "budget unusable" (a *missing* block is gate 2), and
  `show: "auto"` admits only `team` and `enterprise` — so "every Pro and Max
  seat" understated it, since a cache with no plan recorded is hidden too.

**The PR-secret guard was a text slice with two demonstrated bypasses.** It cut
the workflow at `\n  build:` and `\n  deploy:` and asserted over the halves. A
workflow-level `env:` holding the token sits *above* `jobs:` — above the first
cut — and a third job appended after `deploy:` lands inside the "deploy" half,
where the real job already satisfies every assertion. Both survived; both are
ordinary edits, not contrivances. Rewritten to enumerate job spans by
indentation and assert that **every** `secrets.` reference sits inside a
tag-guarded job and that **every** non-build job carries the guard — stated for
all jobs rather than for `deploy` by name, so adding a job cannot create an
unguarded path. No YAML dependency taken for one assertion. Verified: control
green, both bypasses now red.

Two smaller ones: a comment in `site/build` cited `tests/e2e.rs` for tests that
live in `tests/site.rs` — the "cites a file that moved" class, now four cycles
running — and a comment about link checking had been stranded above
`generate_feeds` by taplo's `reorder_keys`. The link commentary now sits in a
file-level block, with a note explaining why comments cannot be attached to
individual keys here.

### Verified correct, worth recording

The build is genuinely Node-free: with `node`, `npm`, `pnpm`, `npx`, `tsc`,
`tsup`, `yarn`, `bun`, `deno` and `esbuild` all shimmed to exit 127 — and the
harness proven able to fail by a control that trips one — `site:build` exits 0
with zero shims tripped, and two clean builds are byte-identical by sha256.
`otool -L` on zola shows system libraries only.

Criterion 6 holds without a link checker: breaking an internal link in content
**or** in a template hard-fails `zola build`, so the build is the check. The
silent forms (a root-relative `/page/`, a literal `href`) are closed by
`tests/site.rs` instead, and both mutations were killed.

The `site/config.toml` theme list was probed against 0.23.4 and is exactly
right: twelve available, eight rejected, and a nonsense name rejected
identically as a control. **This corrects the recon pass**, which reported only
four themes exist — its probe had failed for an unrelated reason
(`mise x --
zola` does not resolve outside the repo) and reported the failure as
absence. The orchestrator repeated that claim in the implementation brief, and
built two vacuous probes of its own before a control caught them.

### Left open

- **Criterion 2 (a live site) is deferred**, with evidence: neither
  `claude-status.virajp.dev` nor the apex resolves, though the zone is on
  Cloudflare, and the repository has zero secrets. Four out-of-band human steps
  are listed in the workflow header.
- **Criterion 8's real check is human.** The static proxy — viewport meta, an
  `@media` breakpoint, nav as plain anchors, no `<script>` — is verified in the
  *built* output, but it only says the page is not built in a way that
  guarantees failure on a phone.
- **The hero screenshot still shows `@askviraj/claude-status`**, the retired npm
  scope, on both the site and the readme. `.config/claude-status.json` was
  corrected this cycle, so a re-render fixes it; the recipe is above and cannot
  be automated (Nerd Font private-use glyphs, no headless renderer with that
  font). A test asserts the file exists and is a real PNG; **nothing can assert
  it is current.**
- **The readme now routes all user documentation through five links to a domain
  that does not resolve.** Recorded as a scope consequence rather than a defect,
  but the cost lands on users until a `site-v*` tag ships.
- **`code:sec` cannot fail at any severity** — no `.config/grype.yaml`, no
  `--fail-on`. Pre-existing. The standing `actions/download-artifact@v4`
  advisory is almost certainly a false positive (`@v4` floats well past the
  fixed 4.1.3; grype compares the literal tag), and this cycle adds a second use
  of that action rather than diverging from house style.
- **`grype` run from a worktree under `/tmp` scans sibling worktrees**, which
  produced a phantom npm finding from another agent's pre-drop-npm checkout.
  Worth knowing before believing a `code:sec` result from there.
