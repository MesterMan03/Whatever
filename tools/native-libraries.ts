import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve, sep } from "node:path";

function run(command: string, args: string[], env = process.env): string {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    env,
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.status !== 0)
    throw Error(`${command}: ${result.stderr ?? ""}`, { cause: result.error });
  return result.stdout;
}

/** Read normal and delay-loaded PE imports without requiring developer tools on PATH. */
export function windowsImports(file: string): string[] {
  const bytes = readFileSync(file);
  const pe = bytes.readUInt32LE(0x3c);
  if (bytes.readUInt32LE(pe) !== 0x4550) throw Error(`Invalid PE image ${file}`);
  const sections = bytes.readUInt16LE(pe + 6);
  const optional = pe + 24;
  const optionalSize = bytes.readUInt16LE(pe + 20);
  const wide = bytes.readUInt16LE(optional) === 0x20b;
  const directories = optional + (wide ? 112 : 96);
  const imageBase = wide
    ? Number(bytes.readBigUInt64LE(optional + 24))
    : bytes.readUInt32LE(optional + 28);
  function offset(rva: number): number {
    if (rva < bytes.readUInt32LE(optional + 60)) return rva;
    for (let i = 0; i < sections; i++) {
      const section = optional + optionalSize + i * 40;
      const start = bytes.readUInt32LE(section + 12);
      const size = bytes.readUInt32LE(section + 16);
      if (rva >= start && rva < start + size) return bytes.readUInt32LE(section + 20) + rva - start;
    }
    throw Error(`Invalid import address in ${file}`);
  }
  function name(rva: number): string {
    const start = offset(rva);
    const end = bytes.indexOf(0, start);
    if (end < start) throw Error(`Unterminated import in ${file}`);
    const value = bytes.toString("ascii", start, end);
    if (!/^[\w.+-]+\.dll$/i.test(value)) throw Error(`Invalid imported library in ${file}`);
    return value;
  }
  const result = new Set<string>();
  for (const [index, stride, field] of [
    [1, 20, 12],
    [13, 32, 4],
  ] as const) {
    const rva = bytes.readUInt32LE(directories + index * 8);
    const size = bytes.readUInt32LE(directories + index * 8 + 4);
    if (!rva) continue;
    const start = offset(rva);
    for (
      let cursor = start;
      cursor + stride <= Math.min(bytes.length, start + size);
      cursor += stride
    ) {
      let value = bytes.readUInt32LE(cursor + field);
      if (!value) break;
      if (index === 13 && !(bytes.readUInt32LE(cursor) & 1)) value -= imageBase;
      result.add(name(value));
    }
  }
  return [...result];
}

function crtDirectory(): string | undefined {
  const vswhere = join(
    process.env["ProgramFiles(x86)"] ?? "C:/Program Files (x86)",
    "Microsoft Visual Studio/Installer/vswhere.exe",
  );
  if (!existsSync(vswhere)) return;
  const installation = run(vswhere, [
    "-latest",
    "-products",
    "*",
    "-property",
    "installationPath",
  ]).trim();
  const root = join(installation, "VC/Redist/MSVC");
  if (!existsSync(root)) return;
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  for (const version of readdirSync(root).sort().reverse()) {
    const directory = join(root, version, arch);
    if (!existsSync(directory)) continue;
    const name = readdirSync(directory).find((file) => /^Microsoft\.VC\d+\.CRT$/.test(file));
    if (name) return join(directory, name);
  }
}

function lookup(directory: string, name: string): string | undefined {
  if (!existsSync(directory)) return;
  const actual = readdirSync(directory).find((file) => file.toLowerCase() === name.toLowerCase());
  return actual ? join(directory, actual) : undefined;
}

type Dependency = { name: string; source: string; reference: string };

/** Stage one private library per basename and record the player's dependency closure. */
export function collectNativeLibraries(
  sourceDirectory: string,
  output: string,
  programs: string[],
): void {
  mkdirSync(output, { recursive: true });
  const rust = run("rustc", ["--print", "target-libdir"]).trim();
  const crt = process.platform === "win32" ? crtDirectory() : undefined;
  const roots = [sourceDirectory, rust, ...(crt ? [crt] : [])];
  const staged = new Map<string, { source: string; name: string; hash: string }>();
  const relinked = new Set<string>();
  const closures: Record<string, string[]> = {};
  const system =
    /^(?:ld-linux|lib(?:c|m|pthread|dl|rt|resolv|util|nss_[^.]*)\.so|linux-vdso|lib(?:GL|EGL|GLX|vulkan))/;
  const env = {
    ...process.env,
    LD_LIBRARY_PATH: [sourceDirectory, rust, process.env.LD_LIBRARY_PATH].filter(Boolean).join(":"),
  };

  function dependencies(
    file: string,
    executable: string,
    inherited: string[],
  ): { files: Dependency[]; search: string[] } {
    if (process.platform === "win32") {
      const files: Dependency[] = [];
      for (const name of windowsImports(file)) {
        if (/^(api-ms-win-|ext-ms-)/i.test(name)) continue;
        const source = roots.map((root) => lookup(root, name)).find(Boolean);
        if (source) files.push({ name: basename(source), source, reference: name });
        else if (/^(vcruntime|msvcp|concrt)/i.test(name))
          throw Error(`Missing redistributable ${name}; install the Visual C++ build tools`);
        else if (!lookup(join(process.env.SystemRoot ?? "C:/Windows", "System32"), name))
          throw Error(`Missing ${name} required by ${file}`);
      }
      return { files, search: [] };
    }
    if (process.platform === "linux") {
      const listing = run("ldd", [file], env);
      if (listing.includes("not found"))
        throw Error(`Unresolved native dependency in ${file}\n${listing}`);
      const files = listing.split("\n").flatMap((line) => {
        const match = line.trim().match(/^(\S+)\s+=>\s+(\/\S+)/);
        return match && !system.test(match[1])
          ? [{ name: match[1], source: match[2], reference: match[1] }]
          : [];
      });
      return { files, search: [] };
    }
    const search = [...inherited, sourceDirectory];
    const commands = run("otool", ["-l", file]).split("\n");
    for (let i = 0; i < commands.length; i++) {
      if (commands[i].trim() !== "cmd LC_RPATH") continue;
      search.push(
        commands[i + 2]
          .trim()
          .replace(/^path /, "")
          .replace(/ \(offset.*$/, "")
          .replace("@loader_path", dirname(file))
          .replace("@executable_path", dirname(executable)),
      );
    }
    const files = run("otool", ["-L", file])
      .split("\n")
      .slice(1)
      .flatMap((line) => {
        const reference = line.trim().split(" (compatibility")[0];
        if (!reference || reference.startsWith("/System/") || reference.startsWith("/usr/lib/"))
          return [];
        const candidates = reference.startsWith("@rpath/")
          ? search.map((root) => join(root, reference.slice(7)))
          : [
              reference
                .replace("@loader_path", dirname(file))
                .replace("@executable_path", dirname(executable)),
            ];
        const source = candidates.find(existsSync);
        if (!source) throw Error(`Missing ${reference} required by ${file}`);
        if (realpathSync(source) === realpathSync(file)) return [];
        return [{ name: basename(reference), source, reference }];
      });
    return { files, search };
  }

  for (const program of programs) {
    const executable = join(sourceDirectory, program);
    const target = join(output, program);
    if (resolve(executable) !== resolve(target)) copyFileSync(executable, target);
    const pending = [{ source: executable, target, search: [] as string[] }];
    const seen = new Set<string>();
    const libraries = new Set<string>();
    while (pending.length) {
      const current = pending.shift()!;
      if (seen.has(current.source)) continue;
      seen.add(current.source);
      const found = dependencies(current.source, executable, current.search);
      for (const dependency of found.files) {
        const key = dependency.name.toLowerCase();
        const hash = createHash("sha256").update(readFileSync(dependency.source)).digest("hex");
        const previous = staged.get(key);
        if (previous && previous.hash !== hash)
          throw Error(`Conflicting native libraries named ${dependency.name}`);
        const name = previous?.name ?? dependency.name;
        libraries.add(name);
        const destination = join(output, name);
        if (!previous) {
          if (resolve(dependency.source) !== resolve(destination))
            copyFileSync(dependency.source, destination);
          staged.set(key, { source: dependency.source, name, hash });
        }
        if (process.platform === "darwin" && !relinked.has(current.target))
          run("install_name_tool", [
            "-change",
            dependency.reference,
            "@loader_path/" + name,
            current.target,
          ]);
        pending.push({ source: dependency.source, target: destination, search: found.search });
      }
      // Every program still traverses the closure, but a shared Mach-O image is patched once.
      relinked.add(current.target);
    }
    closures[program] = [...libraries].sort();
  }
  const files = [...programs, ...[...staged.values()].map((file) => file.name)];
  if (process.platform === "linux") {
    for (const file of files) run("patchelf", ["--set-rpath", "$ORIGIN", join(output, file)]);
  } else if (process.platform === "darwin") {
    for (const file of staged.values())
      run("install_name_tool", ["-id", "@rpath/" + file.name, join(output, file.name)]);
    for (const file of files)
      run("codesign", [
        "--force",
        "--sign",
        "-",
        "--preserve-metadata=entitlements",
        join(output, file),
      ]);
  }
  const suffix = process.platform === "win32" ? ".exe" : "";
  const player = [
    ...new Set([
      ...(closures["Whatever" + suffix] ?? []),
      ...(closures["whatever-launcher" + suffix] ?? []),
    ]),
  ].sort();
  writeFileSync(
    join(output, "native-libraries.json"),
    JSON.stringify({ format: 1, player, programs: closures }, null, 2),
  );
}

if (import.meta.main) {
  const source = process.argv[process.argv.indexOf("--source") + 1];
  const output = process.argv[process.argv.indexOf("--output") + 1];
  if (
    !source ||
    !output ||
    !process.argv.includes("--source") ||
    !process.argv.includes("--output")
  )
    throw Error("Provide --source and --output for the native runtime payload");
  const destination = resolve(output);
  const input = resolve(source);
  if (destination === input || input.startsWith(destination + sep))
    throw Error("Native output overlaps its source");
  const staging = destination + ".stage-" + process.pid;
  if (existsSync(staging)) throw Error(`Staging directory already exists ${staging}`);
  if (existsSync(destination) && !existsSync(join(destination, "native-libraries.json")))
    throw Error("Native output must be new or a previous generated payload");
  try {
    const suffix = process.platform === "win32" ? ".exe" : "";
    collectNativeLibraries(resolve(source), staging, [
      "Whatever" + suffix,
      "whatever-launcher" + suffix,
    ]);
    // This output is explicitly selected and identified by its generated manifest.
    const previous = destination + ".previous-" + process.pid;
    if (existsSync(previous)) throw Error("Previous native payload staging path already exists");
    const replacing = existsSync(destination);
    if (replacing) renameSync(destination, previous);
    try {
      renameSync(staging, destination);
    } catch (error) {
      if (replacing) renameSync(previous, destination);
      throw error;
    }
    if (replacing) rmSync(previous, { recursive: true });
  } finally {
    if (existsSync(staging)) rmSync(staging, { recursive: true });
  }
}
