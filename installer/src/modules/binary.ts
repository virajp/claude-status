/**
 * Finding the binary this package carries, and putting it where Claude Code
 * will run it.
 *
 * Claude Code is wired to `~/.claude/bin/claude-status` — the raw Rust binary —
 * never to this package. Routing a render through Node would pay 30-50 ms of
 * startup every four seconds and negate the entire reason the bar is in Rust.
 *
 * The binary travels **inside** the package. It briefly did not: the
 * `github-artifacts` cycle made it a GitHub release asset downloaded and
 * verified at install time, which bought one npm package instead of three.
 * Having one published target buys that for free, so what remained was only
 * cost — a required network call, broken air-gapped installs, an unhonoured
 * `HTTPS_PROXY`, and a digest manifest maintained against a *mutable* release
 * asset. Embedding deletes the problem instead of verifying around it: there is
 * no second artifact to distrust, because an npm version is immutable.
 */
import { copyFileSync } from "node:fs";
import {
  dirname,
  join,
} from "node:path";
import { fileURLToPath } from "node:url";

import {
  chmodSync,
  existsSync,
  mkdirSync,
} from "../_shared/io.js";
import { BINARY_NAME } from "../_shared/paths.js";

/**
 * The one host this package serves.
 *
 * `os` and `cpu` in the manifest already make npm refuse anything else, so
 * reaching the check below means an install was forced past that gate. It still
 * deserves a real message rather than a crash on a missing file.
 *
 * Kept in step with `supported_targets` in
 * `.config/mise/tasks/_scripts/_rust`, which is what actually builds the
 * package. The two disagreeing means a host being told it is unsupported while
 * its binary sits in the tarball, or the reverse.
 */
const SUPPORTED: readonly string[] = ["darwin:arm64"];

export function supportedPlatforms(): readonly string[] {
  return SUPPORTED;
}

export function hostKey(): string {
  return `${process.platform}:${process.arch}`;
}

/** Where `build:installer` stages the binary, beside the bundle. */
export function bundledPath(): string {
  return join(dirname(fileURLToPath(import.meta.url)), BINARY_NAME);
}

export type Resolution =
  | { ok: true; path: string; }
  | { ok: false; reason: "unsupported"; host: string; }
  | { ok: false; reason: "missing"; path: string; };

/**
 * The binary this package carries, or why there isn't one.
 *
 * Pure — no filesystem writes, no network, nothing. That is what lets the dry
 * run report exactly what a real run would install without touching anything.
 */
export function resolve(): Resolution {
  const host = hostKey();
  if (!SUPPORTED.includes(host)) {
    return { ok: false, reason: "unsupported", host };
  }

  const path = bundledPath();
  return existsSync(path)
    ? { ok: true, path }
    : { ok: false, reason: "missing", path };
}

export function install(source: string, destination: string): void {
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  chmodSync(destination, 0o755);
}
