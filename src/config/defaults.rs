//! The shipped defaults, embedded in the binary.
//!
//! This is the lowest of the three config layers, so a machine with no
//! `~/.config/claude-status.json` still renders a full bar. (The JS
//! implementation had only two file layers and rendered blank without them —
//! see the plan's deviation table.)
//!
//! The asset is byte-identical to `ai-plugins/tools/statusline/statusline.json`
//! and is **irreplaceable**: 28 of its symbols are Nerd Font private-use
//! codepoints that render as nothing or a box in most editors and are silently
//! dropped by copy-paste. It is `-text -diff` in `.gitattributes` and excluded
//! from dprint. Never edit it through an editor buffer; verify it by rendering,
//! or through `tests/defaults_integrity.rs`, never by reading a diff.

/// The shipped defaults as raw JSON text.
pub const DEFAULTS_JSON: &str = include_str!("../../assets/claude-status.defaults.json");
