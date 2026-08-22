/**
 * The receipt: what was here **before** this installer touched anything.
 *
 * It records prior state, not inferred state. The alternative — deducing on
 * uninstall what an install must have written — is safe right up until the user
 * edits a value, at which point it deletes something it never created. Every
 * entry is written unconditionally, whether or not something is currently at
 * the path; gating the record on what is there now is how you end up with a
 * receipt that cannot undo its own install.
 */
import {
  readJson,
  writeJson,
} from "../_shared/io.js";
import type { Paths } from "../_shared/paths.js";

/**
 * A file we wrote. `existedBefore: false` means we created it, so uninstall
 * deletes it.
 *
 * There is deliberately no record of where a migrated file came from. Uninstall
 * removes this project's files and puts nothing else back — reviving the JS
 * bar's `statusline.json` years after it stopped being read would be this
 * installer resurrecting another tool's config, not undoing its own work.
 */
export interface FileEntry {
  kind: "file";
  path: string;
  /** The digest we wrote, so uninstall can tell "untouched" from "user-edited". */
  sha256?: string;
  existedBefore: boolean;
}

/** A directory we may have created. Removed on uninstall only if empty. */
export interface DirEntry {
  kind: "dir";
  path: string;
  existedBefore: boolean;
}

/** A `settings.json` key. `previous: null` means it was absent — delete, don't default. */
export interface ConfigKeyEntry {
  kind: "configKey";
  file: string;
  key: string;
  previous: unknown | null;
}

export type Entry = FileEntry | DirEntry | ConfigKeyEntry;

export interface Receipt {
  version: string;
  installedAt: string;
  entries: Entry[];
  /**
   * The user said no to removing the `ai-plugins` leftovers. Remembered so a
   * second `--install` does not re-ask — a prompt asked on every upgrade is a
   * prompt that stops being read.
   */
  declinedOrphans?: boolean;
}

export function read(paths: Paths): Receipt | null {
  const receipt = readJson<Receipt>(paths.receipt);
  if (!receipt || !Array.isArray(receipt.entries)) {
    return null;
  }
  return receipt;
}

export function write(
  paths: Paths,
  version: string,
  entries: Entry[],
  now: string,
  declinedOrphans = false,
): void {
  const receipt: Receipt = { version, installedAt: now, entries };
  if (declinedOrphans) {
    receipt.declinedOrphans = true;
  }
  writeJson(paths.receipt, receipt);
}
