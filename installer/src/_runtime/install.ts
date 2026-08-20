/**
 * `--install`: put the binary where Claude Code runs it, make sure a config
 * exists, wire the two render keys, and record what was there before.
 */
import {
  confirm,
  existsSync,
  fail,
  hasTty,
  say,
  step,
  warn,
} from "../_shared/io.js";
import {
  resolvePaths,
  tilde,
} from "../_shared/paths.js";
import * as binary from "../modules/binary.js";
import * as config from "../modules/config.js";
import type { Entry } from "../modules/receipt.js";
import * as receipt from "../modules/receipt.js";
import * as settings from "../modules/settings.js";

export async function install(
  version: string,
  env: NodeJS.ProcessEnv = process.env,
): Promise<number> {
  const paths = resolvePaths(env);
  const entries: Entry[] = [];

  say(`Installing claude-status ${version}`);

  // 1. The binary. Resolved before anything is written, so an unsupported
  //    platform fails having touched nothing.
  const resolved = binary.resolvePlatformBinary();
  if (!resolved.ok) {
    if (resolved.reason === "unsupported") {
      fail(
        `unsupported platform ${resolved.host}\n`
          + `  supported: ${binary.supportedPlatforms().join(", ")}\n`
          + `  build from source instead: https://github.com/virajp/claude-status`,
      );
    }
    fail(
      `${resolved.package} is not installed\n`
        + `  npm skips optional dependencies on install failure — reinstall with:\n`
        + `    npm install @askviraj/claude-status --force`,
    );
  }

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
  binary.install(resolved.path, paths.binary);
  entries.push({
    kind: "file",
    path: paths.binary,
    existedBefore: binaryExisted,
  });
  step(`binary   ${tilde(paths.binary, paths)}`);

  // 2. The config: seeded, migrated from the JS bar's name, or left alone.
  const outcome = config.seedOrMigrate(paths);
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
      step(`config   ${tilde(paths.config, paths)} (seeded)`);
      break;
    case "migrated":
      entries.push({
        kind: "file",
        path: paths.config,
        existedBefore: false,
        movedFrom: outcome.from,
        sha256: outcome.sha256,
      });
      step(
        `config   ${tilde(outcome.from, paths)} → ${
          tilde(paths.config, paths)
        } (migrated)`,
      );
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

    if (ownership === "foreign") {
      if (!hasTty()) {
        fail(
          `${key} in ${
            tilde(paths.settings, paths)
          } was not written by this installer.\n`
            + `  Replacing it needs a yes, and there is no terminal to ask in.\n`
            + `  Nothing has been changed. Re-run this from a terminal.`,
        );
      }
      say("");
      say(`  ${key} already holds a status line this installer did not write:`);
      say(`    ${JSON.stringify(previous)}`);
      if (!await confirm("  replace it?")) {
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
    step(
      `wired    ${key}${
        ownership === "ours-stale" ? " (was missing its flag)" : ""
      }`,
    );
  }

  settings.writeSettings(paths.settings, current);

  // 4. The receipt, last: it describes an install that has actually happened.
  receipt.write(paths, version, entries, new Date().toISOString());
  step(`receipt  ${tilde(paths.receipt, paths)}`);

  say("");
  say("Done. Restart Claude Code, or start a new session, to see the bar.");
  return 0;
}
