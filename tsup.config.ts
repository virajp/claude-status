/**
 * Bundling the installer for publication.
 *
 * `installer/src/` is the TypeScript source; `npm/claude-status/bin/` is the
 * built output, and that is what npm ships. The split is necessary rather than
 * cosmetic: shipping `.ts` directly would need Node >= 22.18 (type stripping on
 * by default), and bundling keeps `engines.node` at 18 so the installer runs
 * wherever `npx` does.
 *
 * The installer has **no runtime dependencies** — everything it imports is from
 * `node:*`. That is deliberate: `npx @askviraj/claude-status` should fetch one
 * small tarball plus one platform binary, not a dependency tree.
 */
import { defineConfig } from "tsup";

export default defineConfig({
  // Named, so the output is `bin/installer.mjs` rather than `bin/index.mjs`.
  entry: { installer: "installer/src/installer.ts" },
  outDir: "npm/claude-status/bin",
  format: ["esm"],
  target: "node18",
  // `.mjs` because the published package declares no `type`, so the module
  // system is carried per file extension.
  outExtension: () => ({ js: ".mjs" }),
  // The entry's hashbang comes through and tsup marks the output executable.
  splitting: false,
  sourcemap: false,
  // Safe: `npm/claude-status/bin/` holds nothing but this build.
  clean: true,
});
