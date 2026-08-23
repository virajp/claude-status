---
type: vwf-plan-index
title: website — 2026-08-23
description: Two ordered plans that build claude-status.virajp.dev as a Zola
  static site on Cloudflare Pages, then add a schema-driven config generator
  with a live bar preview.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
timestamp: 2026-08-23T14:20:00Z
tags: [ website, zola, cloudflare, docs, config-generator ]
---

# website — 2026-08-23

| # | Plan                                         | What it changes                                                    |
| - | -------------------------------------------- | ------------------------------------------------------------------ |
| 1 | [site](./01-site.md)                         | `site/`, Zola, Cloudflare Pages on `site-v*`, docs, readme shrinks |
| 2 | [config-generator](./02-config-generator.md) | schema-driven form, live bar preview, fixture-gated in CI          |

## Why the site exists

Three jobs, in the order they matter:

1. **It is where the binary sends people.** After
   [distribution/02](../2026-08-23-distribution/02-homebrew-formula.md), the
   formula's caveats point here, and after
   [config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md), so does
   `--help`. It is not optional marketing; it is part of the install flow.
2. **It is the documentation.** `readme.md` shrinks to a pointer, so the site
   becomes the only complete description of the config, the flags and the repo
   layer.
3. **It generates configs.** Config is now non-defaults-only JSON with a
   301-line schema behind it. Hand-writing that from a docs page is worse than
   filling in a form that emits exactly the keys you changed.

## The stack, and the constraint behind it

**Zola** — one Rust binary, installed by mise, no `node_modules`, no lockfile.

This is not incidental.
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) removes Node from
the repo entirely by deleting the npm installer; a website that reintroduced a
Node build would undo that in the same week. Zola gives templating and markdown
for the docs while keeping the repo single-language.

Everything beyond Zola is hand-written HTML, CSS and vanilla JS. No framework,
no CSS library, no bundler.

## Deployment

**Cloudflare Pages, deployed by `wrangler` from GitHub Actions, on a `site-v*`
tag.**

The separate tag line is the point: a typo in the docs is fixed and shipped
without cutting a binary release, and a binary release does not force a site
deploy. `wrangler` from Actions rather than Cloudflare's git integration because
the git integration deploys on branch pushes and cannot be gated on a tag.

This needs `CLOUDFLARE_API_TOKEN` and an account id as repository secrets —
**the second long-lived credential in this repo**, after
[distribution/02](../2026-08-23-distribution/02-homebrew-formula.md)'s tap
token. Worth counting, because `distribution/01` removes the repo's last one and
these two put the number back at two.

## The one genuine duplication

The live bar preview means a **second implementation of the renderer**, in
JavaScript, that can disagree with the Rust one.

That is accepted for the demo value — seeing your bar as you configure it is
both the best UX and the strongest thing on the landing page — but it is gated:
CI runs the JS renderer over the same fixtures `tests/golden/` uses and diffs
the output. A divergence fails the build rather than shipping a preview that
lies. [Plan 2](./02-config-generator.md) owns that gate, and it is the reason
plan 2 is separate from plan 1 rather than one big website cycle.

## Order

**1 → 2.** Plan 1 ships a site with docs and a real install command. Plan 2 adds
the generator, and depends on
[config-and-cli/04](../2026-08-23-config-and-cli/04-schema-and-validation.md)
having made the schema generated and well-formed — the form is built from that
file, so it stops being documentation and becomes an interface.
