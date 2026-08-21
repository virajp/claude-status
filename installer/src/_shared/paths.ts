/**
 * Every path the installer may touch, resolved in one place.
 *
 * `$HOME` is read directly and joined literally — never a platform config
 * directory, which on macOS resolves to `~/Library/Application Support` and
 * would miss the location the binary itself reads. Taking the environment as a
 * parameter is also what lets the tests point the whole installer at a
 * throwaway directory.
 */
import { homedir } from "node:os";
import { join } from "node:path";

export interface Paths {
  home: string;
  /** `~/.claude` — Claude Code's own directory. */
  claudeDir: string;
  binDir: string;
  binary: string;
  settings: string;
  /** `~/.config` — which this may well be the first tool to create. */
  configDir: string;
  /** `~/.config/claude-status.json` — the user config layer the bar reads. */
  config: string;
  /** `~/.config/statusline.json` — what the JS bar read, migrated on install. */
  legacyConfig: string;
  /** `~/.config/claude-status/` — this installer's own state. */
  stateDir: string;
  receipt: string;
  /** `~/.claude/scripts/statusline` — the JS bar the `ai-plugins` CLI installed. */
  legacyStatusline: string;
  /** `~/.config/ai-plugins/receipts/statusline.json` — that install's receipt. */
  legacyReceipt: string;
  /** `~/.claude/hooks/context-caps.js` — the Node hook `--caps-hook` replaces. */
  legacyHook: string;
}

/** The binary's filename, which carries an extension only on Windows. */
export const BINARY_NAME = process.platform === "win32"
  ? "claude-status.exe"
  : "claude-status";

/**
 * The home directory.
 *
 * `HOME` first — it is what Unix uses, it is what Git Bash sets on Windows, and
 * honouring it is what lets the tests point the installer at a throwaway
 * directory. Then `USERPROFILE`, which is where Claude Code keeps `.claude` on
 * native Windows, and finally the drive/path pair a domain-joined machine may
 * have instead.
 */
export function resolveHome(env: NodeJS.ProcessEnv = process.env): string {
  const home = env["HOME"];
  if (home && home.length > 0) {
    return home;
  }

  if (process.platform === "win32") {
    const profile = env["USERPROFILE"];
    if (profile && profile.length > 0) {
      return profile;
    }

    const drive = env["HOMEDRIVE"];
    const path = env["HOMEPATH"];
    if (drive && path) {
      return `${drive}${path}`;
    }
  }

  return homedir();
}

export function resolvePaths(env: NodeJS.ProcessEnv = process.env): Paths {
  const home = resolveHome(env);
  const claudeDir = join(home, ".claude");
  const configDir = join(home, ".config");
  const stateDir = join(configDir, "claude-status");

  return {
    home,
    claudeDir,
    binDir: join(claudeDir, "bin"),
    binary: join(claudeDir, "bin", BINARY_NAME),
    settings: join(claudeDir, "settings.json"),
    configDir,
    config: join(configDir, "claude-status.json"),
    legacyConfig: join(configDir, "statusline.json"),
    stateDir,
    receipt: join(stateDir, "receipt.json"),
    legacyStatusline: join(claudeDir, "scripts", "statusline"),
    legacyReceipt: join(configDir, "ai-plugins", "receipts", "statusline.json"),
    legacyHook: join(claudeDir, "hooks", "context-caps.js"),
  };
}

/** Renders a path with `~` for display, so output does not leak a username. */
export function tilde(path: string, paths: Paths): string {
  return path.startsWith(paths.home)
    ? `~${path.slice(paths.home.length)}`
    : path;
}
