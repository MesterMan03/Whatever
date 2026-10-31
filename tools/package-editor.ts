import {
  cpSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
  chmodSync,
  statSync,
  mkdtempSync,
  rmSync,
  realpathSync,
} from "node:fs";
import { join, resolve, dirname, basename, sep } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
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
function run(command: string[], cwd = repository, env = process.env) {
  const result = Bun.spawnSync(command, {
    cwd,
    env,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 120000,
  });
  if (result.exitCode !== 0)
    throw Error(command[0] + ": " + result.stdout.toString() + result.stderr.toString());
  return result.stdout.toString();
}
const wmod = join(binaries, "wmod" + extension);
const payload =
  process.platform === "darwin" ? join(output, "Whatever Editor.app/Contents/MacOS") : output;
for (const name of ["wmod", "Whatever", "whatever-launcher", "whatever-editor"]) {
  if (!existsSync(join(binaries, name + extension)))
    throw Error("Build the workspace first: missing " + name);
}
// wmod validates the fork policy, protocol, API version and release profile before copying anything.
run([
  wmod,
  "runtime-kit",
  "--player",
  join(binaries, "Whatever" + extension),
  "--host",
  join(repository, "runtime/dist/whatever-mod-host" + extension),
  "--launcher",
  join(binaries, "whatever-launcher" + extension),
  "--source",
  repository,
  "--output",
  payload,
]);
for (const name of ["whatever-editor", "wmod"]) {
  const target = join(payload, name + extension);
  copyFileSync(join(binaries, name + extension), target);
  chmodSync(target, 0o755);
}
const rootExecutables = [
  join(payload, "whatever-editor" + extension),
  join(payload, "wmod" + extension),
  join(payload, "whatever-launcher" + extension),
];
const engineExecutables = [
  join(payload, "engine/Whatever" + extension),
  join(payload, "engine/runtime" + extension),
];
function copy(path: string, target: string) {
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(path, target);
  chmodSync(target, statSync(path).mode);
}
if (process.platform === "win32") {
  for (const item of readdirSync(binaries)) {
    if (item.toLowerCase().endsWith(".dll")) {
      copy(join(binaries, item), join(payload, item));
      copy(join(binaries, item), join(payload, "engine", item));
    }
  }
  const vswhere = join(
    process.env["ProgramFiles(x86)"] ?? "C:/Program Files (x86)",
    "Microsoft Visual Studio/Installer/vswhere.exe",
  );
  let bundledCrt = false;
  if (existsSync(vswhere)) {
    const installation = run([
      vswhere,
      "-latest",
      "-products",
      "*",
      "-property",
      "installationPath",
    ]).trim();
    const redist = join(installation, "VC/Redist/MSVC");
    if (existsSync(redist)) {
      for (const version of readdirSync(redist).sort().reverse()) {
        const architectureDirectory = join(redist, version, "x64");
        if (!existsSync(architectureDirectory)) continue;
        const folder = readdirSync(architectureDirectory).find((name) =>
          /^Microsoft\.VC[0-9]+\.CRT$/.test(name),
        );
        if (!folder) continue;
        const crt = join(architectureDirectory, folder);
        for (const file of readdirSync(crt)) {
          if (file.endsWith(".dll")) {
            copy(join(crt, file), join(payload, file));
            copy(join(crt, file), join(payload, "engine", file));
          }
        }
        bundledCrt = existsSync(join(payload, "vcruntime140.dll"));
        break;
      }
    }
  }
  if (!bundledCrt)
    throw Error(
      "MSVC redistributable files are missing; install the Visual Studio C++ build tools before packaging.",
    );
}
if (process.platform === "linux") {
  const system =
    /^(ld-linux|lib(c|m|pthread|dl|rt|resolv|util|nss_[^.]*)\.so|linux-vdso|lib(GL|EGL|GLX|vulkan))/;
  for (const group of [rootExecutables, engineExecutables]) {
    const library = join(dirname(group[0]), "lib");
    mkdirSync(library, { recursive: true });
    const pending = [...group];
    const seen = new Set<string>();
    const patch = new Set<string>();
    while (pending.length) {
      const file = pending.shift()!;
      if (seen.has(file)) continue;
      seen.add(file);
      const dependencies = run(["ldd", file]);
      if (dependencies.includes("not found")) throw Error("Unresolved dependency: " + dependencies);
      for (const line of dependencies.split("\n")) {
        const match = line.trim().match(/^(\S+)\s+=>\s+(\/\S+)/);
        if (!match || system.test(match[1])) continue;
        patch.add(file);
        const target = join(library, match[1]);
        if (!existsSync(target)) {
          copy(match[2], target);
          pending.push(target);
        }
      }
    }
    for (const file of group.filter((file) => patch.has(file)))
      run([
        "patchelf",
        "--set-rpath",
        group === rootExecutables ? "$ORIGIN/lib:$ORIGIN/engine/lib" : "$ORIGIN/lib",
        file,
      ]);
    for (const file of readdirSync(library))
      run(["patchelf", "--set-rpath", "$ORIGIN", join(library, file)]);
  }
}
if (process.platform === "darwin") {
  const systemLibrary = (path: string) =>
    path.startsWith("/System/") || path.startsWith("/usr/lib/");
  function rpaths(file: string, source: string) {
    const lines = run(["otool", "-l", file]).split("\n");
    return lines.flatMap((line, i) =>
      line.trim() === "cmd LC_RPATH"
        ? [
            lines[i + 2]
              .trim()
              .replace(/^path /, "")
              .replace(/ \(offset.*$/, "")
              .replace("@loader_path", dirname(source))
              .replace("@executable_path", binaries),
          ]
        : [],
    );
  }
  for (const group of [rootExecutables, engineExecutables]) {
    const library = join(dirname(group[0]), "lib");
    mkdirSync(library, { recursive: true });
    const pending = group.map((file) => ({
      file,
      source:
        basename(file) === "runtime"
          ? join(repository, "runtime/dist/whatever-mod-host")
          : join(binaries, basename(file)),
      inherited: [] as string[],
    }));
    const seen = new Set<string>();
    while (pending.length) {
      const { file, source, inherited } = pending.shift()!;
      if (seen.has(file)) continue;
      seen.add(file);
      const search = [...rpaths(file, source), ...inherited];
      let changed = false;
      for (const line of run(["otool", "-L", file]).split("\n").slice(1)) {
        const dep = line.trim().split(" (compatibility")[0];
        if (!dep || systemLibrary(dep)) continue;
        const resolved = dep.startsWith("@rpath/")
          ? search.map((path) => join(path, dep.slice(7))).find(existsSync)
          : dep.replace("@loader_path", dirname(source)).replace("@executable_path", binaries);
        if (!resolved || !existsSync(resolved))
          throw Error(`Cannot resolve dylib ${dep} for ${source}`);
        if (realpathSync(resolved) === realpathSync(source) || systemLibrary(resolved)) continue;
        const target = join(library, basename(dep));
        if (!existsSync(target)) {
          copy(resolved, target);
          pending.push({ file: target, source: resolved, inherited: search });
        }
        run(["install_name_tool", "-change", dep, "@rpath/" + basename(dep), file]);
        changed = true;
      }
      // Static/system-only executables (notably Bun) need no Mach-O load-command changes.
      if (!changed && group.includes(file)) continue;
      if (group.includes(file)) {
        run(["install_name_tool", "-add_rpath", "@executable_path/lib", file]);
        if (group === rootExecutables)
          run(["install_name_tool", "-add_rpath", "@executable_path/engine/lib", file]);
      } else {
        run(["install_name_tool", "-id", "@rpath/" + basename(file), file]);
        run(["install_name_tool", "-add_rpath", "@loader_path", file]);
      }
      run(["codesign", "--force", "--sign", "-", "--preserve-metadata=entitlements", file]);
    }
  }
  const app = join(output, "Whatever Editor.app");
  const version = JSON.parse(
    readFileSync(join(repository, "runtime/package.json"), "utf8"),
  ).version;
  writeFileSync(
    join(app, "Contents/Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>engine.whatever.editor</string><key>CFBundleName</key><string>Whatever Editor</string><key>CFBundleExecutable</key><string>whatever-editor</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>${version}</string><key>LSMinimumSystemVersion</key><string>13.0</string><key>NSHighResolutionCapable</key><true/></dict></plist>`,
  );
  run(["codesign", "--force", "--deep", "--sign", "-", "--preserve-metadata=entitlements", app]);
}
writeFileSync(
  join(output, "README.txt"),
  `Whatever Editor portable distribution\n\n${process.platform === "darwin" ? "Open Whatever Editor.app. CLI tools and support files are inside Contents/MacOS." : "Run whatever-editor" + extension + ". Keep this folder together; engine/, mods/ and sdk/ are required."}\nThe editor supplies all player, launcher and runtime files when building a game.\nNo system Bun, Rust toolchain, node_modules or source checkout is needed.\nLinux targets the build runner's glibc and uses the system graphics/display drivers.\nmacOS builds are ad hoc signed; these archives are not notarized.\n`,
);
writeFileSync(
  join(output, "distribution.json"),
  JSON.stringify(
    {
      format: 1,
      profile,
      platform,
      architecture,
      version: JSON.parse(readFileSync(join(repository, "runtime/package.json"), "utf8")).version,
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
      : relocated;
  const project = join(temporary, "game");
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
  );
  run([join(installed, "whatever-editor" + extension), project, "--check"], temporary);
  writeFileSync(
    join(project, "mods/portable_smoke/scripts/index.ts"),
    'import {Engine} from "@whatever-engine/api"; Engine.on("init",()=>Engine.log("info","PORTABLE_EDITOR_SMOKE_READY"));',
  );
  const game = join(temporary, "build");
  run([join(installed, "wmod" + extension), "package", project, "--output", game], temporary, {
    ...process.env,
    PATH: "",
  });
  if (process.platform === "win32" && !existsSync(join(game, "vcruntime140.dll")))
    throw Error("Game launcher is missing its bundled MSVC runtime");
  const child = Bun.spawn([join(game, "Portable Smoke" + extension), "--mode", "server"], {
    cwd: temporary,
    env: {
      ...process.env,
      PATH: "",
      DISPLAY: "",
      WAYLAND_DISPLAY: "",
      RUST_LOG: "whatever_engine=info",
    },
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const watchdog = setTimeout(() => {
    if (process.platform === "win32")
      Bun.spawnSync(["taskkill.exe", "/PID", String(child.pid), "/T", "/F"], {
        stdout: "ignore",
        stderr: "ignore",
      });
    else {
      Bun.spawnSync(["pkill", "-TERM", "-P", String(child.pid)], {
        stdout: "ignore",
        stderr: "ignore",
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
  const [code, stderr] = await Promise.all([
    child.exited,
    new Response(child.stderr).text(),
    read,
  ]).finally(() => clearTimeout(watchdog));
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
run(
  process.platform === "win32"
    ? ["tar.exe", "-a", "-c", "-f", archive, "-C", dirname(output), basename(output)]
    : ["tar", "-czf", archive, "-C", dirname(output), basename(output)],
);
console.log("Packaged " + archive);
