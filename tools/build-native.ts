import { spawnSync } from "node:child_process";
import { copyFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const release = process.argv.includes("--release");
const runtimeOnly = process.argv.includes("--runtime-only");
const profile = release ? "release" : "debug";
const target = resolve(process.env.CARGO_TARGET_DIR ?? join(root, "target"));
function run(command: string, args: string[], capture = false): string {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: ["ignore", capture ? "pipe" : "inherit", "inherit"],
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) throw Error(`${command} failed`, { cause: result.error });
  return result.stdout ?? "";
}

const runtime = run("rustc", ["--print", "target-libdir"], true).trim();
const loaderPath =
  process.platform === "darwin"
    ? "DYLD_LIBRARY_PATH"
    : process.platform === "win32"
      ? "PATH"
      : "LD_LIBRARY_PATH";
process.env[loaderPath] =
  runtime + (process.platform === "win32" ? ";" : ":") + (process.env[loaderPath] ?? "");
const options = release ? ["--release"] : [];
run("cargo", [
  "build",
  "--locked",
  ...(runtimeOnly ? ["-p", "Whatever", "-p", "whatever-launcher"] : ["--workspace"]),
  ...options,
]);
// The branded launcher runs before loading any private engine libraries.
run("cargo", [
  "rustc",
  "--locked",
  "-p",
  "whatever-launcher",
  ...options,
  "--",
  "-C",
  "prefer-dynamic=no",
  ...(process.platform === "win32" ? ["-C", "target-feature=+crt-static"] : []),
]);
for (const file of readdirSync(runtime)) {
  if (/^(?:lib)?std-[\w-]+\.(?:dll|so|dylib)$/.test(file)) {
    copyFileSync(join(runtime, file), join(target, profile, file));
    copyFileSync(join(runtime, file), join(target, profile, "deps", file));
  }
}
if (process.argv.includes("--test")) {
  run("cargo", ["test", "--workspace", "--locked", "--", "--include-ignored"]);
}
if (process.argv.includes("--test-vm")) {
  run("cargo", ["test", "-p", "whatever-script-vm", "--locked"]);
}
