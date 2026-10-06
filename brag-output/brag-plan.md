# brag plan — claude-status

**What it is:** a powerline status bar for Claude Code, one Rust binary.
**For:** people who run long Claude Code sessions and keep wondering how much
context, rate limit and money they have left. **Sets it apart:** everything on
one bar, live — and a caps hook that tells Claude itself to hand off and stop
when you cross a line. **Visual hook:** the real bar snapping in under a Claude
Code prompt, segment by segment. **Real material:** every bar, subagent row and
hook message in the video is the installed `claude-status` binary's actual
output (ANSI captured, converted to HTML), in the site's own fonts and glyph
subset. **Tone:** `default`, leaning polished — ink surfaces, one amber accent,
IBM Plex Mono, gruvbox powerline colours as product data. **Share caption:**
"claude-status: context, both rate-limit windows, cost and branch under every
Claude Code prompt — and a hook that makes Claude hand off before it runs past
your caps."

## Storyboard (21.5s, 1920×1080, 30fps)

| #           | Time      | Scene                                                                                          | On-screen text                                                                |
| ----------- | --------- | ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| 1 Hook      | 0.0–3.0   | Terminal window, a Claude Code turn; the real bar builds under it one segment at a time        | "Know what your Claude Code session is up to."                                |
| 2 Reveal    | 3.0–7.0   | Bar lifts out, scales to full width; labels land under each segment                            | "One bar. Everything you keep checking." + labels                             |
| 3 Live      | 7.0–11.2  | Back in the terminal; bar steps through six real renders — gauge fills, 5h climbs, cost ticks  | "Updates as the session burns."                                               |
| 4 Caps      | 11.2–15.4 | Context passes 65%; the real `--caps-hook` directive types into the transcript in an amber box | "Cross a cap, and it tells Claude to hand off and stop."                      |
| 5 Subagents | 15.4–18.0 | Three real subagent rows slide in under the bar                                                | "A row for every subagent."                                                   |
| 6 Outro     | 18.0–21.5 | Lockup, tagline, URL, install routes                                                           | "One Rust binary. No runtime." / claude-status.virajp.dev / brew · mise · npx |

Transitions: soft — stagger out then in, no muddy crossfades. Sound: synthesized
warm pulse bed (A minor, 120bpm), soft ticks in key on segment pops, a low swell
on the cap hit, resolve chord on the outro.
