import { spawnSync } from "node:child_process";
import { copyFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const release = process.argv.includes("--release");
const runtimeOnly = process.argv.includes("--runtime-only");
const profile = release ? "release" : "debug";
const target = resolve(process.env.CARGO_TARGET_DIR ?? join(root, "target"));
function run(command: string, args: string[], capture = false, env = process.env): string {
  const result = spawnSync(command, args, {
    cwd: root,
    env,
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
  ...(runtimeOnly ? ["-p", "Whatever"] : ["--workspace", "--exclude", "whatever-launcher"]),
  ...options,
]);
// Cargo's configured rustflags follow `cargo rustc -- ...` arguments. Override the
// configuration for this dependency-free launcher so it never imports private Rust DLLs.
const launcherEnvironment = { ...process.env };
const launcherFlags = [
  "-C",
  "prefer-dynamic=no",
  ...(process.platform === "win32" ? ["-C", "target-feature=+crt-static"] : []),
];
if (launcherEnvironment.CARGO_ENCODED_RUSTFLAGS !== undefined) {
  launcherEnvironment.CARGO_ENCODED_RUSTFLAGS = [
    launcherEnvironment.CARGO_ENCODED_RUSTFLAGS,
    ...launcherFlags,
  ]
    .filter(Boolean)
    .join("\x1f");
} else {
  launcherEnvironment.RUSTFLAGS = [launcherEnvironment.RUSTFLAGS, ...launcherFlags]
    .filter(Boolean)
    .join(" ");
}
run(
  "cargo",
  ["build", "--locked", "-p", "whatever-launcher", ...options],
  false,
  launcherEnvironment,
);
for (const file of readdirSync(runtime)) {
  if (/^(?:lib)?std-[\w-]+\.(?:dll|so|dylib)$/.test(file)) {
    copyFileSync(join(runtime, file), join(target, profile, file));
    copyFileSync(join(runtime, file), join(target, profile, "deps", file));
  }
}
if (process.argv.includes("--test")) {
  run("cargo", ["test", "--workspace", "--locked", ...options, "--", "--include-ignored"]);
}
if (process.argv.includes("--test-vm")) {
  run("cargo", ["test", "-p", "whatever-script-vm", "--locked", ...options]);
}
