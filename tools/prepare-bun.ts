import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const checkout = join(root, "vendor/bun");
const base = "80772ef5fb64e0fdc242881782ec625a21cf15e4";
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
    throw new Error("The Bun submodule must be pinned to the Whatever fork revision " + base + ".");
  }
  if (!existsSync(patch)) throw new Error("The tracked Whatever Bun patch is missing.");
  if (git(["apply", "--reverse", "--check", patch], checkout, true).exitCode !== 0) {
    throw new Error("The pinned Bun checkout differs from the tracked Whatever policy. Preserve local edits before restoring the submodule.");
  }
  // Applying new eol=lf attributes does not rewrite files already checked out by
  // Windows Git. Normalize only the manifest line endings, preserving local data.
  const manifests = join(checkout, "src/codegen/builtins/data");
  for (const name of readdirSync(manifests)) {
    if (!name.endsWith(".txt")) continue;
    const file = join(manifests, name);
    const text = readFileSync(file, "utf8");
    const normalized = text.replace(/\r\n/g, "\n");
    if (normalized !== text) writeFileSync(file, normalized);
  }
}

if (import.meta.main) {
  prepareBun();
  console.log("Bun v1.4.4 prepared with the Whatever native policy.");
}
