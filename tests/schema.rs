//! The committed JSON schema, held to being generated output *and* a schema.
//!
//! Two halves, and they gate in different builds on purpose:
//!
//! - The **drift** test needs `--features schema`, because generating requires
//!   `schemars` and the release binary must not carry it (criterion 8). It is
//!   why `.config/mise/tasks/code/test` runs the suite with that feature on:
//!   `cargo test` alone would skip it silently, which is exactly the kind of
//!   gate that passes with the thing it protects deleted.
//! - Everything else reads the **committed file** and needs no feature at all,
//!   so a plain `cargo test` still holds the schema to being valid
//!   draft-2020-12, to describing what `--configure` writes, and to carrying
//!   its prose.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use claude_status::caps;
use claude_status::config::Config;
use claude_status::config::write;

/// How many `description` strings the schema carried at this cycle's parent
/// commit (`fca2aa2`), where it was hand-written.
///
/// Criterion 3 is "none is lost", and a count is the cheapest form of that
/// which cannot be satisfied by accident. It was checked by pointer *and* text
/// when the generator landed: all 39 came through with the same JSON pointer,
/// and 38 with the same string — `/$defs/color` gained a sentence saying it
/// admits `null`, which the spec amendment records as deliberate. The count is
/// what keeps the pointers true afterwards; [`DESCRIPTION_DIGEST`] holds the
/// text.
/// A schema that grows a key and describes it makes this go up, which is a
/// deliberate edit rather than a silent loss.
const PARENT_DESCRIPTION_COUNT: usize = 39;

/// FNV-1a of every `(pointer, description)` pair in the committed schema.
///
/// Pins the **text**, which [`PARENT_DESCRIPTION_COUNT`] alone does not: all 39
/// strings can be replaced with `"x"` and the count still reads 39. Update it
/// only when you meant to change prose, and say so in review.
const DESCRIPTION_DIGEST: u64 = 0x5c72_2dcb_8d80_c664;

/// The only four `default` values the published schema has ever carried.
///
/// Every config container is `#[serde(default)]`, so a generator left to
/// itself inlines the shipped value of every field — the whole palette, all
/// twenty symbols, both layout rows. That would re-embed the Nerd Font
/// private-use codepoints `assets/claude-status.defaults.json` is marked
/// `-text -diff` to keep out of ordinary editors, into a file dprint *does*
/// format.
///
/// **Read from [`caps::DEFAULTS`], never hand-copied.**
///
/// A literal table here would be a **third** copy of the four thresholds, and
/// the one nothing checks: hard-code the values in
/// `config::schema::restore_caps_defaults` *and* change a shipped cap, and the
/// schema would advertise a threshold the binary does not use with nothing
/// red. Deriving the expectation is what makes that pair impossible — change
/// `caps::DEFAULTS` alone and the drift test fires; break the wiring alone and
/// this one does.
fn expected_defaults() -> [(&'static str, u64); 4] {
    let d = caps::DEFAULTS;
    [
        ("context", u64::from(d.context)),
        ("fiveHour", u64::from(d.five_hour)),
        ("sevenDay", u64::from(d.seven_day)),
        ("spend", u64::from(d.spend)),
    ]
}

fn schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas").join("claude-status.schema.json")
}

fn schema_text() -> String {
    std::fs::read_to_string(schema_path()).expect("the schema ships in this repo")
}

fn schema() -> Value {
    serde_json::from_str(&schema_text()).expect("the schema parses")
}

/// Compiles the committed schema and validates one instance against it.
///
/// The resource is registered under a **local** loc rather than the schema's
/// own `$id`, so nothing here can reach the network: every `$ref` in the file
/// is a same-document `#/$defs/...` pointer, and a test that quietly fetched
/// `raw.githubusercontent.com` would pass or fail on someone's wifi.
fn validate(instance: &Value) -> Result<(), String> {
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    compiler.add_resource("claude-status.schema.json", schema()).map_err(|e| e.to_string())?;
    let index = compiler.compile("claude-status.schema.json", &mut schemas).map_err(|e| e.to_string())?;
    schemas.validate(instance, index).map_err(|e| format!("{e:#}"))
}

/// Walks every schema node, yielding `(json pointer, object)`.
fn nodes(root: &Value) -> Vec<(String, &serde_json::Map<String, Value>)> {
    fn walk<'a>(node: &'a Value, pointer: String, out: &mut Vec<(String, &'a serde_json::Map<String, Value>)>) {
        match node {
            Value::Object(map) => {
                out.push((pointer.clone(), map));
                for (key, value) in map {
                    walk(value, format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")), out);
                }
            }
            Value::Array(items) => {
                for (i, value) in items.iter().enumerate() {
                    walk(value, format!("{pointer}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(root, String::new(), &mut out);
    out
}

/// **Criteria 1 and 2.** The committed file is exactly what the types produce.
///
/// Byte equality, not a semantic comparison: the file is committed so editors
/// and the website can fetch it from a stable path, and a diff a human has to
/// squint at is a diff that gets committed by mistake.
#[cfg(feature = "schema")]
#[test]
fn the_committed_schema_is_what_the_config_types_generate() {
    let generated = claude_status::config::schema::render();
    assert_eq!(
        schema_text(),
        generated,
        "schemas/claude-status.schema.json has drifted from the config types — run `mise run code:schema`",
    );
}

/// The generator's output is what dprint would write.
///
/// The schema is **not** in dprint's `excludes` and **is** matched by its
/// `includes: **/*.json`, so a generator emitting anything the formatter would
/// rewrite puts the pre-commit hook and the drift check in a loop: the
/// formatter reflows the file, the drift check says it drifted, regenerating
/// undoes the reflow, forever. Asserted here rather than trusted, because
/// dprint's json plugin is pinned by URL and can change under us.
#[cfg(feature = "schema")]
#[test]
fn the_generated_schema_is_already_dprint_formatted() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("claude-status.schema.json");
    std::fs::write(&path, claude_status::config::schema::render()).unwrap();

    let out = std::process::Command::new("mise")
        .args(["x", "--", "dprint", "check", "--config"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("dprint.json"))
        .arg(&path)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();

    // A machine without the toolchain is not a failing schema. CI always has
    // it, which is where this assertion has to hold.
    let Ok(out) = out else {
        eprintln!("skipped: no `mise` on PATH");
        return;
    };
    assert!(
        out.status.success(),
        "dprint would rewrite the generated schema, so the drift check would never settle:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// **Criterion 9.** The published file is a schema, not merely JSON.
///
/// The website builds its config form from this file, so being well-formed
/// stopped being a courtesy. Read from disk rather than fetched: the `$id`
/// serves `main`, so the copy at that URL is the *previous* commit's until
/// this one merges, and a pre-merge fetch would assert nothing about the
/// change under review. The wording of the criterion says "fetched"; this is
/// the closest thing that can gate a pull request.
#[test]
fn the_committed_schema_compiles_as_draft_2020_12() {
    assert_eq!(
        schema()["$schema"].as_str(),
        Some("https://json-schema.org/draft/2020-12/schema"),
        "the schema must declare the draft its consumers assume",
    );
    // Compiling is the assertion: boon resolves every `$ref`, rejects an
    // unknown keyword shape, and fails on a `$defs` entry nothing points at
    // being malformed.
    validate(&serde_json::json!({})).expect("an empty config validates against the schema");
}

/// The pointer an editor resolves against, and the one `--configure` writes.
///
/// Two constants that must be one: `write::SCHEMA_URL` is stamped into every
/// file the binary writes, and the schema's `$id` is what an editor matches it
/// to. The generator reads the first to produce the second.
///
/// **This guards the agreement, not the wiring.** Replacing the injection with
/// a hand-copied literal of today's value leaves this green — the two still
/// match, and no assertion can tell a derived value from a copy that happens
/// to agree. What it catches is divergence: change `SCHEMA_URL` with a literal
/// in place and the regenerated `$id` is the old URL against the new constant,
/// so it fires exactly when the two stop being one. Confirmed by mutation,
/// after an earlier version of this comment claimed the stronger property and
/// was wrong.
#[test]
fn the_schemas_id_is_the_url_the_binary_writes() {
    assert_eq!(schema()["$id"].as_str(), Some(write::SCHEMA_URL));
    assert_eq!(schema()["title"].as_str(), Some("claude-status config"));
}

/// **The binary's own output must validate against the binary's own schema.**
///
/// `--configure` writes `$schema` into every file (`write::non_defaults`), and
/// [`Config`] deliberately does not model that key. The root is
/// `additionalProperties: false`, so the schema has to declare `$schema`
/// itself or every configured machine holds a file its editor marks invalid —
/// and `tests/defaults_integrity.rs` goes red with it.
#[test]
fn a_freshly_written_config_validates_against_the_schema() {
    let shipped = write::non_defaults(&Config::default());
    assert_eq!(shipped[write::SCHEMA_KEY].as_str(), Some(write::SCHEMA_URL), "the writer still stamps the pointer");
    validate(&shipped).expect("the config `--configure` writes on a clean machine validates");

    // Then the real path, layer by layer: a user file on disk, merged through
    // `layers::load` exactly as `--configure` merges it, and written back out.
    // Not `Config::new` on a bare tree — that skips the embedded layer, so the
    // written result would not be a file this binary can produce.
    //
    // `fg: null` and `bold: null` are the cases that matter and the reason
    // `$defs/color` admits `null`: clearing a shipped colour is the one edit
    // whose round trip a schema written from the *types* gets right and a
    // hand-written one got wrong.
    let dir = tempfile::TempDir::new().unwrap();
    let home = dir.path().join("home");
    let path = claude_status::config::layers::user_config_path(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    for layer in [
        r#"{ "caps": { "context": 50 }, "gauge": { "width": 4 } }"#,
        r#"{ "palette": { "mine": [1, 2, 3] }, "defaultFg": "mine", "spend": { "show": "always" } }"#,
        // Two hashes: a hex colour puts `"#` inside the literal.
        r##"{ "segments": { "model": { "bg": "#ff0000" }, "invented": { "fg": [1, 2, 3] } } }"##,
        r#"{ "segments": { "model": { "fg": null } }, "subagent": { "segments": { "head": { "bold": null } } } }"#,
        r#"{ "lines": [["model", { "name": "branch", "fg": [1, 2, 3], "bold": true }]] }"#,
        r#"{ "subagent": { "statuses": { "busy": { "match": "run", "symbol": "*", "bg": "red" } } } }"#,
        r#"{ "projectName": "widget", "worktreePattern": "wt", "symbols": { "model": "M", "invented": "x" } }"#,
        r#"{ "powerline": { "cap": "(", "sep": ">", "sepThin": "|", "thinFg": null } }"#,
    ] {
        std::fs::write(&path, layer).unwrap();
        let merged = claude_status::config::layers::load(Some(&home), None).config;
        let written = write::non_defaults(&merged);
        validate(&written).unwrap_or_else(|e| panic!("`--configure` would write an invalid file for {layer}:\n{e}"));
    }
}

/// **Criterion 3.** The prose survived generation.
///
/// The hand-written schema's real value was its descriptions — `symbols`
/// enumerating the keys the binary reads, `spend.show` explaining the seat
/// rule. A generator emits structure faithfully and prose only if it was told
/// to, and a schema that lost half its descriptions still validates every
/// config, so nothing else in this repo would notice.
#[test]
fn the_schema_carries_every_description_it_was_written_with() {
    let schema = schema();
    let described: Vec<(String, &str)> = nodes(&schema)
        .into_iter()
        .filter_map(|(pointer, node)| Some((pointer, node.get("description")?.as_str()?)))
        .collect();

    assert_eq!(
        described.len(),
        PARENT_DESCRIPTION_COUNT,
        "the schema went from {PARENT_DESCRIPTION_COUNT} descriptions to {} — criterion 3 forbids losing one; at {:?}",
        described.len(),
        described.iter().map(|(p, _)| p).collect::<Vec<_>>(),
    );
    assert!(described.iter().all(|(_, d)| !d.trim().is_empty()), "an empty description is a lost one wearing a key");

    // The count alone does not hold the *prose*. Replacing all 39 strings with
    // `"x"` keeps the count at 39 and every one non-empty, and the drift test
    // cannot help: editing the `#[schemars(description = …)]` attributes and
    // regenerating moves the committed file and the generator together. This
    // digest is the only anchor that does not move with them.
    //
    // A deliberate prose edit updates this constant — that is the point, and it
    // is what makes the edit visible in review rather than silent.
    let digest = description_digest(&described);
    assert_eq!(
        digest, DESCRIPTION_DIGEST,
        "the schema's description *text* changed.\n\
         If you edited prose deliberately, update DESCRIPTION_DIGEST to {digest:#018x}.\n\
         If you did not, a description has been gutted or swapped — criterion 3 forbids losing one.",
    );
}

/// FNV-1a over every `(pointer, description)` pair, in pointer order.
///
/// Inline rather than a dependency: this needs to be stable across Rust
/// releases, which `DefaultHasher` explicitly is not.
fn description_digest(described: &[(String, &str)]) -> u64 {
    let mut pairs: Vec<&(String, &str)> = described.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (pointer, text) in pairs {
        for byte in pointer.as_bytes().iter().chain(b"\0").chain(text.as_bytes()).chain(b"\0") {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

/// Every user-settable key says what it is.
///
/// The count above catches *loss*; this catches the other direction — a field
/// added to `Config` that generates a property nobody wrote prose for. The
/// allowlist is the four the hand-written schema also left bare, and naming
/// them is what makes a fifth fail.
#[test]
fn every_top_level_property_is_described() {
    let schema = schema();
    let properties = schema["properties"].as_object().expect("a plain properties object");

    let undescribed: BTreeSet<&str> = properties
        .iter()
        .filter(|(_, node)| node.get("description").is_none())
        .map(|(key, _)| key.as_str())
        .collect();
    // `$schema` is a pointer rather than a setting — it is not something a
    // user chooses, so there is nothing to tell them about it.
    let expected: BTreeSet<&str> = ["$schema"].into_iter().collect();
    assert_eq!(undescribed, expected, "a config key an editor cannot explain is a key nobody will set correctly");
}

/// The root stays a plain object with plain `properties`.
///
/// `tests/defaults_integrity.rs` reads `schema["properties"].as_object()` and
/// **unwraps**, so a generator that made the root a `$ref` or an `allOf` —
/// which is what schemars does for any type it decides to put in `$defs` —
/// would not fail here with a message about the schema. It would panic over
/// there, in a test about the shipped defaults.
#[test]
fn the_root_is_a_plain_object_schema() {
    let schema = schema();
    assert_eq!(schema["type"].as_str(), Some("object"));
    assert_eq!(schema["additionalProperties"], Value::Bool(false), "the root is closed, which is what --debug reports on");
    assert!(schema.get("$ref").is_none(), "a $ref root breaks tests/defaults_integrity.rs");
    assert!(schema.get("allOf").is_none(), "an allOf root breaks tests/defaults_integrity.rs");
    assert!(schema["properties"].is_object());
}

/// No shipped value is inlined into the schema except the four caps.
///
/// See [`expected_defaults`]. This is the guard on the palette, the symbol
/// table and the layout staying out of a file dprint formats.
#[test]
fn the_only_defaults_in_the_schema_are_the_four_caps() {
    let schema = schema();
    let with_defaults: Vec<(String, &Value)> =
        nodes(&schema).into_iter().filter_map(|(ptr, node)| node.get("default").map(|d| (ptr, d))).collect();

    let found: Vec<(&str, &Value)> = with_defaults
        .iter()
        .map(|(ptr, d)| (ptr.rsplit('/').next().expect("a pointer has a last segment"), *d))
        .collect();
    let expected: Vec<(&str, Value)> = expected_defaults().iter().map(|(k, v)| (*k, Value::from(*v))).collect();
    let expected: Vec<(&str, &Value)> = expected.iter().map(|(k, v)| (*k, v)).collect();

    assert_eq!(found, expected, "schemars inlined a shipped value the published schema has never carried");
}

/// Exactly one `title`, and it is the root's.
///
/// schemars titles a block from its doc comment when — and **only** when —
/// that comment's first non-blank character is `#`, a markdown heading. So
/// this is a live guard rather than a formality: a doc comment opening with a
/// heading anywhere in the config types would put a sentence written for a
/// Rust reader into an editor's hover for someone writing JSON, and this is
/// the only thing that would notice.
#[test]
fn the_schema_has_one_title_and_it_is_the_products_name() {
    let schema = schema();
    let titles: Vec<(String, &Value)> =
        nodes(&schema).into_iter().filter_map(|(ptr, node)| node.get("title").map(|t| (ptr, t))).collect();
    assert_eq!(titles.len(), 1, "found titles at {:?}", titles.iter().map(|(p, _)| p).collect::<Vec<_>>());
    assert_eq!(titles[0].0, "", "the only title belongs to the root");
}

/// No `format` keyword survives generation.
///
/// schemars stamps `"uint32"` on a `u32`, `"uint"` on a `usize` and `"double"`
/// on an `f64` — seven of them across this schema. None is a registered
/// draft-2020-12 format, the published file has never carried one, and the
/// `minimum`/`maximum` beside each already says the same thing to a validator
/// that understands it. The guard on the strip that removes them.
#[test]
fn the_schema_carries_no_generator_specific_format_keywords() {
    let schema = schema();
    let formats: Vec<(String, &Value)> =
        nodes(&schema).into_iter().filter_map(|(ptr, node)| node.get("format").map(|f| (ptr, f))).collect();
    assert!(formats.is_empty(), "schemars' own type vocabulary leaked into the published schema: {formats:?}");
}

/// `$defs` holds the three shapes that are genuinely shared, and no more.
///
/// A fourth would mean schemars decided to reference a type the published
/// schema inlines — which reads fine to a validator and badly to the human
/// this file is largely written for.
#[test]
fn the_schema_defines_exactly_the_three_shared_shapes() {
    let schema = schema();
    let defs: BTreeSet<&str> = schema["$defs"].as_object().expect("a $defs object").keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = ["color", "style", "segmentEntry"].into_iter().collect();
    assert_eq!(defs, expected);
}
