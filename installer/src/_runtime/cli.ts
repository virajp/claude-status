/**
 * The argument surface: three things and nothing else.
 *
 * A bare invocation prints help and **mutates nothing**. `~/.claude` is not a
 * directory to touch on a half-remembered command.
 */
import { say } from "../_shared/io.js";
import { install } from "./install.js";
import { uninstall } from "./uninstall.js";

export type Command = "help" | "install" | "uninstall";

export function parse(argv: string[]): Command {
  const args = new Set(argv);
  // Install and uninstall together is a contradiction, not a sequence.
  if (args.has("--uninstall") && args.has("--install")) {
    return "help";
  }
  if (args.has("--uninstall")) {
    return "uninstall";
  }
  if (args.has("--install")) {
    return "install";
  }
  return "help";
}

export function helpText(version: string): string {
  return `claude-status ${version} — a fast powerline status line for Claude Code

USAGE
  npx @askviraj/claude-status --install     install the status line
  npx @askviraj/claude-status --uninstall   remove it and restore what was there
  npx @askviraj/claude-status --help        this help

WHAT --install DOES
  ~/.claude/bin/claude-status        the binary Claude Code runs
  ~/.config/claude-status.json       your config, seeded if absent
                                     (migrated from statusline.json if found)
  ~/.claude/settings.json            adds statusLine and subagentStatusLine
  ~/.config/claude-status/           a receipt of what was there before

  Replacing a status line this installer did not write asks first, and needs a
  terminal to ask in.

WHAT --uninstall DOES
  Restores every key and file the receipt recorded, then removes the binary.
  A config you have edited since installing is left alone.

The installed binary is what renders the bar — this package is only ever the
installer, and never sits on the render path.
`;
}

export async function run(
  version: string,
  argv: string[],
  env: NodeJS.ProcessEnv = process.env,
): Promise<number> {
  switch (parse(argv)) {
    case "install":
      return install(version, env);
    case "uninstall":
      return uninstall(env);
    case "help":
      say(helpText(version));
      return 0;
  }
}
