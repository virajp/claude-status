+++
title = "Per-repo settings"
description = "One key, one file, written by hand — and nothing creates it for you."
weight = 4
+++

## You probably do not need this file

The `project` segment names your repository whether or not this file exists.
With no configuration anywhere, it draws the **git root's own directory name** —
work in `~/src/my-repo` and the bar reads `my-repo`.

So this file exists for one purpose: calling a repository something **other**
than its directory name. If the directory is already called the right thing,
there is nothing to write.

The segment is omitted in exactly one case — you are not inside a git
repository, so there is no root to take a name from.

## The file

```text
<repo-root>/.config/claude-status.json
```

`<repo-root>` is the git root of the repository you are working in — the
directory that contains `.git`, not the directory you happen to have `cd`'d
into. Below it, `.config/claude-status.json`.

**Nothing creates this file.** Not `--configure`, not the render, not a first
run. If you want one, you write it:

```json
{
  "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
  "projectName": "my-repo"
}
```

That is the whole thing. The `$schema` line is optional and buys you editor
completion; `projectName` is the only key that does anything.

## What it does

`projectName` is the name the `project` segment draws — with the
`symbols.project` glyph in front of it — for that repository and no other. It
overrides the directory name; that is all it does.

The name is resolved in this order, first match winning:

1. `projectName` in **this file** — that repository, and no other.
2. `projectName` in your **user** config. This is not inert: the user layer is
   merged whole, so a name set there applies to every repository that has not
   named itself. Setting it there is almost never what you want, and
   [Configure](@/configure.md) says so.
3. The **git root's directory name**.

It is the only key this file may set. It is not in the shipped defaults, because
a default name would be a name that was never about your repository.

## Every other key is ignored

Not merged, not partially honoured — dropped. A repository you cloned cannot
change how your bar looks, cannot raise your caps, and cannot repaint your
segments.

And it does not fail silently. `claude-status --doctor` names each key it
dropped:

```text
CONFIG LAYERS (low to high)
  embedded loaded         <embedded>
  user     using defaults ~/.config/claude-status/config.json (no file)
  repo     loaded         /path/to/repo/.config/claude-status.json
           ignored        caps — a repo layer may set projectName only
```

## No git root, no repo layer

If the directory you are in is not inside a git repository, there is no repo
root to look under and the layer simply does not exist. `--doctor` says so:

```text
repo     using defaults <no git root>
```

That is normal. The bar renders from the defaults and your user config, and the
`project` segment sits out.

## Committing it

The file is ordinary repository content — commit it and everyone who clones the
repository gets the same name in their bar, which is usually the point. Nothing
in it can affect anything else about their setup, which is why committing it is
safe in a way a general-purpose config file would not be.

See [Configure](@/configure.md) for the layer above it, and
[Segments](@/segments.md) for what `project` draws.
