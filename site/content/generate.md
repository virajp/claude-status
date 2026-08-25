+++
title = "Generate a config"
description = "A form built from the schema, and a file containing only what you changed."
weight = 3
template = "generate.html"
+++

## The form

<div id="config-generator">
  <p>The interactive form needs JavaScript. Everything it would do for you is
  written out below — where the file goes, what belongs in it, what every key
  is, and the three rules that decide what the download contains — so this page
  is a complete reference either way.</p>
</div>

## Where it goes

```text
~/.config/claude-status/config.json
```

That is the **user** layer, the middle of the three. See
[Configure](@/configure.md) for how the layers merge, and
[Per-repo](@/repo-config.md) for the one key that belongs somewhere else.

Nothing creates this file for you except `claude-status --configure`, which
writes exactly the same file the generator produces.

## Three rules decide what comes out

Read these before you wonder why the download looks smaller than the form.

### 1. Only what you changed

The file carries `$schema` and every value that differs from the shipped
defaults. Nothing else. A key you leave alone **follows the binary forward**
when you upgrade — that is the entire point, and a generator that handed you a
full config would freeze today's defaults on your machine forever.

So a file with one line in it is the normal outcome, not a mistake:

```json
{
  "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
  "defaultFg": "aqua"
}
```

Objects are compared **key by key, at every depth**. Change one palette colour
and you get that one colour, not the whole palette.

### 2. Lists come out whole

Arrays replace wholesale when the layers merge, so a list has to be written out
complete or the parts you did not touch would vanish. Reorder one segment in
`lines` and the download carries **both** rows, including the row you never
opened. That looks like rule 1 breaking; it is rule 1 being kept.

### 3. "Remove" means "revert to shipped"

A config file has no way to say *delete this key* — the merge has no delete
operator, so `{"palette": {}}` does nothing at all. Removing a row that the
defaults ship therefore restores the shipped value, and the button says so. A
row **you** added is genuinely removable, because removing it just stops it
being written.

The one thing you can clear is a **colour** or a **bold**: setting either to
`null` is how you turn off something the defaults switched on, and it is what
the "clear" mode emits.

## Colours

A colour is one of four things, anywhere one is expected:

| Form             | Example               | Use it when                                    |
| ---------------- | --------------------- | ---------------------------------------------- |
| **palette name** | `"aqua"`              | almost always — it follows the palette forward |
| **hex string**   | `"#d79921"`, `"#fa0"` | you want one colour that is not in the palette |
| **RGB triple**   | `[215, 153, 33]`      | the same, written as numbers                   |
| **`null`**       | `null`                | to clear a colour the defaults set             |

**Prefer the name.** The shipped defaults reference colours by name, so a
segment set to `"aqua"` changes when you change `palette.aqua`, and a segment
set to `[104, 157, 106]` never does again.

`null` makes a segment fall through to `defaultFg`, which is the only way to
undo a shipped colour.

## The keys

Every key, its shape, and what it does is on the [Configure](@/configure.md)
page, and the generator is built from the
[published schema](https://github.com/virajp/claude-status/blob/main/schemas/claude-status.schema.json)
rather than from a list written here — so the two cannot drift. Each field shows
the schema's own description beside it.

Five of the keys are **open maps**: you choose the keys as well as the values.

| Key                 | Keys are                      | Values are                |
| ------------------- | ----------------------------- | ------------------------- |
| `palette`           | colour names you invent       | `[r, g, b]` triples       |
| `symbols`           | the glyph slots the bar reads | single glyphs             |
| `typeSymbols`       | subagent `type` values        | single glyphs             |
| `segments`          | segment ids                   | `bg` / `fg` / `bold`      |
| `subagent.statuses` | bucket names you invent       | `match` / `symbol` / `bg` |

`subagent.statuses` is the odd one: **order is behaviour**. Buckets are tried
top to bottom and the first whose `match` regex hits the task status wins; the
entry with an empty `match` is the fallback.

Three key names can never take effect — `__proto__`, `constructor` and
`prototype` are dropped by the config merge at every depth, and the form refuses
them.

## Per-repo settings

The repo layer is not a form, because it takes one key. Write this at
`<repo-root>/.config/claude-status.json` by hand:

```json
{
  "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
  "projectName": "my-repo"
}
```

`$schema` is optional and buys editor completion; `projectName` is the only key
a repo layer may set, and every other key in that file is ignored and named by
`--debug`. [Per-repo](@/repo-config.md) has the rest.

`projectName` appears in the generator because it is in the schema. Setting it
there puts it in your **user** config, where it does nothing — the field's own
description says so, and this is the one key worth ignoring.

## No preview yet

This page does not draw your bar. Drawing it would mean a second implementation
of the renderer in JavaScript, and a preview that is plausible but wrong is
worse than no preview — wrong exactly where you are doing something unusual,
which is when you are most likely to be here.

To see the real thing, save the file and run:

```sh
claude-status --debug
```

which prints the resolved config, every layer that fed it, and the bar itself.
See [Diagnosing](@/diagnosing.md).
