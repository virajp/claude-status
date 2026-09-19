# U4 — Re-cut the site's glyph font subset for the three new codepoints

- **Wave:** 2
- **Depends on:** U2
- **Owns:** `site/static/fonts/claude-status-glyphs.woff2`,
  `site/static/style.css`, `tests/site.rs`
- **Model:** opus
- **Kind:** edit
- **Read first:** every owned file, top to bottom, before editing — in
  `style.css` the recipe comment at `:84-93` and the `@font-face` block with its
  `unicode-range` at `:106-128`; in `tests/site.rs` the comment at `:1010-1020`
  and the woff2 header check that follows it.
- **Lazy-load:** `assets/claude-status.defaults.json` as U2 left it (read the
  codepoints from it — never hand-list them).

## Ruling

> 9 — Font subset: U4 re-cuts `site/static/fonts/claude-status-glyphs.woff2`
> with
> `uvx --from fonttools pyftsubset
> ~/Library/Fonts/HackNerdFontMono-Regular.ttf --unicodes=<comma-joined
> U+XXXX list of every distinct codepoint under symbols and typeSymbols in
> the defaults asset> --flavor=woff2 --output-file=…`,
> updates `unicode-range` in `style.css` to the same list, and rewrites the "25
> glyphs, 3.2KB" figures in `style.css:84-93` and `tests/site.rs:1017` to the
> new count and size. Rejected: a `site:glyphs` mise task; a manual
> after-landing step.

The user's words: "Unit re-cuts it via `uvx fonttools`".

## Edits

1. **Derive the codepoint list** from the asset U2 committed — one script, e.g.
   `python3 -c` over `json.load(...)`, collecting every character of every value
   under `symbols` and `typeSymbols`, keeping only codepoints ≥ U+E000 (the
   private-use and supplementary-PUA glyphs; ASCII values like the `reset`
   marker are not font glyphs), de-duplicated and sorted, printed as `U+XXXX`
   (five hex digits where needed, e.g. `U+F0BA0`). Record the count. Expect the
   old 25 minus nothing plus E702, F09B, F0BA0 = 28; if the count differs, say
   why in `DECIDED:` — the asset is the authority, not this expectation.
2. **`site/static/fonts/claude-status-glyphs.woff2`** — overwrite by running
   `uvx --from fonttools pyftsubset
   "$HOME/Library/Fonts/HackNerdFontMono-Regular.ttf"
   --unicodes=<the list, comma-joined> --flavor=woff2 --no-hinting
   --desubroutinize --output-file=site/static/fonts/claude-status-glyphs.woff2`.
   Then
   `uvx --from fonttools ttx -l site/static/fonts/claude-status-glyphs.woff2`
   and confirm the glyph count is the list's length + 1 (`.notdef`). Record the
   resulting byte size (`wc -c`).
3. **`site/static/style.css`**
   - Replace the `unicode-range` value (`:106-128`) with the derived list, one
     codepoint per line in the existing layout, same ordering rule the file uses
     today (ascending).
   - In the recipe comment (`:84-93`), replace "25 glyphs, 3.2KB" with the new
     count and size (one decimal of KB, as today). Leave the rest of the comment
     — the reason, the Nerd Fonts version, the READ FROM THE DEFAULTS sentence —
     untouched.
4. **`tests/site.rs`** — in the comment at `~:1017`, replace "25 glyphs and
   3.2KB" with the new figures. No assertion changes: the woff2 header check is
   what proves the file.

## Verification

- `cargo test --features schema site` green — the woff2 header check passes on
  the new file.
- `mise run site:build` green.
- `uvx --from fonttools ttx -l site/static/fonts/claude-status-glyphs.woff2 |
  grep -c 'uni'`
  (or the `ttx` glyph table) equals the count written into `style.css`.
- `grep -c 'U+E702\|U+F09B\|U+F0BA0' site/static/style.css` → 3.
- `mise run code:all` green (dprint formats `style.css`? — check `dprint.json`;
  if CSS is in scope, run `mise run code:format` before returning).

## Guardrails

- Do not touch `assets/**`, `src/**`, `site/content/**`, `site/templates/**` —
  other units own them.
- Read the codepoints from the asset; never hand-type a list. That is the rule
  the recipe comment states.
- Do not commit a font cut from anything but `HackNerdFontMono-Regular.ttf` —
  the comment names the source.
- Delete with `rm`, never `git rm`.

## Commit

`ops(site): re-cut the glyph font subset for the project-kind glyphs` — written
by the orchestrator after the wave gate, not by the unit.
