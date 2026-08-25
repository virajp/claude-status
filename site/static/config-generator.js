/*
 * The config generator: a form built from the JSON schema, and a download that
 * carries only what the user changed.
 *
 * This is the only JavaScript on this site, and `site/config.toml` records why
 * that sentence had to be edited. Two constraints shaped everything below:
 *
 *   1. NO BUILD STEP. Plain ES modules, no bundler, no dependency, no
 *      package.json — `distribution/01` removed this repository's only
 *      non-Rust toolchain and `tests/site.rs` keeps it out. dprint formats
 *      this file with a pinned wasm plugin, so it is still formatted, with no
 *      npm anywhere.
 *   2. NOTHING IS HAND-WRITTEN PER KEY. The form is a pure function of the
 *      schema document. A key added to the Rust config types appears here with
 *      no edit to this file, which is acceptance criterion 1 and the entire
 *      reason `config-and-cli/04` made the schema generated in the first
 *      place. `tests/site.rs` holds the line negatively: no schema property
 *      name may appear as a literal in any tracked file under `site/` outside
 *      the documentation prose.
 *
 * ---------------------------------------------------------------------------
 * TWO DOCUMENTS, NOT ONE
 * ---------------------------------------------------------------------------
 *
 * The page loads the schema AND `claude-status.defaults.json`, because the
 * schema deliberately carries almost no `default` values — four, all under
 * `caps`, against a tree of about a hundred leaves. `.config/mise/tasks/site/
 * assets` explains why and stages both files. A generator built on the
 * schema's defaults would show an empty config and emit every touched key as a
 * change against nothing.
 *
 * ---------------------------------------------------------------------------
 * WHAT IT EMITS, AND WHY IT MUST MATCH THE BINARY
 * ---------------------------------------------------------------------------
 *
 * `src/modules/config/write.rs` writes `$schema` plus everything that differs
 * from the shipped defaults, and nothing else. A generator that emitted a full
 * config would hand every user a frozen copy of today's defaults — exactly the
 * problem `config-and-cli/02` was about. `nonDefaults` below is that rule,
 * restated: equal emits nothing, two objects recurse at every depth, anything
 * else (scalar, array, type mismatch) goes out wholesale.
 *
 * Three places where a JavaScript port of that rule goes wrong quietly, all
 * closed here and all pinned by `tests/js/generator.test.mjs`:
 *
 *   - `JSON.stringify` comparison. It is key-order sensitive and Rust's
 *     `IndexMap` comparison is not, so reordering a status table would emit
 *     the whole table where the binary emits nothing. `deepEqual` walks.
 *   - Prototype keys. Every open map takes free-text keys from the user.
 *     `src/_shared/json.rs` drops `__proto__`, `constructor` and `prototype`
 *     at every depth; a page assembling plain objects from typed input has to
 *     do the same, and here it is not merely cosmetic.
 *   - `null` versus absent. The binary compares two SERIALIZED `Config`s, in
 *     which every unset `Option` is an explicit `null`. The defaults asset
 *     writes those keys out only when they are set. So `deepEqual` treats a
 *     missing key as `null`, which reproduces the binary's trees exactly
 *     rather than emitting a redundant `"fg": null` for a colour that was
 *     never set.
 *
 * One trap the browser does not have: `json!(15) != json!(15.0)` in
 * serde_json, because `Number` compares its internal variant. JavaScript has
 * one number type, so `refreshMinutes` cannot differ from itself here.
 *
 * ---------------------------------------------------------------------------
 * WHAT THE FORM CANNOT DO
 * ---------------------------------------------------------------------------
 *
 * There is no delete operator in the config merge (`json.rs::deep_merge`), so
 * `{"palette": {}}` is a no-op rather than an erasure. "Remove" on a row the
 * defaults ship therefore means REVERT TO SHIPPED, and the button says so. A
 * row the user added is genuinely removable, because removing it just stops it
 * being emitted.
 *
 * Arrays replace wholesale, so touching one entry of a layout emits every row
 * of it. That is correct — it is what `deep_merge` does with an array — but it
 * looks to a user like the non-defaults promise broke, so the output pane says
 * so whenever it happens.
 *
 * ---------------------------------------------------------------------------
 * NO PREVIEW
 * ---------------------------------------------------------------------------
 *
 * The plan's steps 4 and 5 — a JavaScript port of the renderer, and a CI gate
 * diffing it against `tests/golden/*.txt` — are deferred. The plan's Gaps
 * section records why, and records that when the preview does arrive it will
 * be the Rust renderer compiled to WebAssembly rather than a second
 * implementation. Nothing here draws a bar, and nothing here should start to:
 * a preview that is plausible but wrong is worse than no preview.
 */

/* ------------------------------------------------------------------ *
 * The pure core. Everything above the DOM boundary, and everything
 * `tests/js/generator.test.mjs` exercises.
 * ------------------------------------------------------------------ */

/**
 * Keys a JavaScript object prototype is reachable through.
 *
 * The same three `src/_shared/json.rs` drops at every depth. In Rust they are
 * inert and the filter exists to preserve the old implementation's behaviour;
 * here they are live, and every open map in the schema takes its keys from a
 * text input.
 */
export const FORBIDDEN_KEYS = Object.freeze([
  "__proto__",
  "constructor",
  "prototype",
]);

/** Objects, and not arrays or `null` — the shape `diff` recurses into. */
function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Structural equality, with a missing key read as `null`.
 *
 * Key ORDER is deliberately not part of it: `serde_json::Value` compares its
 * `IndexMap` by content, so a user who reorders a status table has changed
 * nothing as far as the writer is concerned, and `JSON.stringify` would
 * disagree.
 *
 * Missing-as-`null` is the other half. `to_value(Config::default())` renders
 * every unset `Option<Value>` — `bg`, `fg`, `bold`, `thinFg`, `defaultFg`,
 * `projectName` — as an explicit `null`, while the shipped defaults JSON just
 * leaves the key out. Without this, clearing a colour that was never set would
 * emit `"fg": null` where the binary emits nothing.
 */
export function deepEqual(a, b) {
  const left = a === undefined ? null : a;
  const right = b === undefined ? null : b;

  if (left === null || right === null) {
    return left === right;
  }
  if (typeof left !== typeof right) {
    return false;
  }
  if (typeof left !== "object") {
    return left === right;
  }
  if (Array.isArray(left) !== Array.isArray(right)) {
    return false;
  }

  if (Array.isArray(left)) {
    return left.length === right.length
      && left.every((item, i) => deepEqual(item, right[i]));
  }

  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of keys) {
    if (!deepEqual(left[key], right[key])) {
      return false;
    }
  }
  return true;
}

/**
 * A deep copy with the prototype keys removed at every depth.
 *
 * The mirror of `json.rs::sanitised`, and it has to run on a WHOLESALE
 * emission too: a nested object arriving where the defaults had a scalar would
 * otherwise smuggle one through unfiltered.
 */
export function sanitized(value) {
  if (Array.isArray(value)) {
    return value.map(sanitized);
  }
  if (!isPlainObject(value)) {
    return value;
  }

  const out = {};
  for (const key of Object.keys(value)) {
    if (FORBIDDEN_KEYS.includes(key)) {
      continue;
    }
    out[key] = sanitized(value[key]);
  }
  return out;
}

/** `diff` returns this for "identical, emit nothing". */
const UNCHANGED = Symbol("unchanged");

/**
 * What `current` says that `shipped` does not — `write.rs::diff`, restated.
 *
 * Three cases, and the middle one is the whole point:
 *
 *   - equal                 -> nothing
 *   - two objects           -> recurse key by key, so one changed palette
 *                              entry emits one entry rather than the palette
 *   - anything else         -> the value wholesale, matching `deep_merge`,
 *                              which replaces arrays and scalars
 *
 * Iteration follows CURRENT's key order, not the defaults', because that is
 * what decides which status bucket wins on the way back in.
 */
function diff(current, shipped) {
  if (deepEqual(current, shipped)) {
    return UNCHANGED;
  }
  if (!isPlainObject(current) || !isPlainObject(shipped)) {
    return sanitized(current);
  }

  const out = {};
  let changed = 0;
  for (const key of Object.keys(current)) {
    if (FORBIDDEN_KEYS.includes(key)) {
      continue;
    }
    // `shipped[key]` is `undefined` for a key with no default under it, and
    // `deepEqual`/this recursion read that as `null` — so a non-null value
    // there falls to the wholesale branch, which is what `write.rs` does when
    // `shipped.get(key)` is `None`.
    const value = diff(current[key], shipped[key]);
    if (value !== UNCHANGED) {
      out[key] = value;
      changed += 1;
    }
  }
  return changed > 0 ? out : UNCHANGED;
}

/**
 * `current` reduced to the schema pointer plus everything that differs.
 *
 * `pointerKey`/`pointerValue` are read off the schema document rather than
 * written here as literals — see `schemaPointer`.
 */
export function nonDefaults(current, shipped, pointerKey, pointerValue) {
  const out = {};
  if (pointerKey !== null) {
    // Computed, so a `pointerKey` of `__proto__` would still be an own property
    // rather than a prototype assignment. It cannot be one today; relying on
    // that would be relying on a fact this function cannot see.
    out[pointerKey] = pointerValue;
  }

  const changed = diff(current, shipped);
  if (changed !== UNCHANGED && isPlainObject(changed)) {
    for (const key of Object.keys(changed)) {
      if (key === pointerKey) {
        continue; // always emitted, never diffed
      }
      out[key] = changed[key];
    }
  }
  return out;
}

/**
 * The `$schema` pointer, taken from the schema document itself.
 *
 * Both halves are derived rather than copied. The KEY is the one property name
 * in the document that starts with `$` — JSON Schema reserves that prefix, and
 * the config's own keys are all plain identifiers. The VALUE is the
 * document's `$id`, which `config::schema` injects from `write::SCHEMA_URL`,
 * the same constant `--configure` stamps into every file it writes.
 *
 * So changing that URL in Rust changes what this page emits, with no edit
 * here — and no schema property name appears in this file as a literal, which
 * is what `tests/site.rs` asserts.
 */
export function schemaPointer(schema) {
  const properties = schema.properties ?? {};
  const key = Object.keys(properties).find(name => name.startsWith("$"));
  return key === undefined ? null : { key, value: schema.$id ?? "" };
}

/* ------------------------------------------------------------------ *
 * Reading the schema.
 * ------------------------------------------------------------------ */

/**
 * Follows a same-document `$ref`, keeping the siblings beside it.
 *
 * Draft 2020-12 stopped `$ref` erasing its neighbours, and this schema relies
 * on that: the four colour fields are a `$ref` to `#/$defs/color` with their
 * own `description` written next to it. A resolver that replaced the node
 * would drop exactly the prose the form exists to show.
 */
export function resolveRef(node, root) {
  if (!isPlainObject(node) || typeof node.$ref !== "string") {
    return node ?? {};
  }
  if (!node.$ref.startsWith("#/")) {
    return node;
  }

  let target = root;
  for (const segment of node.$ref.slice(2).split("/")) {
    const key = segment.replace(/~1/g, "/").replace(/~0/g, "~");
    target = isPlainObject(target) ? target[key] : undefined;
    if (target === undefined) {
      return node; // unresolvable: leave it alone
    }
  }

  const merged = { ...resolveRef(target, root) };
  for (const key of Object.keys(node)) {
    if (key !== "$ref") {
      merged[key] = node[key];
    }
  }
  return merged;
}

/**
 * The `[r, g, b]` shape, recognised by its bounds rather than by where it sits.
 *
 * This is the one piece of colour knowledge the page has, and it is deliberate
 * that it is structural: `config::schema::rgb_triple` produces this identical
 * block both as `palette`'s value type and as the array branch of
 * `#/$defs/color`, so matching on the shape is matching on the actual
 * relationship. Matching on the property NAME would be the hand-written
 * coupling criterion 1 forbids.
 */
export function isRgbTriple(node) {
  return (
    isPlainObject(node)
    && node.type === "array"
    && node.minItems === 3
    && node.maxItems === 3
    && isPlainObject(node.items)
    && node.items.type === "integer"
    && node.items.minimum === 0
    && node.items.maximum === 255
  );
}

/**
 * The property holding the named-colour table, found by shape.
 *
 * An object whose every value is an RGB triple is a palette; nothing else in
 * the schema has that shape. Returning the NAME lets the colour widgets offer
 * the live names as completions without this file ever saying what the
 * property is called.
 */
export function paletteProperty(schema) {
  const properties = schema.properties ?? {};
  for (const key of Object.keys(properties)) {
    const node = resolveRef(properties[key], schema);
    if (
      isPlainObject(node)
      && node.type === "object"
      && isRgbTriple(resolveRef(node.additionalProperties, schema))
    ) {
      return key;
    }
  }
  return null;
}

/** A human label for one branch of a `oneOf`, derived from the branch's type. */
function branchLabel(node, colour) {
  if (isRgbTriple(node)) {
    return "RGB";
  }
  switch (node.type) {
    case "null":
      return "clear";
    case "string":
      return colour ? "name or hex" : "text";
    case "object":
      return "object";
    case "array":
      return "list";
    case "boolean":
      return "on / off";
    case "integer":
    case "number":
      return "number";
    default:
      return "raw JSON";
  }
}

/**
 * One field of the form, as data.
 *
 * Value-independent on purpose: a descriptor describes what a widget for this
 * schema node looks like, and the renderer walks the descriptor and the
 * current value together. That is what makes the form buildable — and
 * testable — from a schema document alone, with no config in hand, which is
 * how `tests/js/generator.test.mjs` can feed it an invented key.
 *
 * `unsupported` is the honesty valve and is load-bearing. A schema shape none
 * of the rules below covers becomes a raw JSON box rather than disappearing —
 * so a new key is always *editable*, even when it is not yet pretty. A rule
 * that silently dropped what it did not recognise would satisfy criterion 1's
 * letter and break it in practice.
 */
export function describeField(node, root) {
  const schema = resolveRef(node, root);
  const description = typeof schema.description === "string"
    ? schema.description
    : "";
  const base = { description, schema };

  if (Array.isArray(schema.oneOf)) {
    const branches = schema.oneOf.map(branch => resolveRef(branch, root));
    const colour = branches.some(isRgbTriple);
    return {
      ...base,
      kind: "choice",
      colour,
      branches: branches.map(branch => ({
        label: branchLabel(branch, colour),
        field: describeField(branch, root),
      })),
    };
  }

  if (Array.isArray(schema.enum)) {
    return { ...base, kind: "select", options: schema.enum };
  }

  // `"type": ["boolean", "null"]` — tri-state, and a checkbox cannot say
  // `null`. Enumerable only when every member is a type with a finite set of
  // values; anything else falls through to the raw JSON box rather than
  // pretending.
  if (Array.isArray(schema.type)) {
    const states = [];
    for (const type of schema.type) {
      if (type === "boolean") {
        states.push({ label: "on", value: true }, {
          label: "off",
          value: false,
        });
      }
      else if (type === "null") {
        states.push({ label: "clear (null)", value: null });
      }
      else {
        return { ...base, kind: "unsupported" };
      }
    }
    return { ...base, kind: "states", states };
  }

  switch (schema.type) {
    case "object":
      if (isPlainObject(schema.properties)) {
        return {
          ...base,
          kind: "object",
          fields: Object.keys(schema.properties).map(key => ({
            key,
            field: describeField(schema.properties[key], root),
          })),
        };
      }
      if (isPlainObject(schema.additionalProperties)) {
        return {
          ...base,
          kind: "map",
          entry: describeField(schema.additionalProperties, root),
        };
      }
      return { ...base, kind: "unsupported" };

    case "array":
      if (isRgbTriple(schema)) {
        return { ...base, kind: "rgb" };
      }
      if (schema.items === undefined) {
        return { ...base, kind: "unsupported" };
      }
      return { ...base, kind: "list", item: describeField(schema.items, root) };

    case "boolean":
      return { ...base, kind: "boolean" };

    case "integer":
    case "number":
      return {
        ...base,
        kind: "number",
        integer: schema.type === "integer",
        min: schema.minimum,
        max: schema.maximum,
      };

    case "string":
      return { ...base, kind: "text" };

    case "null":
      return { ...base, kind: "null" };

    default:
      return { ...base, kind: "unsupported" };
  }
}

/**
 * The whole form, as data.
 *
 * `$`-prefixed properties are excluded, which is how `$schema` stays out of
 * the form without this file naming it. It is a pointer rather than a setting
 * — there is no Rust field behind it — and `nonDefaults` emits it
 * unconditionally.
 */
export function describeForm(schema) {
  const properties = schema.properties ?? {};
  return {
    pointer: schemaPointer(schema),
    paletteProperty: paletteProperty(schema),
    fields: Object
      .keys(properties)
      .filter(key => !key.startsWith("$"))
      .map(key => ({ key, field: describeField(properties[key], schema) })),
  };
}

/** A usable empty value for a field, used when a row or a branch is created. */
export function blankValue(field) {
  switch (field.kind) {
    case "object": {
      const out = {};
      for (const { key, field: child } of field.fields) {
        out[key] = blankValue(child);
      }
      return out;
    }
    case "map":
      return {};
    case "list":
      return [];
    case "rgb":
      return [0, 0, 0];
    case "boolean":
      return false;
    case "number":
      return typeof field.min === "number" ? field.min : 0;
    case "select":
      return field.options[0];
    case "states":
      return field.states[0].value;
    case "choice":
      return blankValue(field.branches[0].field);
    case "null":
      return null;
    default:
      return "";
  }
}

/** Which branch of a `choice` a value belongs to, by JSON type. */
export function branchFor(value, branches) {
  const wanted = value === null
    ? "null"
    : Array.isArray(value)
    ? "array"
    : typeof value === "object"
    ? "object"
    : typeof value === "number"
    ? "number"
    : typeof value;

  const index = branches.findIndex(({ field }) => {
    const type = field.schema.type;
    if (type === "integer" && wanted === "number") {
      return true;
    }
    return type === wanted;
  });
  return index < 0 ? 0 : index;
}

/* ------------------------------------------------------------------ *
 * The DOM. Nothing below runs under the test harness.
 * ------------------------------------------------------------------ */

const TARGET_PATH = "~/.config/claude-status/config.json";

function element(tag, attributes = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attributes)) {
    if (key === "class") {
      node.className = value;
    }
    else if (key === "text") {
      node.textContent = value;
    }
    else {
      node.setAttribute(key, value);
    }
  }
  for (const child of children) {
    node.append(child);
  }
  return node;
}

/**
 * A field's schema `description`, shown beside it — criterion 7's whole point:
 * a form built from a schema with no descriptions is a form of unlabelled
 * boxes, which is why this cycle put the ten missing ones back at the source.
 *
 * A `<span>` and not a `<p>`, because a row is a `<label>` and a `<label>`'s
 * content model is phrasing content. The stylesheet makes it a block.
 */
function hint(text) {
  return text
    ? element("span", { class: "gen-hint", text })
    : document.createTextNode("");
}

function valueAt(root, path) {
  let node = root;
  for (const step of path) {
    if (node === null || node === undefined) {
      return undefined;
    }
    node = node[step];
  }
  return node;
}

function setValueAt(root, path, value) {
  let node = root;
  for (const step of path.slice(0, -1)) {
    node = node[step];
  }
  node[path.at(-1)] = value;
}

function deleteValueAt(root, path) {
  let node = root;
  for (const step of path.slice(0, -1)) {
    node = node[step];
  }
  const last = path.at(-1);
  if (Array.isArray(node)) {
    node.splice(Number(last), 1);
  }
  else {
    delete node[last];
  }
}

/**
 * The page, once both documents are in.
 *
 * `shipped` is never mutated — it is the right-hand side of every comparison,
 * and it is also what "revert to shipped" restores from.
 */
function start(schema, defaults, mount) {
  const form = describeForm(schema);
  const shipped = Object.freeze(structuredClone(defaults));
  let current = structuredClone(defaults);

  const output = element("pre", { class: "gen-output" });
  const note = element("p", { class: "gen-note" });
  const fields = element("div", { class: "gen-fields" });

  /** The live palette names, read out of the config the user is editing. */
  function paletteNames() {
    if (form.paletteProperty === null) {
      return [];
    }
    const table = current[form.paletteProperty];
    return isPlainObject(table) ? Object.keys(table) : [];
  }

  const datalistId = "gen-palette";

  function refreshOutput() {
    // No fallback pointer. If the schema declares no `$`-prefixed property
    // there is nothing to stamp, and writing the key here as a literal would
    // be the hand-coupling `tests/site.rs` forbids — it would also be wrong,
    // since the whole point is that the key and the URL come from the document.
    const emitted = form.pointer === null
      ? nonDefaults(current, shipped, null, null)
      : nonDefaults(current, shipped, form.pointer.key, form.pointer.value);
    output.textContent = JSON.stringify(emitted, null, 2) + "\n";

    // The `lines` surprise, stated generically so it fires for any array the
    // user touched. `deep_merge` replaces an array wholesale, so a config that
    // changed one entry has to carry every entry — which looks like the
    // non-defaults promise breaking and is in fact it being kept.
    const wholesale = Object.keys(emitted).filter(key =>
      Array.isArray(emitted[key])
    );
    note.textContent = wholesale.length === 0
      ? ""
      : `${
        wholesale.join(", ")
      } came out complete rather than as a partial edit. `
        + "Arrays and scalars replace wholesale when the layers merge, so a list has to be emitted "
        + "whole or the parts you did not touch would be lost. This is the binary's behaviour, not a bug here.";
  }

  function render() {
    fields.replaceChildren();

    const list = element("datalist", { id: datalistId });
    for (const name of paletteNames()) {
      list.append(element("option", { value: name }));
    }
    fields.append(list);

    for (const { key, field } of form.fields) {
      fields.append(widget(field, [key], key));
    }
    refreshOutput();
  }

  /** Re-renders everything. Used after a structural edit, never on a keystroke. */
  function restructure() {
    render();
  }

  function scalarInput(attributes, read, path) {
    const input = element("input", attributes);
    input.addEventListener("input", () => {
      setValueAt(current, path, read(input));
      refreshOutput();
    });
    return input;
  }

  /**
   * One widget, chosen by the descriptor's kind and filled from `current`.
   *
   * `label` is the row's caption — a property name, a map key, or an index.
   */
  function widget(field, path, label) {
    const value = valueAt(current, path);

    switch (field.kind) {
      case "object": {
        const box = element("fieldset", { class: "gen-group" }, [
          element("legend", { text: label }),
          hint(field.description),
        ]);
        for (const { key, field: child } of field.fields) {
          if (!isPlainObject(valueAt(current, path))) {
            setValueAt(current, path, {});
          }
          box.append(widget(child, [...path, key], key));
        }
        return box;
      }

      case "map":
        return mapWidget(field, path, label);

      case "list":
        return listWidget(field, path, label);

      case "choice":
        return choiceWidget(field, path, label);

      case "rgb":
        return rgbWidget(field, path, label);

      case "select": {
        const select = element("select");
        for (const option of field.options) {
          select.append(element("option", { value: option, text: option }));
        }
        select.value = value ?? field.options[0];
        select.addEventListener("change", () => {
          setValueAt(current, path, select.value);
          refreshOutput();
        });
        return row(label, field.description, select);
      }

      case "states": {
        const select = element("select");
        field.states.forEach((state, index) => {
          select.append(
            element("option", { value: String(index), text: state.label }),
          );
        });
        const active = field.states.findIndex(state =>
          deepEqual(state.value, value ?? null)
        );
        select.value = String(active < 0 ? field.states.length - 1 : active);
        select.addEventListener("change", () => {
          setValueAt(current, path, field.states[Number(select.value)].value);
          refreshOutput();
        });
        return row(label, field.description, select);
      }

      case "boolean": {
        const input = element("input", { type: "checkbox" });
        input.checked = value === true;
        input.addEventListener("change", () => {
          setValueAt(current, path, input.checked);
          refreshOutput();
        });
        return row(label, field.description, input);
      }

      case "number": {
        const attributes = {
          type: "number",
          step: field.integer ? "1" : "any",
          value: value ?? "",
        };
        if (typeof field.min === "number") {
          attributes.min = String(field.min);
        }
        if (typeof field.max === "number") {
          attributes.max = String(field.max);
        }
        return row(
          label,
          field.description,
          scalarInput(
            attributes,
            input => (input.value === "" ? null : Number(input.value)),
            path,
          ),
        );
      }

      case "text": {
        const attributes = { type: "text", value: value ?? "" };
        // A colour's string branch gets the live palette names as completions,
        // so the shipped names stay reachable and a colour set by name keeps
        // following the palette forward instead of freezing to a literal.
        if (field.palette) {
          attributes.list = datalistId;
        }
        return row(
          label,
          field.description,
          scalarInput(attributes, input => input.value, path),
        );
      }

      case "null":
        return row(
          label,
          field.description,
          element("span", { class: "gen-static", text: "null" }),
        );

      default:
        return jsonWidget(field, path, label);
    }
  }

  function row(label, description, control) {
    return element("label", { class: "gen-row" }, [
      element("span", { class: "gen-label", text: label }),
      control,
      hint(description),
    ]);
  }

  /** A colour triple: three bounded numbers, plus a picker bound to them. */
  function rgbWidget(field, path, label) {
    const value = Array.isArray(valueAt(current, path))
      ? valueAt(current, path)
      : [0, 0, 0];
    const CHANNELS = ["red", "green", "blue"];
    const inputs = [0, 1, 2].map(i =>
      element("input", {
        type: "number",
        min: "0",
        max: "255",
        step: "1",
        value: String(value[i] ?? 0),
        class: "gen-rgb",
        "aria-label": `${label} ${CHANNELS[i]}`,
      })
    );
    const hex = triple =>
      "#"
      + triple
        .map(n =>
          Math.max(0, Math.min(255, Number(n) || 0)).toString(16).padStart(
            2,
            "0",
          )
        )
        .join("");
    const picker = element("input", {
      type: "color",
      value: hex(value),
      "aria-label": `${label} colour picker`,
    });

    const push = () => {
      const triple = inputs.map(input => Number(input.value) || 0);
      setValueAt(current, path, triple);
      picker.value = hex(triple);
      refreshOutput();
      if (form.paletteProperty !== null && path[0] === form.paletteProperty) {
        restructure();
      }
    };
    for (const input of inputs) {
      input.addEventListener("input", push);
    }
    picker.addEventListener("input", () => {
      const triple = [1, 3, 5].map(at =>
        parseInt(picker.value.slice(at, at + 2), 16)
      );
      inputs.forEach((input, i) => (input.value = String(triple[i])));
      setValueAt(current, path, triple);
      refreshOutput();
    });

    return element("div", {
      class: "gen-row",
      role: "group",
      "aria-label": label,
    }, [
      element("span", { class: "gen-label", text: label }),
      element("span", { class: "gen-rgb-group" }, [...inputs, picker]),
      hint(field.description),
    ]);
  }

  /** A `oneOf`: a mode switcher, then whichever branch the value is in. */
  function choiceWidget(field, path, label) {
    const value = valueAt(current, path) ?? null;
    const active = branchFor(value, field.branches);

    const select = element("select", { class: "gen-mode" });
    field.branches.forEach(({ label: name }, index) => {
      select.append(element("option", { value: String(index), text: name }));
    });
    select.value = String(active);
    select.addEventListener("change", () => {
      setValueAt(
        current,
        path,
        blankValue(field.branches[Number(select.value)].field),
      );
      restructure();
    });

    // The string branch of a colour is where a palette NAME goes, and offering
    // the names is the whole reason `paletteProperty` exists: the shipped
    // defaults reference colours by name, and a widget that could only emit a
    // triple would silently convert every colour it touched into a literal
    // that stops tracking the palette.
    const branch = field.branches[active].field;
    const inner = branch.kind === "text" && field.colour
      ? widget({ ...branch, palette: true }, path, "")
      : widget(branch, path, "");

    return element("fieldset", { class: "gen-group gen-choice" }, [
      element("legend", { text: label }),
      hint(field.description),
      select,
      inner,
    ]);
  }

  /** An open map: one row per entry, plus a key box to add another. */
  function mapWidget(field, path, label) {
    if (!isPlainObject(valueAt(current, path))) {
      setValueAt(current, path, {});
    }
    const table = valueAt(current, path);
    const shippedTable = valueAt(shipped, path);

    const box = element("fieldset", { class: "gen-group" }, [
      element("legend", { text: label }),
      hint(field.description),
    ]);

    for (const key of Object.keys(table)) {
      const isShipped = isPlainObject(shippedTable)
        && Object.hasOwn(shippedTable, key);
      const button = element("button", {
        type: "button",
        class: "gen-remove",
        // Removal is not expressible in the config format: `deep_merge` has no
        // delete operator, so `{"palette": {}}` merges as a no-op. Reverting a
        // shipped row is the honest thing the format CAN say, and the label
        // has to admit it.
        text: isShipped ? "Revert to shipped" : "Remove",
        title: isShipped
          ? "Restores the shipped value. A config file cannot delete a key the defaults ship — the merge has no delete operator."
          : "Removes a key you added, so it is simply not emitted.",
      });
      button.addEventListener("click", () => {
        if (isShipped) {
          setValueAt(
            current,
            [...path, key],
            structuredClone(shippedTable[key]),
          );
        }
        else {
          deleteValueAt(current, [...path, key]);
        }
        restructure();
      });

      box.append(
        element("div", { class: "gen-entry" }, [
          widget(field.entry, [...path, key], key),
          button,
        ]),
      );
    }

    const keyInput = element("input", { type: "text", placeholder: "new key" });
    const add = element("button", { type: "button", text: "Add" });
    const error = element("p", { class: "gen-error" });
    add.addEventListener("click", () => {
      const key = keyInput.value.trim();
      if (key === "") {
        return;
      }
      if (FORBIDDEN_KEYS.includes(key)) {
        error.textContent =
          `\`${key}\` is dropped by the config merge at every depth, so it can never take effect.`;
        return;
      }
      if (Object.hasOwn(table, key)) {
        error.textContent = `\`${key}\` is already here.`;
        return;
      }
      setValueAt(current, [...path, key], blankValue(field.entry));
      restructure();
    });
    box.append(element("div", { class: "gen-add" }, [keyInput, add]), error);
    return box;
  }

  /** An array: one widget per item, with order controls, since order is meaning. */
  function listWidget(field, path, label) {
    if (!Array.isArray(valueAt(current, path))) {
      setValueAt(current, path, []);
    }
    const items = valueAt(current, path);

    const box = element("fieldset", { class: "gen-group" }, [
      element("legend", { text: label }),
      hint(field.description),
    ]);

    items.forEach((_, index) => {
      const move = to => {
        if (to < 0 || to >= items.length) {
          return;
        }
        const [item] = items.splice(index, 1);
        items.splice(to, 0, item);
        restructure();
      };
      const up = element("button", {
        type: "button",
        text: "↑",
        title: "Move up",
      });
      const down = element("button", {
        type: "button",
        text: "↓",
        title: "Move down",
      });
      const remove = element("button", {
        type: "button",
        class: "gen-remove",
        text: "Remove",
      });
      up.addEventListener("click", () => move(index - 1));
      down.addEventListener("click", () => move(index + 1));
      remove.addEventListener("click", () => {
        deleteValueAt(current, [...path, index]);
        restructure();
      });

      box.append(
        element("div", { class: "gen-entry" }, [
          widget(field.item, [...path, index], String(index + 1)),
          element("span", { class: "gen-order" }, [up, down, remove]),
        ]),
      );
    });

    const add = element("button", { type: "button", text: "Add" });
    add.addEventListener("click", () => {
      items.push(blankValue(field.item));
      restructure();
    });
    box.append(element("div", { class: "gen-add" }, [add]));
    return box;
  }

  /**
   * The fallback: whatever the rules above could not name, as raw JSON.
   *
   * This is what makes criterion 1 true for SHAPES rather than only for names.
   * A schema construct nobody anticipated is still editable here — badly, but
   * honestly — instead of vanishing from a form that claims to be complete.
   */
  function jsonWidget(field, path, label) {
    const area = element("textarea", { rows: "3", class: "gen-json" });
    area.value = JSON.stringify(valueAt(current, path) ?? null, null, 2);
    const error = element("span", { class: "gen-error" });
    area.addEventListener("input", () => {
      try {
        setValueAt(current, path, sanitized(JSON.parse(area.value)));
        error.textContent = "";
        refreshOutput();
      }
      catch {
        error.textContent =
          "Not valid JSON — the value below is unchanged until this parses.";
      }
    });
    return element("label", { class: "gen-row" }, [
      element("span", { class: "gen-label", text: label }),
      area,
      hint(
        field.description
          || "This shape has no dedicated widget yet, so it is edited as raw JSON.",
      ),
      error,
    ]);
  }

  /* ---- the panel around the form ---- */

  const download = element("button", {
    type: "button",
    class: "gen-primary",
    text: "Download config.json",
  });
  download.addEventListener("click", () => {
    const blob = new Blob([output.textContent], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = element("a", { href: url, download: "config.json" });
    link.click();
    URL.revokeObjectURL(url);
  });

  const copy = element("button", { type: "button", text: "Copy" });
  const copied = element("span", { class: "gen-copied" });
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(output.textContent);
      copied.textContent = "Copied.";
    }
    catch {
      copied.textContent =
        "The browser refused clipboard access — select the text above instead.";
    }
  });

  const reset = element("button", { type: "button", text: "Start over" });
  reset.addEventListener("click", () => {
    current = structuredClone(defaults);
    restructure();
  });

  mount.replaceChildren(
    element("p", { class: "gen-provenance" }, [
      document.createTextNode(
        "Built from the schema this site was deployed with — ",
      ),
      element("a", {
        href: schema.$id ?? "#",
        text: schema.$id ?? "the published schema",
      }),
      document.createTextNode(
        ". There are no releases yet, so it is the schema on `main` at build time rather than one pinned to a version you installed.",
      ),
    ]),
    fields,
    element("h2", { text: "Your config" }),
    element("p", { class: "gen-path" }, [
      document.createTextNode("Save this at "),
      element("code", { text: TARGET_PATH }),
      document.createTextNode("."),
    ]),
    output,
    note,
    element("p", { class: "gen-actions" }, [download, copy, reset, copied]),
  );

  render();
}

/**
 * Both documents sit beside this file in the static tree, so they are resolved
 * against this module's own URL rather than against a path written here —
 * which keeps the page working from a subdirectory, from a preview host, and
 * from `site/public/` opened over `file:`.
 */
async function boot() {
  const mount = document.getElementById("config-generator");
  if (mount === null) {
    return;
  }

  try {
    const [schema, defaults] = await Promise.all(
      ["./claude-status.schema.json", "./claude-status.defaults.json"].map(
        async name => {
          const response = await fetch(new URL(name, import.meta.url));
          if (!response.ok) {
            throw new Error(`${name}: ${response.status}`);
          }
          return response.json();
        },
      ),
    );
    start(schema, defaults, mount);
  }
  catch (error) {
    // Loud and specific. A generator that silently renders nothing looks
    // exactly like a generator nobody wired up, and the documentation below
    // this element is still the whole reference either way.
    mount.replaceChildren(
      element("p", {
        class: "gen-error",
        text:
          `The form could not load (${error.message}). The reference below is complete without it.`,
      }),
    );
  }
}

if (typeof document !== "undefined") {
  boot();
}
