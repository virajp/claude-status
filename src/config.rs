//! The merged configuration, and the readers every other module reaches it
//! through.
//!
//! Config is kept as a `serde_json::Value` rather than a typed struct: every
//! key is optional at every depth, users hand-edit the file, and a typed model
//! would either reject a config the old bar accepted or degenerate into the
//! same optionality with more code.

pub mod color;
pub mod defaults;
pub mod layers;
pub mod matcher;

use serde_json::{Map, Value};

use crate::config::color::Rgb;
use crate::config::matcher::Matcher;

/// The hard fallback styling for a segment with no configured defaults
/// (contract §4).
pub const FALLBACK_BG: &str = "blue";

pub struct Config {
    root: Value,
}

impl Config {
    pub fn new(root: Value) -> Self {
        Self { root }
    }

    /// Reads a dotted path — `"gauge.width"`, `"segments.model.bg"`.
    pub fn get(&self, path: &str) -> Option<&Value> {
        path.split('.').try_fold(&self.root, |cur, key| cur.get(key))
    }

    /// A configured symbol.
    ///
    /// **Deviation:** a key missing from the merged config renders `""`. The JS
    /// interpolated `undefined` and rendered the literal text. With the
    /// embedded layer this is unreachable in practice; it is a guard, not a
    /// behaviour.
    pub fn symbol(&self, key: &str) -> &str {
        self.get(&format!("symbols.{key}")).and_then(Value::as_str).unwrap_or_default()
    }

    /// A `powerline.*` piece — `cap`, `sep`, `sepThin`, `thinFg`.
    pub fn powerline(&self, key: &str) -> &str {
        self.get(&format!("powerline.{key}")).and_then(Value::as_str).unwrap_or_default()
    }

    pub fn palette(&self) -> Option<&Map<String, Value>> {
        self.get("palette")?.as_object()
    }

    /// Resolves a colour spec against this config's palette.
    pub fn color(&self, spec: Option<&Value>) -> Rgb {
        color::resolve(spec, self.palette())
    }

    pub fn default_fg(&self) -> Option<&Value> {
        self.get("defaultFg")
    }

    /// The gauge width. A configured `0` means ten, as `||` made it.
    pub fn gauge_width(&self) -> usize {
        match self.get("gauge.width").and_then(Value::as_u64) {
            Some(0) | None => 10,
            Some(n) => n as usize,
        }
    }

    pub fn gauge_glyph(&self, key: &str) -> &str {
        self.get(&format!("gauge.{key}")).and_then(Value::as_str).unwrap_or_default()
    }

    pub fn project_name(&self) -> Option<&str> {
        self.get("projectName").and_then(Value::as_str).filter(|s| !s.is_empty())
    }

    /// The layout: a list of lines, each a list of entries. An entry is a
    /// segment id string or an object overriding that segment's styling.
    pub fn lines(&self) -> Vec<Vec<Value>> {
        self.get("lines")
            .and_then(Value::as_array)
            .map(|lines| {
                lines.iter().map(|l| l.as_array().cloned().unwrap_or_default()).collect()
            })
            .unwrap_or_default()
    }

    /// The `worktreePattern` matcher.
    ///
    /// A pattern `regex-lite` rejects falls back to the shipped literal and
    /// warns on **stderr** — never on stdout, which is the bar.
    pub fn worktree_matcher(&self) -> Matcher {
        const DEFAULT: &str = "worktree";
        let pattern = self.get("worktreePattern").and_then(Value::as_str).unwrap_or(DEFAULT);
        match Matcher::compile(pattern) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("claude-status: worktreePattern {pattern:?} is not a valid regex ({e}); using {DEFAULT:?}");
                Matcher::compile(DEFAULT).expect("the default pattern is a literal")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn cfg(v: Value) -> Config {
        Config::new(v)
    }

    #[test]
    fn dotted_paths_read_through_nesting() {
        let c = cfg(json!({ "segments": { "model": { "bg": "blue" } } }));
        assert_eq!(c.get("segments.model.bg"), Some(&json!("blue")));
        assert_eq!(c.get("segments.model.fg"), None);
        assert_eq!(c.get("segments.absent.bg"), None);
    }

    #[test]
    fn a_missing_symbol_renders_empty_not_undefined() {
        let c = cfg(json!({ "symbols": { "branch": "B" } }));
        assert_eq!(c.symbol("branch"), "B");
        assert_eq!(c.symbol("absent"), "", "deviation: the JS rendered the text `undefined`");
    }

    #[test]
    fn a_gauge_width_of_zero_means_ten() {
        assert_eq!(cfg(json!({ "gauge": { "width": 0 } })).gauge_width(), 10);
        assert_eq!(cfg(json!({})).gauge_width(), 10);
        assert_eq!(cfg(json!({ "gauge": { "width": 4 } })).gauge_width(), 4);
    }

    #[test]
    fn an_empty_project_name_is_absent() {
        assert_eq!(cfg(json!({ "projectName": "" })).project_name(), None);
        assert_eq!(cfg(json!({})).project_name(), None);
        assert_eq!(cfg(json!({ "projectName": "x" })).project_name(), Some("x"));
    }

    #[test]
    fn a_bad_worktree_pattern_falls_back_to_the_default() {
        let c = cfg(json!({ "worktreePattern": "(unclosed" }));
        let m = c.worktree_matcher();
        assert!(m.is_match("/x/worktrees/y"), "fell back to the literal default");
        assert!(!m.is_match("/x/src/y"));
    }

    #[test]
    fn lines_survive_a_malformed_entry() {
        let c = cfg(json!({ "lines": [["model"], "not-an-array"] }));
        assert_eq!(c.lines(), vec![vec![json!("model")], vec![]]);
    }
}
