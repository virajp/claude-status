//! The three config layers, deep-merged low → high.
//!
//! **Deviation from the contract:** it describes two file layers, and a machine
//! with neither renders blank. Here the shipped defaults are embedded as a
//! third, lowest layer, so a cold start still draws a full bar. Output is
//! byte-identical for every install whose user file *is* the seeded defaults,
//! which is all of them.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::Config;
use crate::config::defaults::DEFAULTS_JSON;
use crate::json::{deep_merge, read_json_file};

/// The per-user and per-repo file name. Note this is **not** the JS bar's
/// `statusline.json`: the two live side by side until the Phase 5 cutover, and
/// `--install` migrates the old file.
pub const CONFIG_FILE_NAME: &str = "claude-status.json";

/// Where one layer came from and whether it contributed, for `--debug`.
pub struct LayerSource {
    pub label: &'static str,
    pub path: Option<PathBuf>,
    pub loaded: bool,
}

pub struct Layers {
    pub config: Config,
    pub sources: Vec<LayerSource>,
}

/// Merges `embedded → $HOME/.config → <repo-root>/.config`.
///
/// `home` is `$HOME` read directly and joined with `.config` literally — **not**
/// `dirs::config_dir()`, which on macOS resolves to `~/Library/Application
/// Support` and would miss every existing install.
pub fn load(home: Option<&Path>, repo_root: Option<&Path>) -> Layers {
    let embedded: Value = serde_json::from_str(DEFAULTS_JSON).unwrap_or_else(|_| Value::Object(Default::default()));
    let mut merged = embedded;
    let mut sources = vec![LayerSource { label: "embedded", path: None, loaded: true }];

    for (label, base) in [("user", home), ("repo", repo_root)] {
        let path = base.map(|b| b.join(".config").join(CONFIG_FILE_NAME));
        // A layer must be an *object*. A file holding `null`, a number or an
        // array parses fine but would replace the whole merged config
        // wholesale and blank the bar — the old implementation coerced any
        // falsy parse to `{}`, and a one-byte file must not cost the defaults.
        let layer = path.as_deref().and_then(read_json_file).filter(Value::is_object);
        let loaded = match layer {
            Some(layer) => {
                deep_merge(&mut merged, &layer);
                true
            }
            None => false,
        };
        sources.push(LayerSource { label, path, loaded });
    }

    Layers { config: Config::new(merged), sources }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    /// Writes a config layer under `<base>/.config/` and returns `base`.
    fn seed(base: &Path, layer: &str) -> PathBuf {
        let dir = base.join(".config");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CONFIG_FILE_NAME), layer).unwrap();
        base.to_path_buf()
    }

    #[test]
    fn with_no_files_the_embedded_defaults_render_a_full_bar() {
        let layers = load(None, None);
        assert_eq!(layers.config.project_name(), Some("Project-Name"));
        assert_eq!(layers.config.gauge_width(), 10);
        assert_eq!(layers.config.lines().len(), 2, "a cold start still has a layout");
        assert!(layers.sources.iter().filter(|s| s.loaded).count() == 1);
    }

    #[test]
    fn the_repo_layer_beats_the_user_layer_beats_embedded() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed(&dir.path().join("home"), r#"{ "projectName": "from-user", "defaultFg": "aqua" }"#);
        let repo = seed(&dir.path().join("repo"), r#"{ "projectName": "from-repo" }"#);

        let layers = load(Some(&home), Some(&repo));
        assert_eq!(layers.config.project_name(), Some("from-repo"));
        assert_eq!(layers.config.get("defaultFg").and_then(Value::as_str), Some("aqua"), "user still wins over embedded");
        assert_eq!(layers.config.gauge_width(), 10, "untouched keys keep the embedded value");
        assert!(layers.sources.iter().all(|s| s.loaded));
    }

    #[test]
    fn a_malformed_layer_is_ignored_rather_than_fatal() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = seed(&dir.path().join("home"), "{ this is not json");

        let layers = load(Some(&home), None);
        assert_eq!(layers.config.project_name(), Some("Project-Name"), "the render succeeds on the embedded layer");
        let user = layers.sources.iter().find(|s| s.label == "user").unwrap();
        assert!(!user.loaded, "the layer is reported as not loaded");
        assert!(user.path.is_some(), "but the path it looked at is still reported");
    }

    #[test]
    fn a_valid_but_non_object_layer_does_not_wipe_the_defaults() {
        for body in ["null", "0", "[]", "\"x\""] {
            let dir = tempfile::TempDir::new().unwrap();
            let home = seed(dir.path(), body);

            let layers = load(Some(&home), None);
            assert_eq!(layers.config.project_name(), Some("Project-Name"), "{body} blanked the bar");
            assert_eq!(layers.config.lines().len(), 2);
            let user = layers.sources.iter().find(|s| s.label == "user").unwrap();
            assert!(!user.loaded, "{body} should not count as a loaded layer");
        }
    }

    #[test]
    fn a_repo_layer_replaces_the_layout_wholesale() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = seed(dir.path(), r#"{ "lines": [["model"]] }"#);

        let layers = load(None, Some(&repo));
        assert_eq!(layers.config.lines(), vec![vec![json!("model")]]);
    }

    #[test]
    fn the_user_path_is_dot_config_under_home_literally() {
        let layers = load(Some(Path::new("/fake/home")), None);
        let user = layers.sources.iter().find(|s| s.label == "user").unwrap();
        assert_eq!(user.path.as_deref(), Some(Path::new("/fake/home/.config/claude-status.json")));
    }
}
