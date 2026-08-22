//! Creating the repo config layer a render did not find.
//!
//! The same three rules the installer's `--configure` applies, minus the two it
//! only needs because a human is watching: nothing here prompts, and nothing
//! here reports. It is reached only when [`Config::auto_configure_repo`] is
//! `true`, only from `--statusline`, and only when the repo layer is absent —
//! so on every render after the first it costs nothing beyond the stat
//! [`layers::load`] already performs.
//!
//! **Every failure is silent.** A read-only checkout, a `.config` that is a
//! file, a repo on a full disk: the bar renders regardless. Invariant 3 — the
//! render succeeds — outranks seeding a convenience file, and stdout is the bar,
//! so there is nowhere to complain to that would not corrupt it.
//!
//! [`Config::auto_configure_repo`]: crate::config::Config::auto_configure_repo
//! [`layers::load`]: crate::config::layers::load

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::config::layers::CONFIG_FILE_NAME;
use crate::json::{read_json_file, write_json_atomic_pretty};

/// What the JS bar read at repo level, and what this migrates.
pub const LEGACY_CONFIG_FILE_NAME: &str = "statusline.json";

/// The published schema, so a seeded file gets editor completions. The same URL
/// the shipped defaults carry.
pub const SCHEMA_URL: &str = "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json";

/// Where the repo's two config names live.
fn paths(root: &Path) -> (PathBuf, PathBuf) {
    let dir = root.join(".config");
    (dir.join(CONFIG_FILE_NAME), dir.join(LEGACY_CONFIG_FILE_NAME))
}

/// The name the `project` segment should show: the directory the repo was
/// cloned into. Matches the installer's `--configure`, deliberately — two rules
/// for one field is how the bar starts disagreeing with itself.
fn project_name(root: &Path) -> Option<String> {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty())
}

/// Creates `<root>/.config/claude-status.json` when it is missing.
///
/// Returns the path on success, so the caller can re-read the layers and narrate
/// what happened; `None` when nothing was written, for any reason at all.
pub fn ensure(root: &Path) -> Option<PathBuf> {
    let (config, legacy) = paths(root);

    // The caller gates on the layer being absent, but it decided that from a
    // read that has since gone stale — another session's render may have won
    // the race. Checking again is one stat against a duplicate write.
    if config.exists() {
        return None;
    }

    let name = project_name(root)?;

    // A legacy file is rewritten, never renamed: it carries the **JS bar's**
    // `$schema`, pointing at the `ai-plugins` repo, and a file that keeps that
    // URL under the new name gets validated against the wrong schema for the
    // rest of its life. Every other key is carried across untouched, so the
    // user's theming survives; only `$schema` and a missing name change.
    //
    // A legacy file that is **not a JSON object** carries nothing that can be
    // made to conform — there is no key to set `$schema` on. It is discarded
    // and the seed written in its place. Nothing working is lost: the JS bar
    // could not parse that file either, so it was never configuring anything.
    let written = match legacy.exists().then(|| read_json_file(&legacy)) {
        Some(Some(Value::Object(mut map))) => {
            map.insert("$schema".into(), Value::String(SCHEMA_URL.into()));
            if !has_project_name(&map) {
                map.insert("projectName".into(), Value::String(name));
            }
            Value::Object(map)
        }
        _ => seed(&name),
    };

    write_json_atomic_pretty(&config, &written).ok()?;
    // Removed only once the new file is safely on disk, so an interrupted
    // migration leaves the old file rather than neither.
    if legacy.exists() {
        let _ = std::fs::remove_file(&legacy);
    }
    Some(config)
}

/// The minimum a repo layer needs.
///
/// A repo layer is an *override*: it carries the name and the schema, and every
/// other key keeps coming from the user layer.
fn seed(name: &str) -> Value {
    let mut seeded = Map::new();
    seeded.insert("$schema".into(), Value::String(SCHEMA_URL.into()));
    seeded.insert("projectName".into(), Value::String(name.to_owned()));
    Value::Object(seeded)
}

/// The same emptiness test [`Config::project_name`] applies, so a file the bar
/// would ignore is one this still fills in.
///
/// [`Config::project_name`]: crate::config::Config::project_name
fn has_project_name(map: &Map<String, Value>) -> bool {
    map.get("projectName").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(dir: &Path, name: &str) -> PathBuf {
        let root = dir.join(name);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn seeds_the_schema_and_the_directory_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "widget-service");

        let written = ensure(&root).expect("a fresh repo gets a config");

        let value = read_json_file(&written).unwrap();
        assert_eq!(value["projectName"], Value::String("widget-service".into()));
        assert_eq!(value["$schema"], Value::String(SCHEMA_URL.into()));
        assert_eq!(value.as_object().unwrap().len(), 2, "a repo layer is an override, not a copy of the defaults");
    }

    #[test]
    fn creates_the_config_directory_when_the_repo_has_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "bare");

        ensure(&root).unwrap();

        assert!(root.join(".config").join(CONFIG_FILE_NAME).exists());
    }

    #[test]
    fn migrates_a_legacy_file_keeping_its_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "themed");
        std::fs::create_dir_all(root.join(".config")).unwrap();
        std::fs::write(
            root.join(".config").join(LEGACY_CONFIG_FILE_NAME),
            r#"{ "projectName": "hand-picked", "defaultFg": "aqua" }"#,
        )
        .unwrap();

        let written = ensure(&root).unwrap();

        let value = read_json_file(&written).unwrap();
        assert_eq!(value["projectName"], Value::String("hand-picked".into()), "a name the user set is never replaced");
        assert_eq!(value["defaultFg"], Value::String("aqua".into()), "the theming survives the migration");
        assert!(!root.join(".config").join(LEGACY_CONFIG_FILE_NAME).exists(), "the old name is gone");
    }

    #[test]
    fn a_migration_repoints_the_schema_at_this_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "repointed");
        std::fs::create_dir_all(root.join(".config")).unwrap();
        std::fs::write(
            root.join(".config").join(LEGACY_CONFIG_FILE_NAME),
            r#"{ "$schema": "https://raw.githubusercontent.com/virajp/ai-plugins/main/schemas/statusline.schema.json", "defaultFg": "aqua" }"#,
        )
        .unwrap();

        let value = read_json_file(&ensure(&root).unwrap()).unwrap();

        assert_eq!(
            value["$schema"],
            Value::String(SCHEMA_URL.into()),
            "a migrated file kept under the JS bar's schema URL validates against the wrong schema forever"
        );
        assert_eq!(value["defaultFg"], Value::String("aqua".into()));
    }

    #[test]
    fn discards_a_legacy_file_that_cannot_be_made_to_conform() {
        // Every shape the old bar could not parse either, so none of them was
        // configuring anything and none can carry a `$schema`.
        for body in ["[1, 2, 3]", "null", "0", "\"x\"", "{ not json"] {
            let tmp = tempfile::tempdir().unwrap();
            let root = repo(tmp.path(), "discarded");
            std::fs::create_dir_all(root.join(".config")).unwrap();
            std::fs::write(root.join(".config").join(LEGACY_CONFIG_FILE_NAME), body).unwrap();

            let value = read_json_file(&ensure(&root).unwrap()).unwrap();

            assert_eq!(value, seed("discarded"), "{body} should have been replaced by the seed");
            assert!(
                !root.join(".config").join(LEGACY_CONFIG_FILE_NAME).exists(),
                "{body} should have been removed, not left beside the new file"
            );
        }
    }

    #[test]
    fn gives_a_migrated_file_a_name_when_it_carried_none() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "named-by-dir");
        std::fs::create_dir_all(root.join(".config")).unwrap();
        std::fs::write(root.join(".config").join(LEGACY_CONFIG_FILE_NAME), r#"{ "defaultFg": "aqua" }"#).unwrap();

        let value = read_json_file(&ensure(&root).unwrap()).unwrap();

        assert_eq!(value["projectName"], Value::String("named-by-dir".into()));
        assert_eq!(value["defaultFg"], Value::String("aqua".into()));
    }

    #[test]
    fn leaves_an_existing_config_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "already");
        std::fs::create_dir_all(root.join(".config")).unwrap();
        let config = root.join(".config").join(CONFIG_FILE_NAME);
        std::fs::write(&config, r#"{ "projectName": "mine" }"#).unwrap();

        assert_eq!(ensure(&root), None, "an existing layer is never rewritten");
        assert_eq!(std::fs::read_to_string(&config).unwrap(), r#"{ "projectName": "mine" }"#);
    }

    #[test]
    fn is_silent_when_the_config_path_cannot_be_written() {
        let tmp = tempfile::tempdir().unwrap();
        let root = repo(tmp.path(), "blocked");
        // `.config` is a *file*, so both the mkdir and the write must fail.
        std::fs::write(root.join(".config"), "not a directory").unwrap();

        assert_eq!(ensure(&root), None, "a failure returns None rather than panicking");
    }
}
