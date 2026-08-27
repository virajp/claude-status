//! **No workflow may run an action on Node 20.**
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
