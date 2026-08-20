/**
 * Seeding the user config, and migrating the one the JS bar used.
 *
 * The binary knows exactly one config name, `claude-status.json`, so it spends
 * no per-render stat looking for a legacy path. That means the rename is the
 * installer's job and nobody else's.
 */
import {
  dirname,
  join,
} from "node:path";
import { fileURLToPath } from "node:url";

import { copyFileSync } from "node:fs";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  sha256,
  writeFileText,
} from "../_shared/io.js";
import type { Paths } from "../_shared/paths.js";

export type ConfigAction =
  | { action: "kept"; }
  | { action: "seeded"; sha256: string; }
  | { action: "migrated"; from: string; sha256: string; };

/**
 * The shipped defaults, carried beside the bundle.
 *
 * Copied verbatim from `assets/claude-status.defaults.json` at package-build
 * time, so there is exactly one source for the file the binary embeds and the
 * file a fresh install is seeded with. Almost every symbol in it is a Nerd Font
 * private-use codepoint, so it is only ever moved as bytes — never re-encoded,
 * never re-typed.
 */
export function defaultsPath(): string {
  return join(
    dirname(fileURLToPath(import.meta.url)),
    "claude-status.defaults.json",
  );
}

/**
 * Ensures `~/.config/claude-status.json` exists.
 *
 * - Already there → left completely alone, including when a legacy file also
 *   exists. Overwriting a config the user is currently using would throw away
 *   their theming to fix a problem they do not have.
 * - Only the legacy name there → **moved**, so the theming survives the rename.
 * - Neither → seeded from the shipped defaults.
 */
export function seedOrMigrate(paths: Paths): ConfigAction {
  if (existsSync(paths.config)) {
    return { action: "kept" };
  }

  // `~/.config` need not exist yet — this may be the first tool to want it.
  mkdirSync(dirname(paths.config), { recursive: true });

  if (existsSync(paths.legacyConfig)) {
    renameSync(paths.legacyConfig, paths.config);
    return {
      action: "migrated",
      from: paths.legacyConfig,
      sha256: sha256(paths.config),
    };
  }

  const defaults = defaultsPath();
  if (!existsSync(defaults)) {
    throw new Error(`the package is missing its defaults at ${defaults}`);
  }
  // A byte copy, not a parse-and-write: re-encoding is how a Nerd Font glyph
  // gets lost in transit.
  copyFileSync(defaults, paths.config);
  return { action: "seeded", sha256: sha256(paths.config) };
}

/** Reads a file's bytes as text, for the tests and the report. */
export function read(path: string): string {
  return readFileSync(path, "utf8");
}

export { writeFileText };
