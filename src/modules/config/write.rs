//! Writing the user config back out, carrying **only what differs** from the
//! shipped defaults.
//!
//! The installer used to seed `assets/claude-status.defaults.json` verbatim as
//! the user's config, so every install froze a full copy of the shipped values
//! at the version it happened to install. A user who never touched a key still
//! had it pinned, and a later release changing that default reached nobody.
//!
//! So a written config holds a key only where the user's value and the
//! binary's disagree. An unset key follows the binary forward, which is the
//! whole point — and it is only possible because [`Config::default`] is a value
//! the code can *name*, pinned to the asset by
//! `the_embedded_defaults_deserialize_to_the_default_config`. If that test is
//! ever weakened, everything here becomes unsound in a way nothing here can
//! detect: it would omit a key the user set, or emit one they did not.
//!
//! # Why the diff is structural rather than `skip_serializing_if`
//!
//! `#[serde(skip_serializing_if = "…is_empty")]` is not sufficient for the open
//! maps, and the failure would look like it worked. `palette`, `symbols`,
//! `typeSymbols` and `segments` all have **non-empty defaults**, so "skip if
//! empty" would emit the whole shipped palette the moment a user changed one
//! colour. The file would be valid, the bar would render, and the user would
//! have silently frozen every other colour at today's values. Every map is
//! therefore diffed **entry by entry**, at every depth.
//!
//! # What cannot be expressed
//!
//! Two exemptions, both tested, both about **removal** — which
//! [`crate::json::deep_merge`] has no operator for.
//!
//! 1. **A key the defaults carry and the config does not** — a deleted palette
//!    entry, say — is skipped. No file content could express it either, so this
//!    is a property of the merge rather than a shortcut here.
//!    (`a_key_the_defaults_carry_cannot_be_removed`)
//! 2. **A block degraded to its `unstyled` state** — `{"subagent": 5}` — is
//!    emitted *partially*: the half that differs and is expressible goes out,
//!    the emptied half does not, and a reload restores the shipped values for
//!    it. This is the sharper of the two, because the output looks deliberate.
//!    (`a_degraded_block_does_not_survive_a_round_trip`)
//!
//! The second is technically expressible — `"statuses": null` reproduces an
//! empty table, since a non-object degrades to empty on the way back in — and
//! is deliberately **not** done: it would write a malformed value into a file a
//! human edits, to reproduce a state that exists only because their input was
//! malformed. Degradation is lossy by design, mapping `5`, `"x"`, `null` and
//! `[]` onto one state; round-tripping it would preserve the damage.
//!
//! Neither can bite yet — nothing calls [`write()`]. They are pinned so the
//! cycle that adds `--configure` inherits a known boundary.

use std::path::Path;

use serde_json::{Map, Value};

use crate::config::Config;
use crate::json::write_json_atomic_pretty;

/// The published schema, so a written file gets editor completions.
///
/// **Always emitted**, and the one key in the output that is not a
/// non-default: it is a pointer that makes the file editable rather than a
/// setting, and a config the user is invited to hand-edit without completions
/// is a config they will mistype. The same URL the shipped defaults carry.
pub const SCHEMA_URL: &str = "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json";

/// The `$schema` key itself, which [`Config`] does not model — it is an
/// unknown key to the types, deliberately.
const SCHEMA_KEY: &str = "$schema";

/// `config` reduced to `$schema` plus everything that differs from
/// [`Config::default`].
pub fn non_defaults(config: &Config) -> Value {
    // Through `Value` rather than a bespoke `Serialize` impl per type: the
    // comparison is between two *trees*, and the open maps make the shape of
    // the tree depend on the config. Serialising both sides and walking them
    // together is one rule that holds at every depth, where a per-field
    // implementation would be one rule per field to keep in step with the
    // types — which is precisely the drift this module cannot survive.
    let current = serde_json::to_value(config).unwrap_or(Value::Null);
    let shipped = serde_json::to_value(Config::default()).unwrap_or(Value::Null);

    let mut out = Map::new();
    out.insert(SCHEMA_KEY.to_string(), Value::String(SCHEMA_URL.to_string()));
    if let Some(Value::Object(changed)) = diff(&current, &shipped) {
        out.extend(changed);
    }
    Value::Object(out)
}

/// Writes [`non_defaults`] to `path`.
///
/// Atomic and pretty, through the shared writer: two sessions can reconfigure
/// at once, and unlike the spend cache this is a file a person opens.
pub fn write(path: &Path, config: &Config) -> std::io::Result<()> {
    write_json_atomic_pretty(path, &non_defaults(config))
}

/// What `current` says that `shipped` does not. `None` means "identical, emit
/// nothing".
///
/// # The two `f64` fields, and the trap under them
///
/// `subagent.descBudgetFraction` and `spend.refreshMinutes` are `f64`, compared
/// here through [`Value`]'s `PartialEq`. Three facts, all measured rather than
/// assumed, because the instinct is to reach for an epsilon and the real hazard
/// is somewhere else entirely:
///
/// 1. **`Value` equality is representation-sensitive, not lenient.**
///    `json!(15) == json!(15.0)` is **`false`** — `serde_json::Number` compares
///    its internal variant, and those are `PosInt(15)` and `Float(15.0)`. So no
///    int/float leniency is being relied on here. There is none.
/// 2. **It never matters, because both sides go through `to_value`.** The
///    asset carries `"refreshMinutes": 15` — a JSON *integer* — but it is
///    deserialized into an `f64` long before this runs, and
///    `serde_json::to_value(15.0f64)` is always `Number(15.0)`. The raw asset
///    value never reaches the comparison. Safety comes from **what is compared**,
///    not from how numbers compare.
/// 3. **A JSON round trip is bit-identical.** `serde_json` emits the shortest
///    representation that reparses to the same bits, verified for `0.45`,
///    `15.0`, `15.5`, `0.1 + 0.2`, `1.0/3.0` and `f64::MIN_POSITIVE`. A value
///    that came from a file and goes back to one crosses no lossy boundary.
///
/// **The trap fact 2 implies:** comparing against the raw `DEFAULTS_JSON` tree
/// instead of `to_value(Config::default())` would make every `f64` field differ
/// — `Number(15)` vs `Number(15.0)` — and this module would emit both of them
/// into *every* config it ever wrote, for every user, having changed nothing.
/// The file would be valid and the bar identical, so nothing would notice.
/// Pinned by `comparing_against_the_raw_asset_would_emit_every_f64_field`.
///
/// An epsilon would be wrong on its own terms too. The question is not "are
/// these close" but "did the user set this", and the error costs are not
/// symmetric: a false *difference* costs one redundant key the merge applies to
/// no effect, while a false *sameness* costs the user their setting. Exact
/// comparison can only err the cheap way.
///
/// Three cases, and the middle one is the whole module:
///
/// - equal → nothing
/// - **two objects** → recurse key by key, so one changed palette entry emits
///   one entry rather than the whole palette
/// - anything else → the value wholesale, matching [`deep_merge`], which
///   replaces arrays and scalars rather than merging them. `lines` is the case
///   that matters: a user who reorders one row means to replace the layout.
///
/// Iteration follows **`current`'s** key order, not the defaults'. That is
/// load-bearing for `subagent.statuses`, whose order decides which bucket wins.
///
/// [`deep_merge`]: crate::json::deep_merge
fn diff(current: &Value, shipped: &Value) -> Option<Value> {
    if current == shipped {
        return None;
    }
    let (Value::Object(current), Value::Object(shipped)) = (current, shipped) else {
        return Some(current.clone());
    };

    let mut out = Map::new();
    for (key, value) in current {
        match shipped.get(key) {
            Some(default) => {
                if let Some(changed) = diff(value, default) {
                    out.insert(key.clone(), changed);
                }
            }
            // A key with no default under it: emitted whole, because there is
            // nothing beneath it for the merge to fill in.
            None => {
                out.insert(key.clone(), value.clone());
            }
        }
    }

    // Empty means every difference was a key the defaults carry and this
    // config does not — see the module note on what cannot be expressed.
    (!out.is_empty()).then(|| Value::Object(out))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::config::layers;

    /// The real path a written config takes: a user layer on disk, merged and
    /// deserialized exactly as a render does it.
    fn load_user(home: &Path, layer: &Value) -> Config {
        let path = layers::user_config_path(home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_string(layer).unwrap()).unwrap();
        layers::load(Some(home), None).config
    }

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path().to_path_buf();
        (dir, home)
    }

    /// Writes `layer`, serialises what it merged into, writes *that* back as
    /// the only user layer, and reloads. The two configs must agree — the
    /// property every other test here is a special case of.
    fn round_trip(layer: Value) -> (Config, Config, Value) {
        let (_a, home) = tmp();
        let before = load_user(&home, &layer);
        let emitted = non_defaults(&before);

        let (_b, home2) = tmp();
        let after = load_user(&home2, &emitted);
        (before, after, emitted)
    }

    /// Criterion 2.
    #[test]
    fn a_config_differing_in_one_key_emits_that_key_and_the_schema() {
        let (_dir, home) = tmp();
        let config = load_user(&home, &json!({ "defaultFg": "aqua" }));

        assert_eq!(
            non_defaults(&config),
            json!({ "$schema": SCHEMA_URL, "defaultFg": "aqua" }),
            "the whole shipped config came out, not the one key that moved",
        );
    }

    /// The zero-config case: a config nobody has touched writes out as nothing
    /// but the pointer that lets them touch it.
    #[test]
    fn an_untouched_config_emits_only_the_schema() {
        assert_eq!(non_defaults(&Config::default()), json!({ "$schema": SCHEMA_URL }));
        assert_eq!(non_defaults(&layers::load(None, None).config), json!({ "$schema": SCHEMA_URL }));
    }

    /// Criterion 3, and the failure this module exists to prevent: emitting the
    /// whole palette would be valid, would render, and would silently freeze
    /// the other nine colours at today's values.
    #[test]
    fn changing_one_palette_entry_emits_one_entry() {
        let (_dir, home) = tmp();
        let config = load_user(&home, &json!({ "palette": { "blue": [1, 2, 3] } }));

        let out = non_defaults(&config);
        assert_eq!(out["palette"], json!({ "blue": [1, 2, 3] }));
        assert_eq!(out["palette"].as_object().unwrap().len(), 1, "the other nine colours were frozen: {out}");
        assert_eq!(out.as_object().unwrap().len(), 2, "and nothing else came along: {out}");
    }

    /// The Risks section extends criterion 3 to the other maps by hand. All
    /// five are here, plus the one nested inside `segments`, because the
    /// previous cycle's inventory of these was short and it mattered.
    #[test]
    fn every_open_map_is_diffed_entry_by_entry() {
        let (_dir, home) = tmp();
        let config = load_user(
            &home,
            &json!({
                "palette": { "blue": [1, 2, 3] },
                "symbols": { "model": "M" },
                "typeSymbols": { "task": "T" },
                "segments": { "model": { "bg": "red" } },
                "subagent": { "statuses": { "done": { "bg": "purple" } } },
            }),
        );

        let out = non_defaults(&config);
        assert_eq!(out["palette"], json!({ "blue": [1, 2, 3] }));
        assert_eq!(out["symbols"], json!({ "model": "M" }));
        assert_eq!(out["typeSymbols"], json!({ "task": "T" }));
        // `segments.model` is the nested case: the entry survives, but only the
        // one style key inside it that moved. The shipped `model` also sets
        // `fg` and `bold`, and emitting those would freeze them.
        assert_eq!(out["segments"], json!({ "model": { "bg": "red" } }));
        assert_eq!(out["subagent"], json!({ "statuses": { "done": { "bg": "purple" } } }));
    }

    /// An inline entry override lives inside `lines`, which replaces wholesale
    /// — so this is really a check that the array is emitted intact rather
    /// than diffed element by element, which the merge could not reassemble.
    #[test]
    fn a_layout_change_emits_the_whole_layout() {
        let (_dir, home) = tmp();
        let layout = json!([["model", { "name": "cost", "bg": "red" }]]);
        let config = load_user(&home, &json!({ "lines": layout }));

        assert_eq!(non_defaults(&config)["lines"], layout);
    }

    /// **`subagent.statuses` order is behaviour**: the first bucket whose
    /// `match` hits wins, and the last empty-`match` entry is the fallback. A
    /// subset emission is order-safe here, and the reason is worth spelling
    /// out, because it is not obvious and the safe-looking alternative is not
    /// safer.
    ///
    /// Emitting only the changed entries preserves order because
    /// [`deep_merge`] merges a key the base already has **in place** and
    /// appends only genuinely new ones. So a reload reproduces "the defaults'
    /// order, then the extras" — which is exactly the order the config being
    /// serialised already has, since it came from that same merge.
    ///
    /// Emitting the map **whole** would not do better: the merge would still
    /// place `done` where the embedded layer has it and still append `mine`
    /// last, whatever order the file listed them in. The subset is chosen
    /// because it is smaller, not because the alternative was rejected as
    /// wrong.
    ///
    /// The standing limitation that leaves — a hand-added bucket can never be
    /// ranked *before* a shipped one — is a property of the merge having no
    /// insert-at-position operator, and is not this module's to fix.
    ///
    /// [`deep_merge`]: crate::json::deep_merge
    #[test]
    fn an_added_status_bucket_keeps_its_rank_across_a_round_trip() {
        let (before, after, emitted) = round_trip(json!({
            "subagent": { "statuses": { "mine": { "match": "mine", "symbol": "!", "bg": "red" } } },
        }));

        let order = |c: &Config| c.subagent.statuses.keys().cloned().collect::<Vec<_>>();
        assert_eq!(order(&before), ["done", "error", "pending", "running", "mine"]);
        assert_eq!(order(&after), order(&before), "the reload re-ranked the buckets: {emitted}");
        assert_eq!(before.subagent.statuses, after.subagent.statuses);
    }

    /// Reordering the shipped buckets is a change to `statuses` as a *map*,
    /// and the subset diff cannot see it — `done` is still `done`. The reload
    /// therefore restores the shipped order. Asserted rather than left to be
    /// discovered: it is the sharp edge of the paragraph above.
    #[test]
    fn reordering_the_shipped_status_buckets_does_not_survive_a_round_trip() {
        let (_dir, home) = tmp();
        let shipped = Config::default().subagent.statuses.clone();
        let mut reversed = Map::new();
        for key in shipped.keys().rev() {
            reversed.insert(key.clone(), shipped[key].clone());
        }

        let before = load_user(&home, &json!({ "subagent": { "statuses": reversed } }));
        // The merge already flattened it on the way in — the file's order never
        // reached the config, so the serialiser is not what lost it.
        assert_eq!(
            before.subagent.statuses.keys().collect::<Vec<_>>(),
            shipped.keys().collect::<Vec<_>>(),
            "deep_merge, not this module, is where a reorder is lost",
        );
        assert_eq!(non_defaults(&before), json!({ "$schema": SCHEMA_URL }), "and nothing differs to emit");
    }

    /// The `f64` case the differ's doc comment argues about, made concrete.
    ///
    /// The asset carries `"refreshMinutes": 15` — a JSON **integer** — while
    /// `SpendConfig::default()` is the `f64` `15.0`. If those compared unequal
    /// the serialiser would emit `refreshMinutes` into every file it ever
    /// wrote, for every user, having changed nothing. They do not, because both
    /// sides of the diff are produced by `serde_json::to_value` from the same
    /// `f64`, so the integer literal in the asset never reaches the comparison.
    #[test]
    fn an_f64_equal_to_its_default_is_not_emitted() {
        let (_dir, home) = tmp();

        // Written back as the integer the asset uses, and as the float.
        for written in [json!(15), json!(15.0)] {
            let config = load_user(&home, &json!({ "spend": { "refreshMinutes": written } }));
            assert_eq!(config.spend.refresh_minutes, 15.0);
            assert_eq!(non_defaults(&config), json!({ "$schema": SCHEMA_URL }), "{written} was emitted as a change");
        }

        // And a genuine change still is emitted — including a fractional one,
        // which is the case an epsilon comparison would be tempted to swallow.
        let config = load_user(&home, &json!({ "spend": { "refreshMinutes": 15.5 } }));
        assert_eq!(non_defaults(&config)["spend"], json!({ "refreshMinutes": 15.5 }));

        let config = load_user(&home, &json!({ "subagent": { "descBudgetFraction": 0.46 } }));
        assert_eq!(non_defaults(&config)["subagent"], json!({ "descBudgetFraction": 0.46 }));
    }

    /// The trap the differ's doc comment names, made to bite.
    ///
    /// `non_defaults` compares `to_value(config)` against
    /// `to_value(Config::default())`. Comparing against the **raw**
    /// `DEFAULTS_JSON` tree instead looks equivalent and is not: the asset
    /// writes `"refreshMinutes": 15` as a JSON integer, `Number(15)`, while the
    /// typed default serialises to `Number(15.0)`, and `serde_json` compares
    /// those as **unequal**. Every `f64` field would then be emitted into every
    /// config this module ever wrote, for every user, having changed nothing —
    /// and the file would be valid and the bar identical, so nothing downstream
    /// would notice.
    ///
    /// This asserts the hazard is real *and* that the shipping code avoids it,
    /// because a test that only asserted the outcome would go on passing if
    /// someone "simplified" the comparison to the asset.
    #[test]
    fn comparing_against_the_raw_asset_would_emit_every_f64_field() {
        let raw: Value = serde_json::from_str(crate::config::defaults::DEFAULTS_JSON).expect("the asset parses");
        let typed = serde_json::to_value(Config::default()).expect("the default serialises");

        // The hazard: the two representations of the same number differ.
        assert_eq!(raw["spend"]["refreshMinutes"], json!(15), "the asset carries a JSON integer");
        assert_eq!(typed["spend"]["refreshMinutes"], json!(15.0), "the typed default carries a float");
        assert_ne!(
            raw["spend"]["refreshMinutes"], typed["spend"]["refreshMinutes"],
            "`serde_json` compares `Number(15)` and `Number(15.0)` as equal, so this test is now inert",
        );

        // And the shipping path is on the safe side of it: a config that has
        // touched neither field emits neither.
        let (_dir, home) = tmp();
        let untouched = load_user(&home, &json!({ "defaultFg": "aqua" }));
        let out = non_defaults(&untouched);
        assert!(out.get("spend").is_none(), "refreshMinutes was emitted unprompted: {out}");
        assert!(out.get("subagent").is_none(), "descBudgetFraction was emitted unprompted: {out}");
    }

    /// **The second limitation, and it is not the palette one.**
    ///
    /// A block that was *degraded* — present but not an object — does not
    /// survive a round trip, and it fails differently from the emptied-map case
    /// below. `{"subagent": 5}` degrades to [`SubagentConfig::unstyled`], whose
    /// `statuses` is empty; the diff emits the `segments` half (which differs
    /// and is expressible) and drops the empty `statuses` (which is not), so a
    /// reload brings the four shipped buckets back and the panel silently
    /// regains its status symbols and colours.
    ///
    /// The palette case emits **nothing**. This emits something **partial**,
    /// which is the sharper failure: the output looks like a deliberate config.
    ///
    /// **Why this is documented rather than fixed.** It is expressible, in a
    /// form worse than the bug: emitting `"statuses": null` would reproduce the
    /// empty table exactly, because a non-object table degrades to empty on the
    /// way back in. That means writing a deliberately malformed value into a
    /// file a human is invited to edit, in order to faithfully reproduce a
    /// state that only exists because their input was malformed to begin with.
    /// Degradation is lossy **by design** — it maps `5`, `"x"`, `null` and `[]`
    /// onto one state — and a writer that round-tripped it would be preserving
    /// the damage rather than the configuration.
    ///
    /// Nothing writes a config yet, so this is latent. It is pinned here so the
    /// `--configure` cycle inherits a known boundary instead of discovering it.
    #[test]
    fn a_degraded_block_does_not_survive_a_round_trip() {
        for layer in [
            json!({ "subagent": 5 }),
            json!({ "subagent": "x" }),
            json!({ "subagent": null }),
            json!({ "subagent": [] }),
            json!({ "subagent": { "statuses": "x" } }),
        ] {
            let (before, after, emitted) = round_trip(layer.clone());
            assert!(before.subagent.statuses.is_empty(), "{layer} did not degrade as expected");
            assert_eq!(
                after.subagent.statuses.len(),
                4,
                "{layer} unexpectedly round-tripped; emitted:\n{emitted:#}",
            );
            assert_ne!(before, after, "{layer}");
        }

        // The same shape one level down, where a dropped *entry* is restored
        // rather than a dropped table.
        for layer in [json!({ "symbols": { "model": 5 } }), json!({ "segments": { "model": 5 } })] {
            let (before, after, _) = round_trip(layer.clone());
            assert_ne!(before, after, "{layer}");
        }
    }

    /// The documented limitation, pinned so it is a decision and not a
    /// surprise. There is no file content that could express it either.
    #[test]
    fn a_key_the_defaults_carry_cannot_be_removed() {
        let (_dir, home) = tmp();
        // A non-object palette is the one way to reach an *empty* one through
        // the real path: it costs its colours and nothing else.
        let config = load_user(&home, &json!({ "palette": "gruvbox" }));
        assert!(config.palette.is_empty());

        assert_eq!(
            non_defaults(&config),
            json!({ "$schema": SCHEMA_URL }),
            "an emptied map has nothing to say that the merge could hear",
        );
    }

    /// An explicit `null` where the defaults carry a value is a real setting —
    /// `deep_merge` assigns it, and the reader treats it as unset. Emitting
    /// nothing would give the user their `defaultFg` back.
    #[test]
    fn clearing_a_default_emits_an_explicit_null() {
        let (_dir, home) = tmp();
        let config = load_user(&home, &json!({ "defaultFg": null }));
        assert_eq!(config.default_fg, None);

        assert_eq!(non_defaults(&config), json!({ "$schema": SCHEMA_URL, "defaultFg": null }));
    }

    /// The property the rest of this module is in service of, over a config
    /// that moves something in every shape the types have: a scalar, a block,
    /// each open map, the layout, and the panel.
    ///
    /// **`well_formed` is load-bearing in the name.** This held the unqualified
    /// claim "a written config reloads to the config it came from", which is
    /// not true of every `Config` — only of one deserialized from a layer whose
    /// values were all *usable*. Two documented exemptions break it, and both
    /// have tests of their own: a map emptied by degradation
    /// (`a_key_the_defaults_carry_cannot_be_removed`) and a block degraded to
    /// its `unstyled` state (`a_degraded_block_does_not_survive_a_round_trip`).
    ///
    /// A test naming a property it does not have is the exact failure this
    /// cycle has been finding elsewhere; the name is narrowed rather than the
    /// exemptions being quietly excluded from the fixture.
    #[test]
    fn a_well_formed_config_reloads_to_the_config_it_came_from() {
        let (before, after, emitted) = round_trip(json!({
            "projectName": "widget",
            "defaultFg": "aqua",
            "worktreePattern": "wt",
            "caps": { "context": 50 },
            "gauge": { "width": 3, "filled": "#" },
            "powerline": { "sep": "|" },
            "spend": { "refreshMinutes": 0, "show": "always" },
            "palette": { "blue": [1, 2, 3], "mine": [9, 9, 9] },
            "symbols": { "model": "M", "mine": "X" },
            "typeSymbols": { "task": "T" },
            "segments": { "model": { "bg": "red" }, "mine": { "bg": "green", "bold": true } },
            "lines": [["model", "cost"], ["project"]],
            "subagent": {
                "descBudgetFraction": 0.9,
                "statuses": { "done": { "bg": "purple" } },
                "segments": { "name": { "bg": "red" } },
            },
        }));

        assert_eq!(before, after, "the round trip lost or invented something. emitted:\n{emitted:#}");
        // Not vacuous: the config genuinely moved off the defaults.
        assert_ne!(before, Config::default());
    }

    /// The same property for a config that changes **nothing**, which is the
    /// state a fresh install is now in.
    #[test]
    fn a_default_config_reloads_to_the_defaults() {
        let (before, after, _) = round_trip(json!({}));
        assert_eq!(before, Config::default());
        assert_eq!(after, Config::default());
    }

    #[test]
    fn a_written_file_is_pretty_newline_terminated_and_reloadable() {
        let (_dir, home) = tmp();
        let config = load_user(&home, &json!({ "defaultFg": "aqua" }));

        let (_out_dir, out_home) = tmp();
        let path = layers::user_config_path(&out_home);
        write(&path, &config).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("}\n"), "not newline-terminated: {text:?}");
        assert!(text.contains('\n'), "written as one line: {text:?}");
        assert_eq!(layers::load(Some(&out_home), None).config.default_fg, Some(json!("aqua")));
    }

    /// The writer creates its own directory — `~/.config/claude-status/` will
    /// not exist on a machine that has never been configured, which after this
    /// cycle is every machine until someone configures one.
    #[test]
    fn writing_creates_the_config_directory() {
        let (_dir, home) = tmp();
        let path = layers::user_config_path(&home);
        assert!(!path.parent().unwrap().exists());

        write(&path, &Config::default()).unwrap();
        assert!(path.exists());
    }
}
