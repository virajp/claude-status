/**
 * `--configure`: give the repo you are standing in a repo-level config layer.
 *
 * This is the only command scoped to a directory rather than to `$HOME`, and
 * the only one that writes nothing under `~`. It is deliberately **not**
 * recorded in the receipt: the file it writes lives in the repo, is meant to be
 * committed, and git is a better undo than an installer that would have to
 * remember every repo it had ever been run in.
 *
 * Nothing here prompts. The name is derived rather than asked for, and no case
 * overwrites a value the user set — so there is no question to put to them, and
 * `--yes` has nothing to answer.
 */
import { dirname } from "node:path";

import {
  existsSync,
  fail,
  mkdirSync,
  readJson,
  rmSync,
  say,
  step,
  warn,
  wouldStep,
  writeJson,
} from "../_shared/io.js";
import {
  resolvePaths,
  tilde,
} from "../_shared/paths.js";
import * as repo from "../modules/repo.js";
import type { Options } from "./cli.js";

const NO_OPTIONS: Options = { dryRun: false, yes: false, force: false };

/** `projectName` counts as set only when it is a non-empty string — the same
 * test the binary applies before it will draw the segment. */
function hasProjectName(config: Record<string, unknown>): boolean {
  const name = config["projectName"];
  return typeof name === "string" && name.length > 0;
}

export function configure(
  env: NodeJS.ProcessEnv = process.env,
  opts: Options = NO_OPTIONS,
  cwd: string = process.cwd(),
): number {
  // Only for rendering `~` in the output — this command writes nothing here.
  const home = resolvePaths(env);
  const did = (message: string) =>
    opts.dryRun ? wouldStep(message) : step(message);

  const found = repo.findRepoRoot(cwd);
  if (!found.ok) {
    if (found.reason === "git-missing") {
      fail(
        "git is not on PATH, and --configure asks git where the repo starts.\n"
          + "  Install git, or write the file by hand at <repo-root>/.config/claude-status.json.",
      );
    }
    fail(
      "--configure writes a repo-level config, so run it from inside a repo.\n"
        + `  ${tilde(cwd, home)} is not in a git working tree.\n`
        + "  cd into any repo and run it again. Nothing has been changed.",
    );
  }

  const paths = repo.repoPaths(found.root);
  const name = repo.projectNameFor(found.root);

  say(
    opts.dryRun
      ? `Dry run — configuring ${paths.root}. Nothing will be changed.`
      : `Configuring ${paths.root}`,
  );

  // 1. A config already there is the user's. Fill in `projectName` if it is
  //    missing, and leave every other key exactly as written.
  if (existsSync(paths.config)) {
    const current = readJson<Record<string, unknown>>(paths.config);
    if (
      current === null || typeof current !== "object" || Array
        .isArray(current)
    ) {
      fail(
        `${paths.config} is not a JSON object.\n`
          + "  Refusing to overwrite it — fix or remove it and run --configure again.",
      );
    }

    if (hasProjectName(current)) {
      step(`config   ${paths.config} (kept — projectName already set)`);
    }
    else {
      current["projectName"] = name;
      if (!opts.dryRun) {
        writeJson(paths.config, current);
      }
      did(`config   ${paths.config} (projectName → ${name})`);
    }

    if (existsSync(paths.legacyConfig)) {
      warn(
        `${paths.legacyConfig} also exists and was left alone — `
          + "remove it once you are happy with the new one",
      );
    }
    return report(paths.config, opts);
  }

  // 2. Only the legacy name — migrate it. **Not** a rename: the file carries
  //    the JS bar's `$schema`, and one kept under that URL is validated against
  //    the wrong schema for the rest of its life. Every other key is carried
  //    across untouched.
  //
  //    That carrying no longer preserves anything, and the comment here used to
  //    claim it did ("so the theming survives"). This writes the **repo-level**
  //    layer, and since the `config-relocation` cycle that layer may set
  //    `projectName` and nothing else — so a migrated `defaultFg`, `palette` or
  //    `segments` lands in a file the binary reads, ignores, and reports under
  //    `--debug` as a dropped key. The keys are preserved as *bytes*, not as
  //    behaviour.
  //
  //    Left as-is deliberately rather than narrowed to `projectName`: dropping
  //    a user's keys silently would be worse than carrying inert ones they can
  //    see and delete, and this whole module goes with the installer in
  //    `docs/plans/2026-08-23-distribution/01-drop-npm.md`.
  if (existsSync(paths.legacyConfig)) {
    const carried = readJson<Record<string, unknown>>(paths.legacyConfig);
    const isObject = carried !== null
      && typeof carried === "object"
      && !Array.isArray(carried);

    // Not an object: there is no key to set `$schema` on, so nothing here can
    // be made to conform. It is discarded for the seed — the JS bar could not
    // parse this file either, so it was never configuring anything.
    const migrated: Record<string, unknown> = isObject
      ? { ...carried, $schema: repo.SCHEMA_URL }
      : { $schema: repo.SCHEMA_URL };
    if (!isObject) {
      did(
        `config   ${paths.legacyConfig} is not a JSON object — discarded for a fresh config`,
      );
    }
    const named = hasProjectName(migrated);
    if (!named) {
      migrated["projectName"] = name;
    }

    if (!opts.dryRun) {
      mkdirSync(dirname(paths.config), { recursive: true });
      writeJson(paths.config, migrated);
      // Removed only once the new file is on disk, so an interrupted migration
      // leaves the old file rather than neither.
      rmSync(paths.legacyConfig, { force: true });
    }
    did(
      isObject
        ? `config   ${paths.legacyConfig} → ${paths.config} (migrated, $schema repointed)`
        : `config   ${paths.config} (seeded)`,
    );
    if (!named) {
      did(`config   projectName → ${name}`);
    }
    return report(paths.config, opts);
  }

  // 3. Nothing there — seed the minimum. A repo layer is an *override*, so it
  //    carries the name and the schema and nothing else; every other key should
  //    keep coming from the user layer the install seeded.
  if (!opts.dryRun) {
    mkdirSync(dirname(paths.config), { recursive: true });
    writeJson(paths.config, {
      $schema: repo.SCHEMA_URL,
      projectName: name,
    });
  }
  did(`config   ${paths.config} (seeded, projectName → ${name})`);
  return report(paths.config, opts);
}

/** The closing line, identical whichever of the three routes got here. */
function report(config: string, opts: Options): number {
  say("");
  say(
    opts.dryRun
      ? "Dry run complete. Nothing was changed."
      : `Done. Commit ${config} to share it with the repo.`,
  );
  return 0;
}
