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

*(filled in during execution)*
