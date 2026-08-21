//! Git resolution: filesystem first, subprocess only where unavoidable.
//!
//! Root and branch are read from the filesystem — never from `git` — because
//! this is a hot path and a `git rev-parse` is a whole process to learn
//! something a file already says.
//!
//! **Deviation from the contract**, which says "two subprocesses, 250 ms each":
//! there are up to *four*, and they run on two threads under **one shared
//! 250 ms deadline**. Sequentially at 250 ms each the worst case is about a
//! second, on a bar that must never block.

use std::path::{Component, Path, PathBuf};

use crate::config::matcher::Matcher;
use crate::proc::{Deadline, run_bounded};

/// The whole git budget for one render.
pub const BUDGET_MS: u64 = 250;

/// How far up the tree to look for a `.git`.
const MAX_WALK: usize = 40;

#[derive(Debug, Clone, Default)]
pub struct GitFacts {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    pub ahead: bool,
    pub additions: u64,
    pub deletions: u64,
    pub worktree_subpath: Option<String>,
}

/// Resolves everything the `branch` and `worktree` segments need.
///
/// Convenience for tests and for a caller that has no config to load. The real
/// render splits this: the root is found *first*, because the repo config layer
/// is read from it, and only then is the pattern available to match with.
pub fn resolve(cwd: Option<&Path>, worktree: &Matcher) -> GitFacts {
    let Some(cwd) = cwd else {
        return GitFacts::default();
    };
    let (root, branch) = find_root_and_branch(cwd);
    let mut facts = GitFacts { worktree_subpath: worktree_subpath(cwd, worktree), root, branch, ..Default::default() };
    resolve_markers(&mut facts);
    facts
}

/// Runs the two git pipelines and fills in the markers.
///
/// Gated on a **branch**, not on a root: a repo whose HEAD is empty has a root
/// but no branch, and runs no subprocesses at all.
pub fn resolve_markers(facts: &mut GitFacts) {
    let (Some(root), Some(_)) = (facts.root.clone(), facts.branch.as_ref()) else {
        return;
    };

    // The two pipelines are independent, so they share one deadline rather than
    // taking one each.
    let deadline = Deadline::in_ms(BUDGET_MS);
    let (ahead, dirty) = std::thread::scope(|s| {
        let ahead = s.spawn(|| ahead_count(&root, deadline) > 0);
        let dirty = s.spawn(|| dirty_counts(&root, deadline));
        (ahead.join().unwrap_or(false), dirty.join().unwrap_or_default())
    });

    facts.ahead = ahead;
    (facts.additions, facts.deletions) = dirty;
}

/// What one directory's `.git` told us.
enum Probe {
    /// `.git` resolved and `HEAD` was read. The walk stops here. The branch is
    /// `None` when HEAD was empty — the root is still this directory.
    Resolved(Option<String>),
    /// `.git` is a pointer file whose `gitdir:` line could not be parsed. The
    /// walk stops here too, with a root but no branch.
    RootOnly,
    /// Nothing usable here — including a `.git` directory whose `HEAD` is
    /// missing or unreadable. Keep walking.
    Continue,
}

/// Walks up from `start` looking for `.git`, at most [`MAX_WALK`] levels.
///
/// A `.git` **directory** means read `.git/HEAD`; a `.git` **file** is a
/// worktree or submodule pointer — parse `gitdir: <path>` and read `HEAD`
/// there.
///
/// A `.git` whose `HEAD` cannot be *read* does **not** stop the walk: it
/// continues to the parent, so a nested repo with a broken HEAD reports the
/// outer repo. A HEAD that reads but says nothing useful *does* stop it. That
/// asymmetry reproduces the old implementation's try-block scoping, where an IO
/// throw escaped the per-directory attempt but a parse miss returned.
pub fn find_root_and_branch(start: &Path) -> (Option<PathBuf>, Option<String>) {
    let mut dir = start;
    for _ in 0..MAX_WALK {
        match probe(dir) {
            Probe::Resolved(branch) => return (Some(dir.to_path_buf()), branch),
            Probe::RootOnly => return (Some(dir.to_path_buf()), None),
            Probe::Continue => {}
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => break,
        }
    }
    (None, None)
}

fn probe(dir: &Path) -> Probe {
    let dot_git = dir.join(".git");
    let Ok(meta) = std::fs::metadata(&dot_git) else {
        return Probe::Continue;
    };

    let git_dir = if meta.is_dir() {
        dot_git
    } else {
        let Ok(pointer) = std::fs::read_to_string(&dot_git) else {
            return Probe::Continue;
        };
        match pointer.trim().strip_prefix("gitdir:").map(str::trim).filter(|t| !t.is_empty()) {
            // A relative gitdir is relative to the directory *containing*
            // `.git`, and is normalised lexically — the filesystem is never
            // consulted, so a symlinked worktree still reports what git wrote.
            Some(target) => normalise(&dir.join(target)),
            None => return Probe::RootOnly,
        }
    };

    match std::fs::read_to_string(git_dir.join("HEAD")) {
        Ok(head) => Probe::Resolved(parse_head(&head)),
        Err(_) => Probe::Continue,
    }
}

/// `ref:` + optional whitespace + `refs/heads/<branch>` → the branch; anything
/// else → the first seven characters of the file.
///
/// So a detached HEAD gives the short SHA, and a HEAD holding
/// `ref: refs/tags/v1` gives the branch `"ref: re"`. That is what the old
/// implementation rendered and it is preserved deliberately — it is reachable
/// only in a repo state git itself does not create.
///
/// An empty HEAD gives `None`, which suppresses the branch segment *and* both
/// git subprocesses, exactly as the old implementation's falsy `""` did.
fn parse_head(content: &str) -> Option<String> {
    let head = content.trim();
    if head.is_empty() {
        return None;
    }
    let symbolic = head.strip_prefix("ref:").map(str::trim_start).and_then(|r| r.strip_prefix("refs/heads/"));
    match symbolic {
        Some(branch) if !branch.is_empty() && !branch.contains('\n') => Some(branch.to_string()),
        _ => Some(head.chars().take(7).collect()),
    }
}

/// Lexical `.`/`..` resolution, matching `path.normalize`. Never touches the
/// filesystem — `canonicalize` would resolve symlinks and require the path to
/// exist, which diverges on exactly the worktree and submodule layouts this is
/// for.
///
/// A `..` that would climb above an absolute root is **dropped**; on a relative
/// path it is kept.
fn normalise(path: &Path) -> PathBuf {
    let rooted = path.has_root();
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !rooted {
                    out.push(Component::ParentDir);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Splits `cwd` on `/`, drops empty components, and takes everything after the
/// **last** component matching the pattern. Nothing after it — or no match at
/// all — means this is not a worktree.
///
/// The subpath is rejoined with `/` because it is display text, not a path to
/// open. This briefly split on `\` as well, for Windows; the `macos-only` cycle
/// removed the platform, and on macOS a backslash is a legal character in a
/// directory name rather than a separator — so accepting it was a latent bug
/// here, not portability.
pub fn worktree_subpath(cwd: &Path, pattern: &Matcher) -> Option<String> {
    let parts: Vec<&str> = cwd.to_str()?.split('/').filter(|p| !p.is_empty()).collect();
    let last_match = parts.iter().rposition(|p| pattern.is_match(p))?;
    let tail = parts.get(last_match + 1..)?;
    (!tail.is_empty()).then(|| tail.join("/"))
}

/// `↑` when the branch is ahead of its upstream. Any error — including no
/// upstream at all — is zero.
fn ahead_count(root: &Path, deadline: Deadline) -> u64 {
    run_bounded("git", &["rev-list", "--count", "@{upstream}..HEAD"], root, deadline)
        .and_then(|out| out.trim().parse().ok())
        .unwrap_or(0)
}

/// Sums the working tree's additions and deletions.
///
/// `diff --numstat HEAD` falls back to `--cached` for a repo with no commits.
/// Untracked files then add exactly **one** to additions, however many there
/// are. If the untracked probe fails, the whole marker is dropped even though
/// numstat succeeded — a partial count would be a quietly wrong number.
fn dirty_counts(root: &Path, deadline: Deadline) -> (u64, u64) {
    let numstat = run_bounded("git", &["diff", "--numstat", "HEAD"], root, deadline)
        .or_else(|| run_bounded("git", &["diff", "--numstat", "--cached"], root, deadline));
    let Some(numstat) = numstat else {
        return (0, 0);
    };

    let (mut additions, deletions) = parse_numstat(&numstat);

    let Some(untracked) = run_bounded("git", &["ls-files", "--others", "--exclude-standard"], root, deadline) else {
        return (0, 0);
    };
    if !untracked.trim().is_empty() {
        additions += 1;
    }

    (additions, deletions)
}

/// Sums `<additions>\t<deletions>\t<path>` lines.
///
/// Each count is `\d+` or `-`, and the two sides are suppressed
/// **independently**. Git emits `-` on both sides for a binary file, so a
/// change touching only binaries renders **clean** — which looks like a bug and
/// is the shipped behaviour.
///
/// A line must carry the trailing tab before the path; anything else, including
/// the blank final line, is skipped.
fn parse_numstat(out: &str) -> (u64, u64) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in out.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(add), Some(del), Some(_path)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let (Some(add), Some(del)) = (count(add), count(del)) else {
            continue;
        };
        additions += add;
        deletions += del;
    }
    (additions, deletions)
}

/// One numstat count: a number, or `-` meaning "contributes nothing".
fn count(field: &str) -> Option<u64> {
    match field {
        "-" => Some(0),
        n => n.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn matcher() -> Matcher {
        Matcher::compile("worktree").unwrap()
    }

    /// Builds `<base>/.git/HEAD` with the given contents.
    fn repo(base: &Path, head: &str) {
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::write(base.join(".git").join("HEAD"), head).unwrap();
    }

    #[test]
    fn a_plain_repo_reports_its_root_and_branch() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/main\n");
        let nested = dir.path().join("src").join("deep");
        fs::create_dir_all(&nested).unwrap();

        let (root, branch) = find_root_and_branch(&nested);
        assert_eq!(root.as_deref(), Some(dir.path()));
        assert_eq!(branch.as_deref(), Some("main"));
    }

    #[test]
    fn a_branch_name_may_contain_slashes() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/feature/a-b\n");
        assert_eq!(find_root_and_branch(dir.path()).1.as_deref(), Some("feature/a-b"));
    }

    #[test]
    fn a_detached_head_reports_seven_characters() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "d0527dd592f568a96b1eae646df1a8f98f8f1885\n");
        assert_eq!(find_root_and_branch(dir.path()).1.as_deref(), Some("d0527dd"));
    }

    #[test]
    fn a_non_branch_ref_renders_its_first_seven_characters() {
        // Deliberately faithful: `ref: refs/tags/v1` is not a heads ref, so it
        // falls through to the short-SHA branch and renders "ref: re".
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/tags/v1\n");
        assert_eq!(find_root_and_branch(dir.path()).1.as_deref(), Some("ref: re"));
    }

    #[test]
    fn a_worktree_pointer_file_is_followed() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("real-git-dir");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/wt\n").unwrap();

        let wt = dir.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();

        let (root, branch) = find_root_and_branch(&wt);
        assert_eq!(root.as_deref(), Some(wt.as_path()));
        assert_eq!(branch.as_deref(), Some("wt"));
    }

    #[test]
    fn a_relative_gitdir_resolves_against_the_directory_holding_dot_git() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("store").join("gitdir");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/rel\n").unwrap();

        let wt = dir.path().join("checkout");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: ../store/gitdir\n").unwrap();

        assert_eq!(find_root_and_branch(&wt).1.as_deref(), Some("rel"));
    }

    #[test]
    fn a_broken_head_does_not_stop_the_walk() {
        // The inner repo has a `.git` but no readable HEAD. The walk must
        // continue and report the *outer* repo, not give up.
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/outer\n");

        let inner = dir.path().join("vendor").join("inner");
        fs::create_dir_all(inner.join(".git")).unwrap();

        let (root, branch) = find_root_and_branch(&inner);
        assert_eq!(root.as_deref(), Some(dir.path()));
        assert_eq!(branch.as_deref(), Some("outer"), "the outer repo answers");
    }

    #[test]
    fn an_empty_head_stops_the_walk_with_a_root_but_no_branch() {
        // The asymmetry with the test above is deliberate: an unreadable HEAD
        // is an IO error and keeps walking, a *readable* but empty one is an
        // answer. No branch means no git subprocesses run at all.
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/outer\n");
        let inner = dir.path().join("inner");
        repo(&inner, "   \n");

        let (root, branch) = find_root_and_branch(&inner);
        assert_eq!(root.as_deref(), Some(inner.as_path()));
        assert_eq!(branch, None);
    }

    #[test]
    fn an_unparseable_gitdir_pointer_stops_the_walk_with_a_root_but_no_branch() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/outer\n");
        let inner = dir.path().join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join(".git"), "this is not a gitdir pointer\n").unwrap();

        let (root, branch) = find_root_and_branch(&inner);
        assert_eq!(root.as_deref(), Some(inner.as_path()), "the pointer file still marks a root");
        assert_eq!(branch, None);
    }

    #[test]
    fn a_head_written_without_a_space_still_parses() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref:refs/heads/tight\n");
        assert_eq!(find_root_and_branch(dir.path()).1.as_deref(), Some("tight"));
    }

    #[test]
    fn markers_do_not_run_without_a_branch() {
        // Gated on the branch, not the root: this must not spawn git.
        let mut facts = GitFacts { root: Some(PathBuf::from("/nonexistent")), branch: None, ..Default::default() };
        resolve_markers(&mut facts);
        assert!(!facts.ahead);
        assert_eq!((facts.additions, facts.deletions), (0, 0));
    }

    #[test]
    fn the_walk_gives_up_past_forty_levels() {
        let dir = tempfile::TempDir::new().unwrap();
        repo(dir.path(), "ref: refs/heads/main\n");

        let mut deep = dir.path().to_path_buf();
        for i in 0..41 {
            deep = deep.join(format!("l{i}"));
        }
        fs::create_dir_all(&deep).unwrap();

        assert_eq!(find_root_and_branch(&deep), (None, None));
    }

    #[test]
    fn no_repo_at_all_resolves_to_nothing() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(find_root_and_branch(dir.path()), (None, None));
    }

    #[test]
    fn worktree_subpath_takes_everything_after_the_last_match() {
        let m = matcher();
        let sub = |p: &str| worktree_subpath(Path::new(p), &m);

        assert_eq!(sub("/Users/x/repo/.worktrees/main-bar").as_deref(), Some("main-bar"));
        assert_eq!(sub("/Users/x/repo/worktrees/feat/src").as_deref(), Some("feat/src"));
        assert_eq!(sub("/a/WORKTREES/b").as_deref(), Some("b"), "matching is case-insensitive");
        assert_eq!(sub("/a/worktree/b/worktrees/c").as_deref(), Some("c"), "the *last* match wins");
        assert_eq!(sub("//a//worktrees//b//").as_deref(), Some("b"), "empty components are dropped");
    }

    #[test]
    fn worktree_subpath_treats_a_backslash_as_an_ordinary_character() {
        // This used to split on `\` too, so Windows paths resolved. macOS has
        // no such paths, and a backslash is a legal character in a directory
        // name here — splitting on it would find a worktree segment inside a
        // name that merely contains one.
        let m = matcher();
        let sub = |p: &str| worktree_subpath(Path::new(p), &m);

        assert_eq!(sub(r"/Users/x/repo\worktrees\feat"), None, "one component, not three");
        assert_eq!(
            sub(r"/Users/x/worktrees/a\b").as_deref(),
            Some(r"a\b"),
            "a backslash inside a component survives verbatim",
        );
    }

    #[test]
    fn worktree_subpath_is_absent_without_a_tail() {
        let m = matcher();
        assert_eq!(worktree_subpath(Path::new("/Users/x/repo/.worktrees"), &m), None);
        assert_eq!(worktree_subpath(Path::new("/Users/x/repo/src"), &m), None);
        assert_eq!(worktree_subpath(Path::new("/"), &m), None);
    }

    #[test]
    fn numstat_suppresses_each_binary_side_independently() {
        // Git emits `-` on both sides for a binary, so a binary-only change
        // renders clean.
        assert_eq!(parse_numstat("-\t-\tassets/logo.png\n"), (0, 0));
        assert_eq!(parse_numstat("3\t1\ta.rs\n-\t-\tb.png\n2\t0\tc.rs\n"), (5, 1));
        // Per-side, not per-line: a half-binary line still contributes.
        assert_eq!(parse_numstat("3\t-\tmixed\n"), (3, 0));
        assert_eq!(parse_numstat("-\t4\tmixed\n"), (0, 4));
    }

    #[test]
    fn numstat_needs_the_tab_before_the_path() {
        assert_eq!(parse_numstat(""), (0, 0));
        assert_eq!(parse_numstat("garbage\n\n4\t2\tok.rs\n"), (4, 2));
        assert_eq!(parse_numstat("4\t2\n"), (0, 0), "a line with no path field is not a numstat line");
        // A rename line carries the arrow in the path field, which is ignored.
        assert_eq!(parse_numstat("1\t2\told.rs => new.rs\n"), (1, 2));
    }

    #[test]
    fn normalise_resolves_dot_and_dotdot_lexically() {
        assert_eq!(normalise(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
        // Cross-checked against node's `path.join`/`path.normalize`.
        assert_eq!(normalise(Path::new("/a/../../b")), PathBuf::from("/b"), "a climb above root is dropped");
        assert_eq!(normalise(Path::new("a/../../b")), PathBuf::from("../b"), "but kept on a relative path");
    }

    #[test]
    fn with_no_cwd_there_are_no_git_facts() {
        let facts = resolve(None, &matcher());
        assert!(facts.root.is_none() && facts.branch.is_none() && !facts.ahead);
    }
}
