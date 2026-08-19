DECISION 2026-08-18 ★4 | setup/scope | onboard blank repo, defer architecture —
/vwf:setup ran the blank sub-path: repo held only LICENSE, so no manifest, no
source dirs, no docs/blueprint/. Topology, linkage, roles, platforms and stack
pins deliberately NOT written — why: those decisions belong to /vwf:product and
/vwf:architecture, which have a product contract to derive them from; asking
them of a blank repo asks the user to invent an architecture at the moment they
have the least information — result is the structure-pending config state, which
/vwf:doctor reads as early, not as drift

DECISION 2026-08-18 ★4 | mise/runtime | no language runtime pinned yet — user
chose "none as of now"; claude-status has no manifest and its implementation
language is still open (a rust-rewrite-plan.md sits in docs/scratchpad/) —
consequence: only the common/ task overlay was copied. The stack overlays
(node/flutter/python) own code/format, code/lint and setup/all, so two shipped
templates were edited to stay runnable: code/all has its code:lint line
commented out, mise.dev.toml has its `setup` alias commented out. Both carry a
comment naming what restores them — code/format is hand-written, dprint-only —
the stack-agnostic half every overlay shares. Replace it with the real overlay
when a runtime is picked, do not extend it

DECISION 2026-08-18 ★3 | mise/ci | three-file MISE_ENV split despite no pipeline
yet — user confirmed the repo will be built/deployed through CI/CD, so
mise.dev.toml and mise.ci.toml were written now rather than deferred. Both are
near-empty: no runtime env vars were named, so the dev/prod value split has
nothing in it yet

DECISION 2026-08-18 ★3 | graphify | graph built --code-only, not a full
extraction — no LLM API key available in the environment, and the full build
hard-fails on doc files without one — indexed the 8 mise task shell scripts (31
nodes); CLAUDE.md and 2 other docs are absent from the graph — accepted because
the repo has no real code yet, so the graph would be rebuilt regardless. Re-run
`graphify extract .` with a key once there is something worth indexing
