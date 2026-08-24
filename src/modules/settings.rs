//! Claude Code's `~/.claude/settings.json` — the three keys this tool owns.
//!
//! **Merge, never rewrite.** The file holds the user's entire Claude Code
//! configuration and this tool owns three keys of it. Everything here reads a
//! parsed settings object and returns a new one; deciding what to do with the
//! result — printing it, writing it, refusing — belongs to the `--configure`
//! runtime (`crate::_runtime::configure`, private), so the merge itself can be
//! tested without a `$HOME`.
//!
//! # Why the command is the bare name
//!
//! The npm installer wrote an absolute `<HOME>/.claude/bin/claude-status`,
//! because it had placed the binary itself and knew where. This writes
//! `claude-status` and lets `PATH` resolve it, and that is a **tradeoff taken
//! deliberately, not an oversight**.
//!
//! Under Homebrew the obvious alternative — `std::env::current_exe()` — is
//! actively wrong: it resolves the `bin/claude-status` symlink through to a
//! **versioned Cellar path**, and `brew upgrade` deletes that directory. The
//! wiring would point at a binary that no longer exists, silently, until the
//! next upgrade. The bare name survives every upgrade.
//!
//! **The accepted risk** is that it depends on Claude Code's own `PATH`
//! containing Homebrew's `bin`. A GUI-launched application often inherits a
//! minimal `PATH` that does not, in which case the bar renders nothing and the
//! user sees an empty status line rather than an error. That is the price, and
//! it is the recoverable failure of the two: a `PATH` can be fixed from
//! outside, a path into a deleted Cellar directory cannot be noticed at all.
//!
//! # What the shapes are
//!
//! Both render keys are **objects**, not scalars, and `padding` and
//! `refreshInterval` belong to `statusLine` alone — `refreshInterval: 4` is
//! what makes the bar redraw every four seconds, so dropping it is a visible
//! behaviour change. `PostToolUse` is not a top-level key at all: it is
//! `hooks.PostToolUse`, an array of **groups**, each holding its own `hooks`
//! array. A bare string at either render key is a file Claude Code silently
//! ignores.

use serde_json::{Map, Value, json};

pub const STATUS_LINE: &str = "statusLine";
pub const SUBAGENT_STATUS_LINE: &str = "subagentStatusLine";
pub const HOOKS: &str = "hooks";
pub const POST_TOOL_USE: &str = "PostToolUse";

/// The `hooks.PostToolUse` key as one dotted string, for the report.
pub const HOOK_KEY: &str = "hooks.PostToolUse";

/// How the wired commands name this binary. See the module note.
pub const COMMAND: &str = "claude-status";

/// The `ai-plugins` node actuator — this tool's caps hook in its **previous
/// form**, matched at the path it was actually installed to.
///
/// It matters far more than it looks. A machine carrying it that is only
/// matched on `--caps-hook` ends up with *both* wired, and the actuator fires
/// twice per tool call. `--debug` already knows both forms
/// (`app.rs::caps_hook_command`); so must the writer.
///
/// **The directory is part of the match, not decoration.** This is the one
/// ownership test whose consequence is *deletion*, and a bare `context-caps.js`
/// also matches `node /work/vendor/context-caps.js --lint` — some other
/// project's hook, which we would then silently remove. `ai-plugins` only ever
/// wrote it under `~/.claude/hooks/`.
pub const LEGACY_HOOK_PATH: &str = ".claude/hooks/context-caps.js";

/// Who wrote the value currently at a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// Nothing is there — `undefined` or `null`.
    Absent,
    /// This tool's current form.
    Ours,
    /// This tool's **previous** form: a flagless command from an install before
    /// the render flags existed, or the node caps hook. Ours, so it is
    /// rewritten — and rewritten *quietly*. Shouting about it would be shouting
    /// at a user about their own last install.
    OursStale,
    /// Somebody else's. The one case worth interrupting a user over.
    Foreign,
}

/// Why `--configure` declined to touch the file.
///
/// **Refusing rather than discarding is the whole point of this type.** The
/// TypeScript this replaces parsed `settings.json` inside a bare
/// `catch { return null }`, fell back to `{}`, and then wrote — replacing the
/// user's entire Claude Code configuration with three keys. That is silent data
/// loss on a file this tool does not own, and it is the single most dangerous
/// behaviour in the code being ported. The same reasoning covers the two
/// sub-shapes below, which the TypeScript silently dropped on the floor: a
/// `hooks` value we cannot read is a `hooks` value we must not overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The file parsed, but its root is not a JSON object.
    NotAnObject,
    /// `hooks` is present and is not an object.
    HooksNotAnObject,
    /// `hooks.PostToolUse` is present and is not an array.
    PostToolUseNotAnArray,
}

impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotAnObject => "its root is not a JSON object",
            Self::HooksNotAnObject => "its `hooks` key is not a JSON object",
            Self::PostToolUseNotAnArray => "its `hooks.PostToolUse` key is not a JSON array",
        }
    }
}

/// What happened at one of the three keys.
pub struct Change {
    pub key: &'static str,
    pub ownership: Ownership,
    /// The value being replaced, compactly, and **only** when it was
    /// [`Ownership::Foreign`]. Untrusted: it came out of the user's file, so
    /// every consumer sanitizes it before printing.
    pub replaced: Option<String>,
    /// Whether this key's value actually differs from what was there.
    pub changed: bool,
}

pub struct Wiring {
    /// The settings object to write.
    pub settings: Value,
    pub changes: Vec<Change>,
}

impl Wiring {
    /// Whether writing would change anything at all.
    ///
    /// When this is false the file is left **completely alone** — not rewritten
    /// with identical values. That is what makes idempotence (the plan's
    /// criterion 3) structural rather than incidental, and it is also why a
    /// second `--configure` cannot reformat a file whose indentation is not
    /// ours to normalise.
    pub fn changed(&self) -> bool {
        self.changes.iter().any(|c| c.changed)
    }
}

/// `statusLine`'s value. `padding` and `refreshInterval` live here and nowhere
/// else; `refreshInterval` is the bar's redraw cadence.
pub fn desired_status_line() -> Value {
    json!({ "type": "command", "command": format!("{COMMAND} --statusline"), "padding": 0, "refreshInterval": 4 })
}

/// `subagentStatusLine`'s value. The panel is drawn on demand, so it carries
/// neither of `statusLine`'s two extra keys.
pub fn desired_subagent_status_line() -> Value {
    json!({ "type": "command", "command": format!("{COMMAND} --subagent") })
}

/// One `PostToolUse` hook entry.
pub fn desired_hook() -> Value {
    json!({ "type": "command", "command": format!("{COMMAND} --caps-hook") })
}

/// The group appended when nothing of ours is in `PostToolUse` yet.
///
/// **No `matcher` key at all**, which is how a group fires for every tool. An
/// empty-string or `"*"` matcher would look equivalent and is not guaranteed to
/// be.
fn desired_group() -> Value {
    let mut group = Map::new();
    group.insert(HOOKS.to_string(), Value::Array(vec![desired_hook()]));
    Value::Object(group)
}

/// The **program** a command line runs — its first shell word, reduced to a
/// basename.
///
/// Ownership turns on this and not on a substring, and the difference is the
/// difference between replacing our own wiring and destroying somebody else's.
/// `"claude-status".contains()` is true of `claude-statusline`,
/// `claude-status-pro` and `/opt/claude-statusbar`, none of which are ours —
/// and because they are not ours *and* look ours, they were overwritten
/// **without the warning** that the module doc calls the entire mitigation for
/// having no undo. That name family is not hypothetical.
///
/// Quoting is handled because a path with a space in it is ordinary on macOS
/// (`"/Users/a b/bin/claude-status" --statusline`). Anything more than one
/// level of quoting is a shell construct this has no business parsing, and
/// falls through to "not ours", which is the safe direction: the cost of a
/// false negative is that we append beside a command instead of replacing it,
/// and the cost of a false positive is destroying it.
fn program_of(command: &str) -> &str {
    let command = command.trim_start();
    let (word, _) = match command.as_bytes().first() {
        Some(&q @ (b'"' | b'\'')) => command[1..].split_once(q as char).unwrap_or((&command[1..], "")),
        _ => command.split_once(char::is_whitespace).unwrap_or((command, "")),
    };
    word.rsplit('/').next().unwrap_or(word)
}

/// Whether a command line runs **this** binary.
fn is_our_program(command: &str) -> bool {
    program_of(command) == COMMAND
}

/// Who owns a render key's value, decided **only** from the value itself.
///
/// There is no receipt to consult — the npm installer's is deliberately gone —
/// so this is the whole of the evidence. Note that a bare *string* at the key
/// is `Foreign`: `value.command` is not a string, and a string is not a shape
/// this tool ever wrote.
pub fn ownership_of(value: Option<&Value>) -> Ownership {
    match value {
        None | Some(Value::Null) => Ownership::Absent,
        Some(value) => match value.get("command").and_then(Value::as_str) {
            None => Ownership::Foreign,
            Some(command) if !is_our_program(command) => Ownership::Foreign,
            // Ours, but from before the render flags existed: it would render
            // the missing-flag line instead of a bar.
            Some(command) if !command.contains("--statusline") && !command.contains("--subagent") => {
                Ownership::OursStale
            }
            Some(_) => Ownership::Ours,
        },
    }
}

/// Who owns one `PostToolUse` entry's `command`.
///
/// The legacy arm is the delicate one. `ai-plugins` wired
/// `node ${HOME}/.claude/hooks/context-caps.js`, so the *program* is `node` and
/// ownership has to be read from the argument instead — but a bare
/// `context-caps.js` match also claims `node /work/vendor/context-caps.js
/// --lint`, an unrelated project's hook, and this is the one path that
/// **deletes** what it matches. So the legacy form is recognised only at the
/// full path we actually wrote it to, under `.claude/hooks/`.
pub fn hook_ownership_of(command: Option<&Value>) -> Ownership {
    let Some(command) = command.and_then(Value::as_str) else {
        return Ownership::Foreign;
    };
    if is_our_program(command) && command.contains("--caps-hook") {
        Ownership::Ours
    } else if command.contains(LEGACY_HOOK_PATH) {
        Ownership::OursStale
    } else {
        Ownership::Foreign
    }
}

/// Puts the three keys into a parsed `settings.json`, preserving everything
/// else.
///
/// Key **order** is preserved by `serde_json`'s `preserve_order` feature:
/// `Map::insert` on a key that is already there leaves it where it was, so a
/// re-run cannot reshuffle a user's file. That is what makes the plan's
/// criteria 1 and 3 reachable at all.
///
/// **That feature is a dependency of this design, not an incidental manifest
/// property.** Without it, this read-modify-write would **alphabetise the
/// user's entire Claude Code configuration** — silently, on a file this tool
/// owns three keys of. `Cargo.toml` says so at the declaration, and
/// `unrelated_keys_survive_with_their_values_and_their_order` goes red if it is
/// ever "tidied" away.
///
/// Deliberately **not** [`crate::json::deep_merge`]: that strips `__proto__`,
/// `constructor` and `prototype` at every depth, which is correct for a config
/// this tool owns and is silent key deletion in a file it does not.
///
/// # Why concurrent `--configure` runs cannot corrupt anything
///
/// **This function is a pure, deterministic function of the bytes it was
/// given** — no clock, no environment, no randomness. That is the structural
/// reason two racing runs are safe, and it is stronger than any number of
/// passing races: both read the same file, both compute the same output, so
/// whichever atomic rename lands last replaces the file with bytes identical to
/// what the other one wrote. A run that starts *after* another finished reads
/// an already-wired file and takes the "nothing to change" path, writing
/// nothing at all.
///
/// The residual is a lost update against a **third-party** writer — Claude Code
/// itself, or the user's editor — landing inside the read→rename window. The
/// file it leaves is always valid; only an unrelated edit can be missed.
/// Measured across ~1,600 races: no corrupt file, no torn read, no leftover
/// temp.
pub fn wire(settings: &Value) -> Result<Wiring, Refusal> {
    let Some(root) = settings.as_object() else {
        return Err(Refusal::NotAnObject);
    };

    let mut out = root.clone();
    let mut changes = Vec::new();

    for (key, desired) in
        [(STATUS_LINE, desired_status_line()), (SUBAGENT_STATUS_LINE, desired_subagent_status_line())]
    {
        let was = root.get(key);
        let ownership = ownership_of(was);
        let next = merged_over(was, ownership, desired);
        changes.push(Change {
            key,
            ownership,
            replaced: (ownership == Ownership::Foreign).then(|| compact(was)),
            changed: was != Some(&next),
        });
        out.insert(key.to_string(), next);
    }

    let (hooks, hook_change) = wire_hook(root)?;
    out.insert(HOOKS.to_string(), Value::Object(hooks));
    changes.push(hook_change);

    Ok(Wiring { settings: Value::Object(out), changes })
}

/// Merges our command into `hooks.PostToolUse`, leaving every other group,
/// matcher and entry exactly as it was.
///
/// **Every entry of ours is replaced, not only the first — and the TypeScript's
/// first-only behaviour is a bug — but so is replacing every copy in place.**
///
/// `settings.ts:152` stopped after one, leaving a second entry of ours behind.
/// `settings.ts:130-135`'s own doc says why that is wrong: replacing in place
/// matters because "appending while an old form is still present would fire the
/// same actuator twice per tool call". First-only prevents that in the common
/// ordering and **fails in the exact case the rule exists for** — a stale
/// `context-caps.js` sitting *after* a current entry, where the first match is
/// consumed by the current one and the stale form survives untouched.
///
/// **Making every copy identical does not fix it either**, which is the trap
/// this landed in first. `PostToolUse` is a list Claude Code *iterates*: two
/// identical entries are two invocations per tool call, and `--caps-hook`
/// output is injected verbatim into the agent's context. "Deduplicated by being
/// made identical" is not deduplication.
///
/// So: **the first entry of ours is updated in place and every later one is
/// removed.** In place, so the group's `matcher` and any sibling keys survive —
/// where a user has scoped our hook, that scoping is theirs. When removing our
/// copies empties a group, the group goes too: a group holding nothing but our
/// hook is one we appended, and leaving `{"hooks": []}` behind is litter in a
/// file we are trying to leave clean.
///
/// `exactly_one_entry_of_ours_survives` is the case, and it counts.
fn wire_hook(root: &Map<String, Value>) -> Result<(Map<String, Value>, Change), Refusal> {
    let mut hooks = match root.get(HOOKS) {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(existing)) => existing.clone(),
        Some(_) => return Err(Refusal::HooksNotAnObject),
    };
    let before = hooks.get(POST_TOOL_USE).cloned();
    let groups = match &before {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(groups)) => groups.clone(),
        Some(_) => return Err(Refusal::PostToolUseNotAnArray),
    };

    let mut found = Ownership::Absent;
    let mut kept_one = false;
    // Rebuilt rather than edited in place: a group can disappear entirely, and
    // deciding that needs the entry pass to have finished. `retain` on the
    // outer list cannot see it, and a sentinel key inserted into the user's own
    // group to carry the answer across could collide with a key they set.
    let mut rebuilt = Vec::with_capacity(groups.len());
    for mut group in groups {
        // A group whose `hooks` is not an array — or which is not an object at
        // all — is skipped and kept **verbatim**. It is not ours to repair.
        let Some(entries) = group.get_mut(HOOKS).and_then(Value::as_array_mut) else {
            rebuilt.push(group);
            continue;
        };
        let was_populated = !entries.is_empty();
        entries.retain_mut(|entry| {
            let ownership = hook_ownership_of(entry.get("command"));
            match ownership {
                // Somebody else's hook, preserved alongside ours, untouched.
                Ownership::Foreign | Ownership::Absent => return true,
                // The stale form is the more informative answer of the two, so
                // it wins the report even when a current entry was seen first.
                Ownership::OursStale => found = Ownership::OursStale,
                Ownership::Ours if found == Ownership::Absent => found = Ownership::Ours,
                Ownership::Ours => {}
            }
            if kept_one {
                return false; // A duplicate of ours: drop it, do not clone it.
            }
            kept_one = true;
            *entry = merged_over(Some(entry), ownership, desired_hook());
            true
        });

        // Emptied by our own removals, so it held nothing but our duplicates —
        // a group we appended on some earlier run. A group that arrived empty
        // is left alone; it is not ours and not ours to tidy.
        if was_populated && entries.is_empty() {
            continue;
        }
        rebuilt.push(group);
    }
    let mut groups = rebuilt;

    if !kept_one {
        groups.push(desired_group());
    }

    let after = Value::Array(groups);
    let changed = before.as_ref() != Some(&after);
    hooks.insert(POST_TOOL_USE.to_string(), after);

    Ok((hooks, Change { key: HOOK_KEY, ownership: found, replaced: None, changed }))
}

/// `desired` applied **over** what was already there, rather than in place of
/// it.
///
/// "Merge, never rewrite" has to hold *inside* the keys this tool owns too, not
/// only around them. Replacing the whole value drops any sibling the user set:
/// `timeout` is a real Claude Code key on a hook and on a status line, and an
/// unknown sibling is by definition something a newer Claude Code understands
/// and this binary does not. Neither is ours to delete — and because it happens
/// on **every** run, a single `--configure` after an upgrade would silently
/// strip a setting the user could not see us take.
///
/// Only for values that are **ours**. A `Foreign` value is another tool's
/// object and its keys mean nothing in ours, so that one is replaced whole; an
/// `Absent` one has nothing to merge into.
fn merged_over(was: Option<&Value>, ownership: Ownership, desired: Value) -> Value {
    let (Some(Value::Object(existing)), Ownership::Ours | Ownership::OursStale) = (was, ownership) else {
        return desired;
    };
    let Value::Object(desired) = desired else {
        return desired;
    };
    let mut merged = existing.clone();
    // Our keys win — `--help` documents their values, and `refreshInterval` in
    // particular is the bar's redraw cadence rather than a user preference.
    // Everything else the user has there stays exactly where it was.
    for (key, value) in desired {
        merged.insert(key, value);
    }
    Value::Object(merged)
}

/// A value as one line, for a diagnostic. The caller sanitizes and truncates —
/// this is still the user's file talking.
fn compact(value: Option<&Value>) -> String {
    value.map(ToString::to_string).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wired(settings: Value) -> Value {
        wire(&settings).expect("this fixture is wirable").settings
    }

    fn change<'a>(wiring: &'a Wiring, key: &str) -> &'a Change {
        wiring.changes.iter().find(|c| c.key == key).expect("every key is reported")
    }

    #[test]
    fn the_two_render_keys_are_objects_and_carry_the_bars_refresh_cadence() {
        // A bare string here is a file Claude Code silently ignores, and
        // dropping `refreshInterval` stops the bar redrawing every four
        // seconds. Both are the kind of wrong that renders as "nothing
        // happened" rather than as an error.
        let out = wired(json!({}));
        assert_eq!(
            out[STATUS_LINE],
            json!({ "type": "command", "command": "claude-status --statusline", "padding": 0, "refreshInterval": 4 }),
        );
        assert_eq!(out[SUBAGENT_STATUS_LINE], json!({ "type": "command", "command": "claude-status --subagent" }));
        assert!(out[SUBAGENT_STATUS_LINE].get("refreshInterval").is_none(), "the panel has no cadence to set");
    }

    #[test]
    fn the_command_is_the_bare_name_so_a_brew_upgrade_cannot_orphan_it() {
        // `current_exe()` resolves Homebrew's symlink to a versioned Cellar
        // path that `brew upgrade` deletes. See the module note.
        for key in [STATUS_LINE, SUBAGENT_STATUS_LINE] {
            let command = wired(json!({}))[key]["command"].as_str().unwrap().to_string();
            assert!(command.starts_with("claude-status "), "{key} is not invoked by bare name: {command}");
            assert!(!command.contains('/'), "{key} carries a path: {command}");
        }
    }

    #[test]
    fn ownership_reads_only_the_value_at_the_key() {
        assert_eq!(ownership_of(None), Ownership::Absent);
        assert_eq!(ownership_of(Some(&Value::Null)), Ownership::Absent);

        // A bare string is `foreign`: `.command` is not a string, and a string
        // is not a shape this tool has ever written.
        assert_eq!(ownership_of(Some(&json!("claude-status --statusline"))), Ownership::Foreign);
        assert_eq!(ownership_of(Some(&json!({ "command": 7 }))), Ownership::Foreign);
        assert_eq!(ownership_of(Some(&json!({ "command": "starship prompt" }))), Ownership::Foreign);

        assert_eq!(ownership_of(Some(&json!({ "command": "claude-status --statusline" }))), Ownership::Ours);
        assert_eq!(ownership_of(Some(&json!({ "command": "/o/claude-status --subagent" }))), Ownership::Ours);
        // A pre-flags install: ours, and unusable as it stands.
        assert_eq!(ownership_of(Some(&json!({ "command": "/o/.claude/bin/claude-status" }))), Ownership::OursStale);
    }

    #[test]
    fn the_legacy_node_caps_hook_is_recognised_as_ours() {
        // If it were not, a machine carrying it would end up with **both**
        // wired and the actuator would fire twice per tool call.
        assert_eq!(hook_ownership_of(Some(&json!("node /h/.claude/hooks/context-caps.js"))), Ownership::OursStale);
        assert_eq!(hook_ownership_of(Some(&json!("claude-status --caps-hook"))), Ownership::Ours);
        assert_eq!(hook_ownership_of(Some(&json!("/usr/bin/some-other-linter --fix"))), Ownership::Foreign);
        assert_eq!(hook_ownership_of(None), Ownership::Foreign);
        assert_eq!(hook_ownership_of(Some(&json!(7))), Ownership::Foreign);
        // The order in the TypeScript's `/claude-status.*--caps-hook/` is
        // load-bearing: the flag has to come *after* the name.
        assert_eq!(hook_ownership_of(Some(&json!("--caps-hook claude-status"))), Ownership::Foreign);
    }

    #[test]
    fn unrelated_keys_survive_with_their_values_and_their_order() {
        let before = json!({
            "model": "opus",
            "env": { "FOO": "bar" },
            "permissions": { "allow": ["Bash(ls:*)"] },
        });
        let after = wired(before.clone());

        for key in ["model", "env", "permissions"] {
            assert_eq!(after[key], before[key], "{key} was altered");
        }
        let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
        assert_eq!(&keys[..3], &["model", "env", "permissions"], "the user's key order moved: {keys:?}");
    }

    #[test]
    fn another_tools_post_tool_use_group_keeps_its_matcher_and_its_hooks() {
        let after = wired(json!({
            "hooks": { "PostToolUse": [
                { "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "/usr/bin/fmt" }] },
            ] },
        }));

        let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(groups[0]["matcher"], "Edit|Write", "their matcher was rewritten");
        assert_eq!(groups[0]["hooks"], json!([{ "type": "command", "command": "/usr/bin/fmt" }]));
        assert_eq!(groups.len(), 2, "ours was not appended as its own group: {groups:?}");
        assert_eq!(groups[1], json!({ "hooks": [{ "type": "command", "command": "claude-status --caps-hook" }] }));
        assert!(groups[1].get("matcher").is_none(), "the appended group must fire for every tool");
    }

    #[test]
    fn a_stale_node_hook_is_replaced_in_place_rather_than_joined() {
        let after = wired(json!({
            "hooks": { "PostToolUse": [
                { "matcher": "*", "hooks": [{ "type": "command", "command": "node /h/.claude/hooks/context-caps.js" }] },
            ] },
        }));

        let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "the actuator would now fire twice per tool call: {groups:?}");
        assert_eq!(groups[0]["hooks"][0]["command"], "claude-status --caps-hook");
        assert_eq!(groups[0]["matcher"], "*", "their matcher was rewritten");
    }

    /// **`PostToolUse` is a list Claude Code iterates**, so the only correct
    /// count is one.
    ///
    /// The TypeScript replaced only the *first* entry of ours
    /// (`settings.ts:152`) and left the rest. Replacing them all with the same
    /// value is not a fix: two identical entries are two invocations per tool
    /// call, and `--caps-hook` output is injected verbatim into the agent's
    /// context. The extras have to be **removed**.
    #[test]
    fn exactly_one_entry_of_ours_survives() {
        let wiring = wire(&json!({
            "hooks": { "PostToolUse": [
                { "matcher": "Edit", "hooks": [
                    { "type": "command", "command": "claude-status --caps-hook" },
                    { "type": "command", "command": "/usr/bin/fmt" },
                    { "type": "command", "command": "node /h/.claude/hooks/context-caps.js" },
                ] },
                { "hooks": [{ "type": "command", "command": "/opt/homebrew/bin/claude-status --caps-hook" }] },
            ] },
        }))
        .unwrap();

        let groups = wiring.settings["hooks"]["PostToolUse"].as_array().unwrap();
        let ours: Vec<&Value> = groups
            .iter()
            .filter_map(|g| g["hooks"].as_array())
            .flatten()
            .filter(|e| hook_ownership_of(e.get("command")) != Ownership::Foreign)
            .collect();
        assert_eq!(ours.len(), 1, "the actuator would fire {} times per tool call: {groups:?}", ours.len());
        assert_eq!(ours[0]["command"], "claude-status --caps-hook");

        // In place: the surviving entry keeps its group, so the user's own
        // scoping survives — and the foreign hook beside it is untouched.
        assert_eq!(groups[0]["matcher"], "Edit");
        assert_eq!(groups[0]["hooks"][1]["command"], "/usr/bin/fmt");
        // The second group held nothing but a duplicate of ours, so it goes
        // rather than being left behind as an empty `{"hooks": []}`.
        assert_eq!(groups.len(), 1, "an emptied group was left as litter: {groups:?}");
        assert_eq!(change(&wiring, HOOK_KEY).ownership, Ownership::OursStale, "the stale form is the reported answer");
    }

    /// **F1.** Substring matching is not an ownership test, and this is the one
    /// path that *deletes* what it matches.
    #[test]
    fn a_similarly_named_tool_is_not_mistaken_for_this_one() {
        // Not ours — the program is a different binary in the same name family.
        for command in [
            "claude-statusline",
            "claude-statusline --statusline",
            "/opt/claude-statusbar --statusline",
            "claude-status-pro --statusline --theme dark",
            "starship prompt",
        ] {
            let value = json!({ "type": "command", "command": command });
            assert_eq!(ownership_of(Some(&value)), Ownership::Foreign, "{command} was claimed as ours");
        }

        // Ours, however it is spelled — including a quoted path with a space.
        for command in [
            "claude-status --statusline",
            "/opt/homebrew/bin/claude-status --subagent",
            "\"/Users/a b/bin/claude-status\" --statusline",
        ] {
            let value = json!({ "type": "command", "command": command });
            assert_eq!(ownership_of(Some(&value)), Ownership::Ours, "{command} was not recognised as ours");
        }
    }

    /// The same test on the hook side, where the consequence is deletion rather
    /// than replacement — an unrelated project's `context-caps.js` must survive.
    /// **What the program-token rule costs, stated rather than discovered.**
    ///
    /// A hook wired as `/Users/me/bin/statusline --caps-hook` — a *renamed*
    /// copy of this binary, or a hand-written line — was ours under the old
    /// unordered substring rule and is `Foreign` now. So `--debug` reports
    /// `<not set>`, `--configure` appends a second group beside it, and the
    /// actuator fires twice per tool call, silently.
    ///
    /// **Accepted deliberately, and less reachable than it was.** No shipped
    /// install can produce it: the npm installer always wrote a path
    /// containing `claude-status`, and since `distribution/01` deleted it the
    /// only wiring path left is `--configure`, which writes the bare name.
    /// Reaching this state now takes a hand edit. What the
    /// narrowing buys is that the report and the writer agree on the same
    /// definition — the divergence that had `--debug` showing a hook as wired
    /// while `--configure` was about to duplicate it — and that is worth more
    /// than a case reachable only by renaming the binary.
    ///
    /// Note this is the same double-fire *family* as the duplicate-entry bug,
    /// and removing extras does not cover it: here our entry never matches at
    /// all, so there is nothing to deduplicate.
    #[test]
    fn a_renamed_copy_of_this_binary_is_not_recognised_as_ours() {
        let renamed = json!("/Users/me/bin/statusline --caps-hook");
        assert_eq!(hook_ownership_of(Some(&renamed)), Ownership::Foreign);

        // And the consequence is what it is: ours is appended alongside.
        let after = wired(json!({ "hooks": { "PostToolUse": [{ "hooks": [renamed.clone()] }] } }));
        let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the accepted cost is a second group, not a replacement: {after}");
        assert_eq!(groups[0]["hooks"][0], renamed, "and theirs is preserved verbatim");
    }

    #[test]
    fn another_projects_context_caps_script_is_not_ours_to_delete() {
        let theirs = json!("node /work/vendor/context-caps.js --lint");
        assert_eq!(hook_ownership_of(Some(&theirs)), Ownership::Foreign);

        let ours = json!("node /Users/someone/.claude/hooks/context-caps.js");
        assert_eq!(hook_ownership_of(Some(&ours)), Ownership::OursStale, "our own legacy hook went unrecognised");

        // And it is preserved through a real merge, not merely classified.
        let after = wired(json!({ "hooks": { "PostToolUse": [{ "hooks": [theirs.clone()] }] } }));
        let kept = after["hooks"]["PostToolUse"][0]["hooks"][0].clone();
        assert_eq!(kept, theirs, "another project's hook was deleted: {after}");
    }

    /// **F2.** "Merge, never rewrite" applies *inside* the keys we own too.
    /// `timeout` is a real Claude Code key, and an unknown sibling is by
    /// definition something a newer Claude Code understands and we do not.
    #[test]
    fn our_own_keys_keep_the_siblings_the_user_set() {
        let wiring = wire(&json!({
            "statusLine": { "type": "command", "command": "claude-status --statusline", "timeout": 45, "future": true },
            "hooks": { "PostToolUse": [
                { "hooks": [{ "type": "command", "command": "claude-status --caps-hook", "timeout": 45 }] },
            ] },
        }))
        .unwrap();

        let status = &wiring.settings["statusLine"];
        assert_eq!(status["timeout"], 45, "a real Claude Code key was dropped: {status}");
        assert_eq!(status["future"], true, "an unknown sibling was dropped: {status}");
        // Ours still win — `--help` documents them, and `refreshInterval` is the
        // bar's cadence rather than a preference.
        assert_eq!(status["refreshInterval"], 4);
        assert_eq!(status["command"], "claude-status --statusline");

        let entry = &wiring.settings["hooks"]["PostToolUse"][0]["hooks"][0];
        assert_eq!(entry["timeout"], 45, "the hook's timeout was dropped: {entry}");
    }

    /// A foreign value is replaced whole: its keys belong to another tool's
    /// schema and mean nothing in ours.
    #[test]
    fn a_foreign_value_is_replaced_rather_than_merged_into() {
        let after = wired(json!({ "statusLine": { "type": "command", "command": "starship prompt", "theme": "x" } }));
        assert!(after["statusLine"].get("theme").is_none(), "another tool's key was carried over: {after}");
    }

    #[test]
    fn a_group_whose_hooks_is_not_an_array_is_skipped_and_kept_verbatim() {
        let odd = json!({ "matcher": "*", "hooks": "not-an-array", "extra": [1, 2] });
        let after = wired(json!({ "hooks": { "PostToolUse": [odd.clone()] } }));

        let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(groups[0], odd, "a group this tool cannot read is not a group it may repair");
        assert_eq!(groups.len(), 2, "and ours is appended alongside");
    }

    /// The TypeScript **silently discarded** every shape below and wrote over
    /// it — the same class of data loss as its `catch { return null }` on the
    /// whole file. Whatever these are, they are the user's.
    #[test]
    fn a_non_object_root_or_unreadable_hook_shape_is_refused() {
        for (settings, expected) in [
            (json!([1, 2, 3]), Refusal::NotAnObject),
            (json!("a string"), Refusal::NotAnObject),
            (json!({ "hooks": "PostToolUse" }), Refusal::HooksNotAnObject),
            (json!({ "hooks": [{ "PostToolUse": [] }] }), Refusal::HooksNotAnObject),
            (json!({ "hooks": { "PostToolUse": {} } }), Refusal::PostToolUseNotAnArray),
            (json!({ "hooks": { "PostToolUse": "all" } }), Refusal::PostToolUseNotAnArray),
        ] {
            match wire(&settings) {
                Err(actual) => assert_eq!(actual, expected, "for {settings}"),
                Ok(_) => panic!("{settings} was wired rather than refused"),
            }
        }
    }

    #[test]
    fn other_keys_under_hooks_are_untouched() {
        let after = wired(json!({ "hooks": { "PreToolUse": [{ "hooks": [] }], "Stop": "whatever" } }));
        assert_eq!(after["hooks"]["PreToolUse"], json!([{ "hooks": [] }]));
        assert_eq!(after["hooks"]["Stop"], json!("whatever"), "an unreadable sibling event is still not ours");
    }

    /// **Criterion 3**, at the merge. A hook list that grows by one entry per
    /// run is the classic failure and it takes three runs to notice.
    #[test]
    fn wiring_is_idempotent_across_three_runs() {
        let start = json!({
            "model": "opus",
            "hooks": { "PostToolUse": [{ "matcher": "Edit", "hooks": [{ "type": "command", "command": "/usr/bin/fmt" }] }] },
        });

        let once = wired(start.clone());
        let twice = wired(once.clone());
        let thrice = wired(twice.clone());

        assert_eq!(once, twice, "the second run changed the file");
        assert_eq!(twice, thrice, "the third run changed the file");
        assert_eq!(once["hooks"]["PostToolUse"].as_array().unwrap().len(), 2, "the hook list grew");

        // And the second run knows it has nothing to do, so nothing is written
        // at all — which is what keeps a file this tool does not own from being
        // reformatted on every invocation.
        assert!(wire(&start).unwrap().changed());
        assert!(!wire(&once).unwrap().changed(), "an already-wired file must be left alone");
    }

    #[test]
    fn a_foreign_status_line_is_reported_with_what_it_replaced() {
        let wiring = wire(&json!({
            "statusLine": { "type": "command", "command": "starship prompt", "padding": 2 },
            "subagentStatusLine": { "type": "command", "command": "claude-status --subagent" },
        }))
        .unwrap();

        let status = change(&wiring, STATUS_LINE);
        assert_eq!(status.ownership, Ownership::Foreign);
        assert!(status.changed);
        let replaced = status.replaced.as_deref().expect("a foreign value must be quoted back");
        assert!(replaced.contains("starship prompt"), "{replaced}");

        // Already correct, so nothing to say and nothing to change.
        let subagent = change(&wiring, SUBAGENT_STATUS_LINE);
        assert_eq!(subagent.ownership, Ownership::Ours);
        assert!(!subagent.changed);
        assert_eq!(subagent.replaced, None);
    }

    /// `ours-stale` is rewritten **quietly**. A test rather than a comment,
    /// because the tempting implementation — "warn whenever the value differs"
    /// — shouts at every user upgrading from a previous install about their own
    /// previous install.
    #[test]
    fn a_stale_value_of_ours_is_rewritten_without_being_quoted_back() {
        let wiring = wire(&json!({ "statusLine": { "type": "command", "command": "/h/.claude/bin/claude-status" } }))
            .unwrap();

        let status = change(&wiring, STATUS_LINE);
        assert_eq!(status.ownership, Ownership::OursStale);
        assert!(status.changed, "a flagless command renders the missing-flag line, so it cannot be left");
        assert_eq!(status.replaced, None, "there is nobody to warn: this is our own last install");
    }

    #[test]
    fn an_absent_hook_appends_and_an_existing_one_does_not() {
        let fresh = wire(&json!({})).unwrap();
        assert_eq!(change(&fresh, HOOK_KEY).ownership, Ownership::Absent);
        assert!(change(&fresh, HOOK_KEY).changed);

        let again = wire(&fresh.settings).unwrap();
        assert_eq!(change(&again, HOOK_KEY).ownership, Ownership::Ours);
        assert!(!change(&again, HOOK_KEY).changed);
    }

    #[test]
    fn the_prototype_keys_deep_merge_strips_are_left_alone_here() {
        // `json::deep_merge` drops these at every depth, which is right for a
        // config this tool owns and is silent key deletion in a file it does
        // not. Whatever they mean in Claude Code's settings, they are not this
        // tool's to remove.
        let after = wired(json!({ "__proto__": { "x": 1 }, "constructor": 2, "prototype": [3] }));
        assert_eq!(after["__proto__"], json!({ "x": 1 }));
        assert_eq!(after["constructor"], json!(2));
        assert_eq!(after["prototype"], json!([3]));
    }
}
