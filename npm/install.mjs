#!/usr/bin/env node

/*
 * `npx @virajp.dev/claude-status --install` — the third route to a
 * `claude-status` on PATH, beside `brew install virajp/tap/claude-status` and
 * `mise use --global`.
 *
 * THE PACKAGE CARRIES NO BINARY. It is this file plus a pinned digest; the
 * bytes live on a GitHub Release. `docs/decisions.md` weighed downloading
 * against *embedding* and chose embedding — this channel is not that
 * comparison. A third channel that embedded the binary would be a fourth copy
 * of the bytes with a fourth digest to keep true. What makes the download safe
 * is where the digest sits: a release asset is mutable and can be re-uploaded
 * at the same URL, and an npm version cannot, so the digest is pinned INSIDE
 * the published package. Same shape as the formula's `sha256`, and the same
 * argument.
 *
 * THE PURE FUNCTIONS ARE THE POINT OF THE FILE'S SHAPE. `tests/npm.rs` imports
 * this module under `node` and calls `parseArgs`, `classifyExisting`,
 * `chooseInstallDir` and `unwireSettings` directly, so an import must install
 * nothing: `main` runs only when this file is the process entry point, and
 * every impure input those four need is injected rather than read. That is why
 * `chooseInstallDir` takes an `isUsable` predicate instead of calling `stat`.
 *
 * NO DEPENDENCIES, EVER — Node 20 built-ins, plus `tar`, which Node does not
 * have. A lockfile here is the npm ecosystem coming back, which is exactly
 * what `tests/site.rs::no_javascript_lockfile_or_node_modules_is_tracked`
 * exists to refuse; it permits this one manifest, by path, and nothing else.
 *
 * Every line printed goes to STDERR. Nothing here has a stdout product except
 * `--help`, which is what the user asked for rather than a diagnostic.
 */

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  accessSync,
  chmodSync,
  constants as fsConstants,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import {
  basename,
  dirname,
  join,
} from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";

/** How the binary names itself, on disk and in the wiring it writes. */
const COMMAND = "claude-status";

const RELEASES = "https://github.com/virajp/claude-status/releases/download";

/*
 * The three keys `--configure` owns in `~/.claude/settings.json`, spelled the
 * way `src/modules/settings.rs` spells them. `unwireSettings` is asserted to
 * be that writer's exact byte-inverse, so a typo here is not a cosmetic one.
 */
const STATUS_LINE = "statusLine";
const SUBAGENT_STATUS_LINE = "subagentStatusLine";
const HOOKS = "hooks";
const POST_TOOL_USE = "PostToolUse";
const HOOK_KEY = "hooks.PostToolUse";

function beside(name) {
  return fileURLToPath(new URL(name, import.meta.url));
}

/**
 * The release artifact this version installs: tag, asset name, SHA-256.
 *
 * A separate file because the publish job rewrites it per release and nothing
 * else in the package changes. The tracked copy holds a REAL released triple
 * rather than placeholders — the same reason `.config/homebrew/claude-status.rb`
 * carries a real `url`/`sha256` pair — so the file is checkable wherever it
 * sits, including here before it has ever been published.
 */
export const ASSET = JSON.parse(readFileSync(beside("asset.json"), "utf8"));

/**
 * The version of the BINARY this package installs — `ASSET.tag` without its
 * leading `v`.
 *
 * NOT this package's own version, and the distinction is the point. The two
 * used to be one number: `package.json`'s version was pinned equal to the
 * crate's, and every use below meant the binary while reading the manifest.
 * That held only while the package carried the binary; it downloads one now, so
 * they are two artifacts on two release lines and the installer may be
 * published without the binary moving.
 *
 * Derived rather than declared, so this file never reads its own version and
 * the two numbers cannot be conflated again by anything written here.
 * `the_installed_version_is_the_assets_and_not_the_packages` pins it.
 */
export const INSTALLS = ASSET.tag.replace(/^v/, "");

/**
 * The flag surface, and nothing else.
 *
 * `every_flag_the_help_lists_is_a_flag_the_parser_accepts` reads the flags back
 * out of this text, so NO FLAG THE PARSER DOES NOT TAKE MAY APPEAR HERE — which
 * is also why the binary's own surfaces are not named here with their dashes.
 * That test needs at least six flags to be sure it is scanning the right thing,
 * and the six below are exactly the six the parser accepts.
 *
 * KEEP IT SHORT. This used to carry `WHAT --install DOES` and
 * `WHAT --uninstall DOES` — where the binary lands, the digest check, the
 * receipt path, the three settings keys, what an uninstall leaves behind — and
 * ran to fifty-eight lines. All of it is on the website, which can format a
 * table and be corrected without a release. The first thing a user reads should
 * be the flags and where to go for the rest.
 */
export function helpText() {
  return `claude-status — install the Claude Code powerline status line

USAGE:
    npx @virajp.dev/claude-status --install     put it on your PATH
    npx @virajp.dev/claude-status --uninstall   take it off, and unwire it
    npx @virajp.dev/claude-status --help        print this help

    pnpx and bunx work identically. Nothing is installed globally: this
    package is an installer, and the binary it fetches is the product.

MODIFIERS:
    --configure      wire Claude Code afterwards without asking
    --no-configure   do not wire it, and do not ask. A decline, not a failure.
    --force          act on a claude-status this installer cannot prove it
                     placed

    Passing --configure and --no-configure together is refused, not ranked.
    With neither, you are asked on a terminal, and nothing is wired without one.

MORE:
    https://claude-status.virajp.dev
    Where the binary lands, what is verified, what the wiring writes, and
    how this route differs from brew and mise.
`;
}

/**
 * The flags, from a plain argument list — `process.argv.slice(2)`, not
 * `process.argv`.
 *
 * A CONTRADICTION IS REFUSED, NEVER RANKED. `--configure --no-configure` has
 * no correct precedence: whichever won, half the scripts that wrote it would
 * silently do the opposite of what they say. The binary's own `--configure` is
 * the one surface in this project that already refuses an argument it does not
 * understand, and this follows it. Two mode flags together is the same
 * mistake, so it gets the same answer.
 *
 * A modifier on its own is not an error — it just does not name a mode, and no
 * mode means `help`.
 */
export function parseArgs(argv) {
  const modes = [];
  const unknown = [];
  let sawConfigure = false;
  let sawNoConfigure = false;
  let force = false;

  for (const arg of argv) {
    switch (arg) {
      case "--install":
      case "--uninstall":
      case "--help": {
        const mode = arg.slice(2);
        if (!modes.includes(mode)) {
          modes.push(mode);
        }
        break;
      }
      case "--configure":
        sawConfigure = true;
        break;
      case "--no-configure":
        sawNoConfigure = true;
        break;
      case "--force":
        force = true;
        break;
      default:
        unknown.push(arg);
    }
  }

  const parsed = {
    mode: modes.length === 1 ? modes[0] : "help",
    configure: sawConfigure === sawNoConfigure
      ? "ask"
      : sawConfigure
      ? "yes"
      : "no",
    force,
    error: null,
  };

  if (unknown.length > 0) {
    // Named back, every one of them. A user who typed `--instal` needs to be
    // shown `--instal`, not told that something somewhere was wrong.
    parsed.error = `unrecognised ${
      unknown.length > 1 ? "arguments" : "argument"
    }: ${unknown.join(" ")}`;
  }
  else if (sawConfigure && sawNoConfigure) {
    parsed.error =
      "--configure and --no-configure contradict each other — pass one";
  }
  else if (modes.length > 1) {
    parsed.error = `${
      modes.map(mode => `--${mode}`).join(" and ")
    } cannot both be what you meant`;
  }
  return parsed;
}

/**
 * Which channel already owns a `claude-status` on PATH.
 *
 * `Cellar` is matched as a path SEGMENT, not as a substring: a user's
 * `~/Projects/MyCellarThing/bin` is not Homebrew, and treating it as one would
 * print `brew upgrade` at somebody whose binary brew has never heard of.
 *
 * mise is identified by agreement rather than by shape — its shims live
 * wherever the user pointed `MISE_DATA_DIR`, so there is no path to match. If
 * `mise which` resolves to the same file `which` did, mise placed it.
 */
export function classifyExisting({ resolvedPath, miseWhich }) {
  if (resolvedPath.split("/").includes("Cellar")) {
    return "homebrew";
  }
  if (
    typeof miseWhich === "string"
    && miseWhich !== ""
    && miseWhich === resolvedPath
  ) {
    return "mise";
  }
  return "unknown";
}

/**
 * Where the binary goes, first match wins.
 *
 * NOTHING OUTSIDE `home` IS EVER CHOSEN — not `/usr/local/bin`, not
 * `/opt/homebrew/bin`. That is the invariant
 * `the_install_directory_never_resolves_outside_home` pins, and it is the whole
 * difference between this installer and one that writes into a directory it
 * shares with a package manager. `~/.local/bin` and `~/bin` come first because
 * a user who has one of them on PATH has already said where their own binaries
 * go.
 *
 * `isUsable` is injected — user-owned and writable is a filesystem question,
 * and keeping it out of here is what lets the ordering be tested without one.
 *
 * Nothing qualifying is not a failure here: it returns `~/.local/bin` with
 * `onPath: false`, and the caller creates it, installs, prints the PATH line
 * and exits non-zero. A binary the user cannot yet run is still better than no
 * binary and an explanation.
 */
export function chooseInstallDir({ pathEntries, home, isUsable }) {
  const preferred = join(home, ".local", "bin");

  for (const dir of [preferred, join(home, "bin")]) {
    if (pathEntries.includes(dir) && isUsable(dir)) {
      return { dir, onPath: true };
    }
  }
  for (const dir of pathEntries) {
    if (isUnder(dir, home) && isUsable(dir)) {
      return { dir, onPath: true };
    }
  }
  return { dir: preferred, onPath: false };
}

/**
 * Strictly beneath `home`, matched at the separator.
 *
 * `startsWith(home)` alone would call `/Users/someone-else` a directory under
 * `/Users/someone`, which is the same segment-versus-substring mistake
 * `classifyExisting` avoids with `Cellar` — here it would hand another
 * account's directory to `isUsable` and rely on ownership to catch it.
 */
function isUnder(dir, home) {
  return dir.startsWith(`${home}/`);
}

/**
 * Takes the three keys `--configure` writes back out, and nothing else.
 *
 * Pure: the argument is cloned, never mutated, because the round-trip test
 * compares the settings it passed in against the file afterwards.
 *
 * ANOTHER TOOL'S `PostToolUse` HOOKS SURVIVE — that is the half of this that
 * has to be right, and `the_unwire_keeps_another_tools_posttooluse_hooks` is
 * the case. Ownership is read the way `settings.rs::program_of` reads it: the
 * command's first shell word, reduced to a basename, compared whole.
 * `"claude-status"` as a substring also claims `claude-statusline` and
 * `claude-status-pro`, and this is the path that DELETES what it matches.
 *
 * A container is pruned only when our own removals emptied it, so a `hooks: {}`
 * or an empty group the user already had is left exactly where it was. That
 * asymmetry is what makes this the byte-inverse of `wire` rather than merely
 * its opposite in spirit.
 *
 * The two render keys are removed unconditionally rather than by ownership,
 * because setting a key inverts to deleting it. A status line belonging to
 * someone else was already gone the moment `--configure` ran — that mode says
 * so, prints what it replaced, and has no undo either.
 */
export function unwireSettings(settings) {
  const next = structuredClone(settings);
  const removed = [];

  for (const key of [STATUS_LINE, SUBAGENT_STATUS_LINE]) {
    if (Object.hasOwn(next, key)) {
      delete next[key];
      removed.push(key);
    }
  }

  const hooks = next[HOOKS];
  if (isObject(hooks) && Array.isArray(hooks[POST_TOOL_USE])) {
    const kept = [];
    let removedAny = false;

    for (const group of hooks[POST_TOOL_USE]) {
      const entries = isObject(group) ? group[HOOKS] : null;
      // A group we cannot read is kept verbatim. It is not ours to repair.
      if (!Array.isArray(entries)) {
        kept.push(group);
        continue;
      }
      const survivors = entries.filter(entry => !isOurEntry(entry));
      if (survivors.length === entries.length) {
        kept.push(group);
        continue;
      }
      removedAny = true;
      group[HOOKS] = survivors;
      // Emptied by our own removals, so it held nothing but our hook — a group
      // `--configure` appended, and litter to leave behind as `{"hooks": []}`.
      if (survivors.length === 0) {
        continue;
      }
      kept.push(group);
    }

    if (removedAny) {
      removed.push(HOOK_KEY);
      if (kept.length === 0) {
        delete hooks[POST_TOOL_USE];
      }
      else {
        hooks[POST_TOOL_USE] = kept;
      }
      if (Object.keys(hooks).length === 0) {
        delete next[HOOKS];
      }
    }
  }

  return { settings: next, removed };
}

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Whether one `PostToolUse` entry runs this binary.
 *
 * Quoting is handled because a path with a space in it is ordinary on macOS.
 * Anything beyond one level of quoting falls through to "not ours", which is
 * the safe direction: a false negative leaves a hook behind, a false positive
 * destroys somebody else's.
 */
function isOurEntry(entry) {
  const command = isObject(entry) ? entry.command : null;
  if (typeof command !== "string") {
    return false;
  }
  const trimmed = command.trimStart();
  const quote = trimmed[0] === "\"" || trimmed[0] === "'" ? trimmed[0] : null;
  const rest = quote === null ? trimmed : trimmed.slice(1);
  const end = quote === null ? rest.search(/\s/) : rest.indexOf(quote);
  const word = end === -1 ? rest : rest.slice(0, end);
  return basename(word) === COMMAND;
}

/* ---- everything below here touches the machine ------------------------- */

/** The one output channel. See the module note. */
function say(line) {
  process.stderr.write(`${line}\n`);
}

/** A command's trimmed stdout, or null when it did not run or did not succeed. */
function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.error !== undefined || result.status !== 0) {
    return null;
  }
  return result.stdout.trim() || null;
}

/**
 * Whether a `which` hit is a package runner's shim for THIS installer rather
 * than an installed binary.
 *
 * `npx`, `pnpx` and `bunx` all put the package's own `node_modules/.bin` at the
 * FRONT of PATH before running it — and this package declares a bin named
 * `claude-status`. So `which claude-status` finds the shim belonging to the very
 * process doing the looking, on a machine with nothing installed at all.
 *
 * Left unhandled that shim classified as `unknown`, and `--install` refused to
 * replace a file that is not an install and that nobody asked it to touch. The
 * whole npx route failed, for every user, with a message naming a path inside a
 * cache directory they had never heard of:
 *
 *     claude-status: /…/pnpm/dlx/3aa68349…/node_modules/.bin/claude-status
 *     was not placed by this installer, or has changed since it was
 *
 * A `node_modules` path SEGMENT is the test, and it is safe for a reason rather
 * than by luck: `chooseInstallDir` never selects a directory under
 * `node_modules` — it only ever picks `~/.local/bin` or `~/bin` — so a
 * `claude-status` found in one was never placed by this installer and is never
 * a destination it would choose. Matching the segment and not the substring is
 * the same care `classifyExisting` takes over `Cellar`.
 */
export function isRunnerShim(resolvedPath) {
  return resolvedPath.split("/").includes("node_modules");
}

/**
 * Every `which -a` hit that could be an install, in PATH order.
 *
 * **`-a`, not the first hit, and that is the whole point.** Under a package
 * runner the shim is always first, so a `locate` that read one line and gave up
 * on finding a shim would report "nothing installed" on a machine that has a
 * Homebrew install two entries further down — and then install a SECOND
 * claude-status into `~/.local/bin`, shadowed by the first, which is exactly
 * the two-channels-fighting case `classifyExisting` exists to prevent. That
 * regression shipped in 1.1.6 and was caught by running the real command.
 */
export function installCandidates(whichOutput) {
  return (whichOutput ?? "")
    .split("\n")
    .map(line => line.trim())
    .filter(line => line !== "" && !isRunnerShim(line));
}

/**
 * The first `claude-status` on PATH that is not a package runner's shim,
 * resolved through every symlink, or null.
 *
 * Both the raw hit and the resolved path are tested against [`isRunnerShim`],
 * because the runners disagree about which one lands in `node_modules`: npm
 * symlinks `.bin/claude-status` into the package, while pnpm writes a real
 * shell shim and leaves the realpath where it was.
 */
function locate() {
  for (const hit of installCandidates(run("which", ["-a", COMMAND]))) {
    let resolved;
    try {
      resolved = realpathSync(hit);
    }
    catch {
      resolved = hit;
    }
    if (!isRunnerShim(resolved)) {
      return resolved;
    }
  }
  return null;
}

function pathEntries() {
  return (process.env.PATH ?? "").split(":").filter(entry => entry !== "");
}

/**
 * User-owned AND writable, which are two different questions.
 *
 * Ownership is the stronger half: a directory on PATH that somebody else owns
 * is a directory somebody else can replace the binary in, and `access` alone
 * would call a group-writable one perfectly fine.
 */
function isUsable(dir) {
  try {
    const stats = statSync(dir);
    if (!stats.isDirectory() || stats.uid !== process.getuid()) {
      return false;
    }
    accessSync(dir, fsConstants.W_OK | fsConstants.X_OK);
    return true;
  }
  catch {
    return false;
  }
}

function sha256Of(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/**
 * `~/.local/state/`, and neither of the other two.
 *
 * NOT `~/.config/claude-status/`: that is the directory people commit to a
 * dotfiles repo, and a receipt naming this machine's install path would arrive
 * on the second machine claiming a binary that is not there. NOT `~/.cache/`
 * either — that holds regenerable things, and clearing a cache must not strand
 * the uninstall.
 */
function receiptPath(home) {
  return join(home, ".local", "state", COMMAND, "install-receipt.json");
}

function readReceipt(home) {
  try {
    return JSON.parse(readFileSync(receiptPath(home), "utf8"));
  }
  catch {
    return null;
  }
}

/**
 * Whether the receipt proves this installer placed the file that is there now.
 *
 * Both halves matter. The path alone says we once installed *somewhere*; the
 * digest is what says the file has not been replaced since — by another
 * installer, or by the user. Either mismatch means it is not ours to overwrite.
 *
 * The paths are compared AS THE FILESYSTEM RESOLVES THEM, not as strings.
 * `locate` goes through `realpath` and the receipt holds the path as it was
 * chosen, so one `$HOME` reached through a symlink — `/var` → `/private/var`
 * on macOS, which is every temp-`$HOME` test and every `$TMPDIR` — gives two
 * spellings of one file, and a string compare would have this installer refuse
 * its own install.
 */
function placedByUs(receipt, path) {
  return receipt !== null
    && resolve(receipt.path) === path
    && receipt.sha256 === sha256Of(path);
}

function resolve(path) {
  try {
    return realpathSync(path);
  }
  catch {
    return path;
  }
}

/**
 * The one refusal a user is likely to meet, so it says what was found, why it
 * was left alone, and what to type — in that order.
 *
 * The previous wording opened with an absolute path and a passive clause that
 * trailed off mid-sentence ("or has changed since it was"), which read as a
 * fault in the tool rather than a decision it had taken deliberately.
 */
function refuse(path, verb) {
  say(`claude-status: a claude-status is already installed at`);
  say(`  ${path}`);
  say(`  This installer did not place that file, or it changed after it did,`);
  say(`  so it will not ${verb} it. Re-run with --force to ${verb} it anyway.`);
}

/** The line that puts a directory on PATH, in the shell the user is running. */
function pathLine(dir) {
  if (basename(process.env.SHELL ?? "") === "fish") {
    return `fish_add_path ${dir}`;
  }
  return `export PATH="${dir}:$PATH"`;
}

async function download(url, into) {
  const response = await fetch(url);
  if (!response.ok) {
    // HTTP/2 carries no reason phrase, so `statusText` is empty over it and a
    // bare interpolation leaves a trailing space in the diagnostic.
    throw new Error(
      `HTTP ${response.status}${
        response.statusText === ""
          ? ""
          : ` ${response.statusText}`
      }`,
    );
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  writeFileSync(into, bytes);
  return createHash("sha256").update(bytes).digest("hex");
}

async function install({ configure, force }) {
  const home = homedir();
  const existing = locate();
  let destination = null;

  if (existing !== null) {
    const channel = classifyExisting({
      resolvedPath: existing,
      miseWhich: run("mise", ["which", COMMAND]),
    });
    // Another channel's binary is that channel's to upgrade. Overwriting a
    // Cellar path is undone by the next `brew upgrade`; overwriting a mise shim
    // is undone by the next `mise install`. Neither is worth doing twice.
    if (channel === "homebrew") {
      say(`claude-status is Homebrew's at ${existing}. Upgrade it there:`);
      say("  brew upgrade claude-status");
      return 0;
    }
    if (channel === "mise") {
      say(`claude-status is mise's at ${existing}. Upgrade it there:`);
      say("  mise upgrade claude-status");
      return 0;
    }
    const receipt = readReceipt(home);
    const ours = placedByUs(receipt, existing);
    if (!ours && !force) {
      refuse(existing, "replace");
      return 1;
    }
    // Only when the receipt actually matched. Under `--force` over somebody
    // else's binary a stale receipt is still sitting there, and reading a
    // version out of it would announce an upgrade from something that was
    // never on this path.
    if (ours) {
      say(`upgrading ${receipt.version} → ${INSTALLS} at ${existing}`);
    }
    // In place: `which` found it, so its directory is on PATH by construction,
    // and moving a user's binary to a different directory on an upgrade would
    // leave the old one shadowing the new one from wherever PATH looks first.
    destination = existing;
  }

  const chosen = destination === null
    ? chooseInstallDir({ pathEntries: pathEntries(), home, isUsable })
    : { dir: dirname(destination), onPath: true };
  mkdirSync(chosen.dir, { recursive: true });
  // `chooseInstallDir` answers from PATH alone, so a `~/.local/bin` that is on
  // PATH but did not exist yet comes back `onPath: false`. Creating it does not
  // change what PATH says, and telling a user to add a directory PATH already
  // has is worse than saying nothing.
  const onPath = chosen.onPath || pathEntries().includes(chosen.dir);
  const dest = destination ?? join(chosen.dir, COMMAND);

  // Staged INSIDE the install directory rather than in $TMPDIR: `rename` is
  // atomic only within one filesystem, and atomicity is the entire reason the
  // binary is unpacked elsewhere and moved in. A `/var/folders` staging area
  // would fail with EXDEV the moment $HOME sits on its own volume — and it is
  // the atomic move that stops a failed install leaving a partial binary on
  // PATH.
  const staging = mkdtempSync(join(chosen.dir, `.${COMMAND}-`));
  try {
    const url = `${RELEASES}/${ASSET.tag}/${ASSET.name}`;
    const tarball = join(staging, ASSET.name);
    let digest;
    try {
      digest = await download(url, tarball);
    }
    catch (error) {
      say(`claude-status: could not download ${url} — ${error.message}`);
      say(
        "  a proxy will not help: Node's fetch ignores HTTPS_PROXY. Use the tap.",
      );
      return 1;
    }

    if (digest !== ASSET.sha256) {
      say(
        `claude-status: ${ASSET.name} is not the file this version was published against`,
      );
      say(`  expected ${ASSET.sha256}`);
      say(`  got      ${digest}`);
      say(
        "  DO NOT RETRY. A mismatch is not a flaky download — the same URL will",
      );
      say(
        "  serve the same wrong bytes. A release asset can be re-uploaded in",
      );
      say(
        "  place; the digest above cannot. Report it before installing anything.",
      );
      return 1;
    }

    // Node has no tar. The archive carries the binary at its root — that is
    // what `reproducible_tar`'s `-C` is for — so this lands exactly one file,
    // and the digest just checked means it cannot be anything else.
    const extract = spawnSync("tar", ["-xzf", tarball, "-C", staging], {
      encoding: "utf8",
    });
    if (extract.status !== 0) {
      say(
        `claude-status: could not extract ${ASSET.name} — ${
          extract
            .stderr
            ?.trim() ?? extract.error?.message
        }`,
      );
      return 1;
    }

    const staged = join(staging, COMMAND);
    chmodSync(staged, 0o755);
    renameSync(staged, dest);
  }
  finally {
    rmSync(staging, { recursive: true, force: true });
  }

  // `--version` prints the bare version and nothing else, which is the one
  // output shape safe to match on exactly — `version_is_exactly_the_version_
  // with_or_without_debug` asserts stdout EQUALS it. `release.yml`'s "Verify
  // the built binary" step does the same check on the same bytes.
  const reported = run(dest, ["--version"]);
  if (reported !== INSTALLS) {
    say(
      `claude-status: ${dest} reports ${
        reported ?? "nothing"
      }, but this package installs ${INSTALLS}`,
    );
    say(
      "  it is left where it was placed — this cannot tell which of the two is wrong",
    );
    return 1;
  }
  say(`installed claude-status ${INSTALLS} to ${dest}`);

  const configured = await wire(dest, configure);
  // Written last, because it records whether wiring ran and that is not known
  // until it has. The digest is of what was actually placed, not of the
  // tarball: it is what the next run compares the file on disk against.
  writeReceipt(home, {
    version: INSTALLS,
    tag: ASSET.tag,
    path: dest,
    sha256: sha256Of(dest),
    configured,
  });

  if (!onPath) {
    say(
      `claude-status: ${chosen.dir} is not on your PATH, so nothing can run it yet. Add it:`,
    );
    say(`  ${pathLine(chosen.dir)}`);
    return 1;
  }
  return 0;
}

function writeReceipt(home, receipt) {
  const path = receiptPath(home);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(receipt, null, 2)}\n`);
}

/**
 * The three consent states. The third exists so a script can say *no* as
 * explicitly as it can say yes.
 *
 * With neither flag there is a prompt on a terminal and SILENCE without one.
 * Not a default either way: wiring overwrites a status line belonging to
 * another tool without asking and has no undo, which is not something to do to
 * a CI job that never said so.
 */
async function wire(dest, configure) {
  if (configure === "ask") {
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
      return false;
    }
    const rl = createInterface({
      input: process.stdin,
      output: process.stderr,
    });
    const answer = await rl.question(
      "Wire Claude Code to claude-status now? [y/N] ",
    );
    rl.close();
    if (!/^y(es)?$/i.test(answer.trim())) {
      configure = "no";
    }
  }

  if (configure === "no") {
    say("not wiring Claude Code. When you want it:");
    say(`  ${COMMAND} --configure`);
    return false;
  }

  const result = spawnSync(dest, ["--configure"], {
    stdio: ["ignore", "inherit", "inherit"],
  });
  return result.status === 0;
}

function uninstall({ force }) {
  const home = homedir();
  const existing = locate();

  if (existing === null) {
    say("no claude-status on PATH — unwiring settings.json anyway");
  }
  else {
    const channel = classifyExisting({
      resolvedPath: existing,
      miseWhich: run("mise", ["which", COMMAND]),
    });
    if (channel === "homebrew") {
      say(`claude-status is Homebrew's at ${existing}. Remove it there:`);
      say("  brew uninstall claude-status");
      return 0;
    }
    if (channel === "mise") {
      say(`claude-status is mise's at ${existing}. Remove it there:`);
      say("  mise uninstall claude-status");
      return 0;
    }
    // Receipt-guarded exactly as the upgrade is. `--uninstall` deleting a
    // binary somebody else put on PATH is the same wrong as `--install`
    // overwriting one, and the same proof answers both.
    if (!placedByUs(readReceipt(home), existing) && !force) {
      refuse(existing, "remove");
      return 1;
    }
    unlinkSync(existing);
    say(`removed ${existing}`);
  }

  if (!unwireFile(join(home, ".claude", "settings.json"))) {
    return 1;
  }
  try {
    unlinkSync(receiptPath(home));
  }
  catch {
    // Never written, or already gone. Either way there is nothing to say.
  }
  // Deliberately untouched: ~/.config/claude-status/config.json is the user's
  // own settings, not wiring, and a reinstall should find it exactly as it was.
  return 0;
}

/**
 * Reads `~/.claude/settings.json`, takes our keys out, writes it back.
 *
 * REFUSES RATHER THAN GUESSES, the same three ways `--configure` does. A file
 * this tool does not own, that it cannot parse, is a file it must not rewrite:
 * the TypeScript installer this channel replaces parsed inside a bare `catch`,
 * fell back to `{}`, and wrote — replacing the user's entire Claude Code
 * configuration. Nothing here may do that.
 */
function unwireFile(path) {
  let raw;
  try {
    raw = readFileSync(path, "utf8");
  }
  catch {
    say("no ~/.claude/settings.json — nothing to unwire");
    return true;
  }

  let settings;
  try {
    settings = JSON.parse(raw);
  }
  catch (error) {
    say(`claude-status: could not read ${path} — ${error.message}`);
    say(
      "  changing nothing. It is your file, and an unreadable one is not ours to rewrite.",
    );
    return false;
  }
  if (!isObject(settings)) {
    say(`claude-status: ${path} is not a JSON object — changing nothing`);
    return false;
  }

  const { settings: next, removed } = unwireSettings(settings);
  if (removed.length === 0) {
    say(`nothing of ours in ${path}`);
    return true;
  }
  writeSettings(path, next);
  say(`unwired ${removed.join(", ")} from ${path}`);
  return true;
}

/**
 * The write, shaped to be comparable with the binary's own.
 *
 * `_shared/json.rs` writes `serde_json::to_vec_pretty` plus a trailing newline,
 * which is byte-for-byte what `JSON.stringify(_, null, 2)` produces for these
 * shapes — and that is what lets
 * `the_unwire_is_the_exact_inverse_of_configure` compare bytes rather than
 * parsed values.
 *
 * The symlink resolution is `configure.rs`'s, for its reason: a rename over a
 * symlink replaces the LINK with a regular file, so a settings.json linked into
 * a dotfiles repo would be silently unlinked and the real file orphaned. Only
 * when the file itself is a link — canonicalizing unconditionally resolves
 * every parent directory too.
 */
function writeSettings(path, settings) {
  let target = path;
  if (lstatSync(path).isSymbolicLink()) {
    target = realpathSync(path);
  }
  const mode = statSync(target).mode & 0o777;
  const tmp = `${target}.${process.pid}.tmp`;
  writeFileSync(tmp, `${JSON.stringify(settings, null, 2)}\n`, { mode });
  // The `mode` above is masked by the umask, which can only narrow — a target
  // at 0664 under a 0022 umask would come back 0644. This sets the exact bits,
  // on the temp file, so the real path never exists at the wrong mode.
  chmodSync(tmp, mode);
  renameSync(tmp, target);
}

async function main(argv) {
  const args = parseArgs(argv);
  if (args.error !== null) {
    say(`claude-status: ${args.error}`);
    say("  run with --help for the flags this accepts");
    return 1;
  }
  if (args.mode === "help") {
    process.stdout.write(helpText());
    return 0;
  }
  if (args.mode === "uninstall") {
    return uninstall(args);
  }

  // The manifest's `os`/`cpu` make npm refuse with EBADPLATFORM before this
  // file runs, but only when npm is the one resolving the package. This is the
  // second half of the same gate, and it NAMES THE HOST rather than reporting a
  // tar that will not execute. `distribution/01` left this window knowingly
  // open when it deleted the npm channel; it closes here.
  if (process.platform !== "darwin" || process.arch !== "arm64") {
    say(
      `claude-status: there is no build for ${process.platform}-${process.arch}`,
    );
    say(
      "  the release carries macOS on Apple Silicon (darwin-arm64) and nothing else",
    );
    return 1;
  }
  return install(args);
}

/**
 * `main` runs ONLY as the process entry point.
 *
 * `tests/npm.rs` imports this module to call the pure functions above, and an
 * import that downloaded and placed a binary would make that impossible. The
 * `realpath` on both sides is because npx runs this through a symlink in
 * `node_modules/.bin`, so the two paths are never literally equal.
 */
function isEntryPoint() {
  const entry = process.argv[1];
  if (entry === undefined) {
    return false;
  }
  try {
    return realpathSync(entry) === realpathSync(fileURLToPath(import.meta.url));
  }
  catch {
    return false;
  }
}

if (isEntryPoint()) {
  // `.then`, not top-level await: this file is a bin, and `process.exitCode`
  // rather than `process.exit` so stderr is flushed before the process leaves.
  main(process.argv.slice(2)).then(
    code => {
      process.exitCode = code;
    },
    error => {
      say(`claude-status: ${error?.stack ?? error}`);
      process.exitCode = 1;
    },
  );
}
