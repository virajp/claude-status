/**
 * `--install`: put the binary where Claude Code runs it, make sure a config
 * exists, wire the three keys, sweep what the JS install left behind, and
 * record what was there before.
 *
 * Every mutation is guarded by `--dry-run`, which reports the same decisions
 * and performs none of them. That is only trustworthy if the guard is at the
 * write itself rather than around a block, so each one is inline.
 */
import {
  confirm,
  existsSync,
  fail,
  hasTty,
  rmSync,
  say,
  step,
  warn,
  wouldStep,
} from "../_shared/io.js";
import {
  resolvePaths,
  tilde,
} from "../_shared/paths.js";
import type { Paths } from "../_shared/paths.js";
import * as binary from "../modules/binary.js";
import * as config from "../modules/config.js";
import type { Entry } from "../modules/receipt.js";
import * as receipt from "../modules/receipt.js";
import * as settings from "../modules/settings.js";
import type { Options } from "./cli.js";

const NO_OPTIONS: Options = { dryRun: false, yes: false, force: false };

export async function install(
  version: string,
  env: NodeJS.ProcessEnv = process.env,
  opts: Options = NO_OPTIONS,
): Promise<number> {
  const paths = resolvePaths(env);
  const entries: Entry[] = [];
  const did = (message: string) =>
    opts.dryRun ? wouldStep(message) : step(message);

  // 1. The binary. Resolved before anything is written — and before anything
  //    is *said*, so a host this package cannot serve is told so instead of
  //    reading "Installing claude-status" and then an error.
  const resolved = binary.resolvePlatformBinary();
  if (!resolved.ok) {
    if (resolved.reason === "unsupported") {
      fail(
        `unsupported platform ${resolved.host}\n`
          + `  claude-status ships macOS binaries only.\n`
          + `  supported: ${binary.supportedPlatforms().join(", ")}\n`
          + `  npm normally refuses this install outright — reaching this\n`
          + `  message means it was forced past. Nothing has been changed.\n`
          + `  Building from source is the only other option, and it is\n`
          + `  unsupported: https://github.com/virajp/claude-status#requirements`,
      );
    }
    fail(
      `${resolved.package} is not installed\n`
        + `  npm skips optional dependencies on install failure — reinstall with:\n`
        + `    npm install @askviraj/claude-status --force`,
    );
  }

  say(
    opts.dryRun
      ? `Dry run — claude-status ${version}. Nothing will be changed.`
      : `Installing claude-status ${version}`,
  );

  // Every directory this install might bring into being is recorded before it
  // is created, so an uninstall can take an empty one back out. Miss one and
  // "install then uninstall is a no-op" quietly stops being true.
  const dirsBefore = [
    paths.claudeDir,
    paths.binDir,
    paths.configDir,
    paths.stateDir,
  ]
    .map(path => ({
      kind: "dir" as const,
      path,
      existedBefore: existsSync(path),
    }));
  entries.push(...dirsBefore);

  const binaryExisted = existsSync(paths.binary);
  if (!opts.dryRun) {
    binary.install(resolved.path, paths.binary);
  }
  entries.push({
    kind: "file",
    path: paths.binary,
    existedBefore: binaryExisted,
  });
  did(`binary   ${tilde(paths.binary, paths)}`);

  // 2. The config: seeded, migrated from the JS bar's name, or left alone.
  const outcome = config.seedOrMigrate(paths, opts.dryRun);
  switch (outcome.action) {
    case "kept":
      step(`config   ${tilde(paths.config, paths)} (kept)`);
      if (existsSync(paths.legacyConfig)) {
        warn(
          `${
            tilde(paths.legacyConfig, paths)
          } also exists and was left alone — `
            + `remove it once you are happy with the new one`,
        );
      }
      break;
    case "seeded":
      entries.push({
        kind: "file",
        path: paths.config,
        existedBefore: false,
        sha256: outcome.sha256,
      });
      did(`config   ${tilde(paths.config, paths)} (seeded)`);
      // Said out loud, because it is the one path that removes a file without
      // carrying anything out of it. Nothing in the receipt points back at it —
      // the bytes are gone, and an uninstall must not claim otherwise.
      if (outcome.discarded) {
        did(
          `config   discarded ${
            tilde(outcome.discarded, paths)
          } — it was not a JSON object`,
        );
      }
      break;
    case "migrated":
      // Recorded as an ordinary file of ours: uninstall removes it under this
      // name and does not revive the legacy one. The digest is what tells an
      // untouched migration from one the user has since edited.
      entries.push({
        kind: "file",
        path: paths.config,
        existedBefore: false,
        sha256: outcome.sha256,
      });
      did(
        `config   ${tilde(outcome.from, paths)} → ${
          tilde(paths.config, paths)
        } (migrated)`,
      );
      // Said out loud, because it is the one key the migration takes away.
      // `projectName` is repo-level only, and one left in the user layer would
      // name every repo after whichever one it was written for.
      if (outcome.droppedProjectName) {
        did(
          "config   dropped projectName — it is repo-level only; "
            + "run --configure inside a repo to set it there",
        );
      }
      // Named, not counted. A user who theming-edited that file deserves to
      // see which keys the installer put back into it.
      if (outcome.added.length > 0) {
        did(
          `config   added ${outcome.added.length} missing key${
            outcome.added.length === 1 ? "" : "s"
          } from the template: ${outcome.added.join(", ")}`,
        );
      }
      break;
  }

  // 3. The two render keys. Both are rewritten every time: an upgrade that
  //    left a flagless command in place would render the missing-flag line.
  const current = settings.readSettings(paths.settings);
  const wanted = settings.desired(paths.binary);

  for (
    const key of [
      settings.STATUS_LINE,
      settings.SUBAGENT_STATUS_LINE,
    ] as const
  ) {
    const previous = current[key];
    const ownership = settings.ownershipOf(previous);

    if (ownership === "foreign" && !opts.force) {
      if (!hasTty() && !opts.yes) {
        fail(
          `${key} in ${
            tilde(paths.settings, paths)
          } was not written by this installer.\n`
            + `  Replacing it needs a yes, and there is no terminal to ask in.\n`
            + `  Nothing has been changed. Re-run from a terminal, or pass --yes or --force.`,
        );
      }
      say("");
      say(`  ${key} already holds a status line this installer did not write:`);
      say(`    ${JSON.stringify(previous)}`);
      if (!await confirm("  replace it?", opts.yes)) {
        say("  left it alone — install stopped.");
        say(
          `  ${
            tilde(paths.binary, paths)
          } is in place but nothing is wired to it.`,
        );
        return 1;
      }
    }

    entries.push({
      kind: "configKey",
      file: paths.settings,
      key,
      previous: previous === undefined ? null : previous,
    });
    current[key] = key === settings.STATUS_LINE
      ? wanted.statusLine
      : wanted.subagentStatusLine;
    did(
      `wired    ${key}${
        ownership === "ours-stale" ? " (was missing its flag)" : ""
      }`,
    );
  }

  // 4. The caps hook — the third key, and the only one that **removes** a
  //    `node` invocation rather than adding one. The whole `hooks` value is
  //    recorded, because that is the only way to restore it verbatim.
  const previousHooks = current[settings.HOOKS];
  const wiring = settings.wireHook(current, paths.binary);
  entries.push({
    kind: "configKey",
    file: paths.settings,
    key: settings.HOOKS,
    previous: previousHooks === undefined ? null : previousHooks,
  });
  current[settings.HOOKS] = wiring.hooks;
  did(
    `wired    ${settings.POST_TOOL_USE} caps hook${
      wiring.ownership === "ours-stale" ? " (replaced the node one)" : ""
    }`,
  );

  if (!opts.dryRun) {
    settings.writeSettings(paths.settings, current);
  }

  // 5. What the `ai-plugins` install left behind. Another tool's files, so
  //    they are reported and offered — never removed without a yes.
  const declined = await sweepOrphans(paths, opts);

  // 6. The receipt, last: it describes an install that has actually happened.
  if (!opts.dryRun) {
    receipt.write(
      paths,
      version,
      entries,
      new Date().toISOString(),
      declined,
    );
  }
  did(`receipt  ${tilde(paths.receipt, paths)}`);

  say("");
  say(
    opts.dryRun
      ? "Dry run complete. Nothing was changed."
      : "Done. Restart Claude Code, or start a new session, to see the bar.",
  );
  return 0;
}

/**
 * Finds and offers to remove what the `ai-plugins` JS install left behind: the
 * bar script, its receipt, and the Node caps hook `--caps-hook` replaces.
 *
 * **Declining is remembered**, so a second `--install` does not re-ask. Being
 * asked the same question on every upgrade is how a prompt gets answered
 * without being read.
 *
 * Returns whether the offer was declined this run.
 */
async function sweepOrphans(paths: Paths, opts: Options): Promise<boolean> {
  const previous = receipt.read(paths);
  if (previous?.declinedOrphans) {
    return true;
  }

  const orphans = [
    paths.legacyStatusline,
    paths.legacyReceipt,
    paths.legacyHook,
  ]
    .filter(path => existsSync(path));

  if (orphans.length === 0) {
    return false;
  }

  say("");
  say("  found a previous ai-plugins statusline install:");
  for (const path of orphans) {
    say(`    ${tilde(path, paths)}`);
  }
  say("  nothing will run these again — the binary above replaces them.");

  if (opts.dryRun) {
    wouldStep("offer to remove them");
    return false;
  }

  if (!await confirm("  remove them as part of this install?", opts.yes)) {
    step(
      "kept     the ai-plugins files (ai-plugins --uninstall also knows them)",
    );
    return true;
  }

  for (const path of orphans) {
    rmSync(path, { force: true });
    step(`removed  ${tilde(path, paths)}`);
  }
  return false;
}
