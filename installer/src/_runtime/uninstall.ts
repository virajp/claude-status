/**
 * `--uninstall`: put back what was there before.
 *
 * Order is load-bearing. The `settings.json` keys are restored **before** the
 * binary is removed, so a failure part-way through never leaves Claude Code
 * pointing at a file that is already gone.
 *
 * Without a receipt this removes only what it can prove is its own and reports
 * the rest. It never falls back to guessing.
 */
import {
  readdirSync,
  rmdirSync,
} from "node:fs";
import {
  existsSync,
  readFileSync,
  renameSync,
  rmSync,
  say,
  sha256,
  step,
  warn,
} from "../_shared/io.js";
import {
  resolvePaths,
  tilde,
} from "../_shared/paths.js";
import type {
  ConfigKeyEntry,
  DirEntry,
  FileEntry,
} from "../modules/receipt.js";
import * as receipt from "../modules/receipt.js";
import * as settings from "../modules/settings.js";

export function uninstall(env: NodeJS.ProcessEnv = process.env): number {
  const paths = resolvePaths(env);
  const found = receipt.read(paths);

  if (!found) {
    return withoutReceipt(paths);
  }

  say(`Uninstalling claude-status ${found.version}`);

  // 1. Config keys first — see the note above.
  for (
    const entry of found.entries.filter((e): e is ConfigKeyEntry =>
      e.kind === "configKey"
    )
  ) {
    const current = settings.readSettings(entry.file);
    if (entry.previous === null) {
      // It was absent before we ran, so absent is what "restored" means.
      delete current[entry.key];
      step(`removed  ${entry.key}`);
    }
    else {
      current[entry.key] = entry.previous;
      step(`restored ${entry.key}`);
    }
    settings.writeSettings(entry.file, current);
  }

  // 2. Files.
  for (
    const entry of found.entries.filter((e): e is FileEntry =>
      e.kind === "file"
    )
  ) {
    if (entry.movedFrom) {
      // Migrated in, so move it back under its old name rather than deleting a
      // file that carries the user's theming.
      if (existsSync(entry.path)) {
        renameSync(entry.path, entry.movedFrom);
        step(`restored ${tilde(entry.movedFrom, paths)}`);
      }
      continue;
    }

    if (entry.existedBefore) {
      step(`kept     ${tilde(entry.path, paths)} (predates this install)`);
      continue;
    }

    if (!existsSync(entry.path)) {
      continue;
    }

    // Something we seeded. If the user has since edited it, that is their work
    // and deleting it would be the installer overreaching.
    if (entry.sha256 && sha256(entry.path) !== entry.sha256) {
      step(`kept     ${tilde(entry.path, paths)} (edited since install)`);
      continue;
    }

    rmSync(entry.path, { force: true });
    step(`removed  ${tilde(entry.path, paths)}`);
  }

  // The receipt goes before the directories are considered, or the very
  // directory holding it would always look non-empty and always be kept.
  rmSync(paths.receipt, { force: true });

  // 3. Directories, only when empty — the user shares `~/.claude/bin`.
  //    Deepest first, so a parent is considered only after its children have
  //    had their chance to disappear.
  const dirs = found
    .entries
    .filter((e): e is DirEntry => e.kind === "dir")
    .sort((a, b) => b.path.length - a.path.length);

  for (const entry of dirs) {
    if (entry.existedBefore || !existsSync(entry.path)) {
      continue;
    }
    try {
      if (readdirSync(entry.path).length === 0) {
        rmdirSync(entry.path);
        step(`removed  ${tilde(entry.path, paths)}`);
      }
      else {
        step(`kept     ${tilde(entry.path, paths)} (not empty)`);
      }
    }
    catch {
      // A directory we cannot read is a directory we leave alone.
    }
  }

  say("");
  say("Done.");
  return 0;
}

/**
 * No receipt — a hand-deleted one, or an install from before receipts existed.
 * Remove the one thing whose identity is unambiguous and report everything else
 * rather than inferring.
 */
function withoutReceipt(paths: ReturnType<typeof resolvePaths>): number {
  warn(
    `no receipt at ${
      tilde(paths.receipt, paths)
    } — removing only what is unambiguously ours`,
  );

  if (existsSync(paths.binary)) {
    rmSync(paths.binary, { force: true });
    step(`removed  ${tilde(paths.binary, paths)}`);
  }
  else {
    step(`absent   ${tilde(paths.binary, paths)}`);
  }

  const current = settings.readSettings(paths.settings);
  for (
    const key of [
      settings.STATUS_LINE,
      settings.SUBAGENT_STATUS_LINE,
    ] as const
  ) {
    const ownership = settings.ownershipOf(current[key]);
    if (ownership === "ours" || ownership === "ours-stale") {
      say(
        `  ${key} still points at claude-status; remove it by hand if you want it gone:`,
      );
      say(`    ${tilde(paths.settings, paths)}`);
    }
  }

  if (existsSync(paths.config)) {
    say(
      `  ${
        tilde(paths.config, paths)
      } was left in place — it may hold your theming.`,
    );
  }
  return 0;
}

export { readFileSync };
