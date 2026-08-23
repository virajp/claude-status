//! The merged configuration, deserialized into the types every other module
//! reads it through.
//!
//! Config used to be a `serde_json::Value` walked by dotted string paths, on
//! the grounds that every key is optional at every depth and a typed model
//! would either reject a config the old bar accepted or degenerate into the
//! same optionality with more code. Two things overtook that. The code now has
//! to be able to **name a default** — a config writer that stores only what
//! differs from the shipped values cannot ask a `Value` what those are — and a
//! dotted path is a string the compiler never checks, so a renamed key fails
//! silently, at render time, in the one place stdout is the product.
//!
//! The forgiveness survives, in three deliberate places:
//!
//! - **absence** — every struct carries `#[serde(default)]` over a [`Default`]
//!   that *is* the embedded layer, so a key nobody wrote is the shipped value
//!   rather than an error or a zero.
//! - **a present but useless value** — a zero width, an empty glyph, an empty
//!   project name. `#[serde(default)]` fires only for absence, so each of
//!   those carries a `deserialize_with` of its own.
//! - **a tree that will not deserialize at all** — [`Config::new`] falls back
//!   to [`Config::default`] and says so on stderr, so a hand-broken config
//!   costs its own settings and never the bar.
//!
//! Leaves stay [`Value`] wherever the shape is genuinely polymorphic: a colour
//! is a palette name, a hex string *or* an `[r, g, b]` triple, and `bold` is
//! read with JavaScript truthiness, so `"bold": 1` still means bold. Typing
//! those would reject configs that render today.

pub mod autoseed;
pub mod color;
pub mod defaults;
pub mod layers;
pub mod matcher;

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::caps::Caps;
use crate::config::color::{Palette, Rgb};
use crate::config::matcher::Matcher;
use crate::json::leaf;
use crate::spend::SpendConfig;

/// The hard fallback styling for a segment with no configured defaults
/// (contract §4).
pub const FALLBACK_BG: &str = "blue";

/// A ceiling on `gauge.width`, which is a repeat count. Applied by the
/// `gauge_width` deserializer, which explains why it is load-bearing.
pub const MAX_GAUGE_WIDTH: usize = 1000;

/// The shipped gauge width, and what a configured `0` resolves to.
const DEFAULT_GAUGE_WIDTH: usize = 10;

/// The shipped `worktreePattern`, and the fallback for an empty or
/// uncompilable one. See [`Config::worktree_matcher`].
const DEFAULT_WORKTREE_PATTERN: &str = "worktree";

/// Opt-**out**. The shipped defaults carry `true`, so this has to agree with
/// them or a config that failed to parse would silently behave differently
/// from one that never existed.
const DEFAULT_AUTO_CONFIGURE_REPO: bool = true;

/// The shipped `subagent.descBudgetFraction`.
const DEFAULT_DESC_BUDGET_FRACTION: f64 = 0.45;

/// The merged config: embedded → user → repo, deep-merged as `Value` and
/// deserialized here **once**, after the merge and never before.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Whether a render may create the repo config layer it did not find.
    ///
    /// Opt-**out**: the shipped defaults carry `true`, so only a literal
    /// `false` disables it. See `DEFAULT_AUTO_CONFIGURE_REPO`.
    #[serde(deserialize_with = "auto_configure_repo")]
    pub auto_configure_repo: bool,
    /// Thresholds the `--caps-hook` actuator measures usage against. Typed by
    /// [`crate::caps`], which owns both the shape and the per-key fallback.
    pub caps: Caps,
    /// The repo-level display name. Never shipped in the defaults — it is a
    /// per-repo key — so this is the one field whose default is `None`.
    #[serde(deserialize_with = "project_name")]
    pub project_name: Option<String>,
    #[serde(deserialize_with = "palette")]
    pub palette: Palette,
    pub powerline: PowerlineConfig,
    /// The foreground for a segment that sets no `fg` of its own. A [`Value`]
    /// because it is a colour spec, resolved by [`color::resolve`].
    pub default_fg: Option<Value>,
    pub gauge: Gauge,
    /// The raw `worktreePattern`. It is compiled — and can fail to compile —
    /// in [`Config::worktree_matcher`], not here.
    #[serde(deserialize_with = "worktree_pattern")]
    pub worktree_pattern: String,
    /// An open map: the keys are the glyph names the segment builders ask for.
    #[serde(deserialize_with = "glyph_table")]
    pub symbols: BTreeMap<String, String>,
    /// Owned by [`crate::spend`], for the same reason `caps` is owned by
    /// [`crate::caps`]: the module that reads a block should own its shape.
    pub spend: SpendConfig,
    /// An open map keyed by a task's `type`, plus the `_default` entry.
    #[serde(deserialize_with = "glyph_table")]
    pub type_symbols: BTreeMap<String, String>,
    /// An open map: the keys are segment ids, and a user may style a segment
    /// this build does not know about.
    pub segments: BTreeMap<String, SegmentStyle>,
    #[serde(deserialize_with = "lines")]
    pub lines: Vec<Vec<SegmentEntry>>,
    pub subagent: SubagentConfig,
}

impl Config {
    /// Deserializes a merged tree, falling back to the shipped defaults.
    ///
    /// The single `from_value` in the program. A tree that will not
    /// deserialize renders the defaults and reports **one** line on stderr:
    /// the alternative is a hand-broken config blanking the bar, which is the
    /// one outcome the third invariant forbids. The fallback is a value in
    /// code rather than a second parse that could fail in turn.
    pub fn new(root: Value) -> Self {
        serde_json::from_value(root).unwrap_or_else(|e| {
            crate::_shared::diag(&format!(
                "claude-status: the merged config could not be read ({e}); using the shipped defaults"
            ));
            Self::default()
        })
    }

    /// A configured symbol.
    ///
    /// **Deviation:** a key missing from the merged config renders `""`. The
    /// JS interpolated `undefined` and rendered the literal text. With the
    /// embedded layer this is unreachable in practice; it is a guard, not a
    /// behaviour — and it is a *lookup* fallback, which is why it stays a
    /// method rather than becoming a `serde` default.
    pub fn symbol(&self, key: &str) -> &str {
        self.symbols.get(key).map_or("", String::as_str)
    }

    /// Resolves a colour spec against this config's palette.
    pub fn color(&self, spec: Option<&Value>) -> Rgb {
        color::resolve(spec, Some(&self.palette))
    }

    /// The `worktreePattern` matcher.
    ///
    /// Compiling can fail and failing warns on **stderr** — never on stdout,
    /// which is the bar — so this stays a method: a side effect does not
    /// belong in a `Deserialize` impl, where it would fire once per render
    /// whether or not the panel ever asks for a matcher.
    pub fn worktree_matcher(&self) -> Matcher {
        // An *empty* pattern falls back too. It would otherwise match every
        // path component, making the last match always the final one, so the
        // worktree prefix would silently vanish instead of erroring.
        let pattern = Some(self.worktree_pattern.as_str()).filter(|p| !p.is_empty()).unwrap_or(DEFAULT_WORKTREE_PATTERN);
        match Matcher::compile(pattern) {
            Ok(m) => m,
            Err(e) => {
                crate::_shared::diag(&format!(
                    "claude-status: worktreePattern {pattern:?} is not a valid regex ({e}); using {DEFAULT_WORKTREE_PATTERN:?}"
                ));
                Matcher::compile(DEFAULT_WORKTREE_PATTERN).expect("the default pattern is a literal")
            }
        }
    }
}

impl Default for Config {
    /// The embedded defaults, as values rather than as JSON text.
    ///
    /// This is the mirror of `assets/claude-status.defaults.json`, and the two
    /// are pinned to each other by
    /// `the_embedded_defaults_deserialize_to_the_default_config` — field by
    /// field, so a drift names the field rather than dumping two JSON blobs.
    /// The duplication is the point: a default the code cannot name is a
    /// default no config writer can subtract.
    fn default() -> Self {
        Self {
            auto_configure_repo: DEFAULT_AUTO_CONFIGURE_REPO,
            caps: Caps::default(),
            project_name: None,
            palette: default_palette(),
            powerline: PowerlineConfig::default(),
            default_fg: Some(Value::String("white".into())),
            gauge: Gauge::default(),
            worktree_pattern: DEFAULT_WORKTREE_PATTERN.into(),
            symbols: default_symbols(),
            spend: SpendConfig::default(),
            type_symbols: default_type_symbols(),
            segments: default_segments(),
            lines: default_lines(),
            subagent: SubagentConfig::default(),
        }
    }
}

/// The row's own glyphs. Closed in the schema, and closed here.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PowerlineConfig {
    /// Absence and a wrong type land in **different** places, and both are the
    /// old behaviour: a missing `cap` is the shipped glyph, because the
    /// embedded layer always carries one, while a `cap` that is not a string
    /// renders `""` — `powerline()` read it with `as_str().unwrap_or_default()`
    /// and a seamless bar is what that produced.
    #[serde(deserialize_with = "separator")]
    pub cap: String,
    #[serde(deserialize_with = "separator")]
    pub sep: String,
    #[serde(deserialize_with = "separator")]
    pub sep_thin: String,
    /// The seam colour between two same-background segments. A colour spec,
    /// so a [`Value`].
    pub thin_fg: Option<Value>,
}

impl Default for PowerlineConfig {
    fn default() -> Self {
        Self {
            cap: "\u{e0b6}".into(),
            sep: "\u{e0b0}".into(),
            sep_thin: "\u{e0b1}".into(),
            thin_fg: Some(Value::String("grey".into())),
        }
    }
}

/// The fixed-width meter the `context` segment draws.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct Gauge {
    #[serde(deserialize_with = "gauge_width")]
    pub width: usize,
    #[serde(deserialize_with = "filled_glyph")]
    pub filled: String,
    #[serde(deserialize_with = "empty_glyph")]
    pub empty: String,
}

impl Default for Gauge {
    fn default() -> Self {
        Self { width: DEFAULT_GAUGE_WIDTH, filled: "\u{25b0}".into(), empty: "\u{25b1}".into() }
    }
}

/// One segment's styling, from `segments.<id>`, `subagent.segments.<key>`, or
/// an inline entry override.
///
/// Every leaf is an `Option<Value>`: `bg` and `fg` are colour specs with three
/// accepted forms, and `bold` is read with JavaScript truthiness. An explicit
/// `null` deserializes to `None`, which is exactly what the old `??` ladder
/// did with it — while `false` and `0` arrive as themselves and do **not**
/// fall through.
#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SegmentStyle {
    pub bg: Option<Value>,
    pub fg: Option<Value>,
    pub bold: Option<Value>,
}

impl SegmentStyle {
    /// A shipped style. `None` is not the same as a colour: an unset `fg` is
    /// what makes a segment fall through to `defaultFg`.
    fn shipped(bg: Option<&str>, fg: Option<&str>, bold: bool) -> Self {
        Self {
            bg: bg.map(|c| Value::String(c.into())),
            fg: fg.map(|c| Value::String(c.into())),
            bold: bold.then(|| Value::Bool(true)),
        }
    }
}

/// One entry in a layout row.
///
/// The one place the config is genuinely heterogeneous: a bare segment id, or
/// an object naming a segment through `name`/`id` and overriding its styling.
/// [`SegmentEntry::Other`] is what keeps a row containing a number rendering
/// its siblings — the old ladder reached for `.name` on it, got nothing, and
/// omitted that entry alone.
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentEntry {
    Id(String),
    Styled(Map<String, Value>),
    Other(Value),
}

impl SegmentEntry {
    /// The segment this entry names, if it names one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Id(id) => Some(id),
            Self::Styled(obj) => obj.get("name").or_else(|| obj.get("id"))?.as_str(),
            Self::Other(_) => None,
        }
    }

    /// The inline style overrides this entry carries, if it carries any.
    pub fn overrides(&self) -> Option<&Map<String, Value>> {
        match self {
            Self::Styled(obj) => Some(obj),
            _ => None,
        }
    }
}

impl From<Value> for SegmentEntry {
    /// Total, and deliberately so — see [`SegmentEntry::Other`].
    fn from(v: Value) -> Self {
        match v {
            Value::String(id) => Self::Id(id),
            Value::Object(obj) => Self::Styled(obj),
            other => Self::Other(other),
        }
    }
}

/// The subagent panel's own block. Closed in the schema, and closed here.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubagentConfig {
    #[serde(deserialize_with = "desc_budget_fraction")]
    pub desc_budget_fraction: f64,
    /// **Order is load-bearing**: statuses are tried in config order and the
    /// first match wins, so this is a `serde_json::Map` — an `IndexMap` under
    /// `preserve_order` — and not a [`BTreeMap`], which would silently
    /// re-rank a user's buckets alphabetically.
    ///
    /// The entries stay [`Value`]: a bucket with no `match` is the fallback,
    /// and a bucket that is not an object has no `match` either, so it lands
    /// there too rather than costing the whole config.
    pub statuses: Map<String, Value>,
    pub segments: SubagentSegments,
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            desc_budget_fraction: DEFAULT_DESC_BUDGET_FRACTION,
            statuses: default_statuses(),
            segments: SubagentSegments::default(),
        }
    }
}

/// The panel's six row segments. Fixed keys, unlike the main bar's open map:
/// a panel row is built by this program, not by the layout.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct SubagentSegments {
    /// Only `fg` and `bold` are ever read: the head's background always comes
    /// from the matched status.
    pub head: SegmentStyle,
    pub name: SegmentStyle,
    pub model: SegmentStyle,
    pub desc: SegmentStyle,
    pub tokens: SegmentStyle,
    pub duration: SegmentStyle,
}

impl Default for SubagentSegments {
    fn default() -> Self {
        Self {
            head: SegmentStyle::shipped(None, None, true),
            name: SegmentStyle::shipped(Some("orange"), None, true),
            model: SegmentStyle::shipped(Some("blue"), None, false),
            desc: SegmentStyle::shipped(Some("bg3"), None, false),
            tokens: SegmentStyle::shipped(Some("aqua"), None, false),
            duration: SegmentStyle::shipped(Some("purple"), None, false),
        }
    }
}

/// The gauge width. A configured `0` means ten, as `||` made it, and anything
/// that is not a non-negative integer means ten too — `as_u64` rejected a
/// string and a negative before this was typed, and still does.
///
/// Capped at [`MAX_GAUGE_WIDTH`]: the width feeds `str::repeat`, and an
/// enormous one aborts on allocation failure. An abort is unrecoverable —
/// `catch_unwind` cannot catch it, so the fallback line would never print and
/// the bar would go blank, which is the one outcome the third invariant
/// forbids.
fn gauge_width<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    Ok(match Value::deserialize(d)?.as_u64() {
        Some(0) | None => DEFAULT_GAUGE_WIDTH,
        Some(n) => (n as usize).min(MAX_GAUGE_WIDTH),
    })
}

fn filled_glyph<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    glyph(d, "\u{25b0}")
}

fn empty_glyph<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    glyph(d, "\u{25b1}")
}

/// A gauge glyph, with the shipped fallback for an *empty* one.
///
/// Unlike `symbols.*`, these are not allowed to resolve to `""`: the old
/// implementation hard-coded `▰`/`▱` inside the gauge builder, and an empty
/// glyph would erase the whole bar rather than one symbol. Absence is handled
/// by `Default`; this is the case `#[serde(default)]` cannot see.
fn glyph<'de, D: Deserializer<'de>>(d: D, fallback: &str) -> Result<String, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(v.as_str().filter(|s| !s.is_empty()).unwrap_or(fallback).to_string())
}

/// Only a literal `false` disables seeding. A missing key or a non-boolean is
/// the shipped `true`, as `as_bool().unwrap_or(true)` made it — and it has to
/// stay a *leaf* fallback: an opt-out that a mistyped neighbouring key could
/// flip is not an opt-out.
fn auto_configure_repo<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    leaf(d, Value::as_bool, DEFAULT_AUTO_CONFIGURE_REPO)
}

/// A non-string pattern is the shipped literal, as `as_str()` made it. An
/// *empty* one is kept here and rejected in [`Config::worktree_matcher`],
/// which is where the fallback can be explained to the user.
fn worktree_pattern<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    leaf(d, string, DEFAULT_WORKTREE_PATTERN.to_string())
}

/// A separator glyph. See the note on [`PowerlineConfig::cap`] for why a wrong
/// type is `""` here while absence is the shipped glyph.
fn separator<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    leaf(d, string, String::new())
}

fn desc_budget_fraction<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    leaf(d, Value::as_f64, DEFAULT_DESC_BUDGET_FRACTION)
}

/// `symbols` and `typeSymbols`: an open map of glyphs, where a non-string
/// entry is **dropped** rather than fatal.
///
/// Dropping reproduces both readers exactly. `Config::symbol` renders `""` for
/// a key it cannot find, which is what `as_str().unwrap_or_default()` gave;
/// `type_glyph` falls through to `_default`, which is what it gave there. A
/// non-object table drops every entry, as a dotted read into a scalar did.
fn glyph_table<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let Value::Object(entries) = Value::deserialize(d)? else {
        return Ok(BTreeMap::new());
    };
    Ok(entries.into_iter().filter_map(|(key, v)| Some((key, v.as_str()?.to_string()))).collect())
}

/// The `as_str` half of the old accessors, owned so it can be a `leaf` reader.
fn string(v: &Value) -> Option<String> {
    Some(v.as_str()?.to_string())
}

/// The palette, forgiving of a non-object as the old accessor was — it asked
/// for `as_object()` and took `None` for an answer.
///
/// An unusable palette costs its colours and nothing else: every spec then
/// fails to resolve and falls back to Gruvbox white, which is a bar you can
/// read. Rejecting the whole config over it would cost the layout too.
fn palette<'de, D: Deserializer<'de>>(d: D) -> Result<Palette, D::Error> {
    let Value::Object(entries) = Value::deserialize(d)? else {
        return Ok(Palette::new());
    };
    Ok(entries.into_iter().collect())
}

/// An empty `projectName` is absent, not a segment reading `{project} `.
fn project_name<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(v.as_str().filter(|s| !s.is_empty()).map(str::to_string))
}

/// The layout, forgiving in the two places the old ladder was.
///
/// A `lines` that is not an array renders **nothing**, and a row inside it
/// that is not an array degrades to an empty row while its siblings render.
/// Neither is an error: a layout is the one config key that can blank the bar
/// on its own, so it is the last place to be strict.
fn lines<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<SegmentEntry>>, D::Error> {
    let Value::Array(rows) = Value::deserialize(d)? else {
        return Ok(Vec::new());
    };
    Ok(rows
        .into_iter()
        .map(|row| match row {
            Value::Array(entries) => entries.into_iter().map(SegmentEntry::from).collect(),
            _ => Vec::new(),
        })
        .collect())
}

fn default_palette() -> Palette {
    [
        ("blue", [69, 133, 136]),
        ("aqua", [104, 157, 106]),
        ("green", [152, 151, 26]),
        ("yellow", [215, 153, 33]),
        ("orange", [214, 93, 14]),
        ("red", [204, 36, 29]),
        ("purple", [177, 98, 134]),
        ("grey", [60, 56, 54]),
        ("bg3", [102, 92, 84]),
        ("white", [251, 241, 199]),
    ]
    .into_iter()
    .map(|(name, [r, g, b])| (name.to_string(), serde_json::json!([r, g, b])))
    .collect()
}

/// The shipped `symbols`.
///
/// **Written as escapes, deliberately.** These are the same glyphs
/// `assets/claude-status.defaults.json` carries, and most of them are Nerd Font
/// private-use characters that render as nothing or a box in an editor and do
/// not survive copy-paste. An escape is the only form that can be reviewed in a
/// diff; never paste the rendered glyph in.
fn default_symbols() -> BTreeMap<String, String> {
    pairs([
        ("model", "\u{26a1}"),
        ("context", "\u{f1c0}"),
        ("win5h", "\u{f252}"),
        ("win7d", "\u{f073}"),
        ("reset", "\u{21bb}"),
        ("session", "\u{f02b}"),
        ("cost", "\u{23f1}\u{fe0f}"),
        ("spend", "\u{f09d}"),
        ("duration", "\u{f017}"),
        ("project", "\u{f401}"),
        ("repo", "\u{f401}"),
        ("worktree", "\u{1f332}"),
        ("folder", "\u{f07b}"),
        ("branch", "\u{e0a0}"),
        ("ahead", "\u{2191}"),
        ("dirtyAdd", "+"),
        ("dirtyDel", "-"),
        ("dirtyMix", "\u{b1}"),
        ("agent", "\u{f007}"),
        ("tokens", "\u{f51e}"),
    ])
}

/// The shipped `typeSymbols`. Escapes, for the reason [`default_symbols`]
/// gives.
fn default_type_symbols() -> BTreeMap<String, String> {
    pairs([
        ("_default", "\u{f544}"),
        ("local_agent", "\u{f109}"),
        ("cloud_agent", "\u{f0c2}"),
        ("remote_agent", "\u{f0c2}"),
        ("background", "\u{f013}"),
        ("task", "\u{f0ae}"),
        ("review", "\u{f06e}"),
        ("test", "\u{f0c3}"),
        ("mcp", "\u{f1e6}"),
    ])
}

fn pairs<const N: usize>(entries: [(&str, &str); N]) -> BTreeMap<String, String> {
    entries.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn default_segments() -> BTreeMap<String, SegmentStyle> {
    [
        ("model", SegmentStyle::shipped(Some("blue"), Some("white"), true)),
        ("context", SegmentStyle::shipped(Some("aqua"), None, false)),
        ("rl5h", SegmentStyle::shipped(Some("blue"), None, false)),
        ("rl7d", SegmentStyle::shipped(Some("purple"), None, false)),
        ("session", SegmentStyle::shipped(Some("orange"), None, false)),
        ("cost", SegmentStyle::shipped(Some("green"), Some("white"), true)),
        ("spend", SegmentStyle::shipped(Some("orange"), Some("white"), true)),
        ("duration", SegmentStyle::shipped(Some("aqua"), None, false)),
        ("project", SegmentStyle::shipped(Some("green"), Some("white"), true)),
        ("worktree", SegmentStyle::shipped(Some("yellow"), Some("grey"), false)),
        ("branch", SegmentStyle::shipped(Some("aqua"), None, false)),
    ]
    .into_iter()
    .map(|(id, style)| (id.to_string(), style))
    .collect()
}

fn default_lines() -> Vec<Vec<SegmentEntry>> {
    [["model", "context", "rl5h", "rl7d", "spend", "cost"].as_slice(), ["project", "worktree", "branch"].as_slice()]
        .into_iter()
        .map(|row| row.iter().map(|id| SegmentEntry::Id((*id).to_string())).collect())
        .collect()
}

/// The shipped status buckets, **in the order they are tried**: the first
/// whose `match` hits wins, so this order is behaviour and not formatting.
///
/// The asset carries them alphabetically, which puts `pending` third rather
/// than last. That is harmless only because `pending`'s `match` is the *empty
/// string*, which the walk records as the designated fallback and carries
/// past — a pattern that matched everything in third place would swallow
/// `running`.
fn default_statuses() -> Map<String, Value> {
    [
        ("done", "done|complete|success|finish|ok", "\u{f00c}", "green"),
        ("error", "error|fail|cancel|abort", "\u{f00d}", "red"),
        ("pending", "", "\u{f017}", "bg3"),
        ("running", "run|active|progress|working|busy", "\u{f04b}", "blue"),
    ]
    .into_iter()
    .map(|(name, pattern, symbol, bg)| {
        (name.to_string(), serde_json::json!({ "match": pattern, "symbol": symbol, "bg": bg }))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::defaults::DEFAULTS_JSON;

    fn cfg(v: Value) -> Config {
        Config::new(v)
    }

    /// Criterion 2. Field by field, so a drift between the asset and the
    /// `Default` impls names the field rather than printing two JSON blobs.
    #[test]
    fn the_embedded_defaults_deserialize_to_the_default_config() {
        let embedded: Config = serde_json::from_str(DEFAULTS_JSON).expect("the embedded defaults deserialize");
        let coded = Config::default();

        assert_eq!(embedded.auto_configure_repo, coded.auto_configure_repo, "autoConfigureRepo");
        assert_eq!(embedded.caps, coded.caps, "caps");
        assert_eq!(embedded.project_name, coded.project_name, "projectName");
        assert_eq!(embedded.palette, coded.palette, "palette");
        assert_eq!(embedded.powerline, coded.powerline, "powerline");
        assert_eq!(embedded.default_fg, coded.default_fg, "defaultFg");
        assert_eq!(embedded.gauge, coded.gauge, "gauge");
        assert_eq!(embedded.worktree_pattern, coded.worktree_pattern, "worktreePattern");
        assert_eq!(embedded.symbols, coded.symbols, "symbols");
        assert_eq!(embedded.spend, coded.spend, "spend");
        assert_eq!(embedded.type_symbols, coded.type_symbols, "typeSymbols");
        assert_eq!(embedded.segments, coded.segments, "segments");
        assert_eq!(embedded.lines, coded.lines, "lines");
        assert_eq!(embedded.subagent, coded.subagent, "subagent");
    }

    /// The order `subagent.statuses` is tried in is behaviour, not formatting,
    /// so the coded default has to carry the asset's order and not just its
    /// entries — a `BTreeMap` would pass the equality above and still change
    /// which bucket wins.
    #[test]
    fn the_default_statuses_keep_the_shipped_order() {
        let embedded: Config = serde_json::from_str(DEFAULTS_JSON).expect("the embedded defaults deserialize");
        let coded = Config::default();
        let order: Vec<&str> = coded.subagent.statuses.keys().map(String::as_str).collect();
        assert_eq!(order, embedded.subagent.statuses.keys().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(coded.subagent.statuses["pending"]["match"], "", "`pending` is the empty-match fallback");
    }

    /// Criterion 3, and the reason [`Config::new`]'s fallback is safe: an
    /// empty merged tree is the defaults, not a blank bar.
    #[test]
    fn an_empty_merged_config_is_the_defaults() {
        assert_eq!(cfg(json!({})), Config::default());
    }

    /// Criterion 4. Also the guard on [`Config::new`]: a tree the types reject
    /// must not take the bar down with it.
    ///
    /// Reaching this takes a **structural** mistake — a scalar where a whole
    /// block belongs. A mistyped *leaf* does not reach it and must not: see
    /// the `costs_only_itself` tests below. A root that is not an object does
    /// not reach it either, because `layers::load` drops such a layer before
    /// the merge; see `a_valid_but_non_object_layer_does_not_wipe_the_defaults`.
    #[test]
    fn a_config_that_will_not_deserialize_falls_back_to_the_defaults() {
        assert_eq!(cfg(json!({ "gauge": 5 })), Config::default(), "a scalar where a block belongs");
        assert_eq!(cfg(json!({ "segments": { "model": "blue" } })), Config::default(), "and where a style belongs");
    }

    #[test]
    fn a_palette_that_is_not_an_object_costs_its_colours_and_nothing_else() {
        // The old accessor asked for `as_object()` and took `None`; a config
        // that keeps its layout and loses its colours still renders a bar.
        let c = cfg(json!({ "palette": "gruvbox", "projectName": "kept" }));
        assert!(c.palette.is_empty());
        assert_eq!(c.color(Some(&json!("blue"))), color::FALLBACK);
        assert_eq!(c.project_name.as_deref(), Some("kept"), "the rest of the config survived");
    }

    #[test]
    fn an_unknown_key_is_ignored_rather_than_fatal() {
        // `$schema` ships in the asset itself, so this is not hypothetical.
        let c = cfg(json!({ "$schema": "https://example.invalid/s.json", "nosuchkey": 1 }));
        assert_eq!(c, Config::default());
    }

    /// The one field whose default a plain `#[derive(Default)]` would get
    /// **backwards** — `bool::default()` is `false`, which turns the shipped
    /// opt-out into an opt-in. It has a single call site and no other test, so
    /// nothing else in the suite would notice.
    #[test]
    fn seeding_the_repo_layer_is_opt_out_and_only_a_literal_false_disables_it() {
        assert!(cfg(json!({})).auto_configure_repo, "the shipped default is on");
        assert!(!cfg(json!({ "autoConfigureRepo": false })).auto_configure_repo);
        assert!(cfg(json!({ "autoConfigureRepo": true })).auto_configure_repo);
    }

    // A mistyped scalar costs its own key and nothing else. Each of these
    // carries a **sibling** key, because the fallback value alone cannot tell
    // "this leaf degraded" from "the whole tree was discarded and `Default`
    // supplied the same answer" — and those are very different outcomes for
    // the user, who loses one glyph in the first and their whole theme in the
    // second. `projectName` is the sibling throughout: it defaults to `None`,
    // so its presence proves the layer applied.

    #[test]
    fn a_non_boolean_auto_configure_repo_costs_only_itself() {
        let c = cfg(json!({ "autoConfigureRepo": "no", "projectName": "kept" }));
        assert!(c.auto_configure_repo, "as `as_bool().unwrap_or(true)` made it");
        assert_eq!(c.project_name.as_deref(), Some("kept"), "the layer around it survived");
    }

    #[test]
    fn a_non_string_symbol_costs_only_that_glyph() {
        let c = cfg(json!({ "symbols": { "model": 5, "branch": "B" }, "projectName": "kept" }));
        assert_eq!(c.symbol("model"), "", "as `as_str().unwrap_or_default()` made it");
        assert_eq!(c.symbol("branch"), "B", "its neighbour in the same table survived");
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_string_type_symbol_falls_through_to_the_default_glyph() {
        // Not `""`: `type_glyph` reaches for `_default` when the named entry
        // is unusable, which is what dropping the key reproduces.
        let c = cfg(json!({ "typeSymbols": { "task": 5, "_default": "D" }, "projectName": "kept" }));
        assert_eq!(crate::render::subagent::type_glyph("task", &c), "D");
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_string_worktree_pattern_costs_only_the_pattern() {
        let c = cfg(json!({ "worktreePattern": 5, "projectName": "kept" }));
        assert_eq!(c.worktree_pattern, DEFAULT_WORKTREE_PATTERN);
        assert!(c.worktree_matcher().is_match("/x/worktrees/y"));
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_string_powerline_glyph_costs_only_that_glyph() {
        // `""` rather than the shipped glyph: a seamless bar is what
        // `powerline()` produced, and absence is the case that keeps the glyph.
        let c = cfg(json!({ "powerline": { "sep": 5 }, "projectName": "kept" }));
        assert_eq!(c.powerline.sep, "");
        assert_eq!(c.powerline.cap, "\u{e0b6}", "an absent sibling is still the shipped glyph");
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_numeric_refresh_interval_costs_only_itself() {
        let c = cfg(json!({ "spend": { "refreshMinutes": "15" }, "projectName": "kept" }));
        assert_eq!(c.spend.refresh_minutes, 15.0);
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_string_spend_show_costs_only_itself() {
        let c = cfg(json!({ "spend": { "show": 1, "refreshMinutes": 0 }, "projectName": "kept" }));
        assert_eq!(c.spend.show, "auto");
        assert_eq!(c.spend.refresh_minutes, 0.0, "and a real zero beside it is still a real zero");
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_non_numeric_desc_budget_fraction_costs_only_itself() {
        let c = cfg(json!({ "subagent": { "descBudgetFraction": "0.9" }, "projectName": "kept" }));
        assert_eq!(c.subagent.desc_budget_fraction, DEFAULT_DESC_BUDGET_FRACTION);
        assert_eq!(c.project_name.as_deref(), Some("kept"));
    }

    #[test]
    fn a_missing_symbol_renders_empty_not_undefined() {
        let c = cfg(json!({ "symbols": { "branch": "B" } }));
        assert_eq!(c.symbol("branch"), "B");
        assert_eq!(c.symbol("absent"), "", "deviation: the JS rendered the text `undefined`");
    }

    #[test]
    fn a_gauge_width_of_zero_means_ten() {
        assert_eq!(cfg(json!({ "gauge": { "width": 0 } })).gauge.width, 10);
        assert_eq!(cfg(json!({})).gauge.width, 10);
        assert_eq!(cfg(json!({ "gauge": { "width": 4 } })).gauge.width, 4);
    }

    #[test]
    fn an_empty_project_name_is_absent() {
        assert_eq!(cfg(json!({ "projectName": "" })).project_name, None);
        assert_eq!(cfg(json!({})).project_name, None);
        assert_eq!(cfg(json!({ "projectName": "x" })).project_name.as_deref(), Some("x"));
    }

    #[test]
    fn an_enormous_gauge_width_is_capped() {
        // Uncapped this reaches `str::repeat` and aborts on allocation
        // failure, which `catch_unwind` cannot catch — a blank bar.
        assert_eq!(cfg(json!({ "gauge": { "width": 1_000_000_000_000u64 } })).gauge.width, MAX_GAUGE_WIDTH);
    }

    #[test]
    fn an_empty_gauge_glyph_falls_back_to_the_shipped_one() {
        let c = cfg(json!({ "gauge": { "filled": "", "empty": "" } }));
        assert_eq!(c.gauge.filled, "\u{25b0}");
        assert_eq!(c.gauge.empty, "\u{25b1}");
        assert_eq!(cfg(json!({})).gauge.filled, "\u{25b0}");
        assert_eq!(cfg(json!({ "gauge": { "filled": "#" } })).gauge.filled, "#");
    }

    #[test]
    fn a_missing_gauge_glyph_falls_back_by_a_different_path_than_an_empty_one() {
        // Absence is `#[serde(default)]`; an empty string is the
        // `deserialize_with`. Criterion 6 asks for both, separately.
        let missing = cfg(json!({ "gauge": { "width": 4 } }));
        assert_eq!((missing.gauge.filled.as_str(), missing.gauge.empty.as_str()), ("\u{25b0}", "\u{25b1}"));
    }

    #[test]
    fn an_empty_worktree_pattern_falls_back_to_the_default() {
        // An empty pattern matches every component, so the last match would
        // always be the final one and the worktree prefix would vanish.
        let m = cfg(json!({ "worktreePattern": "" })).worktree_matcher();
        assert!(m.is_match("worktrees"));
        assert!(!m.is_match("src"));
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
        assert_eq!(c.lines, vec![vec![SegmentEntry::Id("model".into())], vec![]]);
    }

    #[test]
    fn a_layout_that_is_not_an_array_renders_nothing_rather_than_the_defaults() {
        assert_eq!(cfg(json!({ "lines": 7 })).lines, Vec::<Vec<SegmentEntry>>::new());
    }

    #[test]
    fn an_entry_names_a_segment_by_string_by_name_or_by_id() {
        let c = cfg(json!({ "lines": [["model", { "name": "cost" }, { "id": "spend" }, 7]] }));
        let ids: Vec<Option<&str>> = c.lines[0].iter().map(SegmentEntry::id).collect();
        assert_eq!(ids, [Some("model"), Some("cost"), Some("spend"), None]);
        assert!(c.lines[0][0].overrides().is_none(), "a bare id carries no overrides");
        assert!(c.lines[0][1].overrides().is_some());
    }

    #[test]
    fn an_explicit_null_style_reads_as_unset_while_false_does_not() {
        // What reproduces `??`: `null` falls through to the next rung, `false`
        // and `0` are answers.
        let c = cfg(json!({ "segments": { "model": { "bg": null, "bold": false } } }));
        let model = &c.segments["model"];
        assert_eq!(model.bg, None, "an explicit null is indistinguishable from absent");
        assert_eq!(model.bold, Some(json!(false)), "but false is a value");
    }
}
