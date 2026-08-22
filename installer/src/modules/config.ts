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

import { createHash } from "node:crypto";
import { copyFileSync } from "node:fs";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  sha256,
  writeFileText,
} from "../_shared/io.js";
import type { Paths } from "../_shared/paths.js";

export type ConfigAction =
  | { action: "kept"; }
  /** `discarded` names a legacy file that held nothing worth migrating. */
  | { action: "seeded"; sha256: string; discarded?: string; }
  | { action: "migrated"; from: string; sha256: string; added: string[]; };

/**
 * The published schema for **this** repo's config format.
 *
 * A migrated file arrives pointing at the `ai-plugins` schema the JS bar used,
 * and keeping that URL under the new name means the file is validated against
 * the wrong document for the rest of its life — an editor then flags every key
 * this format added and misses every one it dropped. So a migration rewrites
 * it, which is exactly why a migration cannot be a plain rename.
 */
export const SCHEMA_URL =
  "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json";

/**
 * Adds the template's missing **top-level** keys to a migrated config.
 *
 * Top-level only, deliberately: a user who has customised `segments.cost` has
 * an opinion about that whole object, and reaching inside it to add the keys
 * they left out would be the installer second-guessing an edit rather than
 * filling a gap. A key they have never seen is a gap; a key they trimmed from
 * an object they own is not.
 *
 * Nothing is ever overwritten — only absent keys are added. `projectName` needs
 * no special case here: it is repo-level only and the template does not carry
 * it, which `the template carries no projectName` in the installer suite pins.
 */
export function topUp(
  migrated: Record<string, unknown>,
  template: Record<string, unknown>,
): { config: Record<string, unknown>; added: string[]; } {
  const config = { ...migrated };
  const added: string[] = [];

  for (const key of Object.keys(template)) {
    if (key in config) {
      continue;
    }
    config[key] = template[key];
    added.push(key);
  }

  return { config, added };
}

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
 * - Only the legacy name there → **migrated**: `$schema` repointed at this
 *   repo, the template's missing top-level keys added, the old file removed.
 *   Never a rename — a file left pointing at the `ai-plugins` schema is
 *   validated against the wrong document for the rest of its life.
 * - A legacy file that is not a JSON object → **discarded** for the seed. There
 *   is no key to set `$schema` on, and the JS bar could not parse it either.
 * - Neither → seeded from the shipped defaults.
 */
export function seedOrMigrate(paths: Paths, dryRun = false): ConfigAction {
  if (existsSync(paths.config)) {
    return { action: "kept" };
  }

  const legacy = existsSync(paths.legacyConfig)
    ? migratedContent(paths.legacyConfig)
    : null;

  // A dry run answers the same question without touching anything. The digests
  // are the ones the real run would record — for a migration that means
  // hashing the bytes the top-up *would* write, not the legacy file's, or the
  // report would promise a file the real run does not produce.
  if (dryRun) {
    if (legacy === null || !legacy.usable) {
      return {
        action: "seeded",
        sha256: sha256(defaultsPath()),
        ...(legacy === null ? {} : { discarded: paths.legacyConfig }),
      };
    }
    return {
      action: "migrated",
      from: paths.legacyConfig,
      sha256: digestOf(legacy.text),
      added: legacy.added,
    };
  }

  // `~/.config` need not exist yet — this may be the first tool to want it.
  mkdirSync(dirname(paths.config), { recursive: true });

  if (legacy !== null && legacy.usable) {
    // Written under the new name first, then the old one removed, so an
    // interrupted migration leaves the legacy file rather than neither.
    writeFileText(paths.config, legacy.text);
    rmSync(paths.legacyConfig, { force: true });
    return {
      action: "migrated",
      from: paths.legacyConfig,
      sha256: sha256(paths.config),
      added: legacy.added,
    };
  }

  const defaults = defaultsPath();
  if (!existsSync(defaults)) {
    throw new Error(`the package is missing its defaults at ${defaults}`);
  }
  // A byte copy, not a parse-and-write: re-encoding is how a Nerd Font glyph
  // gets lost in transit.
  copyFileSync(defaults, paths.config);
  if (legacy !== null) {
    rmSync(paths.legacyConfig, { force: true });
  }
  return {
    action: "seeded",
    sha256: sha256(paths.config),
    ...(legacy === null ? {} : { discarded: paths.legacyConfig }),
  };
}

/**
 * What a migrated config should end up holding: itself with `$schema` repointed
 * at this repo, plus the template's missing top-level keys.
 *
 * `usable: false` means the legacy file is **not a JSON object** — hand-broken,
 * an array, a bare scalar. There is no key to set `$schema` on, so nothing in
 * it can be made to conform, and the caller discards it for the seed. Nothing
 * working is lost: the JS bar could not parse that file either, so it was never
 * configuring anything.
 */
function migratedContent(
  path: string,
): { text: string; added: string[]; usable: boolean; } {
  const original = readFileSync(path, "utf8");

  let parsed: unknown;
  try {
    parsed = JSON.parse(original);
  }
  catch {
    return { text: original, added: [], usable: false };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { text: original, added: [], usable: false };
  }

  const template = JSON.parse(readFileSync(defaultsPath(), "utf8")) as Record<
    string,
    unknown
  >;
  const { config, added } = topUp(
    { ...parsed as Record<string, unknown>, $schema: SCHEMA_URL },
    template,
  );

  // Byte-identical output when nothing actually changed — a file already
  // carrying this schema and every template key is left exactly as written,
  // rather than reformatted for no reason.
  const rewritten = `${JSON.stringify(config, null, 2)}\n`;
  const unchanged = added.length === 0
    && (parsed as Record<string, unknown>)["$schema"] === SCHEMA_URL;

  return unchanged
    ? { text: original, added, usable: true }
    : { text: rewritten, added, usable: true };
}

/** The digest of text that has not been written yet, for the dry run. */
function digestOf(text: string): string {
  return createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex");
}

/** Reads a file's bytes as text, for the tests and the report. */
export function read(path: string): string {
  return readFileSync(path, "utf8");
}

export { writeFileText };
