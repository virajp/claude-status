//! Invariants that hold across **every** workflow, not one of them.
//!
//! Both cases below are the same failure: a fix applied to `ci.yml` and not to
//! `release.yml`, or the reverse. The workflows are edited one at a time and
//! read one at a time, so "we already fixed that" is true of the file someone
//! is looking at and false of the one that matters.
//!
//! ---
//!
//! # No workflow may run an action on Node 20
//!
//! GitHub is moving the Actions runner off Node 20. An action whose
//! `runs.using` is `node20` currently prints a deprecation warning on every
//! step that uses it; when the runner drops the runtime, the same step stops
//! running at all. That is a scheduled outage in `release.yml` and `site.yml`,
//! which are the two workflows that ship anything — and neither runs on a pull
//! request, so nothing would surface it before a tag.
//!
//! Three actions were on `node20` until 2026-08-27:
//! `actions/upload-artifact@v4`, `actions/download-artifact@v4` and
//! `cloudflare/wrangler-action@v3`. Moving them was not a version bump for its
//! own sake, and this pins that they do not come back.
//!
//! **The floors below were read out of each action's own `action.yml`, not
//! inferred from release notes.** The notes are ambiguous on purpose:
//! `upload-artifact@v5` is described as supporting Node 24, but its `action.yml`
//! still says `using: 'node20'` — v6 is the first major that actually runs on
//! Node 24, and v5 would have looked like a fix while changing nothing. Re-read
//! one with:
//!
//! ```sh
//! gh api "repos/actions/upload-artifact/contents/action.yml?ref=v6" \
//!   --jq '.content' | base64 -d | grep -A1 '^runs:'
//! ```
//!
//! **This is a floor, not a pin.** A newer major passes without an edit here,
//! which is what keeps the guard from arguing with a routine upgrade. The
//! deliberate consequence is that it only covers the actions named in the
//! table: a *new* action added on `node20` is not caught, and adding a row is
//! the price of adding an action.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// `(action, first major whose `runs.using` is `node24`)`.
///
/// The repository uses v7, v8 and v4 respectively — above the floor in every
/// case, because each was taken to the newest major rather than the oldest one
/// that would clear this test.
const NODE24_FLOOR: &[(&str, u32)] = &[
    ("actions/upload-artifact", 6),
    ("actions/download-artifact", 7),
    ("cloudflare/wrangler-action", 4),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every workflow, as `(file name, contents)`.
fn workflows() -> Vec<(String, String)> {
    let dir = root().join(".github/workflows");
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", dir.display()));

    let mut out = Vec::new();
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let name = path.file_name().expect("a file name").to_string_lossy().into_owned();
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));
        out.push((name, body));
    }
    out.sort();
    out
}

/// Every `uses:` reference in `source`, as `(action, major)`.
///
/// Comments are stripped before the match, for the reason `release.rs`'s `job`
/// helper records: these workflows explain themselves at length and name the
/// very constructs they use, so a scan that read the prose would pass on a
/// comment alone. `site.yml` has `cloudflare/wrangler-action@v4` in a comment
/// forty lines above the step that runs it, and that comment must not be what
/// makes this test green.
///
/// The major is `None` when the reference carries something this scan cannot
/// read a major out of — a commit SHA, most likely. That is reported rather
/// than skipped: a SHA pin is a deliberate change of policy (see
/// `.config/grype.yaml`), not something to pass silently.
fn action_majors(source: &str) -> Vec<(String, Option<u32>)> {
    let mut out = Vec::new();
    for line in source.lines() {
        let code = line.split('#').next().unwrap_or("");
        let Some((_, rest)) = code.split_once("uses:") else {
            continue;
        };
        let reference = rest.trim().trim_matches('"').trim_matches('\'');
        // A local action (`./.github/actions/…`) has no `@` and no major.
        let Some((action, version)) = reference.rsplit_once('@') else {
            continue;
        };
        let major = version.trim_start_matches('v').split('.').next().and_then(|d| d.parse::<u32>().ok());
        out.push((action.to_string(), major));
    }
    out
}

#[test]
fn no_workflow_runs_an_action_on_node_20() {
    let mut offenders = Vec::new();
    let mut covered: BTreeSet<&str> = BTreeSet::new();
    let mut total = 0usize;

    for (file, source) in workflows() {
        for (action, major) in action_majors(&source) {
            total += 1;
            let Some(entry) = NODE24_FLOOR.iter().find(|e| action == e.0) else {
                continue;
            };
            covered.insert(entry.0);
            match major {
                Some(m) if m >= entry.1 => {}
                Some(m) => offenders.push(format!(
                    "{file}: {action}@v{m} runs on Node 20; v{} is the first major that runs on Node 24",
                    entry.1
                )),
                None => offenders.push(format!(
                    "{file}: {action} is pinned to something with no readable major; check its `runs.using` by hand"
                )),
            }
        }
    }

    // Vacuity guards. This test reads files off disk and filters twice, so
    // every way it can quietly check nothing is worth an assertion of its own:
    // a renamed workflow directory, or an action dropped from the table's
    // reach, would otherwise leave it green and empty.
    assert!(total > 0, "no `uses:` reference found in any workflow — this scan is vacuous");
    let missing: Vec<&str> =
        NODE24_FLOOR.iter().map(|e| e.0).filter(|a| !covered.contains(a)).collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "these actions have a floor here but appear in no workflow — delete the row if the action is \
         genuinely gone, rather than leaving a guard that checks nothing"
    );

    assert_eq!(offenders, Vec::<String>::new(), "a workflow runs an action on Node 20");
}

/// The lint commands that need clippy on the runner. `code:all` includes
/// `code:lint`.
const LINT_COMMANDS: &[&str] = &["code:lint", "code:all"];

/// The idempotent step that makes the lint gate deterministic.
const CLIPPY_GUARD: &str = "rustup component add clippy";

/// Split a workflow into `(job name, body)`, comments stripped.
///
/// Comments are stripped for the reason the `uses:` scan strips them, and the
/// stakes are higher here: `release.yml` explains the `minimal`-profile resync
/// in a comment **eleven lines long**, naming the exact symptom, directly above
/// the job that then failed on it. A scan that read prose would have called
/// that job guarded.
fn jobs(source: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, String)> = None;
    let mut in_jobs = false;

    for line in source.lines() {
        let code = line.split('#').next().unwrap_or("");

        if !in_jobs {
            in_jobs = code.trim_end() == "jobs:";
            continue;
        }

        // Any further top-level key ends the block.
        if !code.trim().is_empty() && !code.starts_with(' ') {
            break;
        }

        // A job header is a key at exactly two spaces of indent.
        let is_header =
            code.starts_with("  ") && !code.starts_with("   ") && code.trim_end().ends_with(':');
        if is_header {
            out.extend(current.take());
            current = Some((code.trim().trim_end_matches(':').to_string(), String::new()));
            continue;
        }

        if let Some((_, body)) = current.as_mut() {
            body.push_str(code);
            body.push('\n');
        }
    }

    out.extend(current.take());
    out
}

/// **A lint gate that fails at random is worse than no lint gate.**
///
/// `mise-action` installs the toolchain, and rustup has been observed
/// resyncing it to a `minimal` profile at lint time — three runs of the *same*
/// commit produced clippy twice and not the third. Dropping `install_args` made
/// that rarer, not impossible, so `ci.yml` gained an explicit
/// `rustup component add clippy` on 2026-08-27.
///
/// **`release.yml` did not, and the next tag paid for it.** v1.1.0 failed in
/// its `test` job with "'cargo-clippy' is not installed for the toolchain
/// '1.98.0'"; `publish` and `bump-tap` were skipped, and the release was lost
/// to a third-party download profile rather than to anything wrong with the
/// commit. The exposure was always worse here than in CI — a pull request costs
/// a re-run, a tag costs the release — and it was the one left unfixed.
#[test]
fn every_job_that_lints_installs_clippy_first() {
    let mut offenders = Vec::new();
    let mut checked = 0usize;

    for (file, source) in workflows() {
        for (name, body) in jobs(&source) {
            let Some(lint_at) = LINT_COMMANDS.iter().filter_map(|c| body.find(c)).min() else {
                continue;
            };
            checked += 1;

            match body.find(CLIPPY_GUARD) {
                Some(guard_at) if guard_at < lint_at => {}
                Some(_) => offenders
                    .push(format!("{file}: job `{name}` installs clippy only after it has already linted")),
                None => offenders.push(format!("{file}: job `{name}` lints with no `{CLIPPY_GUARD}` ahead of it")),
            }
        }
    }

    assert!(checked > 0, "no job runs a lint command — this scan is vacuous");
    assert_eq!(offenders, Vec::<String>::new(), "a lint gate here can fail at random, and on a tag that costs the release");
}

/// **The control for the job splitter.** `every_job_that_lints_installs_clippy_first`
/// passes trivially if `jobs` returns nothing or merges the workflow into one
/// blob — the first would check no jobs, the second would let any job's clippy
/// step vouch for every other job's lint. Both are checked here against input
/// with a known answer.
#[test]
fn the_job_splitter_separates_jobs() {
    let probe = "\
on:
  push:
jobs:
  alpha:
    steps:
      - run: mise run code:lint
  beta:
    runs-on: ubuntu-24.04
    steps:
      # - run: rustup component add clippy
      - run: echo hi
";

    let found = jobs(probe);
    let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta"], "the splitter does not separate the jobs");
    assert!(found[0].1.contains("code:lint"), "job `alpha` lost its body");
    assert!(
        !found[1].1.contains(CLIPPY_GUARD),
        "the commented-out guard in `beta` was read as real, so a comment could vouch for a job"
    );
    assert!(!found[0].1.contains("echo hi"), "job `alpha` swallowed job `beta`'s body");
}

/// **The control.** The assertion above passes just as well when the parser
/// returns nothing, and a `uses:` scan has two specific ways to return nothing
/// that look identical from the outside: not matching the line shape, and
/// matching the commented-out copy instead of the real one. Both are checked
/// here against input with a known answer.
#[test]
fn the_scan_reads_a_major_and_ignores_the_comments() {
    let probe = "\
jobs:
  x:
    steps:
      # - uses: actions/download-artifact@v1
      - uses: actions/download-artifact@v8
      - uses: actions/upload-artifact@v4 # the shape this test exists to catch
      - uses: \"cloudflare/wrangler-action@v4.0.0\"
      - uses: ./.github/actions/local
      - uses: actions/checkout@0ad4b8fadaa221de15dcec353f45205ec38ea70b
";

    let found = action_majors(probe);

    assert_eq!(
        found,
        vec![
            ("actions/download-artifact".to_string(), Some(8)),
            ("actions/upload-artifact".to_string(), Some(4)),
            ("cloudflare/wrangler-action".to_string(), Some(4)),
            ("actions/checkout".to_string(), None),
        ],
        "the `uses:` scan does not read what it is supposed to read"
    );
}

/// The check that proves a tagged ref is on `main`.
///
/// Matched as a substring of a `run:` block rather than by parsing the step:
/// what matters is that the comparison happens, and the two workflows spell the
/// surrounding step differently — `release.yml` guards it with `github.ref_type`
/// inside a job that does other work, `site.yml` gives it a job of its own.
const MAIN_CONTAINMENT: &str = "git merge-base --is-ancestor \"$GITHUB_SHA\" origin/main";

/// The fetch without which the comparison above cannot run.
const MAIN_FETCH: &str = "git fetch --no-tags origin main";

/// **Every tag line ships from `main`, and neither of them can prove it alone.**
///
/// `release.yml` builds and publishes a binary; `site.yml` deploys the public
/// site. Both fire on a tag, and a tag can be cut from any branch — so without
/// this check either one would happily ship a commit that was never merged,
/// carrying a version number `main` does not describe.
///
/// The `--is-ancestor` spelling is the point and not an implementation detail.
/// An equality check against `refs/heads/main` would refuse a tag cut before
/// later work landed, which is a legitimate release of an earlier commit; the
/// question is containment, not identity.
#[test]
fn every_tag_triggered_workflow_refuses_a_tag_that_is_not_on_main() {
    let gated: Vec<String> = workflows()
        .into_iter()
        .filter(|(_, body)| {
            // Comments are stripped for `jobs`' reason: both files explain this
            // gate at length directly above it, and prose must not be what
            // makes this test green.
            let code: String =
                body.lines().map(|l| l.split('#').next().unwrap_or("")).collect::<Vec<_>>().join("\n");
            code.contains("tags:")
        })
        .map(|(name, body)| {
            let code: String =
                body.lines().map(|l| l.split('#').next().unwrap_or("")).collect::<Vec<_>>().join("\n");
            assert!(
                code.contains(MAIN_CONTAINMENT),
                "{name} runs on a tag but never checks the tag is on main — a tag cut from develop would ship"
            );
            assert!(
                code.contains(MAIN_FETCH),
                "{name} compares against origin/main without fetching it; a checkout at a tag ref does not \
                 bring that ref, so the comparison would fail on a missing ref rather than on merit"
            );
            name
        })
        .collect();

    // **The control.** Every assertion above lives inside a `map` over a
    // filtered list, so a filter that matched nothing would pass this test
    // while checking no workflow at all. Both tag lines must be found.
    assert!(
        gated.contains(&"release.yml".to_string()) && gated.contains(&"site.yml".to_string()),
        "the tag-triggered set is {gated:?}; expected both release.yml and site.yml — a filter that \
         stopped matching would make the assertions above vacuous"
    );
}

/// **CI runs on `develop`, because that is where every change now lands.**
///
/// Work is authored on `develop` and merged to `main`; a CI trigger that named
/// only `main` would leave the branch all the work happens on unchecked until
/// the merge, which is the one moment the check is too late to be cheap.
///
/// `main` stays listed for a reason that is not symmetry: a merge commit is a
/// state neither side tested, so the merge itself has to be checked.
#[test]
fn ci_runs_on_both_the_work_branch_and_the_release_branch() {
    let (_, body) = workflows()
        .into_iter()
        .find(|(name, _)| name == "ci.yml")
        .expect("ci.yml is missing");

    let code: String =
        body.lines().map(|l| l.split('#').next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    let on = code.split("jobs:").next().expect("ci.yml has a jobs: key");

    for branch in ["main", "develop"] {
        assert!(
            on.contains(&format!("- {branch}")),
            "ci.yml does not run on pushes to {branch}; its triggers are:\n{on}"
        );
    }
}
