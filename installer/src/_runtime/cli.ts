/**
 * The argument surface: three things and nothing else.
 *
 * A bare invocation prints help and **mutates nothing**. `~/.claude` is not a
 * directory to touch on a half-remembered command.
 */
import { say } from "../_shared/io.js";
import { install } from "./install.js";
import { uninstall } from "./uninstall.js";

export type Command = "help" | "install" | "uninstall" | "version";

/** The modifiers, resolved once and threaded through every mutation. */
export interface Options {
  /** Report every intended change and touch nothing. */
  dryRun: boolean;
  /** Treat every prompt as accepted — the non-interactive yes. */
  yes: boolean;
  /** Replace a status line this installer did not write, without asking. */
  force: boolean;
}

export function parse(argv: string[]): Command {
  const args = new Set(argv);
  // `--version` first and undecorated, for the same reason the binary's is.
  if (args.has("--version")) {
    return "version";
  }
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

export function options(argv: string[]): Options {
  const args = new Set(argv);
  return {
    dryRun: args.has("--dry-run"),
    yes: args.has("--yes") || args.has("-y"),
    force: args.has("--force"),
  };
}

export function helpText(version: string): string {
  return `claude-status ${version} — a fast powerline status line for Claude Code

USAGE
  npx @askviraj/claude-status --install     install the status line
  npx @askviraj/claude-status --uninstall   remove it and restore what was there
  npx @askviraj/claude-status --help        this help
  npx @askviraj/claude-status --version     print this installer's version

MODIFIERS
  --dry-run   report every change and touch nothing
  --yes, -y   treat prompts as accepted; needed when there is no terminal
  --force     replace a status line this installer did not write, without asking

WHAT --install DOES
  ~/.claude/bin/claude-status        the binary Claude Code runs
  ~/.config/claude-status.json       your config, seeded if absent
                                     (migrated from statusline.json if found)
  ~/.claude/settings.json            adds statusLine, subagentStatusLine and
                                     the PostToolUse caps hook
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
  const opts = options(argv);
  switch (parse(argv)) {
    case "install":
      return install(version, env, opts);
    case "uninstall":
      return uninstall(env, opts);
    case "version":
      // The installer's version. The **binary's** `--version` is a different
      // answer and must stay undecorated — that shape is how an installed
      // binary is told from a bundled one.
      say(version);
      return 0;
    case "help":
      say(helpText(version));
      return 0;
  }
}
