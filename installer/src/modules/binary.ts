/**
 * Fetching the platform binary from its GitHub release, and putting it where
 * Claude Code will run it.
 *
 * Claude Code is wired to `~/.claude/bin/claude-status` — the raw Rust binary —
 * never to this package. Routing a render through Node would pay 30-50 ms of
 * startup every four seconds and negate the entire reason the bar is in Rust.
 *
 * The binary used to travel inside one npm package per platform. It now travels
 * as a GitHub release asset, verified against a digest pinned in `checksums.json`
 * beside this bundle. Release assets are **mutable** — one can be deleted and
 * re-uploaded at the same URL — and npm versions are not, so pinning the digest
 * in the immutable artifact is what keeps the trust root on npm and reduces
 * GitHub to a bytes-mover.
 *
 * This is not a `postinstall` hook. It runs only inside `--install`, a command
 * the user typed, which already writes to `~/.claude` and `~/.config`.
 */
import { createHash } from "node:crypto";
import {
  dirname,
  join,
} from "node:path";
import { fileURLToPath } from "node:url";

import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "../_shared/io.js";
import { BINARY_NAME } from "../_shared/paths.js";

/**
 * Where the release assets live.
 *
 * `$CLAUDE_STATUS_RELEASE_BASE` overrides it — for tests, which serve a fake
 * binary from a local server rather than reaching GitHub. The same seam the
 * crate exposes as `$CLAUDE_STATUS_SPEND_URL`, for the same reason: a suite that
 * hits the real endpoint is a suite that fails when the network does.
 */
/** How long to wait on the download before giving up. */
const TIMEOUT_MS = 120_000;

function releaseBase(): string {
  const override = process.env["CLAUDE_STATUS_RELEASE_BASE"];
  return override && override.length > 0
    ? override
    : "https://github.com/virajp/claude-status/releases/download";
}

/** One target's asset, as `checksums.json` records it. */
export interface AssetEntry {
  file: string;
  sha256: string;
}

/**
 * The pinned manifest, staged beside the bundle by `build:installer`.
 *
 * `version` is the **binary's** version — the release tag to fetch from — and
 * is deliberately not this package's version. The two are on separate lines
 * while the fetch path is being proven, so resolution must never key off
 * `--version`, and never off `latest`: an older `npx @askviraj/claude-status@…`
 * has to install the binary its own manifest names.
 */
export interface Manifest {
  version: string;
  assets: Record<string, AssetEntry>;
}

export function manifestPath(): string {
  return join(dirname(fileURLToPath(import.meta.url)), "checksums.json");
}

export function readManifest(path: string = manifestPath()): Manifest {
  const raw = readFileSync(path, "utf8");
  const parsed = JSON.parse(raw) as Manifest;
  if (
    typeof parsed?.version !== "string"
    || parsed.assets === null
    || typeof parsed.assets !== "object"
  ) {
    throw new Error(`${path} is not a checksums manifest`);
  }
  return parsed;
}

/**
 * The hosts this package can serve, for the unsupported-platform message.
 *
 * Read from the manifest rather than a second hand-maintained list that could
 * disagree with it — that list used to be `PACKAGES`, and keeping it in step
 * with the target table was one of the four manual steps adding a platform
 * cost. Rendered `<os>:<cpu>` to match `hostKey()`, so the message compares
 * like with like; the manifest keys itself with a hyphen because that is what
 * reads well as a filename suffix.
 */
export function supportedPlatforms(manifest: Manifest): string[] {
  return Object
    .keys(manifest.assets)
    .map(key => key.replace("-", ":"))
    .sort();
}

export function hostKey(): string {
  return `${process.platform}:${process.arch}`;
}

/** The manifest keys targets by `<os>-<cpu>`; `hostKey` joins with a colon. */
function manifestKey(): string {
  return `${process.platform}-${process.arch}`;
}

export type Resolution =
  | { ok: true; url: string; sha256: string; version: string; }
  | { ok: false; reason: "unsupported"; host: string; };

/**
 * The URL and expected digest for this host, or why there isn't one.
 *
 * Nothing is fetched here — this is the pure half, so the dry run can report
 * exactly what a real run would install without touching the network.
 */
export function resolve(manifest: Manifest): Resolution {
  const entry = manifest.assets[manifestKey()];
  if (!entry) {
    return { ok: false, reason: "unsupported", host: hostKey() };
  }
  return {
    ok: true,
    url: `${releaseBase()}/v${manifest.version}/${entry.file}`,
    sha256: entry.sha256,
    version: manifest.version,
  };
}

/** Why a download did not produce a usable binary. The three cases have three
 * different fixes, so they are never collapsed into one message. */
export type DownloadError =
  | { kind: "offline"; detail: string; }
  | { kind: "missing"; status: number; }
  | { kind: "corrupt"; expected: string; actual: string; };

export class BinaryFetchError extends Error {
  constructor(readonly info: DownloadError, message: string) {
    super(message);
    this.name = "BinaryFetchError";
  }
}

/**
 * Downloads the asset and returns its bytes, or throws with the case named.
 *
 * The digest is checked **here**, before the caller is given anything to write.
 * A corrupt download must never reach `~/.claude/bin`, even briefly.
 */
export async function download(
  url: string,
  expected: string,
  fetchImpl: typeof fetch = fetch,
): Promise<Buffer> {
  let response: Response;
  try {
    // A server that accepts the connection and then says nothing would
    // otherwise hang `--install` forever, with no output and nothing to
    // interrupt but Ctrl-C. Two minutes is generous for a ~2 MB binary on a
    // slow line and still finite.
    response = await fetchImpl(url, {
      redirect: "follow",
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
  }
  catch (error: unknown) {
    throw new BinaryFetchError(
      { kind: "offline", detail: describe(error) },
      `could not reach ${url}\n  ${describe(error)}${proxyHint()}`,
    );
  }

  if (!response.ok) {
    throw new BinaryFetchError(
      { kind: "missing", status: response.status },
      `${url}\n  returned HTTP ${response.status}. The release asset is not `
        + `there — it may have been yanked, or this package may name a release `
        + `that was never published.`,
    );
  }

  const bytes = Buffer.from(await response.arrayBuffer());
  const actual = createHash("sha256").update(bytes).digest("hex");
  if (actual !== expected) {
    throw new BinaryFetchError(
      { kind: "corrupt", expected, actual },
      `the downloaded binary does not match the digest this package pins.\n`
        + `  expected ${expected}\n`
        + `  received ${actual}\n`
        + `  Nothing has been installed. Do not retry — a mismatch is not a `
        + `flaky download.\n  Report it at `
        + `https://github.com/virajp/claude-status/issues`,
    );
  }

  return bytes;
}

/**
 * Writes verified bytes to their destination.
 *
 * Temp file then rename, so an interrupted write never leaves a half-binary
 * where Claude Code will try to execute it — the same discipline the crate's
 * `write_json_atomic_pretty` uses, and for the same reason.
 */
export function place(bytes: Buffer, destination: string): void {
  mkdirSync(dirname(destination), { recursive: true });
  const temp = `${destination}.download`;
  try {
    writeFileSync(temp, bytes);
    chmodSync(temp, 0o755);
    renameSync(temp, destination);
  }
  catch (error: unknown) {
    if (existsSync(temp)) {
      rmSync(temp, { force: true });
    }
    throw error;
  }
}

function describe(error: unknown): string {
  if (error instanceof Error) {
    const cause = (error as { cause?: { code?: string; }; }).cause;
    return cause?.code ? `${error.message} (${cause.code})` : error.message;
  }
  return String(error);
}

/**
 * Node's built-in fetch ignores `HTTP_PROXY` / `HTTPS_PROXY`.
 *
 * Users behind a proxy used to succeed without noticing, because npm carried
 * the binary and this installer never touched the network. Now they fail, and
 * the failure looks like being offline. Naming the variable we can see set is
 * the difference between a five-minute fix and an unexplainable one.
 */
function proxyHint(): string {
  const set = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"]
    .filter(name => (process.env[name] ?? "").length > 0);
  return set.length === 0 ? "" : (
    `\n  ${
      set.join(", ")
    } is set, and Node's fetch does not honour it. Download the binary`
    + `\n  by hand from the release and place it at ~/.claude/bin/claude-status.`
  );
}
