/**
 * Output, prompting, and the file primitives everything else is built from.
 *
 * The installer's report **is** its product, so it goes to stdout. Errors and
 * warnings go to stderr, so `npx … --install > log` still shows a failure.
 */
import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname } from "node:path";
import { createInterface } from "node:readline/promises";

export function say(message: string): void {
  process.stdout.write(`${message}\n`);
}

/**
 * A step that did not happen because `--dry-run` was passed.
 *
 * Marked rather than silent: a dry run whose output is indistinguishable from
 * a real one teaches the user nothing about which it was.
 */
export function wouldStep(message: string): void {
  process.stdout.write(`  would ${message}\n`);
}

export function step(message: string): void {
  process.stdout.write(`  ${message}\n`);
}

export function warn(message: string): void {
  process.stderr.write(`warning: ${message}\n`);
}

export function fail(message: string): never {
  process.stderr.write(`claude-status: ${message}\n`);
  process.exit(1);
}

/**
 * Asks a yes/no question.
 *
 * With **no TTY** there is nobody to ask, so the caller is told so rather than
 * being guessed at in either direction — silently overwriting someone's own
 * status line is the one unforgivable failure mode here, and silently skipping
 * the install is not much better.
 */
export async function confirm(
  question: string,
  yes = false,
): Promise<boolean> {
  // `--yes` is the non-interactive answer: a setup script or CI has said in
  // advance what it would have said at the prompt.
  if (yes) {
    say(`${question} [y/N] y (--yes)`);
    return true;
  }
  if (!process.stdin.isTTY) {
    return false;
  }

  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    const answer = await rl.question(`${question} [y/N] `);
    return /^y(es)?$/i.test(answer.trim());
  }
  finally {
    rl.close();
  }
}

export function hasTty(): boolean {
  return Boolean(process.stdin.isTTY);
}

export function readJson<T = unknown>(path: string): T | null {
  try {
    return JSON.parse(readFileSync(path, "utf8")) as T;
  }
  catch {
    return null;
  }
}

/** Writes JSON atomically, so an interrupted install cannot truncate a file. */
export function writeJson(path: string, value: unknown): void {
  const text = `${JSON.stringify(value, null, 2)}\n`;
  writeFileText(path, text);
}

export function writeFileText(path: string, text: string): void {
  mkdirSync(dirname(path), { recursive: true });
  const tmp = `${path}.${process.pid}.tmp`;
  try {
    writeFileSync(tmp, text);
    renameSync(tmp, path);
  }
  catch (error) {
    rmSync(tmp, { force: true });
    throw error;
  }
}

export function sha256(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
};
