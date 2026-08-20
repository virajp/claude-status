/**
 * End-to-end tests for the installer, run against the **bundled** artifact —
 * the same `installer.mjs` npm publishes, not the TypeScript source. A bundler
 * misconfiguration is exactly the class of bug that only shows up here.
 *
 * Every test runs under a throwaway `$HOME` from `mkdtemp`. Nothing here may
 * touch the real one, so `HOME` is the only home any assertion knows about.
 */
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import {
  dirname,
  join,
} from "node:path";
import {
  after,
  before,
  describe,
  it,
} from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const BUNDLE = join(ROOT, "npm", "claude-status", "bin", "installer.mjs");
const ASSET = join(ROOT, "assets", "claude-status.defaults.json");
/** The installed binary's filename, which differs on Windows. */
const BINARY_NAME = process.platform === "win32"
  ? "claude-status.exe"
  : "claude-status";

/** A fake platform package, so the installer has a binary to copy. */
let fakeModules;

before(() => {
  assert.ok(
    existsSync(BUNDLE),
    `run \`pnpm exec tsup\` first — no bundle at ${BUNDLE}`,
  );

  // The installer resolves `@askviraj/claude-status-<os>-<cpu>` relative to
  // itself, so stand one up beside the bundle.
  const pkg = `@askviraj/claude-status-${process.platform}-${process.arch}`;
  // The binary carries an extension on Windows, and the installer looks for
  // exactly that name.
  const exe = process.platform === "win32"
    ? "claude-status.exe"
    : "claude-status";
  fakeModules = join(ROOT, "npm", "claude-status", "node_modules", pkg);
  mkdirSync(join(fakeModules, "bin"), { recursive: true });
  writeFileSync(
    join(fakeModules, "package.json"),
    JSON.stringify({ name: pkg, version: "6.0.0" }),
  );
  writeFileSync(
    join(fakeModules, "bin", exe),
    "#!/bin/sh\necho 6.0.0\n",
    { mode: 0o755 },
  );

  // The seeded defaults ship beside the bundle.
  cpSync(
    ASSET,
    join(ROOT, "npm", "claude-status", "bin", "claude-status.defaults.json"),
  );
});

after(() => {
  rmSync(join(ROOT, "npm", "claude-status", "node_modules"), {
    recursive: true,
    force: true,
  });
});

function newHome() {
  return mkdtempSync(join(tmpdir(), "claude-status-test-"));
}

function run(home, args, options = {}) {
  try {
    const stdout = execFileSync(process.execPath, [BUNDLE, ...args], {
      env: { ...process.env, HOME: home },
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      ...options,
    });
    return { code: 0, stdout };
  }
  catch (error) {
    return {
      code: error.status ?? 1,
      stdout: error.stdout ?? "",
      stderr: error.stderr ?? "",
    };
  }
}

const sha = path =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
const json = path => JSON.parse(readFileSync(path, "utf8"));

/** Every path under a home, with content digests, for round-trip comparison. */
function snapshot(dir) {
  const { readdirSync, statSync } = require("node:fs");
  const out = {};
  const walk = (d, prefix) => {
    for (const name of readdirSync(d).sort()) {
      const full = join(d, name);
      const rel = prefix ? `${prefix}/${name}` : name;
      if (statSync(full).isDirectory()) {
        out[`${rel}/`] = "dir";
        walk(full, rel);
      }
      else {
        out[rel] = sha(full);
      }
    }
  };
  walk(dir, "");
  return out;
}
const require = (await import("node:module")).createRequire(import.meta.url);

describe("the argument surface", () => {
  it("prints help and mutates nothing when given no arguments", () => {
    const home = newHome();
    const before = snapshot(home);

    const { code, stdout } = run(home, []);

    assert.equal(code, 0);
    assert.match(stdout, /--install/);
    assert.match(stdout, /--uninstall/);
    assert.deepEqual(
      snapshot(home),
      before,
      "a bare invocation must not touch the filesystem",
    );
  });

  it("prints the same help for --help", () => {
    const home = newHome();
    assert.match(run(home, ["--help"]).stdout, /USAGE/);
    assert.deepEqual(
      snapshot(home),
      {},
      "--help must not create anything either",
    );
  });

  it("treats --install --uninstall together as help rather than as a sequence", () => {
    const home = newHome();
    const { code, stdout } = run(home, ["--install", "--uninstall"]);
    assert.equal(code, 0);
    assert.match(stdout, /USAGE/);
    assert.deepEqual(snapshot(home), {});
  });
});

describe("--install", () => {
  it("places the binary, seeds the config and wires both keys", () => {
    const home = newHome();
    const { code } = run(home, ["--install"]);
    assert.equal(code, 0);

    assert.ok(existsSync(join(home, ".claude", "bin", BINARY_NAME)));

    // The seeded config must be byte-identical to the shipped asset — every
    // Nerd Font glyph intact.
    assert.equal(sha(join(home, ".config", "claude-status.json")), sha(ASSET));

    const settings = json(join(home, ".claude", "settings.json"));
    assert.match(settings.statusLine.command, /claude-status --statusline$/);
    assert.match(
      settings.subagentStatusLine.command,
      /claude-status --subagent$/,
    );
    assert.equal(settings.statusLine.refreshInterval, 4);
  });

  it("merges into settings.json rather than rewriting it", () => {
    const home = newHome();
    mkdirSync(join(home, ".claude"), { recursive: true });
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({ theme: "dark", permissions: { allow: ["Bash"] } }),
    );

    run(home, ["--install"]);

    const settings = json(join(home, ".claude", "settings.json"));
    assert.equal(settings.theme, "dark", "unrelated keys must survive");
    assert.deepEqual(settings.permissions, { allow: ["Bash"] });
    assert.ok(settings.statusLine);
  });

  it("rewrites a flagless command from an older install without asking", () => {
    const home = newHome();
    mkdirSync(join(home, ".claude"), { recursive: true });
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({
        statusLine: { type: "command", command: "/old/path/claude-status" },
      }),
    );

    const { code } = run(home, ["--install"]);
    assert.equal(code, 0, "ours-but-stale is not foreign and must not prompt");
    assert.match(
      json(join(home, ".claude", "settings.json")).statusLine.command,
      /--statusline$/,
    );
  });

  it("refuses to replace a foreign status line when there is no terminal to ask in", () => {
    const home = newHome();
    mkdirSync(join(home, ".claude"), { recursive: true });
    const foreign = { type: "command", command: "/usr/local/bin/my-own-bar" };
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({ statusLine: foreign }),
    );

    const { code, stderr } = run(home, ["--install"]);

    assert.equal(code, 1, "it must fail rather than guess in either direction");
    assert.match(stderr, /not written by this installer/);
    assert.deepEqual(
      json(join(home, ".claude", "settings.json")).statusLine,
      foreign,
      "the foreign status line must be untouched",
    );
  });

  it("is idempotent — a second install does not disturb an edited config", () => {
    const home = newHome();
    run(home, ["--install"]);
    const configPath = join(home, ".config", "claude-status.json");
    writeFileSync(configPath, JSON.stringify({ projectName: "mine" }));

    const { code } = run(home, ["--install"]);

    assert.equal(code, 0);
    assert.deepEqual(
      json(configPath),
      { projectName: "mine" },
      "a user's config is never overwritten",
    );
  });
});

describe("config migration", () => {
  it("moves statusline.json to the new name, keeping its bytes", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const legacy = join(home, ".config", "statusline.json");
    cpSync(ASSET, legacy);
    const original = sha(legacy);

    run(home, ["--install"]);

    assert.ok(!existsSync(legacy), "the old name is gone");
    assert.equal(
      sha(join(home, ".config", "claude-status.json")),
      original,
      "the theming survives verbatim",
    );
  });

  it("leaves the old file alone when both names exist", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({ projectName: "old" }),
    );
    writeFileSync(
      join(home, ".config", "claude-status.json"),
      JSON.stringify({ projectName: "new" }),
    );

    run(home, ["--install"]);

    assert.deepEqual(json(join(home, ".config", "statusline.json")), {
      projectName: "old",
    });
    assert.deepEqual(json(join(home, ".config", "claude-status.json")), {
      projectName: "new",
    });
  });

  it("restores the old name on uninstall", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const legacy = join(home, ".config", "statusline.json");
    cpSync(ASSET, legacy);
    const original = sha(legacy);

    run(home, ["--install"]);
    run(home, ["--uninstall"]);

    assert.ok(existsSync(legacy), "the migration is reversed");
    assert.equal(sha(legacy), original);
    assert.ok(!existsSync(join(home, ".config", "claude-status.json")));
  });
});

describe("--uninstall", () => {
  it("leaves the tree byte-identical to before the install", () => {
    const home = newHome();
    // A home with some unrelated content, so "identical" means something.
    mkdirSync(join(home, ".claude"), { recursive: true });
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({ theme: "dark" }, null, 2) + "\n",
    );
    writeFileSync(join(home, "unrelated.txt"), "keep me");

    const before = snapshot(home);
    run(home, ["--install"]);
    assert.notDeepEqual(snapshot(home), before, "the install did something");

    run(home, ["--uninstall"]);
    assert.deepEqual(
      snapshot(home),
      before,
      "install then uninstall is a no-op",
    );
  });

  it("removes a statusLine key that was absent before, rather than writing a default", () => {
    const home = newHome();
    run(home, ["--install"]);
    run(home, ["--uninstall"]);

    const settings = json(join(home, ".claude", "settings.json"));
    assert.ok(!("statusLine" in settings), "absent before means absent after");
    assert.ok(!("subagentStatusLine" in settings));
  });

  it("restores a foreign status line the user consented to replace", () => {
    const home = newHome();
    mkdirSync(join(home, ".config", "claude-status"), { recursive: true });
    mkdirSync(join(home, ".claude"), { recursive: true });
    const foreign = { type: "command", command: "/usr/local/bin/my-own-bar" };
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({ statusLine: foreign }),
    );

    // Stand in for a consented install by writing the receipt the install
    // would have written, then wiring our own command over the top.
    writeFileSync(
      join(home, ".claude", "settings.json"),
      JSON.stringify({
        statusLine: { type: "command", command: "x --statusline" },
      }),
    );
    writeFileSync(
      join(home, ".config", "claude-status", "receipt.json"),
      JSON.stringify({
        version: "6.0.0",
        installedAt: "2026-08-20T00:00:00.000Z",
        entries: [{
          kind: "configKey",
          file: join(home, ".claude", "settings.json"),
          key: "statusLine",
          previous: foreign,
        }],
      }),
    );

    run(home, ["--uninstall"]);

    assert.deepEqual(
      json(join(home, ".claude", "settings.json")).statusLine,
      foreign,
      "their bar comes back, not ours deleted",
    );
  });

  it("keeps a config the user edited after installing", () => {
    const home = newHome();
    run(home, ["--install"]);
    const configPath = join(home, ".config", "claude-status.json");
    writeFileSync(configPath, JSON.stringify({ projectName: "mine" }));

    run(home, ["--uninstall"]);

    assert.ok(
      existsSync(configPath),
      "edited config is the user's work, not ours to delete",
    );
    assert.deepEqual(json(configPath), { projectName: "mine" });
  });

  it("without a receipt, removes only the binary and reports the rest", () => {
    const home = newHome();
    run(home, ["--install"]);
    rmSync(join(home, ".config", "claude-status", "receipt.json"));

    const { code, stdout } = run(home, ["--uninstall"]);

    assert.equal(code, 0);
    assert.ok(
      !existsSync(join(home, ".claude", "bin", BINARY_NAME)),
      "the binary is unambiguously ours",
    );
    assert.match(
      stdout,
      /remove it by hand/,
      "it reports what it will not infer",
    );
    assert.ok(
      existsSync(join(home, ".config", "claude-status.json")),
      "a config it cannot prove it created is left alone",
    );
  });
});
