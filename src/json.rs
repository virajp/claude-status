//! `serde_json::Value` accessors, file IO, and the config deep merge.
//!
//! Everything here swallows errors into `None`. A syntactically invalid config
//! layer is *ignored*, not fatal — the bar still renders (contract §3).

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

/// Keys JavaScript object prototypes are reachable through. The old
/// implementation skipped them in its merge; in Rust they are inert, but a
/// config written against the old implementation must behave identically, so
/// the *behaviour* of dropping them is preserved.
const FORBIDDEN_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

pub fn opt_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key)?.as_str()
}

pub fn opt_f64(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64()
}

pub fn opt_obj<'a>(v: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    v.get(key)?.as_object()
}

pub fn opt_arr<'a>(v: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    v.get(key)?.as_array()
}

/// Reads and parses a JSON file. Any failure — missing, unreadable, malformed —
/// is `None`. Never propagates, never warns: a broken layer is a layer that
/// does not exist.
pub fn read_json_file(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Writes JSON atomically: a sibling temp file, then a rename. The temp name
/// carries the pid so two concurrent renders cannot clobber each other's
/// in-progress write.
///
/// Best-effort — the caller treats failure as "nothing was written".
pub fn write_json_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;

    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!("{file_name}.{}.tmp", std::process::id()));

    let bytes = serde_json::to_vec(value)?;
    match fs::write(&tmp, &bytes).and_then(|()| fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Deep-merges `over` into `base`, low → high.
///
/// - Objects merge key by key, recursively.
/// - Arrays and scalars **replace wholesale** — a repo overriding `lines` means
///   to replace the layout, not to append to it.
/// - An explicit `null` in the override also replaces. The old implementation
///   assigned it, so a user who wrote `null` got `null`, and the consuming
///   ladder then treated it as absent.
/// - The three prototype keys are skipped at *every* depth.
pub fn deep_merge(base: &mut Value, over: &Value) {
    let (Some(base_obj), Some(over_obj)) = (base.as_object_mut(), over.as_object()) else {
        *base = over.clone();
        return;
    };

    for (key, over_val) in over_obj {
        if FORBIDDEN_KEYS.contains(&key.as_str()) {
            continue;
        }
        match base_obj.get_mut(key) {
            Some(base_val) if base_val.is_object() && over_val.is_object() => deep_merge(base_val, over_val),
            _ => {
                base_obj.insert(key.clone(), sanitised(over_val));
            }
        }
    }
}

/// A wholesale replacement still has to drop the prototype keys it carries at
/// any depth, or a nested object arriving where the base had a scalar would
/// smuggle one through.
fn sanitised(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !FORBIDDEN_KEYS.contains(&k.as_str()))
                .map(|(k, val)| (k.clone(), sanitised(val)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(sanitised).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn merged(base: Value, over: Value) -> Value {
        let mut base = base;
        deep_merge(&mut base, &over);
        base
    }

    #[test]
    fn objects_merge_key_by_key() {
        assert_eq!(
            merged(json!({ "a": 1, "b": { "c": 2, "d": 3 } }), json!({ "b": { "d": 4, "e": 5 } })),
            json!({ "a": 1, "b": { "c": 2, "d": 4, "e": 5 } }),
        );
    }

    #[test]
    fn arrays_replace_wholesale() {
        // A repo overriding `lines` replaces the layout; it does not append.
        assert_eq!(
            merged(json!({ "lines": [[ "model", "cost" ], [ "branch" ]] }), json!({ "lines": [[ "model" ]] })),
            json!({ "lines": [[ "model" ]] }),
        );
    }

    #[test]
    fn scalars_replace_wholesale() {
        assert_eq!(merged(json!({ "a": { "b": 1 } }), json!({ "a": 7 })), json!({ "a": 7 }));
        assert_eq!(merged(json!({ "a": 1 }), json!({ "a": { "b": 2 } })), json!({ "a": { "b": 2 } }));
    }

    #[test]
    fn an_explicit_null_replaces() {
        assert_eq!(merged(json!({ "a": 1 }), json!({ "a": null })), json!({ "a": null }));
        assert_eq!(merged(json!({ "a": { "b": 1 } }), json!({ "a": null })), json!({ "a": null }));
    }

    #[test]
    fn prototype_keys_are_skipped_at_the_top() {
        for key in FORBIDDEN_KEYS {
            let over = json!({ key: { "polluted": true }, "kept": 1 });
            let out = merged(json!({}), over);
            assert_eq!(out.get(key), None, "`{key}` survived at the top level");
            assert_eq!(out.get("kept"), Some(&json!(1)));
        }
    }

    #[test]
    fn prototype_keys_are_skipped_at_every_depth() {
        for key in FORBIDDEN_KEYS {
            // Reached by recursion, because both sides hold an object here.
            let out = merged(json!({ "deep": { "nested": {} } }), json!({ "deep": { "nested": { key: 1, "ok": 2 } } }));
            assert_eq!(out.pointer(&format!("/deep/nested/{key}")), None, "`{key}` survived a recursive merge");
            assert_eq!(out.pointer("/deep/nested/ok"), Some(&json!(2)));

            // Reached by wholesale replacement, where no recursion happens.
            let out = merged(json!({ "deep": 1 }), json!({ "deep": { "nested": { key: 1, "ok": 2 } } }));
            assert_eq!(out.pointer(&format!("/deep/nested/{key}")), None, "`{key}` survived a replacement");
            assert_eq!(out.pointer("/deep/nested/ok"), Some(&json!(2)));

            // And inside an array, which also replaces wholesale.
            let out = merged(json!({}), json!({ "list": [{ key: 1, "ok": 2 }] }));
            assert_eq!(out.pointer(&format!("/list/0/{key}")), None, "`{key}` survived inside an array");
            assert_eq!(out.pointer("/list/0/ok"), Some(&json!(2)));
        }
    }

    #[test]
    fn merging_over_a_non_object_base_replaces_it() {
        assert_eq!(merged(json!(5), json!({ "a": 1 })), json!({ "a": 1 }));
    }

    #[test]
    fn a_missing_or_malformed_file_is_none() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(read_json_file(&dir.path().join("absent.json")), None);

        let bad = dir.path().join("bad.json");
        fs::write(&bad, "{ not json").unwrap();
        assert_eq!(read_json_file(&bad), None, "a malformed layer is ignored, not fatal");
    }

    #[test]
    fn an_atomic_write_round_trips_and_leaves_no_temp_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("out.json");
        write_json_atomic(&path, &json!({ "a": 1 })).unwrap();

        assert_eq!(read_json_file(&path), Some(json!({ "a": 1 })));
        let strays: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "left a temp file behind: {strays:?}");
    }

    #[test]
    fn value_accessors_are_absent_rather_than_wrong_typed() {
        let v = json!({ "s": "x", "n": 1.5, "o": {}, "a": [], "wrong": true });
        assert_eq!(opt_str(&v, "s"), Some("x"));
        assert_eq!(opt_str(&v, "wrong"), None);
        assert_eq!(opt_str(&v, "absent"), None);
        assert_eq!(opt_f64(&v, "n"), Some(1.5));
        assert_eq!(opt_f64(&v, "wrong"), None);
        assert!(opt_obj(&v, "o").is_some());
        assert!(opt_obj(&v, "a").is_none());
        assert!(opt_arr(&v, "a").is_some());
    }
}
