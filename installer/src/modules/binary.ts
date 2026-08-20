/**
 * Finding the platform binary npm installed, and putting it where Claude Code
 * will run it.
 *
 * Claude Code is wired to `~/.claude/bin/claude-status` — the raw Rust binary —
 * never to this package. Routing a render through Node would pay 30-50 ms of
 * startup every four seconds and negate the entire reason the bar is in Rust.
 */
import { copyFileSync } from "node:fs";
import { createRequire } from "node:module";
import {
  dirname,
  join,
} from "node:path";

import {
  chmodSync,
  existsSync,
  mkdirSync,
} from "../_shared/io.js";
import { BINARY_NAME } from "../_shared/paths.js";

const require = createRequire(import.meta.url);

/**
 * npm installs only the optionalDependency matching the host.
 *
 * Every platform Claude Code itself runs on: macOS and Linux on both
 * architectures, and Windows 10 1809+ on x64 and ARM64.
 */
const PACKAGES: Record<string, string> = {
  "darwin:arm64": "@askviraj/claude-status-darwin-arm64",
  "darwin:x64": "@askviraj/claude-status-darwin-x64",
  "linux:arm64": "@askviraj/claude-status-linux-arm64",
  "linux:x64": "@askviraj/claude-status-linux-x64",
  "win32:arm64": "@askviraj/claude-status-win32-arm64",
  "win32:x64": "@askviraj/claude-status-win32-x64",
};

export function supportedPlatforms(): string[] {
  return Object.keys(PACKAGES);
}

export function hostKey(): string {
  return `${process.platform}:${process.arch}`;
}

export type Resolution =
  | { ok: true; path: string; package: string; }
  | { ok: false; reason: "unsupported"; host: string; }
  | { ok: false; reason: "missing"; package: string; };

/**
 * Locates the binary inside the installed platform package.
 *
 * Resolves the package's *manifest* rather than the binary directly: a bare
 * executable has no extension for Node's resolver to reason about, and this
 * works whether or not the package declares `exports`.
 */
export function resolvePlatformBinary(): Resolution {
  const host = hostKey();
  const pkg = PACKAGES[host];
  if (!pkg) {
    return { ok: false, reason: "unsupported", host };
  }

  try {
    const path = join(
      dirname(require.resolve(`${pkg}/package.json`)),
      "bin",
      BINARY_NAME,
    );
    if (!existsSync(path)) {
      return { ok: false, reason: "missing", package: pkg };
    }
    return { ok: true, path, package: pkg };
  }
  catch {
    return { ok: false, reason: "missing", package: pkg };
  }
}

export function install(source: string, destination: string): void {
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  // No-op on Windows, where the `.exe` extension is what makes a file
  // runnable — and where chmod can throw on some filesystems.
  if (process.platform !== "win32") {
    chmodSync(destination, 0o755);
  }
}
