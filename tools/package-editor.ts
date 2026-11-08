import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import { join, resolve, dirname, basename, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { text } from "node:stream/consumers";
import { checkVersions } from "./check-versions.ts";
import { spawnCaptured } from "./process.ts";
import { collectNativeLibraries, windowsImports } from "./native-libraries.ts";
const releaseVersion = checkVersions();
const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const option = (name: string, fallback?: string) => {
  const i = process.argv.indexOf(name);
  if (i < 0) return fallback;
  const value = process.argv[i + 1];
  if (!value || value.startsWith("--")) throw Error(name + " needs a value");
  return value;
};
const profile = option("--profile", "release")!;
if (!["debug", "release"].includes(profile)) throw Error("--profile must be release or debug");
const platform =
  process.platform === "win32" ? "windows" : process.platform === "darwin" ? "macos" : "linux";
const architecture = process.arch === "x64" ? "x86_64" : process.arch;
const output = resolve(
  option("--output", join(repository, "dist", `WhateverEditor-${platform}-${architecture}`))!,
);
if (existsSync(output)) throw Error("Output already exists; choose a new distribution directory.");
const extension = process.platform === "win32" ? ".exe" : "";
const binaries = join(repository, "target", profile);
function run([command, ...args]: string[], cwd = repository, env = process.env) {
  if (!command) throw Error("Missing package command");
  const result = spawnSync(command, args, {
    cwd,
    env,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
    timeout: 120000,
    windowsHide: true,
  });
  if (result.status !== 0)
    throw Error(`${command}: ${result.stdout ?? ""}${result.stderr ?? ""}`, {
      cause: result.error,
    });
  return result.stdout;
}
const wmod = join(binaries, "wmod" + extension);
const payload =
  process.platform === "darwin" ? join(output, "Whatever Editor.app/Contents/MacOS") : output;
const packageBinaries = process.platform === "darwin" ? payload : join(payload, "bin");
for (const name of ["wmod", "Whatever", "whatever-launcher", "whatever-editor"]) {
  if (!existsSync(join(binaries, name + extension)))
    throw Error("Build the workspace first: missing " + name);
}
if (run([wmod, "--version"]).trim() !== `wmod ${releaseVersion}`)
  throw Error("wmod binary does not match the release version; rebuild the workspace.");
// The runtime kit contains the player and its shared native libraries.
run([
  wmod,
  "runtime-kit",
  "--player",
  join(binaries, "Whatever" + extension),
  "--launcher",
  join(binaries, "whatever-launcher" + extension),
  "--source",
  repository,
  "--output",
  payload,
]);
collectNativeLibraries(
  binaries,
  packageBinaries,
  ["whatever-editor", "wmod", "Whatever", "whatever-launcher"].map((name) => name + extension),
);
if (
  process.platform === "win32" &&
  windowsImports(join(packageBinaries, "whatever-launcher.exe")).some((name) =>
    /^(vcruntime|msvcp|std-|whatever_)/i.test(name),
  )
) {
  throw Error("Build the self-contained game launcher with bun run build:native");
}
if (process.platform === "darwin") {
  const app = join(output, "Whatever Editor.app");
  writeFileSync(
    join(app, "Contents/Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>engine.whatever.editor</string><key>CFBundleName</key><string>Whatever Editor</string><key>CFBundleExecutable</key><string>whatever-editor</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>${releaseVersion}</string><key>LSMinimumSystemVersion</key><string>13.0</string><key>NSHighResolutionCapable</key><true/></dict></plist>`,
  );
  run(["codesign", "--force", "--deep", "--sign", "-", "--preserve-metadata=entitlements", app]);
}
writeFileSync(
  join(output, "README.txt"),
  `Whatever Editor portable distribution\n\n${process.platform === "darwin" ? "Open Whatever Editor.app. CLI tools and support files are inside Contents/MacOS." : "Run bin/whatever-editor" + extension + ". Keep bin/, mods/, resources/ and sdk/ together."}\nThe editor supplies all player, launcher and runtime files when building a game.\nNo JavaScript toolchain, Rust toolchain or source checkout is needed.\nLinux targets the build runner's glibc and uses the system graphics/display drivers.\nmacOS builds are ad hoc signed; these archives are not notarized.\n`,
);
writeFileSync(
  join(output, "distribution.json"),
  JSON.stringify(
    {
      format: 2,
      profile,
      platform,
      architecture,
      version: releaseVersion,
    },
    null,
    2,
  ),
);
// Relocate away from both the source checkout and the build directory, then use only installed files.
const temporary = mkdtempSync(join(tmpdir(), "whatever-portable-"));
try {
  const relocated = join(temporary, "editor");
  cpSync(output, relocated, { recursive: true });
  const installed =
    process.platform === "darwin"
      ? join(relocated, "Whatever Editor.app/Contents/MacOS")
      : join(relocated, "bin");
  const project = join(temporary, "game");
  const installedEnvironment = {
    ...process.env,
    PATH: "",
    LD_LIBRARY_PATH: "",
    DYLD_LIBRARY_PATH: "",
  };
  run(
    [
      join(installed, "wmod" + extension),
      "project",
      "create",
      project,
      "--id",
      "portable_smoke",
      "--name",
      "Portable Smoke",
    ],
    temporary,
    installedEnvironment,
  );
  run(
    [join(installed, "whatever-editor" + extension), project, "--check"],
    temporary,
    installedEnvironment,
  );
  writeFileSync(
    join(project, "mods/core/scripts/index.ts"),
    'import {Engine} from "@whatever-engine/api"; Engine.on("init",()=>Engine.log("info","PORTABLE_EDITOR_SMOKE_READY"));',
  );
  const game = join(temporary, "build");
  run(
    [join(installed, "wmod" + extension), "package", project, "--output", game],
    temporary,
    installedEnvironment,
  );
  if (process.platform === "win32" && !existsSync(join(game, "bin/vcruntime140.dll")))
    throw Error("Player is missing its bundled MSVC runtime");
  const child = spawnCaptured([join(game, "Portable Smoke" + extension), "--mode", "server"], {
    cwd: temporary,
    env: {
      ...process.env,
      PATH: "",
      LD_LIBRARY_PATH: "",
      DYLD_LIBRARY_PATH: "",
      DISPLAY: "",
      WAYLAND_DISPLAY: "",
      RUST_LOG: "whatever_engine=info",
    },
  });
  const watchdog = setTimeout(() => {
    if (process.platform === "win32")
      spawnSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
    else {
      spawnSync("pkill", ["-TERM", "-P", String(child.pid)], {
        stdio: "ignore",
      });
      child.kill();
    }
  }, 20000);
  let stdout = "";
  let ready = false;
  const read = (async () => {
    for await (const chunk of child.stdout) {
      stdout += new TextDecoder().decode(chunk);
      if (!ready && stdout.includes("PORTABLE_EDITOR_SMOKE_READY")) {
        ready = true;
        child.stdin.write("quit\n");
      }
    }
  })();
  const [code, stderr] = await Promise.all([child.exited, text(child.stderr), read]).finally(() =>
    clearTimeout(watchdog),
  );
  if (code !== 0 || !ready)
    throw Error(`Portable runtime smoke failed (${code}): ${stdout}\n${stderr}`);
  console.log(
    "Relocated editor, project creation, validation, game build and branded launcher passed.",
  );
} finally {
  if (!resolve(temporary).startsWith(resolve(tmpdir()) + sep))
    throw Error("Unexpected smoke directory");
  rmSync(temporary, { recursive: true, force: true, maxRetries: 30, retryDelay: 100 });
}
const archive = output + (process.platform === "win32" ? ".zip" : ".tar.gz");
// Git Bash can put GNU tar ahead of Windows bsdtar. GNU tar interprets a drive
// letter in an absolute archive path as a remote host.
const tar =
  process.platform === "win32"
    ? join(process.env.SystemRoot || "C:/Windows", "System32/tar.exe")
    : "tar";
run(
  process.platform === "win32"
    ? [tar, "-a", "-c", "-f", archive, "-C", dirname(output), basename(output)]
    : [tar, "-czf", archive, "-C", dirname(output), basename(output)],
);
console.log("Packaged " + archive);
