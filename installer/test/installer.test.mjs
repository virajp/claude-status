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
  statSync,
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
/**
 * The crate version, read from the one place that owns it.
 *
 * The fixtures below stand in for a real release, and a hard-coded number here
 * goes stale on the first bump — quietly, because nothing compares it to
 * anything. This is the BINARY's version, which the manifest names; the npm
 * package carries its own, and the two deliberately differ.
 */
const CRATE_VERSION = readFileSync(join(ROOT, "Cargo.toml"), "utf8")
  .match(/^version\s*=\s*"([^"]+)"/m)[1];
const BUNDLE = join(ROOT, "npm", "claude-status", "bin", "installer.mjs");
const ASSET = join(ROOT, "assets", "claude-status.defaults.json");
/** The installed binary's filename. macOS only, so there is no extension. */
const BINARY_NAME = "claude-status";

/**
 * The binary ships inside the package, so the fixture is a binary written
 * beside the bundle — exactly where `build:installer` stages the real one.
 *
 * This replaced a stand-in release server that had to run in its own process,
 * because `run()` drives the installer with `execFileSync` and an in-process
 * server would have deadlocked against it. Embedding removed the need for any
 * of it.
 */
const FAKE_BINARY = `#!/bin/sh\necho ${CRATE_VERSION}\n`;
const STAGED_BINARY = join(ROOT, "npm", "claude-status", "bin", BINARY_NAME);

const digestOf = text =>
  createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex");

before(() => {
  assert.ok(
    existsSync(BUNDLE),
    `run \`pnpm exec tsup\` first — no bundle at ${BUNDLE}`,
  );

  writeFileSync(STAGED_BINARY, FAKE_BINARY, { mode: 0o755 });

  // The seeded defaults ship beside the bundle.
  cpSync(
    ASSET,
    join(ROOT, "npm", "claude-status", "bin", "claude-status.defaults.json"),
  );
});

after(() => {
  rmSync(STAGED_BINARY, { force: true });
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

describe("unsupported platforms", () => {
  // The bundle reads `process.platform` and `process.arch` directly, and there
  // is no environment override for either. A tiny shim redefines both before
  // importing the bundle, so this exercises the real resolution path on a host
  // this package does not ship to — which is the only way to test it from a
  // Mac.
  function runAs(home, platform, arch, args) {
    // The shim lives OUTSIDE the home under test, so `snapshot(home)` can be
    // asserted empty with no exclusions — an assertion that has to skip a file
    // is an assertion with a hole in it.
    const shim = join(
      mkdtempSync(join(tmpdir(), "claude-status-shim-")),
      "as-host.mjs",
    );
    writeFileSync(
      shim,
      `Object.defineProperty(process, "platform", { value: ${
        JSON.stringify(platform)
      } });\n`
        + `Object.defineProperty(process, "arch", { value: ${
          JSON.stringify(arch)
        } });\n`
        + `process.argv = [process.argv[0], ${JSON.stringify(BUNDLE)}, ${
          JSON.stringify(args)
        }].flat();\n`
        + `await import(${JSON.stringify(BUNDLE)});\n`,
    );
    try {
      const stdout = execFileSync(process.execPath, [shim], {
        env: { ...process.env, HOME: home },
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
      return { code: 0, stdout, stderr: "" };
    }
    catch (error) {
      return {
        code: error.status ?? 1,
        stdout: error.stdout ?? "",
        stderr: error.stderr ?? "",
      };
    }
  }

  for (
    const [platform, arch] of [
      // darwin:x64 belongs in this list now. An Intel Mac is an unsupported
      // host like any other, and pinning it here is what stops it being
      // re-added by accident along with a target row.
      ["darwin", "x64"],
      ["linux", "x64"],
      ["linux", "arm64"],
      ["win32", "x64"],
      ["win32", "arm64"],
    ]
  ) {
    it(`refuses to install on ${platform}:${arch}, naming what is supported`, () => {
      const home = newHome();
      const { code, stderr } = runAs(home, platform, arch, ["--install"]);

      assert.equal(code, 1);
      assert.match(
        stderr,
        new RegExp(`unsupported platform ${platform}:${arch}`),
      );
      // The exact line, not two substring matches — this is the only place
      // `supportedPlatforms()` is observable from outside, so it is where a
      // silently re-added platform would show up.
      assert.match(stderr, /^ {2}supported: darwin:arm64$/m);
      assert.doesNotMatch(
        stderr,
        new RegExp(`claude-status-${platform}`),
        "an unsupported host must not be told to reinstall a package that does not exist",
      );
      assert.deepEqual(
        snapshot(home),
        {},
        "an unsupported platform must fail having touched nothing",
      );
    });
  }
});

describe("the caps hook", () => {
  const postToolUse = home =>
    json(join(home, ".claude", "settings.json")).hooks?.PostToolUse ?? [];
  const commands = home =>
    postToolUse(home).flatMap(group => (group.hooks ?? []).map(h => h.command));

  it("wires --caps-hook as a third key", () => {
    const home = newHome();
    run(home, ["--install"]);
    assert.ok(
      commands(home).some(c => /claude-status.*--caps-hook/.test(c)),
      `got ${JSON.stringify(commands(home))}`,
    );
  });

  it("replaces the node hook in place rather than adding a second one", () => {
    // Two entries for the same actuator would fire it twice per tool call.
    const home = newHome();
    const settings = join(home, ".claude", "settings.json");
    mkdirSync(dirname(settings), { recursive: true });
    writeFileSync(
      settings,
      JSON.stringify({
        hooks: {
          PostToolUse: [{
            hooks: [{
              type: "command",
              command: "node ${HOME}/.claude/hooks/context-caps.js",
            }],
          }],
        },
      }),
    );

    run(home, ["--install"]);
    const found = commands(home);
    assert.equal(found.length, 1, `got ${JSON.stringify(found)}`);
    assert.match(found[0], /--caps-hook/);
    assert.ok(!found[0].includes("context-caps.js"));
  });

  it("preserves a genuinely foreign PostToolUse hook alongside", () => {
    const home = newHome();
    const settings = join(home, ".claude", "settings.json");
    mkdirSync(dirname(settings), { recursive: true });
    writeFileSync(
      settings,
      JSON.stringify({
        hooks: {
          PostToolUse: [{
            matcher: "Write",
            hooks: [{ type: "command", command: "somebody-elses-hook" }],
          }],
        },
      }),
    );

    run(home, ["--install"]);
    const found = commands(home);
    assert.ok(
      found.includes("somebody-elses-hook"),
      `got ${JSON.stringify(found)}`,
    );
    assert.ok(found.some(c => c.includes("--caps-hook")));
    assert.equal(postToolUse(home)[0].matcher, "Write", "the matcher survives");
  });

  it("restores the original hook block verbatim on uninstall", () => {
    const home = newHome();
    const settings = join(home, ".claude", "settings.json");
    mkdirSync(dirname(settings), { recursive: true });
    const original = {
      hooks: {
        PostToolUse: [{
          hooks: [{ type: "command", command: "somebody-elses-hook" }],
        }],
      },
    };
    writeFileSync(settings, JSON.stringify(original));

    run(home, ["--install"]);
    run(home, ["--uninstall"]);
    assert.deepEqual(json(settings).hooks, original.hooks);
  });

  it("leaves hooks absent after uninstalling onto a home that had none", () => {
    const home = newHome();
    run(home, ["--install"]);
    run(home, ["--uninstall"]);
    const settings = join(home, ".claude", "settings.json");
    assert.equal(
      json(settings).hooks,
      undefined,
      "absent before means absent after, not an empty block",
    );
  });
});

describe("--dry-run", () => {
  it("reports an install and changes nothing", () => {
    const home = newHome();
    const before = snapshot(home);
    const { code, stdout } = run(home, ["--install", "--dry-run"]);

    assert.equal(code, 0);
    assert.match(stdout, /would/, "the report says what it would have done");
    assert.match(stdout, /Nothing was changed/);
    assert.deepEqual(snapshot(home), before);
  });

  it("reports an uninstall and changes nothing", () => {
    const home = newHome();
    run(home, ["--install"]);
    const installed = snapshot(home);

    const { code, stdout } = run(home, ["--uninstall", "--dry-run"]);
    assert.equal(code, 0);
    assert.match(stdout, /would/);
    assert.deepEqual(snapshot(home), installed);
  });
});

describe("--yes and --force", () => {
  const foreign = home => {
    const settings = join(home, ".claude", "settings.json");
    mkdirSync(dirname(settings), { recursive: true });
    writeFileSync(
      settings,
      JSON.stringify({
        statusLine: { type: "command", command: "my-own-bar" },
      }),
    );
    return settings;
  };

  it("--force replaces a foreign status line without a terminal", () => {
    const home = newHome();
    const settings = foreign(home);
    const { code } = run(home, ["--install", "--force"]);
    assert.equal(code, 0);
    assert.match(json(settings).statusLine.command, /--statusline/);
  });

  it("--yes answers the prompt instead of failing without a terminal", () => {
    const home = newHome();
    const settings = foreign(home);
    const { code, stdout } = run(home, ["--install", "--yes"]);
    assert.equal(code, 0);
    assert.match(stdout, /--yes/, "the answer is reported, not silent");
    assert.match(json(settings).statusLine.command, /--statusline/);
  });

  it("still restores the foreign line on uninstall", () => {
    const home = newHome();
    const settings = foreign(home);
    run(home, ["--install", "--force"]);
    run(home, ["--uninstall"]);
    assert.deepEqual(json(settings).statusLine, {
      type: "command",
      command: "my-own-bar",
    });
  });
});

describe("the ai-plugins leftovers", () => {
  const seedOrphans = home => {
    const paths = [
      join(home, ".claude", "scripts", "statusline"),
      join(home, ".config", "ai-plugins", "receipts", "statusline.json"),
      join(home, ".claude", "hooks", "context-caps.js"),
    ];
    for (const path of paths) {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, "leftover");
    }
    return paths;
  };

  it("reports them and removes them when asked", () => {
    const home = newHome();
    const paths = seedOrphans(home);
    const { stdout } = run(home, ["--install", "--yes"]);

    assert.match(stdout, /previous ai-plugins statusline install/);
    for (const path of paths) {
      assert.ok(!existsSync(path), `${path} should be gone`);
    }
  });

  it("keeps them when declined, and does not ask a second time", () => {
    const home = newHome();
    const paths = seedOrphans(home);

    // No TTY and no --yes is a decline.
    const first = run(home, ["--install"]);
    assert.match(first.stdout, /previous ai-plugins statusline install/);
    for (const path of paths) {
      assert.ok(existsSync(path), `${path} should still be there`);
    }
    assert.equal(
      json(join(home, ".config", "claude-status", "receipt.json"))
        .declinedOrphans,
      true,
    );

    const second = run(home, ["--install"]);
    assert.ok(
      !/previous ai-plugins statusline install/.test(second.stdout),
      "a declined offer is remembered rather than re-asked",
    );
  });

  it("says nothing when there is nothing left behind", () => {
    const home = newHome();
    assert.ok(!/ai-plugins/.test(run(home, ["--install"]).stdout));
  });
});

describe("the binary the package carries", () => {
  it("installs it and makes it executable", () => {
    const home = newHome();
    const { code } = run(home, ["--install", "--yes"]);

    assert.equal(code, 0);
    const installed = join(home, ".claude", "bin", BINARY_NAME);
    assert.ok(existsSync(installed));
    assert.equal(readFileSync(installed, "utf8"), FAKE_BINARY);
    assert.ok(
      statSync(installed).mode & 0o111,
      "the binary must be executable",
    );
  });

  it("records its digest, so uninstall can tell it was edited", () => {
    const home = newHome();
    run(home, ["--install", "--yes"]);

    const receipt = json(
      join(home, ".config", "claude-status", "receipt.json"),
    );
    const entry = receipt.entries.find(e =>
      e.kind === "file" && e.path.endsWith(join(".claude", "bin", BINARY_NAME))
    );
    assert.equal(
      entry.sha256,
      digestOf(FAKE_BINARY),
      "the digest of what was installed is the one recorded",
    );
  });

  it("keeps a binary the user replaced since installing", () => {
    const home = newHome();
    run(home, ["--install", "--yes"]);
    const installed = join(home, ".claude", "bin", BINARY_NAME);
    writeFileSync(installed, "#!/bin/sh\necho mine\n", { mode: 0o755 });

    const { stdout } = run(home, ["--uninstall"]);

    assert.ok(
      existsSync(installed),
      "an edit since install is the user's work",
    );
    assert.match(stdout, /edited since install/);
  });

  it("reaches no network — the binary is already here", () => {
    const home = newHome();
    // Nothing to intercept: the assertion is that the install succeeds with no
    // server standing by, which is what an air-gapped machine looks like.
    const { code } = run(home, ["--install", "--yes"]);

    assert.equal(code, 0);
    assert.equal(
      readFileSync(join(home, ".claude", "bin", BINARY_NAME), "utf8"),
      FAKE_BINARY,
    );
  });
});

describe("--version", () => {
  it("prints the installer's own version and nothing else", () => {
    const home = newHome();
    const { code, stdout } = run(home, ["--version"]);
    assert.equal(code, 0);
    assert.match(stdout.trim(), /^\d+\.\d+\.\d+/);
    assert.deepEqual(snapshot(home), {}, "--version mutates nothing");
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
  it("moves statusline.json to the new name, keeping its bytes when nothing changed", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const legacy = join(home, ".config", "statusline.json");
    // The template already carries this repo's $schema and every key, so this
    // migration has nothing to rewrite and must not reformat the file.
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

  it("repoints a migrated config's $schema at this repo", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({
        $schema:
          "https://raw.githubusercontent.com/virajp/ai-plugins/main/schemas/statusline.schema.json",
        defaultFg: "aqua",
      }),
    );

    run(home, ["--install"]);

    const config = json(join(home, ".config", "claude-status.json"));
    assert.match(
      config.$schema,
      /virajp\/claude-status\/main\/schemas\/claude-status\.schema\.json$/,
      "a file kept under the JS bar's schema URL validates against the wrong schema forever",
    );
    assert.equal(config.defaultFg, "aqua", "every other key is carried across");
  });

  it("tops a migrated config up with the template's missing top-level keys", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({ defaultFg: "aqua" }),
    );

    const { stdout } = run(home, ["--install"]);

    const config = json(join(home, ".config", "claude-status.json"));
    assert.equal(
      config.defaultFg,
      "aqua",
      "the user's value is never replaced",
    );
    const template = json(ASSET);
    for (const key of Object.keys(template)) {
      if (key === "projectName") {
        continue;
      }
      assert.ok(
        key in config,
        `\`${key}\` should have been added from the template`,
      );
    }
    assert.match(stdout, /added \d+ missing keys? from the template/);
  });

  it("the template carries no projectName, so a migration cannot gain one", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({ defaultFg: "aqua" }),
    );

    run(home, ["--install"]);

    assert.ok(
      !("projectName" in json(ASSET)),
      "projectName is repo-level only and must not ship in the defaults",
    );
    assert.ok(
      !("projectName" in json(join(home, ".config", "claude-status.json"))),
      "the user layer must not gain a name the repo layer owns",
    );
  });

  it("drops a projectName the legacy user config carried", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({ projectName: "whichever-repo", defaultFg: "aqua" }),
    );

    const { stdout } = run(home, ["--install"]);

    const config = json(join(home, ".config", "claude-status.json"));
    assert.ok(
      !("projectName" in config),
      "a name kept at user level names every repo the user opens",
    );
    assert.equal(config.defaultFg, "aqua", "every other key is carried across");
    assert.match(stdout, /dropped projectName/);
  });

  it("adds whole top-level keys without reaching inside one the user owns", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    writeFileSync(
      join(home, ".config", "statusline.json"),
      JSON.stringify({ segments: { cost: { bg: "red" } } }),
    );

    run(home, ["--install"]);

    assert.deepEqual(
      json(join(home, ".config", "claude-status.json")).segments,
      { cost: { bg: "red" } },
      "a customised object is left exactly as written",
    );
  });

  it("does not top up a claude-status.json that was already there", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const config = join(home, ".config", "claude-status.json");
    writeFileSync(config, JSON.stringify({ defaultFg: "aqua" }));
    const before = sha(config);

    run(home, ["--install"]);

    assert.equal(sha(config), before, "an existing config is still left alone");
  });

  it("still seeds a fresh install as a byte copy of the template", () => {
    const home = newHome();

    run(home, ["--install"]);

    assert.equal(
      sha(join(home, ".config", "claude-status.json")),
      sha(ASSET),
      "the seed path must stay a byte copy — the glyphs do not survive a re-encode",
    );
  });

  it("discards a legacy file that is not a JSON object and seeds instead", () => {
    for (const body of ["[1, 2, 3]", "null", "0", "\"x\"", "{ not json"]) {
      const home = newHome();
      mkdirSync(join(home, ".config"), { recursive: true });
      const legacy = join(home, ".config", "statusline.json");
      writeFileSync(legacy, body);

      const { stdout } = run(home, ["--install"]);

      assert.equal(
        sha(join(home, ".config", "claude-status.json")),
        sha(ASSET),
        `${body} should have been replaced by the seeded defaults`,
      );
      assert.ok(!existsSync(legacy), `${body} should have been removed`);
      assert.match(stdout, /discarded .*statusline\.json/);
    }
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

  it("removes the migrated config on uninstall without reviving the old name", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const legacy = join(home, ".config", "statusline.json");
    cpSync(ASSET, legacy);

    run(home, ["--install"]);
    run(home, ["--uninstall"]);

    assert.ok(
      !existsSync(join(home, ".config", "claude-status.json")),
      "an unedited config of ours is ours to remove",
    );
    assert.ok(
      !existsSync(legacy),
      "the JS bar's file is not resurrected — uninstall removes, it does not restore",
    );
  });

  it("keeps a migrated config the user has edited since", () => {
    const home = newHome();
    mkdirSync(join(home, ".config"), { recursive: true });
    const legacy = join(home, ".config", "statusline.json");
    cpSync(ASSET, legacy);

    run(home, ["--install"]);
    const config = join(home, ".config", "claude-status.json");
    writeFileSync(
      config,
      JSON.stringify({ ...json(config), defaultFg: "aqua" }),
    );
    run(home, ["--uninstall"]);

    assert.ok(
      existsSync(config),
      "an edit since the install is the user's work",
    );
    assert.equal(json(config).defaultFg, "aqua");
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
        version: CRATE_VERSION,
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

describe("--configure", () => {
  /** A throwaway git repo, since --configure asks git where the root is. */
  function newRepo(name = "my-project") {
    const parent = mkdtempSync(join(tmpdir(), "claude-status-repo-"));
    const root = join(parent, name);
    mkdirSync(root, { recursive: true });
    execFileSync("git", ["init", "-q"], { cwd: root, stdio: "ignore" });
    return root;
  }

  const repoConfig = root => join(root, ".config", "claude-status.json");
  const legacyConfig = root => join(root, ".config", "statusline.json");

  it("refuses outside a repo and names the fix", () => {
    const home = newHome();
    const loose = mkdtempSync(join(tmpdir(), "claude-status-loose-"));

    const { code, stderr } = run(home, ["--configure"], { cwd: loose });

    assert.equal(code, 1);
    assert.match(stderr, /run it from inside a repo/);
    assert.deepEqual(
      snapshot(loose),
      {},
      "a refusal must not leave anything behind",
    );
  });

  it("seeds a repo config carrying the repo's directory name", () => {
    const home = newHome();
    const root = newRepo("widget-service");

    const { code } = run(home, ["--configure"], { cwd: root });

    assert.equal(code, 0);
    const written = json(repoConfig(root));
    assert.equal(written.projectName, "widget-service");
    assert.match(written.$schema, /claude-status\.schema\.json$/);
  });

  it("writes nothing under $HOME — the file belongs to the repo", () => {
    const home = newHome();
    const root = newRepo();
    const before = snapshot(home);

    run(home, ["--configure"], { cwd: root });

    assert.deepEqual(
      snapshot(home),
      before,
      "--configure is the one command scoped to a directory, not to $HOME",
    );
  });

  it("resolves the repo root from a subdirectory", () => {
    const home = newHome();
    const root = newRepo("deep-repo");
    const nested = join(root, "src", "modules");
    mkdirSync(nested, { recursive: true });

    run(home, ["--configure"], { cwd: nested });

    assert.ok(
      existsSync(repoConfig(root)),
      "the config belongs at the root, not beside the cwd",
    );
    assert.equal(json(repoConfig(root)).projectName, "deep-repo");
  });

  it("keeps a projectName the user already set", () => {
    const home = newHome();
    const root = newRepo();
    mkdirSync(dirname(repoConfig(root)), { recursive: true });
    writeFileSync(
      repoConfig(root),
      JSON.stringify({ projectName: "hand-picked", defaultFg: "aqua" }),
    );
    const before = sha(repoConfig(root));

    const { stdout } = run(home, ["--configure"], { cwd: root });

    assert.equal(
      sha(repoConfig(root)),
      before,
      "an set name is never rewritten",
    );
    assert.match(stdout, /kept/);
  });

  it("fills a missing projectName without disturbing the other keys", () => {
    const home = newHome();
    const root = newRepo("filled");
    mkdirSync(dirname(repoConfig(root)), { recursive: true });
    writeFileSync(
      repoConfig(root),
      JSON.stringify({ $schema: "x", defaultFg: "aqua" }),
    );

    run(home, ["--configure"], { cwd: root });

    assert.deepEqual(json(repoConfig(root)), {
      $schema: "x",
      defaultFg: "aqua",
      projectName: "filled",
    });
  });

  it("migrates statusline.json, keeping its keys and repointing $schema", () => {
    const home = newHome();
    const root = newRepo();
    mkdirSync(dirname(legacyConfig(root)), { recursive: true });
    writeFileSync(
      legacyConfig(root),
      JSON.stringify({
        $schema:
          "https://raw.githubusercontent.com/virajp/ai-plugins/main/schemas/statusline.schema.json",
        projectName: "from-legacy",
        defaultFg: "aqua",
      }),
    );

    run(home, ["--configure"], { cwd: root });

    const config = json(repoConfig(root));
    assert.equal(
      config.projectName,
      "from-legacy",
      "a name the user set is kept",
    );
    assert.equal(config.defaultFg, "aqua", "the theming survives");
    assert.match(
      config.$schema,
      /claude-status\/main\/schemas\/claude-status\.schema\.json$/,
      "this is why a migration cannot be a plain rename",
    );
    assert.ok(!existsSync(legacyConfig(root)), "the old name is gone");
  });

  it("gives a migrated config a projectName when it carried none", () => {
    const home = newHome();
    const root = newRepo("named-by-dir");
    mkdirSync(dirname(legacyConfig(root)), { recursive: true });
    writeFileSync(legacyConfig(root), JSON.stringify({ defaultFg: "aqua" }));

    run(home, ["--configure"], { cwd: root });

    const config = json(repoConfig(root));
    assert.equal(config.defaultFg, "aqua");
    assert.equal(config.projectName, "named-by-dir");
    assert.match(config.$schema, /claude-status\.schema\.json$/);
  });

  it("leaves the legacy file alone when both names exist", () => {
    const home = newHome();
    const root = newRepo();
    mkdirSync(dirname(repoConfig(root)), { recursive: true });
    writeFileSync(repoConfig(root), JSON.stringify({ projectName: "current" }));
    writeFileSync(legacyConfig(root), JSON.stringify({ projectName: "old" }));

    run(home, ["--configure"], { cwd: root });

    assert.equal(json(repoConfig(root)).projectName, "current");
    assert.equal(json(legacyConfig(root)).projectName, "old");
  });

  it("discards a legacy repo config that is not a JSON object", () => {
    const home = newHome();
    const root = newRepo("discarded-repo");
    mkdirSync(dirname(legacyConfig(root)), { recursive: true });
    writeFileSync(legacyConfig(root), "[1, 2, 3]");

    run(home, ["--configure"], { cwd: root });

    const config = json(repoConfig(root));
    assert.equal(config.projectName, "discarded-repo");
    assert.match(config.$schema, /claude-status\.schema\.json$/);
    assert.ok(!existsSync(legacyConfig(root)), "the unusable file is removed");
  });

  it("refuses a repo config that is not a JSON object", () => {
    const home = newHome();
    const root = newRepo();
    mkdirSync(dirname(repoConfig(root)), { recursive: true });
    writeFileSync(repoConfig(root), "[1, 2, 3]");

    const { code, stderr } = run(home, ["--configure"], { cwd: root });

    assert.equal(code, 1);
    assert.match(stderr, /not a JSON object/);
    assert.equal(
      readFileSync(repoConfig(root), "utf8"),
      "[1, 2, 3]",
      "refusing must not overwrite",
    );
  });

  it("reports under --dry-run and changes nothing", () => {
    const home = newHome();
    const root = newRepo();
    const before = snapshot(root);

    const { code, stdout } = run(home, ["--configure", "--dry-run"], {
      cwd: root,
    });

    assert.equal(code, 0);
    assert.match(stdout, /would config/);
    assert.deepEqual(snapshot(root), before);
  });

  it("treats --configure with another verb as help rather than as a sequence", () => {
    const home = newHome();
    const root = newRepo();

    for (const other of ["--install", "--uninstall"]) {
      const { stdout } = run(home, ["--configure", other], { cwd: root });
      assert.match(stdout, /USAGE/, `--configure ${other} must not act`);
      assert.ok(
        !existsSync(repoConfig(root)),
        `--configure ${other} must not write a repo config`,
      );
    }
  });
});
