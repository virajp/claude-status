/**
 * Finding the repo, and the repo-level config layer inside it.
 *
 * The binary reads three config layers — embedded, `~/.config`, then
 * `<repo-root>/.config` — and `--configure` writes the third. The repo root is
 * resolved the same way the binary anchors that layer: by git, not by walking
 * for a `.git` entry, so a linked worktree and a submodule land where the
 * renderer will actually look.
 */
import { execFileSync } from "node:child_process";
import {
  basename,
  join,
} from "node:path";

/** The name the binary reads. Deliberately the same constant as the user layer. */
export const CONFIG_FILE_NAME = "claude-status.json";
/** What the JS bar read at repo level, and what `--configure` migrates. */
export const LEGACY_CONFIG_FILE_NAME = "statusline.json";

/**
 * The published schema, so an editor gives completions on a freshly written
 * repo config, and so a migrated one stops pointing at the JS bar's.
 *
 * Re-exported from the config module rather than restated: two copies of a URL
 * is one copy that gets updated.
 */
export { SCHEMA_URL } from "./config.js";

export type RepoResult =
  | { ok: true; root: string; }
  | { ok: false; reason: "not-a-repo" | "git-missing"; };

/** Every repo-level path `--configure` may touch. */
export interface RepoPaths {
  root: string;
  configDir: string;
  config: string;
  legacyConfig: string;
}

export function repoPaths(root: string): RepoPaths {
  const configDir = join(root, ".config");
  return {
    root,
    configDir,
    config: join(configDir, CONFIG_FILE_NAME),
    legacyConfig: join(configDir, LEGACY_CONFIG_FILE_NAME),
  };
}

/**
 * The git toplevel of `cwd`, or why there isn't one.
 *
 * `git rev-parse --show-toplevel` rather than a walk up for `.git`: a linked
 * worktree's `.git` is a *file*, a submodule's points elsewhere, and both are
 * cases the renderer already gets right by asking git. Matching it here is what
 * keeps `--configure` from writing a config the bar will never read.
 *
 * A missing git is told apart from a missing repo, because the fix is different.
 */
export function findRepoRoot(cwd: string = process.cwd()): RepoResult {
  try {
    const root = execFileSync("git", ["rev-parse", "--show-toplevel"], {
      cwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .trim();

    // `rev-parse` can succeed with empty output in a bare repo, which has no
    // working tree to hold a `.config/` — not somewhere to write this file.
    return root.length > 0
      ? { ok: true, root }
      : { ok: false, reason: "not-a-repo" };
  }
  catch (error: unknown) {
    return {
      ok: false,
      reason: (error as { code?: string; }).code === "ENOENT"
        ? "git-missing"
        : "not-a-repo",
    };
  }
}

/**
 * The repo's name, as the `project` segment should read it.
 *
 * The directory the repo was cloned into — not the git remote (a repo need not
 * have one, and a fork's slug is the wrong name) and not a `package.json`
 * (which in a monorepo names the workspace, not the product). Anything else is
 * a guess with a failure mode; this one is always available and always the name
 * the user typed when they cloned.
 */
export function projectNameFor(root: string): string {
  return basename(root);
}
