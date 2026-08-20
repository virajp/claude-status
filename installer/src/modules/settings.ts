/**
 * Merging the two render keys into `~/.claude/settings.json`.
 *
 * Merge, never rewrite: the file holds the user's entire Claude Code
 * configuration and this tool owns two keys of it.
 */
import {
  readJson,
  writeJson,
} from "../_shared/io.js";

export const STATUS_LINE = "statusLine";
export const SUBAGENT_STATUS_LINE = "subagentStatusLine";

export interface CommandKey {
  type?: string;
  command?: string;
  [key: string]: unknown;
}

export type Ownership = "absent" | "ours" | "ours-stale" | "foreign";

/**
 * Who wrote the value currently at a key.
 *
 * `ours-stale` is the important one: a previous install wrote a *flagless*
 * command, which after the move to explicit render flags renders the
 * missing-flag line instead of a bar. It is ours, so it is rewritten without
 * asking — but it is not current, so it cannot simply be left alone.
 */
export function ownershipOf(value: unknown): Ownership {
  if (value === undefined || value === null) {
    return "absent";
  }

  const command = (value as CommandKey)?.command;
  if (typeof command !== "string") {
    return "foreign";
  }
  if (!command.includes("claude-status")) {
    return "foreign";
  }

  return /--statusline|--subagent/.test(command) ? "ours" : "ours-stale";
}

export function desired(
  binary: string,
): { statusLine: CommandKey; subagentStatusLine: CommandKey; } {
  return {
    statusLine: {
      type: "command",
      command: `${binary} --statusline`,
      padding: 0,
      refreshInterval: 4,
    },
    subagentStatusLine: {
      type: "command",
      command: `${binary} --subagent`,
    },
  };
}

export function readSettings(path: string): Record<string, unknown> {
  const settings = readJson<Record<string, unknown>>(path);
  return settings && typeof settings === "object" && !Array.isArray(settings)
    ? settings
    : {};
}

export function writeSettings(
  path: string,
  settings: Record<string, unknown>,
): void {
  writeJson(path, settings);
}
