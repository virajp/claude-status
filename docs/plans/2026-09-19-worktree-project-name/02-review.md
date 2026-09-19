# U2 — Review: the main-root resolution and the segment fallback

- **Wave:** 2
- **Depends on:** U1
- **Owns:** —
- **Model:** opus
- **Kind:** review

## Scope

Covers U1. Reviews the branch delta since the branch base — the first review row
— through `/code-review` and `/security-review` plus the two reviewers. The row
exists because U1 lands binary source (decision 7). Points of attention: the
`commondir` read is a new path-derived input reaching the row through `sanitize`
like every other directory name (`docs/decisions.md:693` already argues the
directory name is attacker-nameable and no wider than existing inputs — confirm
the new rung adds nothing); no subprocess was added to root resolution; `root`
semantics are unchanged so the dirty-marker e2e test still passes for the right
reason.
