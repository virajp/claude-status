//! Generates `schemas/claude-status.schema.json` from the config types.
//!
//! **Author-time only.** The whole module sits behind the `schema` feature,
//! which nothing enables by default, so the released binary carries neither
//! this code nor `schemars` — criterion 8. `cargo run --features schema --bin
//! schema` (or `mise run code:schema`) is the only caller.
//!
//! The schema was hand-written for the whole of this tool's life, and the file
//! this module emits is byte-compared against the committed one by
//! `tests/schema.rs`. That check is the point: a field added to [`Config`]
//! changes the generated schema, the comparison fails, and the fix is named in
//! the failure. Nothing else in the repo could notice.
//!
//! ## What the derives cannot say
//!
//! Four shapes in the config are genuinely polymorphic — a colour is a name, a
//! hex string *or* a triple; a layout entry is an id *or* an object — and the
//! types keep them as [`Value`] because typing them would reject configs that
//! render today. A `Value` has no schema worth publishing (`true`, "anything
//! goes"), so the marker types below carry the shape instead, attached to the
//! real fields with `#[schemars(with = "…")]`. They are never constructed:
//! only their [`JsonSchema`] impls are ever used.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde_json::{Map, Value};

use crate::config::Config;
use crate::config::write::{SCHEMA_KEY, SCHEMA_URL};

/// A colour spec, at `#/$defs/color`.
///
/// `SegmentStyle::bg`/`fg`, `PowerlineConfig::thin_fg`, `Config::default_fg`
/// and a status bucket's `bg` are all `Option<Value>` in the types — there is
/// no `Color` in this program, because [`color::resolve`](super::color::resolve)
/// takes the three forms and never names one.
///
/// **`null` is a fourth form, and the hand-written schema was wrong to omit
/// it.** An explicit `null` deserializes to `None`, which is what makes a
/// segment fall through to `defaultFg` — it is the only way to *clear* a
/// colour the shipped defaults set. `write::non_defaults` emits it: a user who
/// writes `"segments": { "model": { "fg": null } }` gets that key back
/// verbatim from `--configure`, so a schema rejecting it made the binary's own
/// output invalid against the binary's own schema. Pinned by
/// `tests/schema.rs::a_freshly_written_config_validates_against_the_schema`.
pub struct ColorSpec;

impl JsonSchema for ColorSpec {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "color".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A palette name, a hex string (#rgb or #rrggbb), or an RGB triple. `null` clears a colour the defaults set, so the segment falls through to `defaultFg`.",
            "oneOf": [ { "type": "string" }, rgb_triple(), { "type": "null" } ],
        })
    }
}

/// A `[r, g, b]` triple, inlined wherever it appears.
///
/// `inline_schema` is `true` so `palette`'s values stay written out rather than
/// becoming a fourth `$def`: the palette is the block a person reads first when
/// they open the schema, and a `$ref` there costs a lookup for three numbers.
pub struct RgbTriple;

impl JsonSchema for RgbTriple {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "rgb".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        rgb_triple()
    }
}

fn rgb_triple() -> Schema {
    json_schema!({
        "type": "array",
        "items": { "type": "integer", "minimum": 0, "maximum": 255 },
        "minItems": 3,
        "maxItems": 3,
    })
}

/// `spend.show`, whose Rust type is a bare [`String`].
///
/// Two values mean something and any third renders — see
/// [`crate::spend::verdict`]. `enum` rather than `const`, so an editor offers
/// both; the binary does not enforce it and neither does `--debug`.
pub struct ShowMode;

impl JsonSchema for ShowMode {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "showMode".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "enum": [ "auto", "always" ] })
    }
}

/// One entry of `subagent.statuses`.
///
/// The **keys** of that map are the user's own bucket names, so the map is
/// open; the value under each is not. `SubagentConfig::statuses` is a
/// `Map<String, Value>` because a bucket that is not an object has no `match`
/// and lands in the fallback rather than costing the config, so there is no
/// Rust struct here for a derive to read.
pub struct StatusBucket;

impl JsonSchema for StatusBucket {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "statusBucket".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let color = generator.subschema_for::<ColorSpec>();
        json_schema!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "match": {
                    "type": "string",
                    "description": "Case-insensitive regex against the lower-cased task status. Empty string = fallback bucket.",
                },
                "symbol": { "type": "string", "description": "Glyph shown for this status." },
                "bg": with_description(color, "Head-segment background for this status."),
            },
        })
    }
}

/// One entry in a `lines` row, at `#/$defs/segmentEntry`.
///
/// [`SegmentEntry`](super::SegmentEntry) is a hand-rolled `Id | Styled | Other`
/// with a hand-written [`Serialize`](serde::Serialize) and a total
/// `From<Value>` — and **no `Deserialize` at all**, so a derive has nothing to
/// read. `Other` is deliberately absent from the schema: it exists so a row
/// containing a number still renders its siblings, which is a tolerance rather
/// than a form to suggest.
pub struct SegmentEntrySchema;

impl JsonSchema for SegmentEntrySchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "segmentEntry".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let color = generator.subschema_for::<ColorSpec>();
        json_schema!({
            "description": "Either a segment id string, or an object that names a segment and overrides its styling.",
            "oneOf": [
                { "type": "string" },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": { "type": "string", "description": "Segment id." },
                        "id": { "type": "string", "description": "Alias for `name`." },
                        "bg": color,
                        "fg": color,
                        "bold": { "type": [ "boolean", "null" ] },
                    },
                },
            ],
        })
    }
}

/// A `$ref` with a `description` beside it — legal in 2020-12, where `$ref` no
/// longer erases its siblings, and how the hand-written schema described the
/// four colour fields that share `#/$defs/color`.
fn with_description(schema: Schema, description: &str) -> Schema {
    let mut schema = schema;
    schema.insert("description".into(), description.into());
    schema
}

/// The generated schema, as the exact [`Value`] that gets written.
pub fn generate() -> Value {
    let generated = SchemaGenerator::default().root_schema_for::<Config>().to_value();
    let mut root = match generated {
        Value::Object(map) => map,
        other => panic!("schemars did not produce an object root: {other}"),
    };

    strip(&mut root);
    add_schema_pointer_property(&mut root);
    restore_caps_defaults(&mut root);
    reorder(root)
}

/// Removes what schemars adds and the published schema has never carried.
///
/// Two things, and each would be a regression rather than a nicety:
///
/// - **`default`** — every container carries `#[serde(default)]`, so schemars
///   inlines each field's shipped value. That would re-embed the whole palette,
///   all twenty symbols and both layout rows into the schema, including the
///   Nerd Font private-use codepoints `assets/claude-status.defaults.json` is
///   marked `-text -diff` to protect. The four `caps` defaults are the only
///   ones the published schema has ever had, and they are put back by
///   [`restore_caps_defaults`].
/// - **`format`** — `"uint32"`, `"uint"` and `"double"` are schemars' own
///   vocabulary rather than registered draft-2020-12 formats, and the
///   `minimum`/`maximum` beside them already say the same thing to a validator
///   that understands it. Seven of them appear without this.
///
/// **`title` is deliberately *not* stripped.** The instinct is to, on the
/// theory that schemars titles every block with the first line of its doc
/// comment — it does not. `_private::rustdoc::get_title_and_description`
/// returns a title only when the doc comment's first non-blank byte is `#`, a
/// markdown heading, and nothing in this crate writes one. Stripping would
/// therefore remove nothing, and would silently disarm
/// `tests/schema.rs::the_schema_has_one_title_and_it_is_the_products_name` —
/// leaving a guard that could no longer fail. The root's own title is set by
/// [`reorder`], from the type's `#[schemars(rename)]`.
fn strip(schema: &mut Map<String, Value>) {
    for key in ["default", "format"] {
        schema.remove(key);
    }
    for value in schema.values_mut() {
        strip_value(value);
    }
}

fn strip_value(value: &mut Value) {
    match value {
        Value::Object(map) => strip(map),
        Value::Array(items) => items.iter_mut().for_each(strip_value),
        _ => {}
    }
}

/// Puts the four `caps` defaults back, from the constant the hook reads.
///
/// Written from [`crate::caps::DEFAULTS`] rather than repeated as literals, so
/// changing a shipped cap changes the schema and the drift check says so.
fn restore_caps_defaults(schema: &mut Map<String, Value>) {
    let defaults = crate::caps::DEFAULTS;
    let pairs = [
        ("context", defaults.context),
        ("fiveHour", defaults.five_hour),
        ("sevenDay", defaults.seven_day),
        ("spend", defaults.spend),
    ];
    for (key, value) in pairs {
        let Some(Value::Object(property)) = schema
            .get_mut("properties")
            .and_then(|p| p.get_mut("caps"))
            .and_then(|c| c.get_mut("properties"))
            .and_then(|p| p.get_mut(key))
        else {
            panic!("the generated schema has no caps.{key} to give a default to");
        };
        property.insert("default".into(), value.into());
    }
}

/// Declares `$schema` as a property.
///
/// [`Config`] deliberately does not model it — it is a pointer that buys editor
/// completions, not a setting, and `write::non_defaults` emits it into every
/// file `--configure` writes. Without this the binary's own output would fail
/// to validate against the binary's own schema, because the root is
/// `additionalProperties: false`.
fn add_schema_pointer_property(schema: &mut Map<String, Value>) {
    let Some(Value::Object(properties)) = schema.get_mut("properties") else {
        panic!("the generated schema has no properties object");
    };
    let mut with_pointer = Map::new();
    with_pointer.insert(SCHEMA_KEY.to_string(), serde_json::json!({ "type": "string" }));
    with_pointer.extend(std::mem::take(properties));
    *properties = with_pointer;
}

/// Fixes the root's key order and injects `$id`.
///
/// `$id` is **never** emitted by schemars, and it is the key an editor resolves
/// `$ref`s against. It is read from [`SCHEMA_URL`] — the same constant
/// `--configure` writes into every config — so the published file and the
/// pointer inside it cannot disagree.
fn reorder(mut root: Map<String, Value>) -> Value {
    let mut out = Map::new();
    let meta = root.remove("$schema").unwrap_or_else(|| panic!("schemars emitted no $schema"));
    out.insert("$schema".into(), meta);
    out.insert("$id".into(), Value::String(SCHEMA_URL.to_string()));
    out.insert("title".into(), Value::String("claude-status config".into()));
    for key in ["description", "type", "additionalProperties", "properties", "$defs"] {
        if let Some(value) = root.remove(key) {
            out.insert(key.into(), value);
        }
    }
    // Anything schemars grows in a future version lands here rather than being
    // dropped on the floor: a silently discarded keyword is how a generated
    // schema stops describing the thing it was generated from.
    out.extend(root);
    Value::Object(out)
}

/// The generated schema as the exact bytes of the committed file.
///
/// Two-space indent and a trailing newline, which is what
/// `dprint`'s json plugin produces for this file — the schema is **not** in
/// dprint's excludes, so a generator whose output the formatter would rewrite
/// makes the drift check oscillate between two "correct" files forever.
/// `tests/schema.rs::the_generated_schema_is_already_dprint_formatted` holds
/// that line.
pub fn render() -> String {
    let mut text = serde_json::to_string_pretty(&generate()).expect("a generated schema serialises");
    text.push('\n');
    text
}
