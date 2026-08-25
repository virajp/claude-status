//! The three config layers, deep-merged low → high.
//!
//! **Deviation from the contract:** it describes two file layers, and a machine
//! with neither renders blank. Here the shipped defaults are embedded as a
//! third, lowest layer, so a cold start still draws a full bar — and with the
//! `config-relocation` cycle that is the *supported* state rather than a
//! degraded one: no file has to exist anywhere for the bar to be complete.
//!
//! The two file layers are **not symmetric**, and that asymmetry is the whole
//! shape of this module:
//!
//! - The **user** layer is the whole config. It lives at
//!   `~/.config/claude-status/config.json`.
//! - The **repo** layer may set [`REPO_LAYER_KEY`] and nothing else. It lives at
//!   `<repo-root>/.config/claude-status.json`.
//!
//! Both used to be built from one shared expression and merged by one shared
//! loop. They are written out separately now because they no longer agree on
//! either half — neither the path nor what the file is allowed to say.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::config::Config;
use crate::config::defaults::DEFAULTS_JSON;
use crate::json::deep_merge;

/// The directory the user's config lives in, under `~/.config`.
///
/// A **directory** rather than the bare `~/.config/claude-status.json` this
/// used to be, for two reasons: the tool will accumulate more than one thing to
/// store, and a directory is one thing to delete when someone wants the tool
/// gone. There is deliberately **no fallback to the old path** — nothing has
/// ever been released, so a fallback would be compatibility with a state that
/// never existed, paid for on every render as an extra stat.
pub const USER_CONFIG_DIR_NAME: &str = "claude-status";

/// The user config's file name inside [`USER_CONFIG_DIR_NAME`].
pub const USER_CONFIG_FILE_NAME: &str = "config.json";

/// The per-repo file name. It stays a bare file under the repo's `.config/`,
/// where a repo's tool configs already live, and it keeps the tool's name
/// because it is one file among many others' rather than a directory of its
/// own.
pub const REPO_CONFIG_FILE_NAME: &str = "claude-status.json";

/// The only key a repo layer may set.
///
/// The repo layer existed to name the project. Letting it override styling
/// made every repo a place where the bar could look different for reasons
/// nobody could find — so §3's three-layer merge is deliberately **narrowed**
/// here, and the narrowing is a reduction of the contract rather than a
/// clarification of it.
///
/// (The cycle plan cites this as §2 throughout. §2 is *Input contracts*; the
/// merge is §3. Corrected here rather than copied.)
pub const REPO_LAYER_KEY: &str = "projectName";

/// Not a setting: a pointer that buys the file editor completions. The shipped
/// defaults carry one, so a hand-written repo config carrying one is expected
/// rather than a mistake. It is neither merged nor reported as ignored —
/// reporting it would put a line in `--debug` for every correctly written file.
const SCHEMA_KEY: &str = "$schema";

/// What became of one layer.
///
/// Three states rather than a `loaded` boolean, because the boolean answered
/// two questions at once and `--debug` needs them apart. Before
/// `config-relocation`, "no user config" meant a half-installed machine and
/// `not found` was the right word for it. Now the shipped defaults are embedded
/// and **no file has to exist anywhere** — so absence is the supported state,
/// while a file that is present and will not parse is still a real problem.
/// Rendering both as `not found` told a user with a broken config that nothing
/// was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerState {
    /// Read and merged. Always the embedded layer's state.
    Loaded,
    /// Nothing to read: no `$HOME`, no git root, or simply no file there. The
    /// normal case, and not an error.
    Absent,
    /// A file is there and did not contribute — unreadable, not JSON, or not a
    /// JSON object. Never silent, because nothing else in the binary is allowed
    /// to complain about it: §1's invariant 3 means a broken layer costs its
    /// own settings and never the bar, which leaves `--debug` as the only place
    /// a user can find out.
    Unusable,
}

impl LayerState {
    /// The word `--debug` prints. `using defaults` rather than `not found`:
    /// what the user wants to know is what the bar is drawing from, and with no
    /// file that is the embedded layer, which is a complete answer rather than
    /// a missing one.
    ///
    /// None of these may contain the substring `ignored` — that word belongs to
    /// the continuation row below the path, and
    /// `a_well_formed_repo_config_is_reported_as_ignoring_nothing` asserts its
    /// absence over the whole report.
    pub fn label(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Absent => "using defaults",
            Self::Unusable => "UNREADABLE",
        }
    }
}

/// Where one layer came from and what became of it, for `--debug`.
pub struct LayerSource {
    /// One of [`LABEL_EMBEDDED`], [`LABEL_USER`] or [`LABEL_REPO`]. A `&str`
    /// rather than an enum because it is only ever displayed — but the three
    /// constants exist so a consumer matching on it cannot be silently wrong
    /// when a layer is renamed or a fourth is added.
    pub label: &'static str,
    pub path: Option<PathBuf>,
    pub state: LayerState,
    /// Keys the layer carried that were **dropped rather than merged**, in the
    /// order the file listed them.
    ///
    /// Only the repo layer can populate this, and only since it was narrowed
    /// to [`REPO_LAYER_KEY`]. It exists because a silently ignored key is the
    /// worst of the three possible answers: erroring would break the
    /// never-fail rule (§1, invariant 3), merging is what the narrowing
    /// forbids, and dropping it without saying so leaves the user editing a
    /// file that does nothing. So `--debug` names them.
    pub ignored: Vec<String>,
    /// The layer's **own** tree, as the file wrote it, kept so
    /// [`validate`](crate::config::validate) can attribute a finding to the
    /// file that caused it.
    ///
    /// This is the whole reason it exists. The merge produces one tree, and a
    /// typo in it is un-attributable by the time `Config` reads it — "there is
    /// a stray `powerlin` somewhere in your three config files" is not a
    /// diagnostic. Retained rather than re-read: `read_layer` already parsed
    /// it and `deep_merge` only borrows it, so keeping it costs no second parse
    /// and no second stat.
    ///
    /// `None` for the **embedded** layer, which is this binary's own and would
    /// only ever report on itself, and for any layer that contributed nothing.
    /// For the **repo** layer it is what survived [`narrow`] — the rest is
    /// already on the `ignored` row, and saying it twice in two vocabularies
    /// helps nobody.
    pub raw: Option<Value>,
}

/// The defaults compiled into the binary. Never has a path.
pub const LABEL_EMBEDDED: &str = "embedded";
/// `~/.config/claude-status/config.json`. No path means no `$HOME`.
pub const LABEL_USER: &str = "user";
/// `<repo-root>/.config/claude-status.json`. No path means no git root.
pub const LABEL_REPO: &str = "repo";

pub struct Layers {
    pub config: Config,
    pub sources: Vec<LayerSource>,
}

/// `~/.config/claude-status/config.json`.
///
/// `home` is `$HOME` read directly and joined with `.config` literally — **not**
/// `dirs::config_dir()`, which on macOS resolves to `~/Library/Application
/// Support`. That is a platform convention this tool does not follow: the
/// config directory is the thing people commit to a dotfiles repo and sync
/// between machines, and `~/.config` is where such a repo expects to find it.
pub fn user_config_path(home: &Path) -> PathBuf {
    home.join(".config").join(USER_CONFIG_DIR_NAME).join(USER_CONFIG_FILE_NAME)
}

/// `<repo-root>/.config/claude-status.json`.
///
/// Written by a human and by nothing else. Nothing in this binary creates it —
/// see the module note on [`load`].
pub fn repo_config_path(root: &Path) -> PathBuf {
    root.join(".config").join(REPO_CONFIG_FILE_NAME)
}

/// Merges `embedded → user → repo`.
///
/// **This function reads. It never writes**, and neither does anything it
/// calls. A `--statusline` render used to be able to create the repo layer it
/// did not find; that is gone, and the invariant it buys is worth naming — a
/// status line that redraws every four seconds provably touches nothing on disk
/// during a render. That is easier to reason about than any amount of care
/// about *when* it writes.
pub fn load(home: Option<&Path>, repo_root: Option<&Path>) -> Layers {
    let embedded: Value = serde_json::from_str(DEFAULTS_JSON).unwrap_or_else(|_| Value::Object(Default::default()));
    let mut merged = embedded;
    let mut sources = vec![LayerSource {
        label: LABEL_EMBEDDED,
        path: None,
        state: LayerState::Loaded,
        ignored: Vec::new(),
        raw: None,
    }];

    // The user layer: the whole config, merged as it stands.
    let user_path = home.map(user_config_path);
    let (user_state, user_raw) = match read_layer(user_path.as_deref()) {
        Ok(Some(layer)) => {
            deep_merge(&mut merged, &layer);
            (LayerState::Loaded, Some(layer))
        }
        Ok(None) => (LayerState::Absent, None),
        Err(()) => (LayerState::Unusable, None),
    };
    sources.push(LayerSource {
        label: LABEL_USER,
        path: user_path,
        state: user_state,
        ignored: Vec::new(),
        raw: user_raw,
    });

    // The repo layer: one key, and a list of what it was not allowed to say.
    let repo_path = repo_root.map(repo_config_path);
    let (repo_state, ignored, repo_raw) = match read_layer(repo_path.as_deref()) {
        Ok(Some(Value::Object(entries))) => {
            let (kept, ignored) = narrow(entries);
            let kept = Value::Object(kept);
            deep_merge(&mut merged, &kept);
            (LayerState::Loaded, ignored, Some(kept))
        }
        // `read_layer` returns only objects in the `Some` arm, so this is the
        // "no file" case alone.
        Ok(_) => (LayerState::Absent, Vec::new(), None),
        Err(()) => (LayerState::Unusable, Vec::new(), None),
    };
    sources.push(LayerSource { label: LABEL_REPO, path: repo_path, state: repo_state, ignored, raw: repo_raw });

    Layers { config: Config::new(merged), sources }
}

/// One layer file, as the three answers `--debug` has words for:
/// `Ok(Some(object))`, `Ok(None)` for "there is no file here", and `Err(())`
/// for "there is one and it cannot be used".
///
/// **Deliberately not [`crate::json::read_json_file`]**, which collapses
/// missing, unreadable and malformed into a single `None`. That is the right
/// shape for a caller that only wants the value, and it is precisely the
/// distinction this cycle needs: a machine with no config is working, and a
/// machine with a config that will not parse is not. Nothing about the *merge*
/// changes — an unusable layer still contributes nothing and still never fails
/// the render.
fn read_layer(path: Option<&Path>) -> Result<Option<Value>, ()> {
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read_to_string(path) {
        // A directory at the path lands here too, and `Unusable` is the honest
        // answer for it: something is there and it is not a config.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => Ok(Some(value)),
            _ => Err(()),
        },
    }
}

/// Splits a repo layer into the one key it may set and the keys it may not.
///
/// Ignoring rather than erroring, because the never-fail rule still holds
/// (§1, invariant 3): a repo config carrying a stale `gauge` block must cost
/// that block and never the bar. The dropped keys are returned rather than
/// discarded so `--debug` can name them.
fn narrow(entries: Map<String, Value>) -> (Map<String, Value>, Vec<String>) {
    let mut kept = Map::new();
    let mut ignored = Vec::new();
    for (key, value) in entries {
        match key.as_str() {
            REPO_LAYER_KEY => {
                kept.insert(key, value);
            }
            SCHEMA_KEY => {}
            _ => ignored.push(key),
        }
    }
    (kept, ignored)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    /// Writes the user layer under `<base>/.config/claude-status/` and returns
    /// `base`, to be passed as `home`.
    fn seed_user(base: &Path, layer: &str) -> PathBuf {
        let path = user_config_path(base);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, layer).unwrap();
        base.to_path_buf()
    }

    /// Writes the repo layer under `<base>/.config/` and returns `base`, to be
    /// passed as `repo_root`.
    fn seed_repo(base: &Path, layer: &str) -> PathBuf {
        let path = repo_config_path(base);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, layer).unwrap();
        base.to_path_buf()
    }

    fn source<'a>(layers: &'a Layers, label: &str) -> &'a LayerSource {
        layers.sources.iter().find(|s| s.label == label).unwrap()
    }

    #[test]
    fn with_no_files_the_embedded_defaults_render_a_full_bar() {
        let layers = load(None, None);

        // Asserted as *equality with the whole default*, not as three sampled
        // fields. Sampling `defaultFg == "white"` proves nothing here: it is
        // also what `Config::default()` carries, so the probe passes whether
        // the embedded layer was read or dropped on the floor.
        //
        // Equality is not a discriminator either — see the note on
        // `a_non_object_user_layer_can_only_be_caught_by_its_loaded_flag` — but
        // it is at least exact, and it pins the *whole* cold-start state
        // rather than a corner of it.
        assert_eq!(layers.config, Config::default());
        assert_eq!(layers.config.project_name, None, "the defaults carry no project name");
        assert_eq!(layers.config.lines.len(), 2, "a cold start still has a layout");

        // The layer *structure* is the part that is genuinely discriminating:
        // both file layers must be reported, with no path to look at and
        // nothing loaded.
        let labels: Vec<&str> = layers.sources.iter().map(|s| s.label).collect();
        assert_eq!(labels, [LABEL_EMBEDDED, LABEL_USER, LABEL_REPO]);
        assert!(source(&layers, LABEL_EMBEDDED).state == LayerState::Loaded);
        for label in [LABEL_USER, LABEL_REPO] {
            let s = source(&layers, label);
            // `Absent`, not merely "not loaded": with the defaults embedded,
            // having no file is the supported state rather than a broken one.
            assert_eq!(s.state, LayerState::Absent, "{label} loaded with no file");
            assert_eq!(s.path, None, "{label} named a path with no base to build one from");
        }
    }

    #[test]
    fn the_repo_layer_beats_the_user_layer_beats_embedded() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed_user(&dir.path().join("home"), r#"{ "projectName": "from-user", "defaultFg": "aqua" }"#);
        let repo = seed_repo(&dir.path().join("repo"), r#"{ "projectName": "from-repo" }"#);

        let layers = load(Some(&home), Some(&repo));
        assert_eq!(layers.config.project_name.as_deref(), Some("from-repo"));
        assert_eq!(layers.config.default_fg, Some(json!("aqua")), "user still wins over embedded");
        assert_eq!(layers.config.gauge.width, 10, "untouched keys keep the embedded value");
        assert!(layers.sources.iter().all(|s| s.state == LayerState::Loaded));
    }

    /// **A user-layer `projectName` is not inert — it names every repo.**
    ///
    /// Only the repo layer is narrowed ([`narrow`], called at one site); the
    /// user layer merges whole. So a `projectName` written into the user config
    /// reaches every repository that has not named itself.
    ///
    /// Pinned as a test because the opposite belief reached three documents at
    /// once — the generator page, `configure.md`, and the cycle plan — each
    /// telling users the key was safe to set here. Prose can be wrong quietly;
    /// this cannot.
    #[test]
    fn a_user_layer_project_name_reaches_a_repo_that_has_not_named_itself() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed_user(&dir.path().join("home"), r#"{ "projectName": "from-user" }"#);
        let repo = dir.path().join("repo-with-no-config");
        std::fs::create_dir_all(&repo).unwrap();

        let layers = load(Some(&home), Some(&repo));
        assert_eq!(
            layers.config.project_name.as_deref(),
            Some("from-user"),
            "the user layer is not narrowed, so its projectName applies to a repo that set none"
        );

        // The control: without the user config the segment has no name at all,
        // which is what proves the assertion above observed the user layer
        // rather than some default.
        let empty = dir.path().join("empty-home");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            load(Some(&empty), Some(&repo)).config.project_name,
            None,
            "with no user config there is no project name to inherit"
        );
    }

    #[test]
    fn a_malformed_layer_is_ignored_rather_than_fatal() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed_user(&dir.path().join("home"), "{ this is not json");

        let layers = load(Some(&home), None);
        assert_eq!(layers.config.default_fg, Some(json!("white")), "the render succeeds on the embedded layer");
        let user = source(&layers, LABEL_USER);
        assert!(user.state != LayerState::Loaded, "the layer is reported as not loaded");
        assert!(user.path.is_some(), "but the path it looked at is still reported");
    }

    /// The regression `7152c8a` fixed, at the layer it actually lands on.
    ///
    /// **The user position has no in-process discriminator, and that is a
    /// property of the design rather than of this test.** If a non-object user
    /// layer replaced the merged tree wholesale, `Config::new` would fall back
    /// to `Config::default()` — which is, by
    /// `the_embedded_defaults_deserialize_to_the_default_config`, *exactly*
    /// what the surviving embedded layer produces. Every value assertion
    /// therefore holds in both outcomes. [`LayerState`] and the stderr
    /// diagnostic are the only things that differ, so the state is what is
    /// asserted, and the case with real data at stake is the sibling test below.
    #[test]
    fn a_non_object_user_layer_can_only_be_caught_by_its_layer_state() {
        for body in ["null", "0", "[]", "\"x\""] {
            let dir = tempfile::TempDir::new().unwrap();
            let home = seed_user(dir.path(), body);

            let layers = load(Some(&home), None);
            // `Unusable` and not merely "not loaded": these files exist, so
            // reporting them as a machine with no config is the failure this
            // cycle's step 6 is about.
            assert_eq!(source(&layers, LABEL_USER).state, LayerState::Unusable, "{body}");
        }
    }

    /// The same filter one layer up, where a wipe would cost something a test
    /// can see: the repo layer merges **last**, so a non-object one replacing
    /// the tree wholesale would take the user's whole config with it.
    #[test]
    fn a_non_object_repo_layer_does_not_wipe_the_user_layer() {
        for body in ["null", "0", "[]", "\"x\""] {
            let dir = tempfile::TempDir::new().unwrap();
            let home = seed_user(&dir.path().join("home"), r#"{ "defaultFg": "aqua", "gauge": { "width": 3 } }"#);
            let repo = seed_repo(&dir.path().join("repo"), body);

            let layers = load(Some(&home), Some(&repo));
            assert_eq!(layers.config.default_fg, Some(json!("aqua")), "{body} wiped the user layer");
            assert_eq!(layers.config.gauge.width, 3, "{body} wiped the user layer");
            assert_eq!(source(&layers, LABEL_REPO).state, LayerState::Unusable, "{body}");
        }
    }

    /// **The distinction `--debug` is built on.** `loaded == false` used to
    /// cover four different situations at once — no `$HOME`, no file, an
    /// unreadable file, and a file that is not JSON — so a *broken* config
    /// rendered in the report exactly like a *missing* one. After
    /// `config-relocation`, having no file is the normal supported state; a
    /// file that will not parse is not, and the report must not describe them
    /// with the same word.
    #[test]
    fn an_absent_layer_and_an_unusable_one_are_different_states() {
        let dir = tempfile::TempDir::new().unwrap();

        // No `$HOME` and no git root at all: nothing to look at.
        let nowhere = load(None, None);
        assert_eq!(source(&nowhere, LABEL_USER).state, LayerState::Absent);
        assert_eq!(source(&nowhere, LABEL_REPO).state, LayerState::Absent);
        assert_eq!(source(&nowhere, LABEL_EMBEDDED).state, LayerState::Loaded, "the embedded layer always is");

        // A home with no config file in it — still normal.
        let empty_home = dir.path().join("empty");
        fs::create_dir_all(&empty_home).unwrap();
        assert_eq!(source(&load(Some(&empty_home), None), LABEL_USER).state, LayerState::Absent);

        // A file that is there and cannot contribute. Every one of these used
        // to be indistinguishable from the two cases above.
        for (i, body) in ["{ this is not json", "null", "[]", "\"x\"", "0"].iter().enumerate() {
            let home = seed_user(&dir.path().join(format!("broken{i}")), body);
            let state = source(&load(Some(&home), None), LABEL_USER).state;
            assert_eq!(state, LayerState::Unusable, "{body} reads as a machine with no config");
        }

        let good = seed_user(&dir.path().join("good"), r#"{ "defaultFg": "aqua" }"#);
        assert_eq!(source(&load(Some(&good), None), LABEL_USER).state, LayerState::Loaded);
    }

    /// A path that exists but is a directory is a file that cannot be read, not
    /// a file that is absent — `~/.config/claude-status/config.json/` is a
    /// plausible mistake and it should say so rather than look like a clean
    /// machine.
    #[test]
    fn a_directory_where_the_config_should_be_is_unusable_rather_than_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(user_config_path(dir.path())).unwrap();

        assert_eq!(source(&load(Some(dir.path()), None), LABEL_USER).state, LayerState::Unusable);
    }

    #[test]
    fn the_user_path_is_a_directory_under_dot_config() {
        let layers = load(Some(Path::new("/fake/home")), None);
        assert_eq!(
            source(&layers, LABEL_USER).path.as_deref(),
            Some(Path::new("/fake/home/.config/claude-status/config.json")),
        );
    }

    /// The path this config used to live at. There is deliberately no
    /// fallback, so a file left there is invisible — and a test says so, because
    /// "we removed the fallback" and "we forgot to remove the old path" look
    /// identical from the outside.
    #[test]
    fn a_config_at_the_old_bare_path_is_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        let old = dir.path().join(".config").join("claude-status.json");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, r#"{ "defaultFg": "aqua", "projectName": "old" }"#).unwrap();

        let layers = load(Some(dir.path()), None);
        assert_eq!(layers.config.default_fg, Some(json!("white")), "the old path was still read");
        assert_eq!(layers.config.project_name, None, "the old path was still read");
        assert!(source(&layers, LABEL_USER).state != LayerState::Loaded);
    }

    #[test]
    fn the_repo_path_stays_a_bare_file_under_dot_config() {
        let layers = load(None, Some(Path::new("/fake/repo")));
        assert_eq!(
            source(&layers, LABEL_REPO).path.as_deref(),
            Some(Path::new("/fake/repo/.config/claude-status.json")),
        );
    }

    /// The narrowing, at the key it exists to allow.
    #[test]
    fn a_repo_layer_may_set_the_project_name() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = seed_repo(dir.path(), r#"{ "projectName": "widget-service" }"#);

        let layers = load(None, Some(&repo));
        assert_eq!(layers.config.project_name.as_deref(), Some("widget-service"));
        let source = source(&layers, LABEL_REPO);
        assert!(source.state == LayerState::Loaded);
        assert!(source.ignored.is_empty(), "nothing was dropped");
    }

    /// Replaces `a_repo_layer_replaces_the_layout_wholesale`, which pinned the
    /// capability this cycle removes. The layout is the sharpest case: it is
    /// the one key a repo could use to make the bar unrecognisable.
    #[test]
    fn a_repo_layer_no_longer_replaces_the_layout() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = seed_repo(dir.path(), r#"{ "lines": [["model"]] }"#);

        let layers = load(None, Some(&repo));
        assert_eq!(layers.config.lines, Config::default().lines, "the repo layer overrode the layout");
        assert_eq!(source(&layers, LABEL_REPO).ignored, ["lines"], "and said so");
    }

    /// Replaces `a_repo_config_overrides_the_user_one_outright`'s unit half:
    /// the user layer is what a repo key now loses to, not the other way round.
    #[test]
    fn a_repo_key_other_than_the_project_name_is_ignored_and_reported() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed_user(&dir.path().join("home"), r#"{ "caps": { "context": 50 }, "defaultFg": "aqua" }"#);
        let repo = seed_repo(
            &dir.path().join("repo"),
            r#"{ "projectName": "kept", "caps": { "context": 90 }, "gauge": { "width": 3 } }"#,
        );

        let layers = load(Some(&home), Some(&repo));
        assert_eq!(layers.config.project_name.as_deref(), Some("kept"), "the one key it may set applied");
        assert_eq!(layers.config.caps.context, 50, "the user's cap survived the repo's");
        assert_eq!(layers.config.default_fg, Some(json!("aqua")), "and so did the rest of the user layer");
        assert_eq!(layers.config.gauge.width, 10, "a key neither layer may set here is the shipped one");

        let source = source(&layers, LABEL_REPO);
        assert!(source.state == LayerState::Loaded, "the file was read — it just did not get to say most of it");
        assert_eq!(source.ignored, ["caps", "gauge"], "in the order the file listed them");
    }

    /// `$schema` is the one key a hand-written repo config is *expected* to
    /// carry, so reporting it as ignored would put a line in `--debug` for
    /// every correctly written file.
    #[test]
    fn a_repo_layers_schema_pointer_is_neither_merged_nor_reported() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = seed_repo(dir.path(), r#"{ "$schema": "https://example.invalid/s.json", "projectName": "n" }"#);

        let layers = load(None, Some(&repo));
        assert_eq!(layers.config.project_name.as_deref(), Some("n"));
        assert!(source(&layers, LABEL_REPO).ignored.is_empty(), "`$schema` is a pointer, not a setting");
    }
}
