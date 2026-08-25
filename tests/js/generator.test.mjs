/*
 * The config generator's pure core, exercised against the real schema and the
 * real shipped defaults.
 *
 * RUN IT THROUGH `tests/site.rs`, not by hand. That test copies
 * `site/static/config-generator.js` next to this file as `generator.mjs` and
 * runs `node` over it — the rename is the whole reason for the copy. Node
 * decides a file's module system from its extension, and the browser file has
 * to stay `.js` so a static host serves it as `text/javascript`; a `.mjs` in
 * `site/static/` would be served as `application/octet-stream` by some hosts
 * and rejected as a module.
 *
 * NOTHING IS INSTALLED TO RUN THIS. No package.json, no lockfile, no
 * node_modules — `tests/site.rs::no_javascript_lockfile_or_node_modules_is_
 * tracked` still passes, and `code:sec`'s grype scan still sees no npm
 * ecosystem. It is `node` invoked as a bare binary, the same way the suite
 * already invokes `git`, `mise` and `dprint`. When `node` is absent the Rust
 * side skips loudly and says so, following
 * `tests/schema.rs::the_generated_schema_is_already_dprint_formatted`.
 *
 * Usage: node generator.test.mjs <schema.json> <defaults.json> [--self-check]
 *
 * `--self-check` asserts something false on purpose. `tests/site.rs` runs it
 * and REQUIRES a non-zero exit, because a harness that cannot fail is worth
 * nothing and this repository has shipped two guards that could not.
 */

import { readFileSync } from "node:fs";

import {
  branchFor,
  deepEqual,
  describeField,
  describeForm,
  isRgbTriple,
  nonDefaults,
  paletteProperty,
  sanitized,
  schemaPointer,
} from "./generator.mjs";

const [schemaPath, defaultsPath, ...flags] = process.argv.slice(2);
const schema = JSON.parse(readFileSync(schemaPath, "utf8"));
const defaults = JSON.parse(readFileSync(defaultsPath, "utf8"));

const failures = [];
let checks = 0;

function check(name, condition, detail = "") {
  checks += 1;
  if (!condition) {
    failures.push(`${name}${detail ? `\n    ${detail}` : ""}`);
  }
}

function equal(name, actual, expected) {
  check(
    name,
    deepEqual(actual, expected),
    `expected ${JSON.stringify(expected)}\n    got      ${
      JSON.stringify(actual)
    }`,
  );
}

/** A fresh editable copy of the shipped defaults — what the form starts from. */
function config() {
  return structuredClone(defaults);
}

const pointer = schemaPointer(schema);

/** What the page emits for a config, exactly as the download button does. */
function emitted(current) {
  return nonDefaults(current, defaults, pointer.key, pointer.value);
}

/* ---- the pointer, derived rather than written down --------------------- */

equal(
  "the schema pointer's key is the one `$`-prefixed property",
  pointer.key,
  "$schema",
);
equal(
  "the schema pointer's value is the schema's own $id",
  pointer.value,
  schema.$id,
);
check(
  "the shipped defaults carry the same pointer",
  defaults[pointer.key] === schema.$id,
);

/* ---- the emitter: write.rs's rule, restated ---------------------------- */

equal("an untouched config emits only the pointer", emitted(config()), {
  [pointer.key]: schema.$id,
});

{
  // Criterion 2 — one changed value, one key out.
  const current = config();
  current.defaultFg = "aqua";
  equal("one changed scalar emits that key and the pointer", emitted(current), {
    [pointer.key]: schema.$id,
    defaultFg: "aqua",
  });
}

{
  // Criterion 3 — the failure the whole module exists to prevent. Emitting the
  // full palette would be valid, would render, and would silently freeze the
  // other nine colours at today's values.
  const current = config();
  const palette = paletteProperty(schema);
  current[palette].blue = [1, 2, 3];
  const out = emitted(current);
  equal("one palette entry emits one entry", out[palette], { blue: [1, 2, 3] });
  equal(
    "and nothing else comes with it",
    Object.keys(out).sort(),
    [pointer.key, palette].sort(),
  );
}

{
  // The nested case: a style block inside an open map. `segments.model` also
  // ships `fg` and `bold`, and emitting those would freeze them.
  const current = config();
  current.segments.model.bg = "red";
  equal("a nested style diffs key by key", emitted(current).segments, {
    model: { bg: "red" },
  });
}

{
  // All five open maps, the fifth being `subagent.statuses` — whose entries are
  // closed objects rather than scalars, and whose count the plan got wrong.
  const current = config();
  current.palette.blue = [1, 2, 3];
  current.symbols.model = "M";
  current.typeSymbols.task = "T";
  current.segments.model.bg = "red";
  current.subagent.statuses.done.bg = "purple";

  const out = emitted(current);
  equal("open map 1 of 5", out.palette, { blue: [1, 2, 3] });
  equal("open map 2 of 5", out.symbols, { model: "M" });
  equal("open map 3 of 5", out.typeSymbols, { task: "T" });
  equal("open map 4 of 5", out.segments, { model: { bg: "red" } });
  equal("open map 5 of 5 — subagent.statuses", out.subagent, {
    statuses: { done: { bg: "purple" } },
  });
}

{
  // Arrays replace wholesale, matching `deep_merge`. A layout diffed element by
  // element could not be reassembled by the merge.
  const current = config();
  current.lines[0][0] = { name: "model", bg: "red" };
  equal(
    "a touched layout comes out whole",
    emitted(current).lines,
    current.lines,
  );
  check(
    "which is more rows than were edited",
    emitted(current).lines.length === defaults.lines.length,
  );
}

{
  // The `JSON.stringify` trap. `serde_json::Value` compares an `IndexMap` by
  // content, so reordering a status table changes nothing the writer can see —
  // a stringify comparison would emit the entire table.
  const current = config();
  const statuses = current.subagent.statuses;
  current.subagent.statuses = Object.fromEntries(
    Object.keys(statuses).reverse().map(key => [key, statuses[key]]),
  );
  equal("reordering a map emits nothing", emitted(current), {
    [pointer.key]: schema.$id,
  });
}

{
  // `null` versus absent, in both directions. The binary compares serialized
  // `Config`s in which every unset `Option` is an explicit `null`; the shipped
  // JSON just omits those keys.
  const cleared = config();
  cleared.segments.model.fg = null; // shipped as "white" — a real clear
  equal(
    "clearing a colour the defaults set emits null",
    emitted(cleared).segments,
    { model: { fg: null } },
  );

  const noop = config();
  noop.segments.branch.fg = null; // never set — `null` and absent are the same
  equal("clearing a colour that was never set emits nothing", emitted(noop), {
    [pointer.key]: schema.$id,
  });
}

{
  // Prototype keys, at every depth and through a wholesale emission. Every open
  // map in the schema takes its keys from a text input.
  const current = config();
  current.palette.__proto__x = [1, 2, 3]; // a control: an ordinary new key survives
  current.symbols.constructor = "boom";
  current.subagent.statuses.mine = {
    match: "x",
    symbol: "y",
    prototype: { polluted: true },
  };

  const out = emitted(current);
  check(
    "an ordinary added key survives",
    deepEqual(out.palette, { __proto__x: [1, 2, 3] }),
  );
  check(
    "a forbidden key is dropped at the top of a map",
    out.symbols === undefined,
    JSON.stringify(out.symbols),
  );
  check(
    "a forbidden key is dropped inside a wholesale emission",
    out.subagent.statuses.mine.prototype === undefined,
    JSON.stringify(out.subagent),
  );
  check("and its siblings survive", out.subagent.statuses.mine.match === "x");
  check(
    "the emitted object has no polluted prototype",
    ({}).polluted === undefined,
  );
}

check(
  "sanitized drops a forbidden key nested in an array",
  deepEqual(sanitized([{ constructor: 1, ok: 2 }]), [{ ok: 2 }]),
);

/* ---- deepEqual, on its own ---------------------------------------------- */

check("deepEqual ignores key order", deepEqual({ a: 1, b: 2 }, { b: 2, a: 1 }));
check(
  "deepEqual reads a missing key as null",
  deepEqual({ a: 1 }, { a: 1, b: null }),
);
check(
  "deepEqual still separates null from a value",
  !deepEqual({ a: null }, { a: 1 }),
);
check("deepEqual separates an array from an object", !deepEqual([], {}));
check("deepEqual compares arrays by position", !deepEqual([1, 2], [2, 1]));

/* ---- the form, as a pure function of a schema --------------------------- */

{
  // CRITERION 1, and the only form of it that can be tested: the committed
  // schema cannot gain a key here — the drift check and the pre-commit hook
  // both stop it — so the form builder is fed an INVENTED one instead. If this
  // passes, a key added to the Rust types shows up with no edit to the page.
  const invented = structuredClone(schema);
  invented.properties.inventedKey = {
    type: "integer",
    minimum: 2,
    maximum: 40,
    description: "A key that has never existed.",
  };
  invented.properties.inventedBlock = {
    type: "object",
    additionalProperties: false,
    properties: {
      nested: {
        type: "string",
        description: "Inside a block nobody wrote a widget for.",
      },
    },
    description: "A block that has never existed.",
  };

  const form = describeForm(invented);
  const found = form.fields.find(entry => entry.key === "inventedKey");
  check("an invented key appears in the form", found !== undefined);
  equal("with the right widget", found.field.kind, "number");
  equal("carrying its bounds", [found.field.min, found.field.max], [2, 40]);
  equal(
    "and its description",
    found.field.description,
    "A key that has never existed.",
  );

  const block = form.fields.find(entry => entry.key === "inventedBlock");
  equal("an invented block becomes a group", block.field.kind, "object");
  equal(
    "with its child described",
    block.field.fields[0].field.description,
    "Inside a block nobody wrote a widget for.",
  );

  // The honesty valve. A shape none of the rules cover must still be editable,
  // or criterion 1 holds for names and fails for shapes.
  const odd = structuredClone(schema);
  odd.properties.oddShape = { description: "No `type` at all." };
  const oddField =
    describeForm(odd).fields.find(entry => entry.key === "oddShape").field;
  equal(
    "an unrecognised shape falls back to raw JSON rather than vanishing",
    oddField.kind,
    "unsupported",
  );
}

{
  const form = describeForm(schema);
  check(
    "the pointer property is excluded from the form",
    form.fields.every(entry => !entry.key.startsWith("$")),
  );
  check(
    "every other top-level property is in it",
    form.fields.length === Object.keys(schema.properties).length - 1,
  );

  // The committed schema is fully covered TODAY. `unsupported` is a valve, not
  // a resting place — if this fires, a shape was added that deserves a widget.
  const raw = form
    .fields
    .filter(entry => entry.field.kind === "unsupported")
    .map(entry => entry.key);
  equal("no property of the committed schema falls back to raw JSON", raw, []);

  // Every field the form renders shows its description — criterion 7, checked
  // where the form actually reads it rather than where the schema stores it.
  const bare = form.fields.filter(entry => entry.field.description === "").map(
    entry => entry.key,
  );
  equal("every top-level field carries prose", bare, []);
}

{
  // Colours, found by shape rather than by name — the coupling criterion 1
  // forbids is a property NAME, and this file must contain none.
  const palette = paletteProperty(schema);
  check(
    "the palette is found by the shape of its values",
    typeof palette === "string",
  );
  check(
    "and it is the table the defaults ship",
    Object.keys(defaults[palette]).length > 0,
  );

  const colour = describeField(schema.properties.defaultFg, schema);
  equal("a colour is a mode switcher", colour.kind, "choice");
  equal("with three branches", colour.branches.length, 3);
  check("recognised as a colour", colour.colour === true);
  equal("whose labels name the forms", colour.branches.map(b => b.label), [
    "name or hex",
    "RGB",
    "clear",
  ]);

  // The shipped defaults reference colours BY NAME. A picker that could only
  // emit a triple would convert every colour it touched into a literal that
  // stops following the palette forward — which is why the string branch is
  // first and gets the palette completions.
  equal(
    "a shipped colour is a name and stays selectable as one",
    branchFor(defaults.defaultFg, colour.branches),
    0,
  );
  equal(
    "a triple lands on the RGB branch",
    branchFor([1, 2, 3], colour.branches),
    1,
  );
  equal("null lands on clear", branchFor(null, colour.branches), 2);

  check(
    "the RGB branch is the triple shape",
    isRgbTriple(colour.branches[1].field.schema),
  );
}

{
  // `bold` is `["boolean", "null"]`. A checkbox cannot say `null`, and `null`
  // is the only way to clear a `bold` the defaults set.
  const style = describeField({ $ref: "#/$defs/style" }, schema);
  const bold = style.fields.find(entry => entry.key === "bold").field;
  equal("bold is tri-state, not a checkbox", bold.kind, "states");
  equal("with all three states", bold.states.map(state => state.value), [
    true,
    false,
    null,
  ]);
  check("and it now has prose", bold.description !== "");
}

{
  // A `$ref` with a `description` beside it. Draft 2020-12 stopped `$ref`
  // erasing its siblings and this schema relies on that; a resolver that
  // replaced the node would drop exactly the prose the form exists to show.
  const thinFg = describeField(
    schema.properties.powerline.properties.thinFg,
    schema,
  );
  equal(
    "a sibling description survives $ref resolution",
    thinFg.description,
    "Foreground colour of the thin divider.",
  );
  equal("and the referenced shape still arrives", thinFg.kind, "choice");
}

{
  // The layout: an array of arrays of `oneOf`. No rule in the plan covered it.
  const lines = describeField(schema.properties.lines, schema);
  equal("a layout is a list", lines.kind, "list");
  equal("of lists", lines.item.kind, "list");
  equal("of mode switchers", lines.item.item.kind, "choice");
  equal("id or styled object", lines.item.item.branches.map(b => b.label), [
    "text",
    "object",
  ]);

  // `name` and `id` are ALIASES for the same thing, and both are in the schema.
  const styled = lines.item.item.branches[1].field;
  const keys = styled.fields.map(entry => entry.key);
  check(
    "both aliases are offered",
    keys.includes("name") && keys.includes("id"),
    keys.join(","),
  );
}

/* ---- the deliberate failure ---------------------------------------------- */

if (flags.includes("--self-check")) {
  check(
    "SELF-CHECK: this assertion is false on purpose",
    false,
    "if this run exits 0, the harness cannot fail",
  );
}

if (failures.length > 0) {
  console.error(
    `${failures.length} of ${checks} checks failed:\n\n  ${
      failures.join("\n  ")
    }\n`,
  );
  process.exit(1);
}
console.log(`${checks} checks passed`);
