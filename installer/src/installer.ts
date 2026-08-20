#!/usr/bin/env node
/**
 * `npx @askviraj/claude-status` — the installer.
 *
 * This package installs and uninstalls; it never renders. Claude Code is wired
 * straight to `~/.claude/bin/claude-status`, because routing a render through
 * Node would pay 30-50 ms of startup every four seconds and negate the whole
 * reason the bar was rewritten in Rust.
 */
import { createRequire } from "node:module";

import { run } from "./_runtime/cli.js";

const require = createRequire(import.meta.url);

function version(): string {
  try {
    return (require("../package.json") as { version?: string; }).version
      ?? "unknown";
  }
  catch {
    return "unknown";
  }
}

run(version(), process.argv.slice(2))
  .then(code => process.exit(code))
  .catch((error: unknown) => {
    process.stderr.write(
      `claude-status: ${
        error instanceof Error ? error.message : String(error)
      }\n`,
    );
    process.exit(1);
  });
