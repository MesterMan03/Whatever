import { existsSync, mkdirSync, renameSync, rmSync } from "node:fs";
import { dirname, resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const extension = process.platform === "win32" ? ".exe" : "";
const overrideIndex = process.argv.indexOf("--bun");
const outputIndex = process.argv.indexOf("--output");
const debugRuntime = process.argv.includes("--debug-runtime");
if (overrideIndex >= 0 && !process.argv[overrideIndex + 1]) throw new Error("--bun needs a fork executable path.");
if (outputIndex >= 0 && !process.argv[outputIndex + 1]) throw new Error("--output needs an executable path.");
const candidates = overrideIndex >= 0
  ? [resolve(process.argv[overrideIndex + 1] ?? "")]
  : [
      join(root, debugRuntime ? "vendor/bun/build/debug/bun-debug" + extension : "vendor/bun/build/release/bun" + extension),
    ];
const bun = candidates.find(existsSync);
if (!bun) {
  throw new Error("Build the pinned release fork with bun run build:bun. Development-only debug hosts require bun run build:host --debug-runtime.");
}

async function run(args: string[]): Promise<string> {
  const child = Bun.spawn([bun!, ...args], {
    cwd: root,
    env: { ...process.env, BUN_OPTIONS: "", BUN_BE_BUN: "", NODE_OPTIONS: "", BUN_DEBUG_QUIET_LOGS: "1" },
    stdout: "pipe",
    stderr: "inherit",
  });
  const [output, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  if (code !== 0) throw new Error("Bun fork command failed with exit code " + code);
  return output.trim();
}

if (await run(["--no-env-file", "--no-install", "-p", "typeof Bun.__whateverLockdown"]) !== "function") {
  throw new Error("The selected Bun binary lacks the native Whatever lockdown policy.");
}

const output = outputIndex >= 0
  ? resolve(process.argv[outputIndex + 1] ?? "")
  : join(root, "runtime/dist/whatever-mod-host" + extension);
const temporary = join(dirname(output), "whatever-mod-host.building" + extension);
mkdirSync(dirname(output), { recursive: true });
try {
  await run([
    "build", "runtime/host/index.ts", "--compile",
    "--no-compile-autoload-dotenv", "--no-compile-autoload-bunfig",
    "--no-compile-autoload-package-json", "--no-compile-autoload-tsconfig",
    "--compile-exec-argv=--no-addons --no-macros --no-install",
    "--outfile", temporary,
  ]);
  const check = Bun.spawn([temporary, "--runtime-info"], {
    env: { ...process.env, BUN_OPTIONS: "", BUN_BE_BUN: "", NODE_OPTIONS: "", BUN_DEBUG_QUIET_LOGS: "1" },
    stdout: "pipe",
    stderr: "inherit",
  });
  const [infoText, code] = await Promise.all([new Response(check.stdout).text(), check.exited]);
  const info = JSON.parse(infoText);
  if (code !== 0 || info.runtime !== "whatever-mod-host" || info.protocol !== 10 || info.native_policy !== 1) {
    throw new Error("The compiled mod host failed its native-policy self check.");
  }
  if (!debugRuntime && String(info.bun_version).split(/[.-]/).includes("debug")) {
    throw new Error("Release hosts require a release Bun fork; use --debug-runtime explicitly for development.");
  }
  renameSync(temporary, output);
  console.log("Built " + output);
  console.log(infoText.trim());
} finally {
  rmSync(temporary, { force: true });
}
