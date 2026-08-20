#!/usr/bin/env node
// Resolves the platform binary npm installed and hands the process over to it.
//
// **This shim is never on the render path.** Claude Code is wired directly to
// `~/.claude/bin/claude-status` — the raw Rust binary — because routing a
// render through Node would pay 30–50 ms of startup every four seconds and
// negate the entire reason for the rewrite. This exists so `npx` works.
//
// It passes every argument through untouched. In particular it must NOT
// intercept `--version`: the *shape* of that answer is how an installed binary
// is told from a bundled one, so the binary has to be the one answering.

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import {
  dirname,
  join,
} from "node:path";

const require = createRequire(import.meta.url);

// npm installs only the optionalDependency matching the host, so exactly one of
// these is present at runtime.
const PACKAGES = {
  "darwin:arm64": "@askviraj/claude-status-darwin-arm64",
  "darwin:x64": "@askviraj/claude-status-darwin-x64",
  "linux:arm64": "@askviraj/claude-status-linux-arm64",
  "linux:x64": "@askviraj/claude-status-linux-x64",
};

const host = `${process.platform}:${process.arch}`;
const pkg = PACKAGES[host];

if (!pkg) {
  process.stderr.write(
    `claude-status: unsupported platform ${host}\n`
      + `  supported: ${Object.keys(PACKAGES).join(", ")}\n`
      + `  build from source instead: https://github.com/virajp/claude-status\n`,
  );
  process.exit(1);
}

let binary;
try {
  // Resolve the package's manifest rather than the binary directly: a bare
  // executable has no extension for Node's resolver to reason about, and this
  // works whether or not the package declares `exports`.
  binary = join(
    dirname(require.resolve(`${pkg}/package.json`)),
    "bin",
    "claude-status",
  );
}
catch {
  process.stderr.write(
    `claude-status: ${pkg} is not installed\n`
      + `  npm skips optional dependencies on install failure — reinstall with:\n`
      + `    npm install @askviraj/claude-status --force\n`,
  );
  process.exit(1);
}

// `stdio: inherit` keeps the three streams exactly as the caller set them, so
// the binary's own stdout-is-the-bar discipline survives the hand-off.
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  process.stderr.write(
    `claude-status: could not run ${binary}: ${result.error.message}\n`,
  );
  process.exit(1);
}

// Re-raise a fatal signal as a signal rather than flattening it to an exit
// code, so a caller can tell a crash from a non-zero return.
if (result.signal) {
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 1);
