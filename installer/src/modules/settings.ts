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
export const HOOKS = "hooks";
export const POST_TOOL_USE = "PostToolUse";

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

/**
 * The `PostToolUse` caps hook — a third key, and the only one that **removes**
 * a `node` invocation rather than adding one.
 *
 * Claude Code's shape here is a list of groups, each with its own `hooks` list
 * and an optional `matcher`. This tool owns exactly one command inside it and
 * must leave every other group, matcher and key untouched.
 */
export interface HookCommand {
  type?: string;
  command?: string;
  [key: string]: unknown;
}

export interface HookGroup {
  hooks?: HookCommand[];
  [key: string]: unknown;
}

/** The command an install writes. */
export function desiredHook(binary: string): HookCommand {
  return { type: "command", command: `${binary} --caps-hook` };
}

/**
 * Whether a command is this tool's, in its current or its previous form.
 *
 * The `ai-plugins` installer wired the hook as
 * `node ${HOME}/.claude/hooks/context-caps.js`. That is **ours in its old
 * form** — the same actuator, one process heavier — so it is replaced without
 * asking. Anything else is someone else's hook and is preserved alongside.
 */
export function hookOwnershipOf(command: unknown): Ownership {
  if (typeof command !== "string") {
    return "foreign";
  }
  if (/claude-status.*--caps-hook/.test(command)) {
    return "ours";
  }
  return /context-caps\.js/.test(command) ? "ours-stale" : "foreign";
}

export interface HookWiring {
  /** The `hooks` value to write. */
  hooks: Record<string, unknown>;
  /** What was found where our command now sits. */
  ownership: Ownership;
}

/**
 * Puts our command into the `PostToolUse` list, replacing our own previous
 * form in place when one is there and appending a group when it is not.
 *
 * Replacing **in place** matters: appending while an old form is still present
 * would fire the same actuator twice per tool call.
 */
export function wireHook(
  settings: Record<string, unknown>,
  binary: string,
): HookWiring {
  const hooks = { ...(asObject(settings[HOOKS]) ?? {}) };
  const groups = Array.isArray(hooks[POST_TOOL_USE])
    ? (hooks[POST_TOOL_USE] as HookGroup[]).map(group => ({ ...group }))
    : [];

  let ownership: Ownership = "absent";
  for (const group of groups) {
    if (!Array.isArray(group.hooks)) {
      continue;
    }
    group.hooks = group.hooks.map(entry => {
      const found = hookOwnershipOf(entry?.command);
      if (found === "foreign" || ownership !== "absent") {
        return entry;
      }
      ownership = found;
      return desiredHook(binary);
    });
  }

  if (ownership === "absent") {
    groups.push({ hooks: [desiredHook(binary)] });
  }

  hooks[POST_TOOL_USE] = groups;
  return { hooks, ownership };
}

function asObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}
