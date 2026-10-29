import { existsSync, readdirSync } from "node:fs";
import { dirname, join, delimiter } from "node:path";
import { prepareBun, root, checkout } from "./prepare-bun.ts";

prepareBun();
const extraPaths: string[] = [];
if (process.platform === "win32") {
  const portable = join(root, "target/build-tools");
  if (existsSync(portable)) {
    for (const item of readdirSync(portable, { withFileTypes: true })) {
      if (!item.isDirectory() || !/^(clang\+llvm-|cmake-|nasm-)/.test(item.name)) continue;
      const directory = join(portable, item.name);
      extraPaths.push(existsSync(join(directory, "bin")) ? join(directory, "bin") : directory);
    }
  }
  const git = Bun.which("git");
  if (git) {
    const perl = join(dirname(git), "../usr/bin");
    if (existsSync(join(perl, "perl.exe"))) extraPaths.push(perl);
  }
}
const debug = process.argv.includes("--debug");
const arguments_ = debug ? ["bd"] : ["run", "build:release"];
const command = process.platform === "win32"
  ? [Bun.which("pwsh.exe") ?? "powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(root, "tools/build-bun-windows.ps1"),
      "-BunExecutable", process.execPath, "-Checkout", checkout, "-Tools", join(root, "target/build-tools"),
      ...(debug ? ["-DebugBuild"] : [])]
  : [process.execPath, ...arguments_, "-j4"];
const build = Bun.spawn(command, {
  cwd: checkout,
  env: { ...process.env, PATH: [...extraPaths, process.env.PATH ?? ""].join(delimiter) },
  stdout: "inherit",
  stderr: "inherit",
  stdin: "inherit",
});
process.exit(await build.exited);