import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const checkout = join(root, "vendor/bun");
const base = "38afee6c84fcf48c0edb2466aa4b27d287a59c30";
const patch = join(root, "vendor/patches/bun-whatever.patch");

function git(args: string[], cwd = checkout, quiet = false) {
  return Bun.spawnSync(["git", ...args], {
    cwd,
    stdout: "pipe",
    stderr: quiet ? "pipe" : "inherit",
  });
}

export function prepareBun() {
  if (!existsSync(join(checkout, ".git"))) {
    const result = git(["submodule", "update", "--init", "--depth", "1", "vendor/bun"], root);
    if (result.exitCode !== 0) throw new Error("Could not initialize the pinned Bun submodule.");
  }
  const revision = git(["rev-parse", "HEAD"]);
  if (revision.exitCode !== 0 || revision.stdout.toString().trim() !== base) {
    throw new Error("The Bun submodule must be pinned to v1.4.4 (" + base + ").");
  }
  if (!existsSync(patch)) throw new Error("The tracked Whatever Bun patch is missing.");
  if (git(["apply", "--reverse", "--check", patch], checkout, true).exitCode === 0) return;
  const check = git(["apply", "--check", patch]);
  if (check.exitCode !== 0) {
    throw new Error("The Bun checkout conflicts with the tracked patch. Preserve local edits before resolving the conflict.");
  }
  if (git(["apply", patch]).exitCode !== 0) throw new Error("Could not apply the Whatever Bun patch.");
}

if (import.meta.main) {
  prepareBun();
  console.log("Bun v1.4.4 prepared with the Whatever native policy.");
}