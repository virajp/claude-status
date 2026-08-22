/**
 * The argument surface: three things and nothing else.
 *
 * A bare invocation prints help and **mutates nothing**. `~/.claude` is not a
 * directory to touch on a half-remembered command.
 */
import { say } from "../_shared/io.js";
import { configure } from "./configure.js";
import { install } from "./install.js";
import { uninstall } from "./uninstall.js";

export type Command =
  | "configure"
  | "help"
  | "install"
  | "uninstall"
  | "version";

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
  // Two of these together is a contradiction, not a sequence. Counted rather
  // than checked pairwise, so a fourth verb cannot be added without the
  // contradiction rule following it.
  const verbs = (["--install", "--uninstall", "--configure"] as const)
    .filter(flag => args.has(flag));
  if (verbs.length !== 1) {
    return "help";
  }

  if (args.has("--uninstall")) {
    return "uninstall";
  }
  if (args.has("--install")) {
    return "install";
  }
  return "configure";
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
  npx @askviraj/claude-status --uninstall   remove it, restoring the keys it changed
  npx @askviraj/claude-status --configure   add a repo-level config to this repo
  npx @askviraj/claude-status --help        this help
  npx @askviraj/claude-status --version     print this installer's version

MODIFIERS
  --dry-run   report every change and touch nothing
  --yes, -y   treat prompts as accepted; needed when there is no terminal
  --force     replace a status line this installer did not write, without asking

WHAT --install DOES
  ~/.claude/bin/claude-status        the binary Claude Code runs, downloaded
                                     from its GitHub release and checked
                                     against the digest this package pins
  ~/.config/claude-status.json       your config, seeded if absent
                                     (migrated from statusline.json if found,
                                     minus projectName — that is repo-level)
  ~/.claude/settings.json            adds statusLine, subagentStatusLine and
                                     the PostToolUse caps hook
  ~/.config/claude-status/           a receipt of what was there before

  Replacing a status line this installer did not write asks first, and needs a
  terminal to ask in.

WHAT --uninstall DOES
  Restores every settings.json key the receipt recorded, then removes the files
  this installer wrote. A config you have edited since installing is left alone,
  and a statusline.json it migrated is not brought back.

WHAT --configure DOES
  Run it from inside a repo. It writes that repo's config layer, which the bar
  reads on top of your user one:

  <repo-root>/.config/claude-status.json   projectName set to the repo's
                                           directory name
                                           (migrated from statusline.json if
                                           found, keeping its bytes)

  An existing file is kept and only gains projectName if it was missing; a
  projectName you already set is never rewritten. Nothing is recorded in the
  receipt and --uninstall does not touch it — the file belongs to the repo, so
  commit it, and let git be the undo.

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
    case "configure":
      return configure(env, opts);
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
