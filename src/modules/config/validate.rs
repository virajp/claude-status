//! What `--doctor` could not make sense of, per layer.
//!
//! **Advisory and nothing else.** Nothing here reaches the render path, nothing
//! it finds changes a byte of stdout, and nothing it finds moves the exit code.
//! the third invariant is not negotiable in a diagnostics module: a config
//! that renders today renders identically with every finding below printed
//! against it.
//!
//! # Why a `Value` walk rather than a second, stricter deserialization
//!
//! The obvious design is one set of types with two derives — the permissive one
//! the renderer uses and a `deny_unknown_fields` one the validator uses. It
//! does not work here, for four separate reasons:
//!
//! 1. **It cannot attribute a finding to a layer.** The whole point is that a
//!    typo is reported under *the file that contains it*. A deserialization
//!    happens once, after the merge, by which time the three layers are one
//!    tree.
//! 2. **It reports one finding and stops.** Serde returns the first error.
//!    A user with three typos would fix one, re-run, and find another.
//! 3. **It cannot see a coercion at all.** `gauge.width: 0` deserializes
//!    successfully — to ten. That is the finding worth having, and there is no
//!    error for it to be.
//! 4. **[`Caps`](crate::caps::Caps) has a hand-written `Deserialize`**, where
//!    `#[serde(deny_unknown_fields)]` is a silent no-op that still compiles.
//!
//! So the walk reads each layer's own raw tree against the **generated
//! schema** — the same file editors validate against, embedded at build time.
//! One source for what a key is called, and a drift check
//! (`tests/schema.rs`) that keeps it in step with the types.
//!
//! # The three kinds, and why the middle one is never a warning
//!
//! - **⚠ unknown key** — a key in a *closed* object. The schema says this
//!   object has a fixed set of keys, so anything else is a typo and can be
//!   said so with confidence.
//! - **· not a key this binary reads** — a key in an *open* map, outside what
//!   the binary asks for. Legal: `symbols` and `segments` are keyed by names
//!   the user chooses, and a key there is not wrong, merely unused. Warning
//!   about it would be warning about a working config.
//! - **· coerced** — what the binary *did* with a value. `gauge.width: 0`
//!   renders ten. No schema can produce this line, and it is the one a user
//!   staring at a wrong-looking bar actually needs.
//!
//! # The five open maps
//!
//! `palette`, `symbols`, `typeSymbols`, `segments` and — the one an
//! object-shaped reading of the schema misses — `subagent.statuses`. They are
//! open at the **key** level only: `segments.foo` is a legal segment id this
//! build may not know, while `segments.foo.bge` is a typo inside a closed
//! [`SegmentStyle`](super::SegmentStyle), and the walk says so.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::config::matcher::Matcher;
use crate::config::{Config, DEFAULT_GAUGE_EMPTY, DEFAULT_GAUGE_FILLED, resolved_gauge_width, resolved_glyph};

/// The generated schema, compiled in.
///
/// The same bytes `schemas/claude-status.schema.json` holds and an editor
/// fetches — so "what `--doctor` calls a typo" and "what your editor underlines"
/// are one answer by construction. `tests/schema.rs` holds that file to being
/// what the types generate, which closes the loop: types → schema → validator.
const SCHEMA_JSON: &str = include_str!("../../../schemas/claude-status.schema.json");

/// One thing the binary could not make sense of, or made sense of differently
/// than the file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A key in a closed object. The only kind that warns.
    UnknownKey { path: String, suggestion: Option<String> },
    /// A key in an open map that nothing in this build asks for.
    Unread { path: String },
    /// A value the binary accepted and changed.
    Coerced { path: String, from: String, to: String },
}

impl Finding {
    /// The line `--doctor` prints, without indentation.
    ///
    /// The marker is part of the text rather than a column of its own: `⚠` and
    /// `·` are the whole difference between "you have a bug" and "here is
    /// something you might not know", and a reader scanning the report should
    /// not have to align two columns to tell them apart.
    pub fn line(&self) -> String {
        match self {
            Self::UnknownKey { path, suggestion: Some(near) } => {
                format!("\u{26a0} unknown key `{path}` (did you mean `{near}`?)")
            }
            Self::UnknownKey { path, suggestion: None } => format!("\u{26a0} unknown key `{path}`"),
            Self::Unread { path } => format!("\u{b7} {path} is not a key this binary reads"),
            Self::Coerced { path, from, to } => format!("\u{b7} {path} {from} \u{2192} {to}"),
        }
    }

    /// Is this the kind that means something is wrong?
    ///
    /// **No production caller today** — `app.rs` renders through [`Self::line`]
    /// and never classifies. Its one use is the unit test below, which asserts
    /// that a legal key does not warn *without* matching on a glyph, so the
    /// classification can be checked independently of how it is drawn. The e2e
    /// tests deliberately do match the glyph, because there the rendered line
    /// is what is being pinned.
    pub fn is_warning(&self) -> bool {
        matches!(self, Self::UnknownKey { .. })
    }
}

/// What the merged config makes "used" mean for the open maps.
///
/// Built from the **merged** [`Config`], not from the layer being walked: a
/// palette entry defined in the user layer and named by the embedded layer's
/// segments is read, and a per-layer view would call it unused.
pub struct Context {
    /// Palette names some colour spec in the merged config actually names.
    referenced_colors: BTreeSet<String>,
    /// The glyph names the binary looks up.
    ///
    /// Taken as the keys the **shipped** `symbols` carries rather than as a
    /// list written out here. The embedded layer is the binary's own statement
    /// of which glyphs it draws, so the two cannot drift; where it errs it errs
    /// towards silence (it ships `repo`, which no builder asks for), and a
    /// note nobody sees beats a note that is wrong.
    read_symbols: BTreeSet<String>,
}

impl Context {
    pub fn new(merged: &Config) -> Self {
        let mut referenced_colors = BTreeSet::new();
        for spec in color_specs(merged) {
            if let Some(name) = spec.as_str() {
                referenced_colors.insert(name.to_string());
            }
        }
        Self { referenced_colors, read_symbols: Config::default().symbols.keys().cloned().collect() }
    }
}

/// Every position in the config where a colour spec may name a palette entry.
///
/// Enumerated from the typed config rather than found by walking the schema for
/// `$ref: color`: the typed version is the one the compiler checks, and a
/// colour position added to the types without being added here shows up as a
/// spurious `·` note rather than as a wrong render.
fn color_specs(config: &Config) -> Vec<&Value> {
    let panel = &config.subagent.segments;
    let styles = config
        .segments
        .values()
        .chain([&panel.head, &panel.name, &panel.model, &panel.desc, &panel.tokens, &panel.duration]);

    [config.default_fg.as_ref(), config.powerline.thin_fg.as_ref()]
        .into_iter()
        .flatten()
        .chain(styles.flat_map(|s| [s.bg.as_ref(), s.fg.as_ref()]).flatten())
        .chain(config.subagent.statuses.values().filter_map(|b| b.get("bg")))
        .chain(config.lines.iter().flatten().filter_map(super::SegmentEntry::overrides).flat_map(|obj| {
            [obj.get("bg"), obj.get("fg")]
        }).flatten())
        .collect()
}

/// Everything one layer's own tree gets wrong, or gets differently.
///
/// `layer` is the object as the file wrote it — for the repo layer, the part
/// that survived the narrowing, because everything else is already reported on
/// its own row and saying it twice in two vocabularies helps nobody.
pub fn findings(layer: &Value, context: &Context) -> Vec<Finding> {
    let Ok(schema) = serde_json::from_str::<Value>(SCHEMA_JSON) else {
        // Unreachable — the file is `include_str!`d and a test parses it — but
        // a `--doctor` that panicked over its own diagnostics would be worse
        // than one that skips them.
        return Vec::new();
    };
    let mut walker = Walker { defs: schema.get("$defs").cloned().unwrap_or(Value::Null), context, findings: Vec::new() };
    walker.walk(layer, &schema, "");
    walker.findings
}

struct Walker<'a> {
    defs: Value,
    context: &'a Context,
    findings: Vec<Finding>,
}

impl Walker<'_> {
    fn walk(&mut self, value: &Value, node: &Value, path: &str) {
        let node = self.resolve(node);
        let Some(node) = node.as_object() else { return };

        // A `oneOf` is a shape choice, and the value has already made it. Pick
        // the branch whose `type` the value satisfies and walk that one: `bg`
        // is a string *or* a triple *or* null, and reporting against all three
        // would report two failures for every correct value.
        if let Some(Value::Array(branches)) = node.get("oneOf") {
            if let Some(branch) = branches.iter().find(|b| matches_type(value, &self.resolve(b))) {
                let branch = self.resolve(branch);
                self.walk(value, &branch, path);
            }
            return;
        }

        match value {
            Value::Object(entries) => self.walk_object(entries, node, path),
            Value::Array(items) => {
                if let Some(items_schema) = node.get("items") {
                    let items_schema = items_schema.clone();
                    for (i, item) in items.iter().enumerate() {
                        self.walk(item, &items_schema, &format!("{path}[{i}]"));
                    }
                }
            }
            _ => {}
        }
        self.note_coercion(value, path);
    }

    fn walk_object(&mut self, entries: &Map<String, Value>, node: &serde_json::Map<String, Value>, path: &str) {
        let properties = node.get("properties").and_then(Value::as_object);
        let additional = node.get("additionalProperties");
        // `Some(false)` is the schema saying "these keys and no others". Every
        // block the binary reads by name is written that way; the five open
        // maps carry a subschema here instead.
        let closed = matches!(additional, Some(Value::Bool(false)));

        for (key, value) in entries {
            let child = join(path, key);
            match properties.and_then(|p| p.get(key)) {
                Some(schema) => {
                    let schema = schema.clone();
                    self.walk(value, &schema, &child);
                }
                None if closed => {
                    let candidates: Vec<&str> =
                        properties.map(|p| p.keys().map(String::as_str).collect()).unwrap_or_default();
                    self.findings.push(Finding::UnknownKey { path: child, suggestion: nearest(key, &candidates) });
                }
                None => {
                    if let Some(unread) = self.unread(path, key) {
                        self.findings.push(unread);
                    }
                    if let Some(schema) = additional.filter(|s| !matches!(s, Value::Bool(_))) {
                        let schema = schema.clone();
                        self.walk(value, &schema, &child);
                    }
                }
            }
        }
    }

    /// Is a key in an open map one nothing in this build asks for?
    ///
    /// Only two of the five maps can answer this. `typeSymbols` is keyed by a
    /// subagent's `type`, which is whatever the running agent called itself,
    /// and `subagent.statuses` by the user's own bucket labels — every key in
    /// both is read, so a note there would be false.
    fn unread(&self, parent: &str, key: &str) -> Option<Finding> {
        let path = join(parent, key);
        match parent {
            "palette" if !self.context.referenced_colors.contains(key) => Some(Finding::Unread { path }),
            "symbols" if !self.context.read_symbols.contains(key) => Some(Finding::Unread { path }),
            "segments" if !crate::render::segments::KNOWN.contains(&key) => Some(Finding::Unread { path }),
            _ => None,
        }
    }

    /// What the binary did with a value that it accepted and changed.
    ///
    /// Every rule here is asked of the function the renderer uses, never
    /// restated: `resolved_gauge_width` is the body of the `gauge.width`
    /// deserializer and `Matcher::compile` is what `worktree_matcher` calls.
    /// A coercion the binary stops making stops being reported, without anyone
    /// remembering to come here.
    fn note_coercion(&mut self, value: &Value, path: &str) {
        let coerced = match path {
            "gauge.width" => {
                let resolved = resolved_gauge_width(value);
                // `as_u64() != Some(resolved)` and not a `Value` comparison:
                // that is the question the deserializer asked, and it answers
                // it for a float, a string and a negative alike — all of which
                // resolve to ten and are worth reporting.
                (value.as_u64() != Some(resolved as u64)).then(|| (render(value), resolved.to_string()))
            }
            "gauge.filled" | "gauge.empty" => {
                let fallback = if path == "gauge.filled" { DEFAULT_GAUGE_FILLED } else { DEFAULT_GAUGE_EMPTY };
                let resolved = resolved_glyph(value, fallback);
                (value.as_str() != Some(resolved)).then(|| (render(value), format!("{resolved:?}")))
            }
            // The pattern is compiled in `Config::worktree_matcher`, not in a
            // deserializer, so an uncompilable one costs nothing until a
            // worktree path is rendered — and then warns on stderr, which is
            // not where anyone is looking.
            "worktreePattern" => match value.as_str() {
                Some(p) if !p.is_empty() && Matcher::compile(p).is_err() => {
                    Some((render(value), format!("{:?} (not a valid regex)", super::DEFAULT_WORKTREE_PATTERN)))
                }
                Some("") => Some((render(value), format!("{:?}", super::DEFAULT_WORKTREE_PATTERN))),
                _ => None,
            },
            _ => None,
        };
        if let Some((from, to)) = coerced {
            self.findings.push(Finding::Coerced { path: path.to_string(), from, to });
        }
    }

    /// Follows a same-document `$ref` into `$defs`.
    ///
    /// Same-document only, and deliberately: the generated schema has no other
    /// kind, and a validator that could be made to fetch a URL by a config file
    /// is a validator that reaches the network from `--doctor`.
    fn resolve(&self, node: &Value) -> Value {
        let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
            return node.clone();
        };
        let Some(name) = reference.strip_prefix("#/$defs/") else {
            return node.clone();
        };
        self.defs.get(name).cloned().unwrap_or(Value::Null)
    }
}

/// Does a value satisfy a schema branch's `type`?
fn matches_type(value: &Value, branch: &Value) -> bool {
    let Some(expected) = branch.get("type") else {
        // A branch with no `type` accepts anything, which is the honest answer
        // for a `$ref` the walk could not resolve.
        return true;
    };
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    match expected {
        Value::String(t) => t == actual || (t == "integer" && actual == "number"),
        Value::Array(types) => types.iter().filter_map(Value::as_str).any(|t| t == actual),
        _ => true,
    }
}

/// `a.b` — and a bare key at the root, so a top-level typo reads
/// `unknown key \`powerlin\`` rather than `unknown key \`.powerlin\``.
fn join(parent: &str, key: &str) -> String {
    if parent.is_empty() { key.to_string() } else { format!("{parent}.{key}") }
}

/// A value as it would appear in the file, short enough for one report line.
fn render(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 24 { text } else { format!("{}…", text.chars().take(23).collect::<String>()) }
}

/// The closest candidate key, if one is close enough to be worth suggesting.
///
/// The threshold is deliberately tight. A wrong suggestion is worse than none:
/// it sends a reader to rename a key that was never the one they meant, and
/// they will believe it, because the tool said it.
fn nearest(key: &str, candidates: &[&str]) -> Option<String> {
    let budget = match key.chars().count() {
        0..=3 => 1,
        4..=8 => 2,
        _ => 3,
    };
    candidates
        .iter()
        .map(|c| (distance(key, c), *c))
        .filter(|(d, _)| *d <= budget)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c.to_string())
}

/// Levenshtein distance, two rows at a time.
///
/// Over `char`s and not bytes: a key may be any JSON string, and measuring a
/// multi-byte one by its UTF-8 length would make every non-ASCII typo look far
/// from everything.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Findings for one layer, against the shipped config as the merged one.
    fn findings_for(layer: Value) -> Vec<Finding> {
        let merged = Config::new(layer.clone());
        findings(&layer, &Context::new(&merged))
    }

    fn lines(layer: Value) -> Vec<String> {
        findings_for(layer).iter().map(Finding::line).collect()
    }

    #[test]
    fn the_embedded_schema_parses() {
        let schema: Value = serde_json::from_str(SCHEMA_JSON).expect("the compiled-in schema is JSON");
        assert!(schema["properties"].is_object(), "and it is the schema, not some other file");
    }

    #[test]
    fn a_config_the_binary_understands_produces_nothing() {
        // Every shape the walk has an opinion about, all of them correct.
        let clean = json!({
            "$schema": "https://example.invalid/s.json",
            "caps": { "context": 50 },
            "gauge": { "width": 4, "filled": "#", "empty": "-" },
            "worktreePattern": "worktree",
            "palette": { "mine": [1, 2, 3] },
            "defaultFg": "mine",
            "segments": { "model": { "bg": "mine", "bold": true } },
            "lines": [["model", { "name": "branch", "fg": [1, 2, 3] }]],
            "subagent": { "statuses": { "anything": { "match": "x", "symbol": "*", "bg": "mine" } } },
            "typeSymbols": { "whatever-the-agent-called-itself": "x" },
        });
        assert_eq!(lines(clean), Vec::<String>::new());
    }

    /// **Criterion 4, the reporting half.** The rendering half is in
    /// `tests/e2e.rs`, because "the bar is unchanged" is only true end to end.
    #[test]
    fn a_typo_in_a_closed_block_is_a_warning_with_a_suggestion() {
        assert_eq!(lines(json!({ "powerlin": {} })), ["\u{26a0} unknown key `powerlin` (did you mean `powerline`?)"]);
        // And nested, where a derive could not have looked: `segments.foo` is a
        // legal id, `segments.foo.bge` is a typo inside a closed style.
        assert_eq!(
            lines(json!({ "segments": { "model": { "bge": "red" } } })),
            ["\u{26a0} unknown key `segments.model.bge` (did you mean `bg`?)"],
        );
    }

    /// A layout entry is the one place the walk has to pick a `oneOf` branch
    /// *and* index an array, so the path it builds is the one most likely to
    /// come out wrong. A bare id is a string and has nothing to check; a styled
    /// entry is closed over `name`/`id`/`bg`/`fg`/`bold`.
    #[test]
    fn a_typo_inside_a_styled_layout_entry_names_its_row_and_position() {
        assert_eq!(
            lines(json!({ "lines": [["model"], ["branch", { "name": "cost", "bg": "red", "bld": true }]] })),
            ["\u{26a0} unknown key `lines[1][1].bld` (did you mean `bold`?)"],
        );
        // The same entry, spelled correctly, says nothing.
        assert_eq!(
            lines(json!({ "lines": [["branch", { "id": "cost", "bg": "red", "bold": true }]] })),
            Vec::<String>::new(),
        );
    }

    #[test]
    fn a_key_nothing_is_near_is_reported_without_a_guess() {
        assert_eq!(lines(json!({ "quuxifier": 1 })), ["\u{26a0} unknown key `quuxifier`"]);
    }

    /// **Criterion 6, asserted positively.** "Not an error" passes with zero
    /// lines written — `palette` is an open map, so no key in it *can* be
    /// unknown. What has to be true is that the key is reported, as a note.
    #[test]
    fn an_unused_palette_entry_is_a_note_and_never_a_warning() {
        let found = findings_for(json!({ "palette": { "nobodys": [1, 2, 3] } }));
        assert_eq!(found, [Finding::Unread { path: "palette.nobodys".into() }]);
        assert!(!found[0].is_warning(), "a legal key must never warn");
        assert_eq!(found[0].line(), "\u{b7} palette.nobodys is not a key this binary reads");
    }

    #[test]
    fn a_palette_entry_something_names_is_not_reported() {
        assert_eq!(lines(json!({ "palette": { "mine": [1, 2, 3] }, "defaultFg": "mine" })), Vec::<String>::new());
    }

    #[test]
    fn a_glyph_name_no_builder_asks_for_is_a_note() {
        assert_eq!(lines(json!({ "symbols": { "contxt": "x" } })), ["\u{b7} symbols.contxt is not a key this binary reads"]);
        assert_eq!(lines(json!({ "symbols": { "context": "x" } })), Vec::<String>::new());
    }

    /// The two open maps whose keys are *all* read. A note here would be a
    /// false one: nothing in the binary has a list to check them against.
    #[test]
    fn arbitrary_keys_are_legal_in_the_two_maps_the_binary_iterates() {
        assert_eq!(lines(json!({ "typeSymbols": { "some-new-agent": "x" } })), Vec::<String>::new());
        assert_eq!(lines(json!({ "subagent": { "statuses": { "whatever": { "match": "x" } } } })), Vec::<String>::new());
        // But the bucket itself is closed.
        assert_eq!(
            lines(json!({ "subagent": { "statuses": { "whatever": { "matches": "x" } } } })),
            ["\u{26a0} unknown key `subagent.statuses.whatever.matches` (did you mean `match`?)"],
        );
    }

    #[test]
    fn a_segment_id_this_build_does_not_know_is_a_note() {
        assert_eq!(lines(json!({ "segments": { "invented": { "bg": "red" } } })), [
            "\u{b7} segments.invented is not a key this binary reads",
        ]);
        assert_eq!(lines(json!({ "segments": { "branch": { "bg": "red" } } })), Vec::<String>::new());
    }

    /// **Criterion 5.**
    #[test]
    fn a_zero_gauge_width_reports_the_ten_it_became() {
        assert_eq!(lines(json!({ "gauge": { "width": 0 } })), ["\u{b7} gauge.width 0 \u{2192} 10"]);
    }

    /// The clamp the `gauge_width` deserializer applies and nothing has ever
    /// reported: a width past `MAX_GAUGE_WIDTH` feeds `str::repeat`, and the
    /// cap is what keeps an allocation failure — which `catch_unwind` cannot
    /// catch — off the render path.
    #[test]
    fn a_gauge_width_past_the_ceiling_reports_the_clamp() {
        assert_eq!(lines(json!({ "gauge": { "width": 9999 } })), ["\u{b7} gauge.width 9999 \u{2192} 1000"]);
        assert_eq!(lines(json!({ "gauge": { "width": 1000 } })), Vec::<String>::new(), "the ceiling itself is not a coercion");
    }

    #[test]
    fn a_gauge_width_that_is_not_a_count_reports_the_ten_it_became() {
        assert_eq!(lines(json!({ "gauge": { "width": -3 } })), ["\u{b7} gauge.width -3 \u{2192} 10"]);
        assert_eq!(lines(json!({ "gauge": { "width": "wide" } })), ["\u{b7} gauge.width \"wide\" \u{2192} 10"]);
    }

    #[test]
    fn an_empty_gauge_glyph_reports_the_shipped_one() {
        assert_eq!(lines(json!({ "gauge": { "filled": "" } })), ["\u{b7} gauge.filled \"\" \u{2192} \"\u{25b0}\""]);
        assert_eq!(lines(json!({ "gauge": { "empty": "" } })), ["\u{b7} gauge.empty \"\" \u{2192} \"\u{25b1}\""]);
    }

    #[test]
    fn a_pattern_that_will_not_compile_reports_the_fallback() {
        assert_eq!(lines(json!({ "worktreePattern": "[" })), [
            "\u{b7} worktreePattern \"[\" \u{2192} \"worktree\" (not a valid regex)",
        ]);
        assert_eq!(lines(json!({ "worktreePattern": "" })), ["\u{b7} worktreePattern \"\" \u{2192} \"worktree\""]);
        assert_eq!(lines(json!({ "worktreePattern": "wt|main" })), Vec::<String>::new());
    }

    /// `$schema` is a pointer a correctly written file carries. Reporting it
    /// would put a warning in `--doctor` for every file `--configure` wrote.
    #[test]
    fn the_schema_pointer_is_not_an_unknown_key() {
        assert_eq!(lines(json!({ "$schema": "https://example.invalid/s.json" })), Vec::<String>::new());
    }

    /// A value of the wrong *shape* is not this module's business — the
    /// deserializers already degrade it, and `--doctor`'s job is to say what
    /// they did, not to duplicate the schema an editor already applies.
    #[test]
    fn a_block_that_is_not_an_object_is_not_walked_into() {
        assert_eq!(lines(json!({ "powerline": "nope", "gauge": 3, "lines": 7 })), Vec::<String>::new());
    }

    #[test]
    fn several_findings_are_all_reported_rather_than_the_first() {
        let found = lines(json!({
            "powerlin": {},
            "symbols": { "contxt": "x" },
            "gauge": { "width": 0 },
        }));
        assert_eq!(found.len(), 3, "serde would have stopped at the first: {found:?}");
    }

    #[test]
    fn a_suggestion_is_withheld_when_nothing_is_close() {
        assert_eq!(nearest("zzzz", &["powerline", "gauge"]), None);
        assert_eq!(nearest("gauge", &["powerline", "gauge"]), Some("gauge".into()));
        assert_eq!(nearest("guage", &["powerline", "gauge"]), Some("gauge".into()));
    }

    #[test]
    fn distance_counts_characters_rather_than_bytes() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("abc", "abc"), 0);
        assert_eq!(distance("abc", "abd"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        // Two three-char keys, six bytes each: measured as bytes this is 6.
        assert_eq!(distance("é\u{e9}a", "é\u{e9}b"), 1);
    }

    #[test]
    fn a_long_value_is_shortened_rather_than_wrapped_across_the_report() {
        let long = "x".repeat(200);
        let found = lines(json!({ "gauge": { "width": long } }));
        assert_eq!(found.len(), 1);
        assert!(found[0].chars().count() < 60, "a coercion line stayed one line: {}", found[0]);
    }
}
