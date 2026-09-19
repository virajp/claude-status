//! Git resolution: filesystem first, subprocess only where unavoidable.
//!
//! Root and branch are read from the filesystem — never from `git` — because
//! this is a hot path and a `git rev-parse` is a whole process to learn
//! something a file already says. The project identity is the same: it is read
//! from `.git`, a worktree's `commondir`, a submodule's `modules/` path and the
//! common `config` — still never from `git`.
//!
//! **The budget is one deadline, not one per call.** The original design said
//! "two subprocesses, 250 ms each"; there are up to *four*, and they run on
//! two threads under **one shared 250 ms deadline**. Sequentially at 250 ms each the worst case is about a
//! second, on a bar that must never block.

use std::path::{Component, Path, PathBuf};

use crate::config::matcher::Matcher;
use crate::proc::{Deadline, run_bounded};

/// The whole git budget for one render.
pub const BUDGET_MS: u64 = 250;

/// How far up the tree to look for a `.git`.
const MAX_WALK: usize = 40;

/// Where a repository lives, which is what picks the `project` segment's glyph.
///
/// `Github` and `Gitlab` match the `origin` host by **substring**, so a
/// self-hosted instance counts (`a_self_hosted_gitlab_matches_by_substring`).
/// Any other host is `Remote`
/// (`an_origin_on_another_host_is_kind_remote_named_parent_slash_base`); no
/// `origin` at all is `Git` (`a_config_without_origin_is_kind_git`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Git,
    Remote,
    Github,
    Gitlab,
}

/// The repository's identity: its kind, the name the `project` segment draws
/// without a `projectName` override, and the identity root the name came from.
///
/// The name is the `origin` URL's full path for `Github`/`Gitlab`
/// (`an_origin_on_gitlab_keeps_every_path_segment`) and `parent/base` of the
/// identity root otherwise (`a_plain_repo_is_kind_git_and_named_parent_slash_base`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub kind: ProjectKind,
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct GitFacts {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    pub ahead: bool,
    pub additions: u64,
    pub deletions: u64,
    pub worktree_subpath: Option<String>,
    /// The repository's identity — main checkout in a linked worktree,
    /// outermost superproject in a submodule — for the `project` segment; every
    /// other field describes the checkout `cwd` is in.
    pub project: Option<Project>,
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
    let mut facts = GitFacts {
        worktree_subpath: worktree_subpath(cwd, worktree),
        project: root.as_deref().and_then(project),
        root,
        branch,
        ..Default::default()
    };
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
        match gitdir_pointer(dir, &pointer) {
            Some(target) => target,
            None => return Probe::RootOnly,
        }
    };

    match std::fs::read_to_string(git_dir.join("HEAD")) {
        Ok(head) => Probe::Resolved(parse_head(&head)),
        Err(_) => Probe::Continue,
    }
}

/// Parses a `.git` pointer file's `gitdir: <path>` line into the git dir it
/// names.
///
/// A relative gitdir is relative to `dir` — the directory *containing* `.git` —
/// and is normalised lexically: the filesystem is never consulted, so a
/// symlinked worktree still reports what git wrote. Shared by [`probe`] and
/// [`identity_root`] so the two cannot drift.
fn gitdir_pointer(dir: &Path, pointer: &str) -> Option<PathBuf> {
    let target = pointer.trim().strip_prefix("gitdir:").map(str::trim).filter(|t| !t.is_empty())?;
    Some(normalise(&dir.join(target)))
}

/// The repository's identity for the `project` segment, from the root
/// [`find_root_and_branch`] returned. Filesystem only — no subprocess.
///
/// `None` when a path component the name is built from is not UTF-8: a name we
/// cannot draw honestly is better omitted than drawn with U+FFFD.
pub fn project(root: &Path) -> Option<Project> {
    let (identity, common) = identity_root(root);
    let (kind, path) = origin_url(&common).map(|url| classify(&url)).unwrap_or((ProjectKind::Git, None));
    let name = match (kind, path) {
        (ProjectKind::Github | ProjectKind::Gitlab, Some(path)) => path,
        _ => parent_base(&identity)?,
    };
    Some(Project { kind, name, root: identity })
}

/// The identity root and the common git dir whose `config` names the remote.
///
/// A `.git` **directory** is its own identity. A pointer file names a gitdir;
/// a `commondir` there (a linked worktree) redirects to the common git dir
/// first. That dir is then read two ways, in order:
///
/// - Under a superproject's `.git/modules/` — or a linked worktree's
///   `.git/worktrees/<x>/modules/` — the identity is the directory holding the
///   **topmost** such `.git`, so a submodule, however nested and from wherever
///   it was added, names the outermost superproject
///   (`a_nested_submodule_names_the_outermost_superproject`,
///   `a_submodule_inside_a_linked_worktree_names_the_superproject`,
///   `a_linked_worktree_of_a_submodule_names_the_superproject`).
/// - Otherwise a common dir named `.git`, or hidden like a `.bare` store, is
///   inside its checkout, whose directory is the identity
///   (`a_linked_worktree_names_its_main_checkout`,
///   `a_linked_worktree_of_a_dot_bare_main_names_the_directory_holding_it`); a
///   visible bare `repo.git` is itself the identity, the suffix coming off the
///   name only (`a_linked_worktree_of_a_bare_main_strips_the_dot_git_suffix`).
///
/// A pointer with neither `commondir` nor a `modules` ancestor is its own
/// identity
/// (`a_pointer_with_neither_commondir_nor_modules_is_its_own_project`).
///
/// Lexical throughout, like [`normalise`] — no `canonicalize`.
fn identity_root(root: &Path) -> (PathBuf, PathBuf) {
    let dot_git = root.join(".git");
    let own = || (root.to_path_buf(), dot_git.clone());
    if dot_git.is_dir() {
        return own();
    }
    let Some(git_dir) = std::fs::read_to_string(&dot_git).ok().and_then(|p| gitdir_pointer(root, &p)) else {
        return own();
    };

    let commondir = std::fs::read_to_string(git_dir.join("commondir")).ok();
    let common = match &commondir {
        Some(rel) => normalise(&git_dir.join(rel.trim())),
        None => git_dir,
    };
    if let Some(found) = superproject(&common) {
        return found;
    }
    if commondir.is_none() {
        return (root.to_path_buf(), common);
    }
    let identity = match (common.file_name().and_then(|n| n.to_str()), common.parent()) {
        (Some(name), Some(parent)) if name.starts_with('.') => parent.to_path_buf(),
        _ => common.clone(),
    };
    (identity, common)
}

/// The outermost superproject a git dir sits under, as `(identity, .git dir)`.
///
/// Matches a `.git` component followed by `modules`, or by
/// `worktrees/<x>/modules`; the topmost match wins.
fn superproject(git_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let parts: Vec<Component> = git_dir.components().collect();
    let at = |i: usize, name: &str| parts.get(i).is_some_and(|c| c.as_os_str() == name);
    let i = (0..parts.len())
        .find(|&i| at(i, ".git") && (at(i + 1, "modules") || (at(i + 1, "worktrees") && at(i + 3, "modules"))))?;
    Some((parts[..i].iter().collect(), parts[..=i].iter().collect()))
}

/// The first `url` under `[remote "origin"]` in `<common_git_dir>/config`.
///
/// A minimal INI walk the way git reads it: the section name and the key are
/// case-insensitive, the `"origin"` subsection is not; the header opens the
/// section, any other `[` line closes it; a value loses a trailing `#`/`;`
/// comment and a pair of surrounding double quotes
/// (`origin_url_reads_the_config_the_way_git_does`). Every other remote,
/// `pushurl`, and `insteadOf` rewriting are ignored on purpose
/// (`a_config_without_origin_is_kind_git`), and an origin declared through
/// `[include]`/`[includeIf]` is not followed.
fn origin_url(common_git_dir: &Path) -> Option<String> {
    let config = std::fs::read_to_string(common_git_dir.join("config")).ok()?;
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let (section, sub) = header.trim().split_once(char::is_whitespace).unwrap_or((header, ""));
            in_origin = section.eq_ignore_ascii_case("remote") && sub.trim() == "\"origin\"";
            continue;
        }
        if line.starts_with('[') {
            in_origin = false;
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            return Some(config_value(value));
        }
    }
    None
}

/// One config value: a quoted value is taken verbatim up to its closing quote;
/// otherwise the value is cut at the first `#` or `;` that follows whitespace.
fn config_value(raw: &str) -> String {
    let value = raw.trim();
    if let Some(inner) = value.strip_prefix('"')
        && let Some((quoted, _)) = inner.split_once('"')
    {
        return quoted.to_string();
    }
    let mut end = value.len();
    let mut after_space = true;
    for (i, c) in value.char_indices() {
        if after_space && (c == '#' || c == ';') {
            end = i;
            break;
        }
        after_space = c.is_whitespace();
    }
    value[..end].trim_end().to_string()
}

/// Splits a remote URL into its kind and its path, per
/// `classify_handles_every_accepted_url_shape`.
///
/// `scheme://[user@]host[:port]/path` and scp-like `[user@]host:path` yield a
/// host; the host, lower-cased, is matched by substring. `file://`, a bare
/// path, or anything with no host or an empty path is `Remote` with no path.
/// The path loses a leading `/`, a trailing `/` and a trailing `.git`.
fn classify(url: &str) -> (ProjectKind, Option<String>) {
    let no_host = (ProjectKind::Remote, None);
    let (host_port, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return no_host;
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        (authority, path)
    } else if let Some((before, after)) = url.split_once(':') {
        if before.contains('/') {
            return no_host;
        }
        (before, after)
    } else {
        return no_host;
    };

    let host = host_port.rsplit_once('@').map_or(host_port, |(_, h)| h);
    let host = host.split_once(':').map_or(host, |(h, _)| h).to_ascii_lowercase();
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return no_host;
    }

    let kind = if host.contains("github") {
        ProjectKind::Github
    } else if host.contains("gitlab") {
        ProjectKind::Gitlab
    } else {
        ProjectKind::Remote
    };
    (kind, Some(path.to_string()))
}

/// `parent/base` of the identity root, or `base` alone when there is no parent
/// component. A trailing `.git` on the base is dropped, so a bare `repo.git`
/// reads as `repo`. `None` when a component is not UTF-8.
fn parent_base(identity_root: &Path) -> Option<String> {
    let base = identity_root.file_name()?.to_str()?;
    let base = base.strip_suffix(".git").unwrap_or(base);
    match identity_root.parent().and_then(Path::file_name) {
        Some(parent) => Some(format!("{}/{base}", parent.to_str()?)),
        None => Some(base.to_string()),
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

    /// Builds `<base>/.git/HEAD` plus a `.git/config` holding one remote.
    fn repo_with_remote(base: &Path, remote: &str, url: &str) {
        repo(base, "ref: refs/heads/main\n");
        fs::write(base.join(".git").join("config"), format!("[remote \"{remote}\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/{remote}/*\n")).unwrap();
    }

    /// Writes `<checkout>/.git` as a `gitdir:` pointer to `target`.
    fn pointer(checkout: &Path, target: &str) {
        fs::create_dir_all(checkout).unwrap();
        fs::write(checkout.join(".git"), format!("gitdir: {target}\n")).unwrap();
    }

    #[test]
    fn a_plain_repo_is_kind_git_and_named_parent_slash_base() {
        // The control: no config at all, so nothing but the path names it.
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("acme").join("widget");
        repo(&root, "ref: refs/heads/main\n");

        let p = project(&root).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Git, name: "acme/widget".into(), root: root.clone() });
    }

    #[test]
    fn an_origin_on_github_names_the_url_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("elsewhere");
        repo_with_remote(&root, "origin", "git@github.com:acme/widget.git");

        let p = project(&root).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Github, "acme/widget"));
    }

    #[test]
    fn an_origin_on_gitlab_keeps_every_path_segment() {
        let dir = tempfile::TempDir::new().unwrap();
        repo_with_remote(dir.path(), "origin", "https://gitlab.com/group/sub/repo.git");

        let p = project(dir.path()).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Gitlab, "group/sub/repo"));
    }

    #[test]
    fn a_self_hosted_gitlab_matches_by_substring() {
        let dir = tempfile::TempDir::new().unwrap();
        repo_with_remote(dir.path(), "origin", "ssh://git@gitlab.example.com:2222/team/repo");

        let p = project(dir.path()).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Gitlab, "team/repo"));
    }

    #[test]
    fn an_origin_on_another_host_is_kind_remote_named_parent_slash_base() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("src").join("thing");
        repo_with_remote(&root, "origin", "https://codeberg.org/someone/thing.git");

        let p = project(&root).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Remote, "src/thing"));
    }

    #[test]
    fn a_config_without_origin_is_kind_git() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("acme").join("fork");
        repo_with_remote(&root, "upstream", "git@github.com:acme/widget.git");

        let p = project(&root).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Git, "acme/fork"));
    }

    #[test]
    fn a_linked_worktree_names_its_main_checkout() {
        let dir = tempfile::TempDir::new().unwrap();
        let main = dir.path().join("main");
        repo_with_remote(&main, "origin", "git@github.com:acme/widget.git");
        let wt_gitdir = main.join(".git").join("worktrees").join("wt");
        fs::create_dir_all(&wt_gitdir).unwrap();
        fs::write(wt_gitdir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        fs::write(wt_gitdir.join("commondir"), "../..\n").unwrap();
        let wt = dir.path().join("wt");
        pointer(&wt, &wt_gitdir.display().to_string());

        let p = project(&wt).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Github, name: "acme/widget".into(), root: main });
        // The checkout itself is unchanged: root and branch are the worktree's.
        let (root, branch) = find_root_and_branch(&wt);
        assert_eq!(root.as_deref(), Some(wt.as_path()));
        assert_eq!(branch.as_deref(), Some("feature"));
    }

    #[test]
    fn a_linked_worktree_of_a_bare_main_strips_the_dot_git_suffix() {
        let dir = tempfile::TempDir::new().unwrap();
        let bare = dir.path().join("src").join("repo.git");
        fs::create_dir_all(bare.join("worktrees").join("wt")).unwrap();
        fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(bare.join("config"), "[core]\n\tbare = true\n").unwrap();
        fs::write(bare.join("worktrees").join("wt").join("HEAD"), "ref: refs/heads/wt\n").unwrap();
        fs::write(bare.join("worktrees").join("wt").join("commondir"), "../..\n").unwrap();
        let wt = dir.path().join("wt");
        pointer(&wt, &bare.join("worktrees").join("wt").display().to_string());

        let p = project(&wt).unwrap();
        assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Git, "src/repo"));
        assert_eq!(p.root, bare, "the suffix comes off the name, never the path");
    }

    #[test]
    fn a_linked_worktree_of_a_dot_bare_main_names_the_directory_holding_it() {
        // The `.bare` layout: `widget/{.bare, .git -> .bare, main/}`. The common
        // dir is hidden, so the checkout is the directory holding it.
        let dir = tempfile::TempDir::new().unwrap();
        let widget = dir.path().join("proj").join("widget");
        let bare = widget.join(".bare");
        fs::create_dir_all(bare.join("worktrees").join("main")).unwrap();
        fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(bare.join("config"), "[core]\n\tbare = true\n").unwrap();
        fs::write(bare.join("worktrees").join("main").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(bare.join("worktrees").join("main").join("commondir"), "../..\n").unwrap();
        fs::write(widget.join(".git"), "gitdir: ./.bare\n").unwrap();
        let main = widget.join("main");
        pointer(&main, "../.bare/worktrees/main");

        let p = project(&main).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Git, name: "proj/widget".into(), root: widget });
    }

    #[test]
    fn a_submodule_inside_a_linked_worktree_names_the_superproject() {
        // A submodule added from a linked worktree has its gitdir under
        // `.git/worktrees/<wt>/modules/`, with no `commondir` of its own.
        let dir = tempfile::TempDir::new().unwrap();
        let sup = dir.path().join("acme").join("super");
        repo_with_remote(&sup, "origin", "git@github.com:acme/super.git");
        let wt_gitdir = sup.join(".git").join("worktrees").join("wt");
        fs::create_dir_all(wt_gitdir.join("modules").join("lib")).unwrap();
        fs::write(wt_gitdir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        fs::write(wt_gitdir.join("commondir"), "../..\n").unwrap();
        fs::write(wt_gitdir.join("modules").join("lib").join("HEAD"), "ref: refs/heads/lib\n").unwrap();
        let wt = dir.path().join("wt");
        pointer(&wt, &wt_gitdir.display().to_string());
        let lib = wt.join("lib");
        pointer(&lib, &wt_gitdir.join("modules").join("lib").display().to_string());

        let p = project(&lib).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Github, name: "acme/super".into(), root: sup });
    }

    #[test]
    fn a_linked_worktree_of_a_submodule_names_the_superproject() {
        // `git worktree add` run inside a submodule: the worktree's `commondir`
        // resolves to `<sup>/.git/modules/<sub>`, which is still under the
        // superproject's `.git`.
        let dir = tempfile::TempDir::new().unwrap();
        let sup = dir.path().join("acme").join("super");
        repo_with_remote(&sup, "origin", "git@github.com:acme/super.git");
        let lib_gitdir = sup.join(".git").join("modules").join("lib");
        fs::create_dir_all(lib_gitdir.join("worktrees").join("wt")).unwrap();
        fs::write(lib_gitdir.join("HEAD"), "ref: refs/heads/lib\n").unwrap();
        fs::write(lib_gitdir.join("config"), "[remote \"origin\"]\n\turl = git@github.com:vendor/lib.git\n").unwrap();
        fs::write(lib_gitdir.join("worktrees").join("wt").join("HEAD"), "ref: refs/heads/wt\n").unwrap();
        fs::write(lib_gitdir.join("worktrees").join("wt").join("commondir"), "../..\n").unwrap();
        let wt = dir.path().join("lib-wt");
        pointer(&wt, &lib_gitdir.join("worktrees").join("wt").display().to_string());

        let p = project(&wt).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Github, name: "acme/super".into(), root: sup }, "the superproject's origin, not the submodule's");
    }

    #[test]
    fn origin_url_reads_the_config_the_way_git_does() {
        let dir = tempfile::TempDir::new().unwrap();
        let check = |config: &str, expected: Option<&str>| {
            fs::write(dir.path().join("config"), config).unwrap();
            assert_eq!(origin_url(dir.path()).as_deref(), expected, "config {config:?}");
        };

        check("[Remote \"origin\"]\n\tURL = git@github.com:acme/widget.git\n", Some("git@github.com:acme/widget.git"));
        check("[remote \"origin\"]\n\turl = \"https://github.com/acme/widget.git\"\n", Some("https://github.com/acme/widget.git"));
        check("[remote \"origin\"]\n\turl = git@github.com:acme/widget.git # mirror\n", Some("git@github.com:acme/widget.git"));
        check("[remote \"origin\"]\n\turl = git@github.com:acme/widget.git ; mirror\n", Some("git@github.com:acme/widget.git"));
        check("[remote \"origin\"]\n\turl = git@github.com:acme/widget#1.git\n", Some("git@github.com:acme/widget#1.git"));
        check("[remote \"Origin\"]\n\turl = git@github.com:acme/widget.git\n", None);
        check("[remote \"origin\"]\n\tpushurl = x\n\turl = first\n\turl = second\n", Some("first"));
    }

    #[test]
    fn a_submodule_names_its_superproject() {
        let dir = tempfile::TempDir::new().unwrap();
        let sup = dir.path().join("acme").join("super");
        repo_with_remote(&sup, "origin", "git@github.com:acme/super.git");
        let lib_gitdir = sup.join(".git").join("modules").join("lib");
        fs::create_dir_all(&lib_gitdir).unwrap();
        fs::write(lib_gitdir.join("HEAD"), "ref: refs/heads/lib\n").unwrap();
        fs::write(lib_gitdir.join("config"), "[remote \"origin\"]\n\turl = git@github.com:vendor/lib.git\n").unwrap();
        let lib = sup.join("lib");
        pointer(&lib, "../.git/modules/lib");

        let p = project(&lib).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Github, name: "acme/super".into(), root: sup }, "the superproject's origin, not the submodule's");
        let (root, branch) = find_root_and_branch(&lib);
        assert_eq!(root.as_deref(), Some(lib.as_path()));
        assert_eq!(branch.as_deref(), Some("lib"));
    }

    #[test]
    fn a_nested_submodule_names_the_outermost_superproject() {
        let dir = tempfile::TempDir::new().unwrap();
        let sup = dir.path().join("acme").join("outer");
        repo_with_remote(&sup, "origin", "https://github.com/acme/outer");
        let b_gitdir = sup.join(".git").join("modules").join("a").join("modules").join("b");
        fs::create_dir_all(&b_gitdir).unwrap();
        fs::write(b_gitdir.join("HEAD"), "ref: refs/heads/b\n").unwrap();
        let b = sup.join("a").join("b");
        pointer(&b, "../../.git/modules/a/modules/b");

        let p = project(&b).unwrap();
        assert_eq!(p, Project { kind: ProjectKind::Github, name: "acme/outer".into(), root: sup });
    }

    #[test]
    fn a_pointer_with_neither_commondir_nor_modules_is_its_own_project() {
        let dir = tempfile::TempDir::new().unwrap();
        let real = dir.path().join("store").join("gitdir");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/rel\n").unwrap();
        let wt = dir.path().join("checkout");
        pointer(&wt, "../store/gitdir");

        let p = project(&wt).unwrap();
        let parent = dir.path().file_name().unwrap().to_str().unwrap();
        assert_eq!((p.kind, p.name), (ProjectKind::Git, format!("{parent}/checkout")));
        assert_eq!(p.root, wt);
    }

    #[test]
    fn a_url_with_no_host_is_kind_remote() {
        for url in ["/srv/git/thing.git", "file:///srv/git/thing.git"] {
            let dir = tempfile::TempDir::new().unwrap();
            let root = dir.path().join("mirrors").join("thing");
            repo_with_remote(&root, "origin", url);

            let p = project(&root).unwrap();
            assert_eq!((p.kind, p.name.as_str()), (ProjectKind::Remote, "mirrors/thing"), "url {url}");
        }
    }

    #[test]
    fn classify_handles_every_accepted_url_shape() {
        use ProjectKind::*;
        let path = |p: &str| Some(p.to_string());
        let cases = [
            ("git@github.com:acme/widget.git", (Github, path("acme/widget"))),
            ("github.com:acme/widget", (Github, path("acme/widget"))),
            ("ssh://git@github.com/acme/widget.git", (Github, path("acme/widget"))),
            ("ssh://git@gitlab.example.com:2222/team/repo", (Gitlab, path("team/repo"))),
            ("https://gitlab.com/group/sub/repo.git", (Gitlab, path("group/sub/repo"))),
            ("http://user@GitHub.com:8080/acme/widget/", (Github, path("acme/widget"))),
            ("git://codeberg.org/someone/thing.git", (Remote, path("someone/thing"))),
            ("https://codeberg.org/someone/thing", (Remote, path("someone/thing"))),
            ("file:///srv/git/thing.git", (Remote, None)),
            ("/srv/git/thing.git", (Remote, None)),
            ("../sibling/thing", (Remote, None)),
            ("https://github.com", (Remote, None)),
            ("https://github.com/", (Remote, None)),
            ("git@github.com:", (Remote, None)),
            ("", (Remote, None)),
        ];
        for (url, expected) in cases {
            assert_eq!(classify(url), expected, "url {url:?}");
        }
    }
}
