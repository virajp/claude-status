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
 * **macOS only, both architectures.** Claude Code runs on Linux and Windows
 * too, and this map used to carry them; the `macos-only` cycle cut it to two.
 * The wrapper's `"os": ["darwin"]` field is the first gate — npm refuses the
 * install outright — and this map is the second, for anyone who forced past it.
 *
 * Kept in step with `supported_targets` in
 * `.config/mise/tasks/_scripts/_rust`, which is what actually builds the
 * packages. The two lists disagreeing means a host resolving a package nobody
 * publishes.
 */
const PACKAGES: Record<string, string> = {
  "darwin:arm64": "@askviraj/claude-status-darwin-arm64",
  "darwin:x64": "@askviraj/claude-status-darwin-x64",
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
  chmodSync(destination, 0o755);
}
