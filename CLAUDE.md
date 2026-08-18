# claude-status

## vwf workflow

This repo uses the **vwf** Product → Blueprint → Plan → Execute workflow. Docs
live under `docs/blueprint/` (the desired state) and `docs/plans/` (the diffs to
apply).

**Order:** `/vwf:setup` → `/vwf:product` → `/vwf:architecture` →
`/vwf:design-system` (once a UI exists) → `/vwf:blueprint` (a full-product sweep
— `plan` halts until its coverage stamp reads complete) → `/vwf:plan <slice>` →
`/vwf:execute` → `/vwf:archive` — then, after you deploy, `/vwf:verify <env>`
and `/vwf:feedback` route what production says back into product/blueprint/plan.

Blueprint flow passes render each flow's screens (happy & sad paths) into the
gitignored `docs/scratchpad/` tree for visual review in your browser before the
pass is approved — mockups are realizations for review, never part of the
contract, and are never pushed to the design tool. Design-first instead:
`/vwf:screens prompt <flow>` writes a brief you paste into the canvas chat (one
interactive page per platform, named `<flow>--<platform>`), `/vwf:screens
import` folds the designs back through blueprint passes. `/vwf:mockups [flow]`
batch re-renders (e.g. after a design-system change); `/vwf:feedback canvas`
harvests the canvas review conversation back into the contracts (as routed
intent, never as files).

**The blueprint is a code-independent contract.** It records only decisions that
have more than one reasonable answer *and* are true regardless of how the code
is written today. Reuse-vs-build, file placement, step ordering, and library
choices are `plan`'s job — not the blueprint's.

**Docs:**

- `docs/blueprint/product.md` — problem, users, measurable goals (every flow
  `Serves:` one; entities trace through flows), slice priority.
- `docs/blueprint/architecture.md` — system shape + machine-readable Project
  Registry.
- `docs/blueprint/design-system.md` — product-wide UX/visual contract (if UI).
- `docs/blueprint/conventions.md` — cross-cutting decisions (auth, errors, ids,
  config…).
- `docs/blueprint/environment.md` — per-project inventory of env vars + secrets,
  no values (if the system has an external integration/secret).
- `docs/blueprint/flows/<project>/<NNN>-<flow>/` — one folder per flow, the
  **primary** blueprint unit: `index.md` carries the platform-agnostic contract
  (trigger, actors, steps, jobs, acceptance) and one `<platform>.md` per
  implemented platform (`mobile` / `tablet` / `desktop` / `web` / `auto`)
  carries that platform's screens, each row with its frame code. Numbers are
  designated — `100` is always `home`. `flows/index.md` holds the catalog +
  inter-service contracts.
- `docs/blueprint/entities/<entity>/` — one folder per entity, the supporting
  data contracts: `index.md` (lifecycle, relationships, invariants) +
  `schema.yaml` (the authoritative data model). `entities/index.md` holds the
  catalog + product-wide ER diagram.
- `docs/blueprint/apis/<project>.openapi.yaml` — the authoritative API contract
  per service project; `apis/released/` holds the frozen production snapshots
  (backward compatibility is enforced against the latest one). The blueprint
  root holds only the system docs above.

**The blueprint is an OKF bundle.** `docs/blueprint/` is an Open Knowledge
Format (OKF) v0.1 bundle — every doc is a typed concept (YAML frontmatter) and
relationships are markdown links. So it is portable: any OKF-aware tool (e.g.
the OKF static-HTML graph visualizer) can render it, and it can be ingested by a
knowledge-graph tool like graphify — no vwf-specific reader required.

Re-run `/vwf:setup` after upgrading vwf to migrate the docs to the latest
format.
