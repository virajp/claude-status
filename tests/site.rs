//! Guards for the documentation site and the shape of the repository around
//! it — `website/01-site`'s acceptance criteria, in the form that is actually
//! checkable.
//!
//! Nothing here runs `zola`. The build is checked by running it (`site:build`,
//! and the workflow); these are the properties a build passing cannot tell you
//! about, because they are about what is *tracked*, what a workflow is allowed
//! to reach, and which link forms the build would have noticed at all.
//!
//! Four criteria arrive here restated, because as written they are not
//! checkable. Each restatement is recorded above the test that carries it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every path git tracks, as repository-relative strings.
///
/// **Tracked files, not a filesystem walk.** This is the pattern
/// `tests/e2e.rs`'s refresh-flag scan arrived at after four fixes, and the
/// reasoning transfers exactly: a walk reaches whatever happens to be sitting
/// in the working copy, so a gitignored tree — `target/`, `site/public/`, a
/// stale `node_modules/` nothing regenerates — makes the answer depend on
/// which checkout you are standing in. The main checkout of this repository
/// has untracked `node_modules/` and `npm/` directories on disk *right now*,
/// left behind when `distribution/01` deleted the npm channel. A walk would
/// fail there and pass in a fresh worktree, with nothing in `git status` to
/// explain it.
///
/// Asking git is also the scope the criterion wanted: an untracked file is one
/// no reader is handed and no commit can change.
fn tracked_files() -> Vec<String> {
    let listing = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .expect("git ls-files runs in a checkout");
    assert!(listing.status.success(), "git ls-files failed: {}", String::from_utf8_lossy(&listing.stderr));

    let files: Vec<String> = std::str::from_utf8(&listing.stdout)
        .expect("git paths are utf-8 here")
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();

    // A scan of nothing passes. Two earlier cycles shipped guards that could
    // not fail; this is what keeps these from joining them.
    assert!(files.len() > 100, "git ls-files returned {} files — every scan below would be vacuous", files.len());
    files
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", path.display()))
}

/// A tracked file's contents with its Tera comments blanked out, for the scans
/// that look for a construct this repository also *documents*.
///
/// Only `.html` files carry Tera comments; everything else is returned as-is.
fn code(rel: &str) -> String {
    let text = read(rel);
    if rel.ends_with(".html") { strip_tera_comments(&text) } else { text }
}

/// Every tracked file under `dir` whose name ends in `ext`.
fn tracked_under(dir: &str, ext: &str) -> Vec<String> {
    let found: Vec<String> = tracked_files()
        .into_iter()
        .filter(|p| p.starts_with(dir) && p.ends_with(ext))
        .collect();
    assert!(!found.is_empty(), "no tracked {ext} files under {dir} — this scan would be vacuous");
    found
}

// ---------------------------------------------------------------------------
// Criterion 1 — the site builds with no JavaScript toolchain in the tree
// ---------------------------------------------------------------------------

/// **Criterion 1, restated.** As written it is "no `node_modules` and no
/// lockfile anywhere in the tree", and read literally that fails on
/// `Cargo.lock` — which is a Rust lockfile, is tracked deliberately, and is
/// nothing the criterion was about.
///
/// What it was about is that adding a documentation site must not put the
/// JavaScript toolchain back that `distribution/01` removed. So: **no JS
/// lockfile and no `node_modules` among TRACKED files.** Zola is a single
/// static Rust binary (`aqua:getzola/zola`; `otool -L` shows system libraries
/// and nothing else), and `cloudflare/wrangler-action@v3` keeps wrangler's Node
/// on the runner rather than in this repository — that is the whole design, and
/// this is what holds it.
#[test]
fn no_javascript_lockfile_or_node_modules_is_tracked() {
    // A manifest, not just a lockfile. `the_generators_pure_core_holds_against_the_real_schema`
    // cites this test for "no `package.json`", and a `package.json` is the
    // first thing anyone would add to run `generator.test.mjs` under a test
    // runner — which is precisely how the npm ecosystem comes back.
    const JS_MANIFESTS: &[&str] = &["package.json"];
    const JS_LOCKFILES: &[&str] = &[
        "package-lock.json",
        "npm-shrinkwrap.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "bun.lockb",
        "bun.lock",
        "deno.lock",
    ];

    let mut offenders = Vec::new();
    for rel in tracked_files() {
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        if JS_LOCKFILES.contains(&name) {
            offenders.push(format!("{rel} (JS lockfile)"));
        }
        if JS_MANIFESTS.contains(&name) {
            offenders.push(format!("{rel} (JS manifest)"));
        }
        if rel.split('/').any(|c| c == "node_modules") {
            offenders.push(format!("{rel} (under node_modules/)"));
        }
    }

    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "a JavaScript toolchain is tracked again — the site was supposed to be built by a Rust binary"
    );
}

/// **Every control the generator builds carries an accessible name.**
///
/// A `<legend>` names the *fieldset*, not the controls inside it, and a
/// `placeholder` is not an accessible name. Both were relied on: the `oneOf`
/// mode switchers sat as a bare `select` under a legend, and each open map's
/// add-a-key box had only a placeholder — 54 controls in all, announced to a
/// screen reader as unlabelled.
///
/// **Found by opening the deployed page in a real browser**, which is the only
/// thing that can see an accessible name. No reviewer caught it from the source
/// in three passes, and this suite had no assertion about label binding at all.
///
/// This is a **source scan, not a behavioural test** — it cannot compute an
/// accessibility tree, only check that the constructs which had no name now ask
/// for one. It is a regression pin, and it is weaker than the thing it pins.
#[test]
fn every_generated_control_asks_for_an_accessible_name() {
    let js = read("site/static/config-generator.js");

    for (construct, needle) in [
        ("the oneOf mode switcher", "class: \"gen-mode\""),
        ("an open map's add-a-key box", "placeholder: \"new key\""),
    ] {
        let at = js.find(needle).unwrap_or_else(|| panic!("{construct} is gone from the generator — this guard is now scanning for nothing"));
        // The attribute bag is small; the name must be inside it, not merely
        // somewhere else in the file.
        let bag_end = js[at..].find("})").map(|e| at + e).unwrap_or(js.len());
        let bag = &js[at.saturating_sub(200)..bag_end];
        assert!(
            bag.contains("aria-label"),
            "{construct} is built without an accessible name; a legend names the group and a placeholder names nothing"
        );
    }
}

/// **Every tool the suite shells out to is declared where CI can see it.**
///
/// `MISE_ENV=ci` loads `.config/mise.toml` and `.config/mise.ci.toml` and
/// **not** `mise.dev.toml`. A tool declared only in the dev file resolves fine
/// on a maintainer's machine and is simply absent on a runner.
///
/// That is not hypothetical. `dprint` lived in the dev file, and
/// `tests/schema.rs::the_generated_schema_is_already_dprint_formatted` shells
/// out to it with a comment reading "CI always has it, which is where this
/// assertion has to hold." CI did not have it. The claim went unchallenged for
/// three cycles because the release workflow had never reached the Test step —
/// the first tag that got that far failed here, on the first release attempt.
///
/// The skip in that test guards the wrong thing: it fires when `mise` itself is
/// missing, not when the tool is. With mise present and the tool absent, it
/// falls through to the assertion and fails with a tool-not-found message
/// dressed up as a formatting complaint.
#[test]
fn every_tool_the_suite_shells_out_to_is_declared_for_ci() {
    let base = read(".config/mise.toml");
    // Declarations only, defensively. No comment in the block names a tool
    // today, so this is not load-bearing yet — a control confirmed the guard
    // goes red on a removed declaration either way. It is here because the
    // block's comments explain tools by name as a matter of style, and three
    // other guards in this suite have already been caught reading prose as
    // code. Cheaper to exclude comments now than to discover it later.
    let base_tools: String = base
        .split("[tools]")
        .nth(1)
        .unwrap_or("")
        .split("\n[")
        .next()
        .unwrap_or("")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.contains('='))
        .collect::<Vec<_>>()
        .join("\n");

    let mut wanted: Vec<(String, String)> = Vec::new();
    for file in ["tests/schema.rs", "tests/site.rs", "tests/e2e.rs", "tests/release.rs"] {
        // Comments stripped first. This guard's own doc comment spells the
        // pattern it hunts for, so a raw scan matches itself — the third time
        // in this suite that a guard read prose instead of code.
        let src: String = read(file)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for (idx, _) in src.match_indices(r#""x", "--", ""#) {
            let rest = &src[idx + r#""x", "--", ""#.len()..];
            if let Some(end) = rest.find('"') {
                wanted.push((file.to_string(), rest[..end].to_string()));
            }
        }
    }

    assert!(
        !wanted.is_empty(),
        "found no `mise x -- <tool>` call anywhere; this guard is scanning for a spelling the suite no longer uses"
    );

    let missing: Vec<String> = wanted
        .iter()
        .filter(|(_, tool)| !base_tools.contains(tool.as_str()))
        .map(|(file, tool)| format!("{tool} (used by {file})"))
        .collect();

    assert_eq!(
        missing,
        Vec::<String>::new(),
        "a tool the suite runs is not in `.config/mise.toml`'s [tools], so `MISE_ENV=ci` will not install it and the test that uses it fails on a runner and nowhere else"
    );
}

/// The build output is not tracked either, and cannot become tracked by
/// accident. `zola build` writes `site/public/`; committing it would put a
/// generated tree in review diffs forever.
#[test]
fn the_built_site_is_ignored_rather_than_tracked() {
    assert!(
        read(".gitignore").lines().any(|l| l.trim() == "site/public/"),
        ".gitignore no longer ignores site/public/ — zola's output would start showing up in diffs"
    );
    assert!(
        !tracked_files().iter().any(|p| p.starts_with("site/public/")),
        "part of the built site has been committed"
    );
}

// ---------------------------------------------------------------------------
// Criterion 6 — no internal link 404s
// ---------------------------------------------------------------------------

/// **Criterion 6, restated.** "Crawl every internal link" needs a crawler, and
/// the obvious one (`zola check`) does far less than its name suggests. Probed
/// against the pinned 0.23.4:
///
/// | link                          | where     | `zola check` |
/// | ----------------------------- | --------- | ------------ |
/// | `@/nope.md`                   | content   | **caught**   |
/// | `/nope/`                      | content   | passes       |
/// | `../gone/`                    | content   | passes       |
/// | any `href` at all             | template  | passes       |
///
/// So `zola check` alone would close this criterion on three pages' worth of
/// links and silently ignore the rest.
///
/// Rather than pin a second crawler, the site holds two conventions that make
/// **the build itself** the check: content links are `@/<page>.md` (which
/// `zola check` validates) and template links go through
/// `get_url(path="@/<page>.md")` (which `zola build` hard-fails on — verified
/// by breaking one on purpose). This test is what keeps the conventions true,
/// because a convention that only the author remembers is not a check.
#[test]
fn every_internal_link_in_site_content_is_in_the_form_zola_check_validates() {
    // Static assets a page may reference by bare filename. `@/` addresses the
    // content library and cannot name these.
    let static_assets: BTreeSet<String> = tracked_files()
        .into_iter()
        .filter_map(|p| p.strip_prefix("site/static/").map(str::to_string))
        .collect();
    assert!(!static_assets.is_empty(), "site/static/ is empty — the allowance below would be vacuous");

    let mut offenders = Vec::new();
    for rel in tracked_under("site/content/", ".md") {
        let text = read(&rel);
        for (n, target) in markdown_link_targets(&text) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
            {
                continue; // external, or an on-page anchor
            }
            if target.starts_with("@/") || static_assets.contains(&target) {
                continue;
            }
            offenders.push(format!("{rel}:{n} -> {target}"));
        }
    }

    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "an internal content link is not `@/<page>.md`, so `zola check` will never notice when it breaks"
    );
}

/// The template half of the same rule. A raw internal `href` in a Tera
/// template is invisible to every check this repository runs — the build does
/// not resolve it and `zola check` does not read templates at all.
#[test]
fn no_template_hardcodes_an_internal_url_behind_the_builds_back() {
    let mut offenders = Vec::new();
    for rel in tracked_under("site/templates/", ".html") {
        // Tera comments are stripped first, and not as a nicety: `base.html`'s
        // own header comment explains this rule by quoting the bad form, and a
        // test that cannot tell a rule from its counter-example is a test that
        // punishes documenting the rule.
        for (n, line) in strip_tera_comments(&read(&rel)).lines().enumerate() {
            for quote in ['"', '\''] {
                let needle = format!("href={quote}");
                for (idx, _) in line.match_indices(&needle) {
                    let rest = &line[idx + needle.len()..];
                    let Some(end) = rest.find(quote) else { continue };
                    let target = &rest[..end];
                    // A Tera expression resolves through `get_url`, which the
                    // build checks. `#` is an on-page anchor. Anything absolute
                    // is external.
                    if target.starts_with("{{")
                        || target.starts_with('#')
                        || target.starts_with("http://")
                        || target.starts_with("https://")
                    {
                        continue;
                    }
                    offenders.push(format!("{rel}:{} -> {target}", n + 1));
                }
            }
        }
    }

    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "a template href is a literal path — write it as get_url(path=\"@/<page>.md\") so the build fails when it rots"
    );
}

/// Blanks out every `{# ... #}` region, keeping the newlines so the line
/// numbers a caller reports still point at the right line.
fn strip_tera_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{#") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("#}") else {
            break; // unterminated: nothing left is a link
        };
        out.extend(after[..close].chars().filter(|c| *c == '\n'));
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    out
}

/// Markdown link targets as `(line number, target)`, for both `[text](target)`
/// and `![alt](target)`. Deliberately small: it reads the sources this
/// repository actually writes, not every construct CommonMark allows.
fn markdown_link_targets(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue; // a link inside a code block is an example, not a link
        }
        // Alt text and link text can both wrap across lines in this repository's
        // 80-column markdown, so match on `](` rather than trying to pair `[`.
        for (idx, _) in line.match_indices("](") {
            let rest = &line[idx + 2..];
            let Some(end) = rest.find(')') else { continue };
            out.push((n + 1, rest[..end].trim().to_string()));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Criteria 3 and 4 — the two tag lines, and what a pull request may reach
// ---------------------------------------------------------------------------

/// **Criterion 3 closes statically.** GitHub tag globs anchor at the start of
/// the ref name, so `v*` cannot match `site-v1` — pushing a binary version tag
/// triggers release.yml and leaves the site alone. There is nothing dynamic to
/// observe; the check is that the two globs stay what they are.
#[test]
fn a_binary_tag_and_a_site_tag_cannot_trigger_each_others_workflow() {
    assert!(read(".github/workflows/release.yml").contains(r#"- "v*""#), "release.yml's tag glob moved");
    assert!(read(".github/workflows/site.yml").contains(r#"- "site-v*""#), "site.yml's tag glob moved");

    // The property those two globs rest on, stated so it cannot be assumed
    // wrongly by whoever edits them next.
    assert!(!"site-v1".starts_with('v'), "a site tag must not match the `v*` glob");
    assert!("v0.1.0".starts_with('v'));
}

/// **Criterion 4, made mechanical.** "A PR touching `site/` builds and does not
/// deploy" is checked by reading the workflow: the pull-request path reaches
/// exactly one job, that job names no Cloudflare secret, and the job that does
/// is guarded on the tag ref.
///
/// This matters more than it looks. It is also the repository's **first PR
/// CI** — `code:lint` and `code:test` run on tags and on `workflow_dispatch`
/// and have never run on a pull request.
#[test]
fn the_pull_request_path_builds_and_cannot_reach_a_deploy_secret() {
    let workflow = read(".github/workflows/site.yml");

    // **Job spans, not a two-way slice.** An earlier version cut the file at
    // `\n  build:` and `\n  deploy:` and asserted over the two halves. Two
    // ordinary edits walked straight past it, both found by mutation: a
    // workflow-level `env:` holding the token sits *above* `jobs:`, and so
    // above the first cut; and a third job appended after `deploy:` lands
    // inside the "deploy" half, where the real deploy job already satisfies
    // every assertion. Enumerating each job closes both.
    let (preamble, jobs) = split_jobs(&workflow);

    assert!(
        !preamble.contains("secrets."),
        "a secret is referenced above `jobs:` — a workflow-level `env` reaches every job, \
         including the one a pull request runs:\n{preamble}"
    );

    assert!(jobs.len() >= 2, "expected at least a build and a deploy job, found {}", jobs.len());
    let names: Vec<&str> = jobs.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"build"), "site.yml lost its `build` job; found {names:?}");
    assert!(names.contains(&"deploy"), "site.yml lost its `deploy` job; found {names:?}");

    for (name, body) in &jobs {
        if *name == "build" {
            assert!(body.contains("mise run site:build"), "the PR job no longer builds the site");
            assert!(!body.contains("wrangler"), "the job a pull request runs deploys");
            assert!(!body.contains("secrets."), "the job a pull request runs names a secret:\n{body}");
        } else {
            // Stated for every non-build job rather than for `deploy` by name,
            // so that adding a job cannot quietly create an unguarded path.
            assert!(
                body.contains("if: startsWith(github.ref, 'refs/tags/site-v')"),
                "job `{name}` has no site-v tag guard, so a pull request could reach it:\n{body}"
            );
        }
    }

    let deploy = jobs.iter().find(|(n, _)| *n == "deploy").map(|(_, b)| b).expect("checked above");
    assert!(deploy.contains("secrets.CLOUDFLARE_API_TOKEN"), "the deploy job stopped authenticating");

    // Every secret in the file must sit inside a tag-guarded job, wherever it
    // was written.
    let guarded: String = jobs.iter().filter(|(n, _)| *n != "build").map(|(_, b)| b.as_str()).collect();
    assert_eq!(
        workflow.matches("secrets.").count(),
        guarded.matches("secrets.").count(),
        "a `secrets.` reference lives outside a tag-guarded job"
    );

    assert!(workflow.contains("pull_request:"), "site.yml has no pull_request trigger");

    // **The path filter, widened by `website/02-config-generator`.** The
    // generator page builds its form from the committed schema and the shipped
    // defaults, staged into `site/static/` by `site:assets` — so a pull
    // request renaming a config key or changing a shipped default changes what
    // that page renders while touching nothing under `site/`. With the filter
    // as it was, the build that would have caught it never ran; the first sign
    // would have been a deployed form describing a schema the binary no longer
    // has. The task that does the staging is in the list for the same reason.
    for path in ["site/**", "schemas/**", "assets/claude-status.defaults.json", ".config/mise/tasks/site/**"] {
        assert!(
            workflow.contains(&format!("- \"{path}\"")),
            "the pull_request path filter no longer covers {path}, so a change to it would not rebuild the site"
        );
    }
}

/// Splits a workflow into everything before `jobs:` and each job under it.
///
/// A job opens at exactly two spaces of indent and runs to the next line at
/// that indent, or EOF. Enough structure to ask "which job is this secret in?"
/// without taking a YAML dependency for one assertion.
fn split_jobs(workflow: &str) -> (String, Vec<(&str, String)>) {
    let jobs_at = workflow.find("\njobs:").expect("site.yml has no `jobs:` block");
    let preamble = workflow[..jobs_at].to_string();

    let mut jobs: Vec<(&str, String)> = Vec::new();
    let mut current: Option<(&str, Vec<&str>)> = None;
    for line in workflow[jobs_at..].lines() {
        let is_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_header {
            if let Some((name, body)) = current.take() {
                jobs.push((name, body.join("\n")));
            }
            current = Some((line.trim().trim_end_matches(':'), vec![line]));
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some((name, body)) = current.take() {
        jobs.push((name, body.join("\n")));
    }
    (preamble, jobs)
}

// ---------------------------------------------------------------------------
// Criterion 7 — the readme is a pointer
// ---------------------------------------------------------------------------

/// **Criterion 7, made mechanical.** "Is a pointer and contains no
/// configuration reference that the site also carries" is a judgement; these
/// are the three marks of the thing it is judging.
///
/// The readme was a 297-line project page one commit before this cycle. Cutting
/// it to a pointer deleted roughly 250 lines — three fenced config examples and
/// four tables — and every one of those facts now lives on the site. This test
/// is what stops them growing back here, because "just a short configuration
/// section" is exactly how two canonical documents start.
#[test]
fn the_readme_is_a_pointer_and_not_a_second_configuration_reference() {
    let readme = read("readme.md");

    let lines = readme.lines().count();
    assert!(lines < 100, "readme.md is {lines} lines — it is growing back into a second copy of the site");

    for fence in ["```json", "```jsonc"] {
        assert!(
            !readme.contains(fence),
            "readme.md carries a {fence} block — configuration examples belong on the site, in one place"
        );
    }

    // Config key names, which is what a configuration reference is made of. A
    // pointer names pages; it does not name keys.
    for key in [
        "projectName",
        "worktreePattern",
        "refreshMinutes",
        "defaultFg",
        "typeSymbols",
        "descBudgetFraction",
        "sepThin",
    ] {
        assert!(!readme.contains(key), "readme.md names the config key `{key}` — that reference lives on the site now");
    }

    // And it points. A readme that dropped the configuration reference without
    // linking to where it went is worse than the one it replaced.
    assert!(readme.contains("https://claude-status.virajp.dev"), "readme.md no longer links to the site");
}

/// The site is where users are sent, so the two places that send them there
/// have to agree on the address. `--help`'s copy is pinned by `src/_runtime/`
/// and by `tests/e2e.rs`; this is the pair the *docs* own.
#[test]
fn the_readme_and_the_behaviour_contract_name_the_same_site() {
    const SITE: &str = "https://claude-status.virajp.dev";
    assert!(read("readme.md").contains(SITE));
    assert!(
        read("docs/spec/statusline-behaviour.md").contains(SITE),
        "the behaviour contract no longer names the site as the user-facing documentation — \
         `spec-retirement` gates on that pointer existing"
    );
}

// ---------------------------------------------------------------------------
// Criterion 8 — readable on a phone
// ---------------------------------------------------------------------------

/// **Criterion 8, restated.** "Readable on a phone and the nav works" needs a
/// headless browser, and adding one is a JavaScript toolchain — which
/// criterion 1 forbids. The two criteria cannot both be satisfied literally.
///
/// So this is a **static proxy** for the three things that actually break a
/// docs page on a phone, and the real check is a human one at the gate:
///
///   1. a `<meta name="viewport">` — without it mobile Safari lays the page out
///      at 980px and scales it down, which makes every media query below
///      measure against the wrong width
///   2. at least one `@media` breakpoint
///   3. a nav of real `<a>` elements that needs no JavaScript to open — the
///      usual mobile-nav failure is a hamburger behind a script
///
/// The proxy is honest about being one: it cannot tell you the page *looks*
/// right, only that it is not built in the way that guarantees it does not.
#[test]
fn the_layout_carries_the_static_marks_of_a_readable_phone_page() {
    let base = read("site/templates/base.html");
    assert!(
        base.contains(r#"<meta name="viewport""#) && base.contains("width=device-width"),
        "base.html lost its viewport meta — every breakpoint below is measured against a 980px lie"
    );

    let css = read("site/static/style.css");
    assert!(css.contains("@media"), "style.css has no breakpoint at all");

    // The nav is anchors.
    let nav_at = base.find("<nav").expect("base.html has a nav");
    let nav = &base[nav_at..base[nav_at..].find("</nav>").map(|i| nav_at + i).expect("the nav closes")];
    assert!(nav.matches("<a ").count() >= 4, "the nav is no longer a set of plain links: {nav}");

    // The nav is anchors, and NOTHING INSIDE IT is a script. Narrower than the
    // scan this used to be, and deliberately so: `website/02-config-generator`
    // added the site's first JavaScript, so "no script anywhere" stopped being
    // true. What the criterion was ever about is right here — the usual mobile
    // nav failure is a hamburger behind a script, and a nav that cannot be
    // opened without one is broken for everybody the script fails for.
    // `only_allowlisted_paths_under_site_may_carry_a_script` holds the rest.
    assert!(
        !nav.contains("<script"),
        "the nav has grown a script — a hamburger behind JavaScript is the failure this criterion names: {nav}"
    );
}

/// **The `<script>` guard, after the site grew a script.**
///
/// This test replaced a blanket "no `<script>` in any tracked `site/` HTML or
/// CSS" assertion, which `website/02-config-generator` made impossible to keep:
/// the config generator is a form, and a form is a script. Deleting the guard
/// would have been the easy move and the wrong one — it is what stops a second
/// script arriving without anyone deciding to allow it.
///
/// So it is stronger than what it replaced, in three ways:
///
/// 1. **A named allowlist**, below. Every other tracked file fails.
/// 2. **Markdown is scanned too.** The old version read `.html` and `.css`
///    only, so a `<script>` written into a content page — which zola passes
///    through verbatim — went straight past it. That hole is closed here.
/// 3. **Each allowlisted file must actually carry one.** An allowlist entry
///    that has gone stale is a permission nobody is using and nobody will
///    notice widening.
///
/// It held ONE path until the copy buttons arrived. A button on every code
/// block needs a script on every page, so "one script, one page" was going to
/// end whichever way that was built; what survives is the property that
/// actually matters, which is that no script arrives unnoticed. The entry was
/// added deliberately, and the count is not the point — the review is.
#[test]
fn only_allowlisted_paths_under_site_may_carry_a_script() {
    /// Both entries are templates rather than markdown, so the content stays
    /// script-free and every exception is a file a reviewer can read in full.
    ///
    /// - `base.html` — the copy buttons, on every page.
    /// - `generate.html` — the config generator's module.
    const ALLOWED: &[&str] = &["site/templates/base.html", "site/templates/generate.html"];

    let mut sources: Vec<String> = tracked_under("site/", ".html");
    sources.extend(tracked_under("site/", ".css"));
    sources.extend(tracked_under("site/", ".md"));

    let mut offenders = Vec::new();
    for rel in &sources {
        // Tera comments stripped first, for the reason
        // `no_template_hardcodes_an_internal_url_behind_the_builds_back`
        // already records: `base.html`'s header comment explains this rule by
        // quoting the bad form, and a test that cannot tell a rule from its
        // counter-example punishes documenting the rule.
        if code(rel).contains("<script") && !ALLOWED.contains(&rel.as_str()) {
            offenders.push(rel.clone());
        }
    }
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "an unallowlisted script arrived under site/. The layout, the nav and every page's content are \
         supposed to work without JavaScript; only these paths may load any, and they are {ALLOWED:?}"
    );

    // The other direction. Without this the allowlist could name a file that
    // stopped having a script — or that stopped existing — and the guard would
    // read as passing while protecting nothing.
    for allowed in ALLOWED {
        assert!(
            sources.iter().any(|rel| rel == allowed),
            "{allowed} is allowlisted for a script but is not a tracked file under site/"
        );
        assert!(
            code(allowed).contains("<script"),
            "{allowed} is allowlisted for a script and no longer has one — remove the entry rather than leaving a spare permission"
        );
    }
}

/// The screenshot the landing page depends on is present and is a real PNG.
/// A landing page for a *visual* tool whose one image 404s is worse than one
/// with no image, and a missing static file is not something `zola build`
/// complains about.
#[test]
fn the_landing_page_screenshot_exists_and_is_a_png() {
    let path: &Path = &root().join("site/static/statusline.png");
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()));

    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "site/static/statusline.png is not a PNG");
    assert!(bytes.len() > 1024, "site/static/statusline.png is {} bytes — that is a placeholder, not a render", bytes.len());

    // Either layer may carry the reference. It used to be markdown; the
    // redesign moved it into the template, because the hero needs the `<img>`
    // to carry real `width`/`height` and markdown cannot write them.
    //
    // Asserted across both rather than repointed at the template, so that
    // moving it back — or into a shortcode — does not fail a test whose
    // subject is "the landing page shows the screenshot", which is what this
    // is actually about.
    let landing = format!("{}{}", read("site/content/_index.md"), read("site/templates/index.html"));
    assert!(landing.contains("statusline.png"), "the landing page no longer shows the screenshot");

    // And the reserved box matches the file, so the image landing does not
    // shift the page. The dimensions were wrong when the hero was first
    // written — 1600x238 against a real 2294x138 — which reserves the wrong
    // aspect ratio and moves everything below it.
    let (w, h) = (
        u32::from_be_bytes(bytes[16..20].try_into().expect("PNG IHDR width")),
        u32::from_be_bytes(bytes[20..24].try_into().expect("PNG IHDR height")),
    );
    if read("site/templates/index.html").contains("statusline.png") {
        let tpl = read("site/templates/index.html");
        assert!(
            tpl.contains(&format!("width=\"{w}\"")) && tpl.contains(&format!("height=\"{h}\"")),
            "the hero reserves a box that is not the screenshot's {w}x{h} — the image will shift the page as it loads"
        );
    }
}

/// **Every landing bullet opens with a bold that can stand as a card title.**
///
/// The landing page renders its list as a card grid, and the CSS promotes each
/// item's leading `<strong>` to the card's heading. So the markdown shape is
/// load-bearing in a way it was not when these were bullets: a bold that is
/// only part of the first clause leaves the body starting mid-sentence.
///
/// That is not hypothetical. `- **Cost as you go**, and — on a seat …` shipped
/// through a build and a passing suite, and rendered a card titled "Cost as you
/// go" whose body began ", and —". Nothing but looking at the page caught it,
/// which is exactly why it is asserted here now.
#[test]
fn every_landing_bullet_opens_with_a_complete_bold_title() {
    let landing = read("site/content/_index.md");

    // Front matter holds a `lede` with its own punctuation; the body starts
    // after the closing fence.
    let body = landing.split("+++").nth(2).expect("the landing has front matter");

    let bullets: Vec<&str> = body.lines().filter(|l| l.starts_with("- ")).collect();
    assert!(
        bullets.len() >= 3,
        "the landing has {} top-level bullets — this test is reading the wrong thing",
        bullets.len()
    );

    for bullet in bullets {
        let rest = bullet.strip_prefix("- ").expect("filtered on this prefix");
        assert!(
            rest.starts_with("**"),
            "landing bullet does not open with a bold, so its card would have no title: {bullet}"
        );
        let title = rest.trim_start_matches("**").split("**").next().unwrap_or("");
        assert!(
            title.ends_with('.'),
            "landing bullet's bold title `{title}` does not end a sentence, so the card body starts mid-clause: {bullet}"
        );
    }
}

/// **Every `@font-face` names a file that exists and is really a woff2.**
///
/// The brand face is IBM Plex, self-hosted rather than pulled from Google. That
/// choice has one failure mode the build cannot see: zola copies `static/`
/// through without reading it, so a deleted or truncated font is a green build
/// that silently falls back to system mono — and the wordmark, the headings and
/// every nav item are mono by design, so the page would look *plausible* while
/// being off-brand everywhere at once.
///
/// Checked by parsing the stylesheet's own `src: url(...)` rather than a list
/// kept here, so adding a weight cannot leave the guard behind.
#[test]
fn every_self_hosted_font_face_resolves_to_a_real_woff2() {
    let css = read("site/static/style.css");

    let refs: Vec<&str> = css
        .match_indices("url(\"")
        .map(|(i, _)| {
            let rest = &css[i + 5..];
            &rest[..rest.find('"').expect("the url literal closes")]
        })
        .filter(|u| u.ends_with(".woff2"))
        .collect();

    assert!(
        refs.len() >= 4,
        "expected at least the four Plex faces, found {} — has the stylesheet stopped self-hosting?",
        refs.len()
    );

    for rel in refs {
        let path = root().join("site/static").join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("style.css asks for {rel}, which is not there: {e}"));

        // `wOF2`. A 404 page or an LFS pointer saved under the right name is
        // the realistic way this goes wrong, and both start with something
        // else.
        assert!(
            bytes.starts_with(b"wOF2"),
            "{rel} is not a woff2 — the browser will ignore it and fall back to system mono"
        );
        assert!(bytes.len() > 4096, "{rel} is {} bytes, which is not a real face", bytes.len());
    }
}

// ---------------------------------------------------------------------------
// website/02-config-generator — the schema-driven form
// ---------------------------------------------------------------------------

/// The word list from `site:assets`'s `for source in …; do` line.
///
/// Parsed, not substring-matched. The assert below used to ask whether the task
/// file *contained* each source path anywhere, and the task's own explanatory
/// header names both paths — so it passed no matter what the staging loop did.
/// Narrowing the loop while leaving that comment intact (the normal thing to
/// do: it is prose about why two files exist, not a manifest) left a green
/// suite and a generator page fetching a 404.
fn staged_sources(assets: &str) -> Vec<String> {
    let line = assets
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("for source in "))
        .expect("site:assets still stages its inputs from a `for source in …; do` loop");

    line.trim_start_matches("for source in ")
        .trim_end_matches("; do")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Both files the config generator loads, and where `site:assets` stages them.
const STAGED: [(&str, &str); 2] = [
    ("schemas/claude-status.schema.json", "site/static/claude-status.schema.json"),
    ("assets/claude-status.defaults.json", "site/static/claude-status.defaults.json"),
];

/// **The two documents reach the browser without a tracked second copy.**
///
/// The page needs the schema (the shape) *and* `assets/claude-status.defaults.json`
/// (the values), because the schema deliberately carries four `default` values
/// against a tree of about a hundred leaves — see
/// `tests/schema.rs::the_only_defaults_in_the_schema_are_the_four_caps`, which
/// is what keeps it that way.
///
/// Committing copies under `site/static/` would have been the obvious way to
/// serve them and is the one that breaks. dprint's `includes` is `**/*.json`
/// and its exclusion of the defaults asset is written at **that path only**, so
/// a tracked copy would be reformatted on commit and would then differ, byte
/// for byte, from the file it is a copy of — with nothing to say so. It is the
/// same formatter-versus-generator loop
/// `the_generated_schema_is_already_dprint_formatted` exists for.
///
/// So they are build output: staged by `site:assets`, gitignored, and excluded
/// from dprint as a second line of defence. This test is what stops any of
/// those three quietly coming undone.
#[test]
fn the_schema_and_the_shipped_defaults_are_staged_rather_than_committed() {
    let ignore = read(".gitignore");
    let dprint = read("dprint.json");
    let assets = read(".config/mise/tasks/site/assets");
    let tracked = tracked_files();

    for (source, staged) in STAGED {
        assert!(
            !tracked.iter().any(|p| p == staged),
            "{staged} has been committed — dprint will reformat it into something that no longer matches {source}"
        );
        assert!(
            ignore.lines().any(|l| l.trim() == staged),
            ".gitignore no longer ignores {staged}, so the next build leaves it ready to be committed by accident"
        );
        assert!(
            dprint.contains(&format!("\"{staged}\"")),
            "dprint.json no longer excludes {staged} — the formatter would rewrite a file that has to stay byte-identical to {source}"
        );
        assert!(
            staged_sources(&assets).iter().any(|s| s == source),
            "the site:assets task no longer stages {source}, so the generator page would load a 404"
        );

        // Byte equality, when a build has actually run. Conditional and loud
        // about it: a fresh checkout has no `site/static/` copy at all, and an
        // assertion that silently passes in that state would be the vacuous
        // guard this file already has two comments about.
        let staged_path = root().join(staged);
        if staged_path.exists() {
            assert_eq!(
                std::fs::read(root().join(source)).expect("the source exists"),
                std::fs::read(&staged_path).expect("the staged copy is readable"),
                "{staged} has drifted from {source} — re-run `mise run site:assets`"
            );
        } else if std::env::var_os("CI").is_some() {
            panic!(
                "{staged} is missing under CI. `code:test` stages it through its `site:assets` dependency, so an absence here means that dependency is gone and this byte comparison has been passing without comparing anything."
            );
        } else {
            eprintln!("skipped the byte comparison for {staged}: no build has run in this checkout");
        }
    }

    // Neither list may grow without the other. `.gitignore` and `dprint.json`
    // name the staged copies by exact path, so a third file staged by the task
    // alone would be neither ignored nor dprint-excluded — exactly the
    // formatter-versus-generator trap this test exists to prevent — and every
    // assert above iterates STAGED, so it would never be looked at.
    let mut by_task = staged_sources(&assets);
    by_task.sort();
    let mut by_test: Vec<String> = STAGED.iter().map(|(source, _)| (*source).to_string()).collect();
    by_test.sort();
    assert_eq!(
        by_task, by_test,
        "site:assets and STAGED have drifted apart; a file staged by the task but not named in STAGED is neither gitignored nor dprint-excluded"
    );

    // The staging has to be part of a build rather than a step somebody
    // remembers. Both entry points, because `site:serve` is how the page is
    // ever looked at during development and a preview missing both documents
    // renders an error where the form should be.
    for task in ["build", "serve"] {
        assert!(
            read(&format!(".config/mise/tasks/site/{task}")).contains(r#"depends=["site:assets"]"#),
            "site:{task} no longer depends on site:assets"
        );
    }
}

/// **Acceptance criterion 1, in the only form that can gate a regression.**
///
/// The criterion is "a key added to the schema shows in the form with no
/// hand-edit to the page". It cannot be tested forwards here: a key cannot be
/// added to the committed schema, because the drift check and the `always_run`
/// pre-commit hook both regenerate it from the Rust types. The *forwards*
/// direction is proved in `tests/js/generator.test.mjs`, which feeds the form
/// builder a synthetic schema carrying an invented key.
///
/// This is the negative half, and it is the one that actually catches a
/// regression: **no config key name may appear as a string literal in the
/// site's executable surface**. The moment somebody writes
/// `if (key === "palette")` to make one field look nicer, the form stops being
/// a function of the schema and criterion 1 is gone — with every existing test
/// still green, because the page would still work.
///
/// **Scope, stated rather than assumed.** String literals only, in tracked
/// `site/templates/` and `site/static/`, with comments stripped. Not prose:
/// `site/content/` is documentation and naming the keys is its job. Not bare
/// identifiers: `width`, `name`, `id`, `match`, `head` and `bold` are all
/// config keys *and* ordinary words in CSS and JavaScript, so a word scan would
/// need an allowlist longer than the schema and would still fail on
/// `input[type="number"]`. A literal is where the coupling would actually be
/// written.
#[test]
fn no_config_key_is_hard_coded_into_the_pages_that_build_the_form() {
    let names = schema_property_names();
    assert!(names.len() > 30, "found only {} schema property names — this scan would be weak", names.len());

    let mut sources: Vec<String> = tracked_under("site/templates/", ".html");
    sources.extend(tracked_under("site/static/", ".js"));
    sources.extend(tracked_under("site/static/", ".css"));

    let mut offenders = Vec::new();
    for rel in &sources {
        for literal in string_literals(&code(rel)) {
            if names.contains(literal.as_str()) {
                offenders.push(format!("{rel} -> {literal:?}"));
            }
        }
    }
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "a config key is written into the page. The form is supposed to be a pure function of the schema, \
         so a key added to the Rust types appears with no edit here — that is acceptance criterion 1, and \
         naming one key is how it stops being true."
    );

    // **The control.** A scan that cannot fail looks exactly like a clean one,
    // and this file already carries two comments about guards that could not.
    // Both halves: a real key in code is caught, and the same word inside a
    // comment is not — every doc block in this repository would trip a scanner
    // that could not tell the difference.
    let probe = "const x = \"palette\"; // and a comment saying \"palette\"";
    let found = string_literals(probe);
    assert_eq!(found, vec!["palette".to_string()], "the literal scanner is not reading what it claims to");
    assert!(names.contains("palette"), "the schema no longer has the key this control is built on");
    assert!(!names.contains("paletteProperty"), "the control's lookalike is a real key now; pick another");
}

/// Every `properties` key anywhere in the committed schema.
fn schema_property_names() -> BTreeSet<String> {
    fn walk(node: &serde_json::Value, out: &mut BTreeSet<String>) {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "properties"
                        && let Some(properties) = value.as_object()
                    {
                        out.extend(properties.keys().cloned());
                    }
                    walk(value, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }

    let schema: serde_json::Value =
        serde_json::from_str(&read("schemas/claude-status.schema.json")).expect("the committed schema parses");
    let mut out = BTreeSet::new();
    walk(&schema, &mut out);
    out
}

/// Every quoted string in a source file, with comments skipped.
///
/// Small on purpose — it reads the three languages this site is written in
/// (JavaScript, CSS, HTML), not every construct they allow. Template literals
/// are taken whole, so a key interpolated into one would be missed; nothing
/// here does that, and the alternative is parsing JavaScript.
fn string_literals(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            quote @ ('"' | '\'' | '`') => {
                i += 1;
                let mut literal = String::new();
                while i < chars.len() && chars[i] != quote {
                    if chars[i] == '\\' {
                        i += 1; // an escaped quote does not close the string
                    }
                    if i < chars.len() {
                        literal.push(chars[i]);
                    }
                    i += 1;
                }
                i += 1;
                out.push(literal);
            }
            _ => i += 1,
        }
    }
    out
}

/// **Criterion 8, restated.** "Degrades to readable documentation rather than a
/// blank area" needs a browser with JavaScript switched off, and there is no
/// headless browser here — adding one is the toolchain `website/01-site`'s
/// criterion 1 forbids, and the same trade is recorded above
/// `the_layout_carries_the_static_marks_of_a_readable_phone_page`.
///
/// So the page is built the way that makes the criterion true by construction,
/// and this asserts the construction: **the reference is real static content**
/// and the script replaces exactly one element. A `<noscript>` block would not
/// do — it is a second copy of the documentation that nothing checks, and the
/// copy that rots is always the one nobody reads.
///
/// The real check is a human one at the gate: open the page with JavaScript
/// disabled and read it.
#[test]
fn the_generator_page_reads_as_documentation_without_its_script() {
    let page = read("site/content/generate.md");

    assert!(
        page.contains(r#"<div id="config-generator">"#),
        "the generator page lost the element the script mounts into"
    );
    assert!(!page.contains("<script"), "the generator's markdown grew a script — the module belongs in the template");
    assert!(!page.contains("<noscript"), "a <noscript> block is a second copy of the docs that nothing checks");

    // The facts a user needs in order to write this file by hand, which is
    // what "readable documentation" has to mean here. Each is a rule the form
    // enforces silently and the prose has to say out loud.
    for fact in [
        "~/.config/claude-status/config.json", // where the file goes
        "$schema",                             // what is always emitted
        "follows the binary forward",          // why only non-defaults
        "wholesale",                           // why a touched list comes out whole
        "revert to shipped",                   // why "remove" is not a delete
        "palette name",                        // the four colour forms
        "hex string",
        "RGB triple",
        "projectName", // the one key that belongs in the other file
        "__proto__",   // the key names that can never take effect
    ] {
        assert!(page.contains(fact), "the generator page no longer explains {fact:?} — with the script off, nothing else does");
    }

    // And it survives the build, outside any script.
    //
    // This one is **developer-only, and knowingly so**: it needs `site/public/`,
    // which only a full `zola` build produces. `code:test` stages the two JSON
    // assets (via `site:assets`) but deliberately does not pull `zola` into the
    // Rust test path, and the one workflow that builds the site (`site.yml`)
    // never runs this suite. So unlike the byte comparison above, this cannot
    // be made CI-strict without coupling the two — it is a local convenience,
    // not a gate. Recorded as a gap in the cycle plan rather than dressed up.
    let built = root().join("site/public/generate/index.html");
    if built.exists() {
        let html = std::fs::read_to_string(&built).expect("the built page is readable");
        let before_script = html.split("<script").next().expect("split yields at least one part");
        assert!(
            before_script.contains("~/.config/claude-status/config.json"),
            "the built page's reference is not present ahead of its script"
        );
        assert!(before_script.contains(r#"id="config-generator""#), "the built page lost the mount element");
    } else {
        eprintln!("skipped the built-page check: run `mise run site:build` in this checkout");
    }
}

/// **The generator's emitter and form builder, run.**
///
/// `tests/js/generator.test.mjs` is the real assertion set — criteria 2 and 3,
/// the five open maps, the prototype keys, the `null`-versus-absent rule, and
/// criterion 1's forward direction against a synthetic schema. It runs against
/// the **real** committed schema and the **real** shipped defaults, so it fails
/// when either moves in a way the page cannot render.
///
/// **No toolchain is added by this.** No `package.json`, no lockfile, no
/// `node_modules` — `no_javascript_lockfile_or_node_modules_is_tracked` still
/// holds (it scans for a manifest as well as a lockfile), and `code:sec`'s
/// grype scan still sees no npm ecosystem. `node` is invoked as a bare binary
/// the way this suite already invokes `git`, `mise` and `dprint`, following
/// `tests/schema.rs::the_generated_schema_is_already_dprint_formatted`.
///
/// When `node` is absent this **fails under CI and skips locally**. The skip is
/// not loud — `cargo test` captures a passing test's stdout, so the `eprintln!`
/// below is invisible without `--nocapture`. That is exactly why the CI branch
/// asserts instead: this test is the only thing that runs the 458-line JS
/// harness, and a silent skip would retire criteria 2 and 3 without a single
/// red line anywhere.
///
/// The copy-and-rename is not a flourish. Node decides a file's module system
/// from its extension, and the browser file has to stay `.js` so a static host
/// serves it as `text/javascript`.
#[test]
fn the_generators_pure_core_holds_against_the_real_schema() {
    let Some(node) = node_on_path() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "no `node` on PATH under CI. This test is the only thing that runs tests/js/generator.test.mjs, so skipping it leaves the generator's emitter entirely unguarded while the suite still reports green."
        );
        eprintln!("skipped: no `node` on PATH — the generator's core was not exercised");
        return;
    };

    let dir = tempfile::TempDir::new().expect("a temp dir");
    std::fs::copy(root().join("site/static/config-generator.js"), dir.path().join("generator.mjs")).expect("the module copies");
    std::fs::copy(root().join("tests/js/generator.test.mjs"), dir.path().join("generator.test.mjs")).expect("the harness copies");

    let run = |extra: &[&str]| {
        std::process::Command::new(&node)
            .arg(dir.path().join("generator.test.mjs"))
            .arg(root().join("schemas/claude-status.schema.json"))
            .arg(root().join("assets/claude-status.defaults.json"))
            .args(extra)
            .output()
            .expect("node runs")
    };

    let out = run(&[]);
    assert!(
        out.status.success(),
        "the config generator's core no longer matches the binary's writer:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // **The control**, and it is not optional. A harness invoked wrongly — bad
    // path, wrong flag, a `node` that refuses the module — exits non-zero and
    // the assertion above catches that; but a harness whose assertions never
    // run exits **zero**, and looks exactly like a clean pass. `--self-check`
    // asserts something false on purpose, so this proves the run reached the
    // assertions at all.
    let control = run(&["--self-check"]);
    assert!(
        !control.status.success(),
        "the harness passed with a deliberately false assertion in it — it is not running its checks:\n{}{}",
        String::from_utf8_lossy(&control.stdout),
        String::from_utf8_lossy(&control.stderr),
    );
}

/// `node` if it is on PATH, resolved the way a shell would.
fn node_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join("node")).find(|candidate| candidate.is_file())
}
