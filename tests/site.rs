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

    let build_at = workflow.find("\n  build:").expect("site.yml has a `build` job");
    let deploy_at = workflow.find("\n  deploy:").expect("site.yml has a `deploy` job");
    assert!(build_at < deploy_at, "the jobs were reordered — the slice below no longer means what it says");

    let build_job = &workflow[build_at..deploy_at];
    let deploy_job = &workflow[deploy_at..];

    assert!(build_job.contains("mise run site:build"), "the PR job no longer builds the site");
    assert!(
        !build_job.contains("secrets."),
        "the job a pull request runs names a secret — a fork PR must not be able to reach one:\n{build_job}"
    );
    assert!(!build_job.contains("wrangler"), "the job a pull request runs deploys");

    // And the deploy is fenced off behind the tag, not merely behind the
    // absence of a reason to run.
    assert!(
        deploy_job.contains("if: startsWith(github.ref, 'refs/tags/site-v')"),
        "the deploy job lost its tag guard, so a pull request could reach it:\n{deploy_job}"
    );
    assert!(deploy_job.contains("secrets.CLOUDFLARE_API_TOKEN"), "the deploy job stopped authenticating");

    // The PR trigger exists at all — the criterion is about a PR *touching*
    // site/, so the path filter is part of it.
    assert!(workflow.contains("pull_request:"), "site.yml has no pull_request trigger");
    assert!(workflow.contains(r#"- "site/**""#), "the pull_request path filter no longer covers site/");
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

    // The nav is anchors, and nothing on this site is a script.
    let nav_at = base.find("<nav").expect("base.html has a nav");
    let nav = &base[nav_at..base[nav_at..].find("</nav>").map(|i| nav_at + i).expect("the nav closes")];
    assert!(nav.matches("<a ").count() >= 4, "the nav is no longer a set of plain links: {nav}");

    let mut markup: Vec<String> = tracked_under("site/", ".html");
    markup.extend(tracked_under("site/", ".css"));
    for rel in markup {
        assert!(
            !read(&rel).contains("<script"),
            "{rel} ships JavaScript — the nav and the layout are supposed to work without any"
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
    assert!(
        read("site/content/_index.md").contains("statusline.png"),
        "the landing page no longer shows the screenshot"
    );
}
