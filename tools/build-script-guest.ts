import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = join(root, "runtime/guest/Cargo.toml");
const environment = { ...process.env };

function run(args: string[], capture = false): string {
  const child = spawnSync("cargo", args, {
    cwd: root,
    env: environment,
    stdio: ["ignore", capture ? "pipe" : "inherit", "inherit"],
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  if (child.status !== 0) {
    throw Error(`cargo ${args[0]} failed; the Rust wasm32-unknown-unknown target and a Wasm-capable clang are required.`, { cause: child.error });
  }
  return child.stdout ?? "";
}

if (process.platform === "win32") {
  const tools = join(root, "target/build-tools");
  const llvm = existsSync(tools) && readdirSync(tools).find(name => name.startsWith("clang+llvm-"));
  if (llvm) {
    environment.CC_wasm32_unknown_unknown ??= join(tools, llvm, "bin/clang.exe");
    environment.AR_wasm32_unknown_unknown ??= join(tools, llvm, "bin/llvm-ar.exe");
  }
  // Clang canonicalizes the crate's -isystem directory to a Windows extended path,
  // then cannot resolve nested headers through it. Use a separate ordinary include path.
  const metadata = JSON.parse(run(["metadata", "--manifest-path", manifest, "--format-version", "1", "--locked"], true));
  const dependency = metadata.packages.find((pkg: { name: string }) => pkg.name === "rquickjs-sys");
  if (!dependency) throw Error("QuickJS dependency metadata is missing");
  const headers = join(root, "target/quickjs-wasm-headers");
  cpSync(join(dirname(dependency.manifest_path), "vendor/wasi-libc/include"), headers, { recursive: true });
  environment.CFLAGS_wasm32_unknown_unknown = `${environment.CFLAGS_wasm32_unknown_unknown ?? ""} -I"${headers.replaceAll("\\", "/")}"`;
  environment.CC_SHELL_ESCAPED_FLAGS = "1";
}

run(["build", "--manifest-path", manifest, "--target", "wasm32-unknown-unknown", "--target-dir", join(root, "target/script-guest"), "--release", "--locked"]);
const output = join(root, "runtime/dist/whatever-script-guest.wasm");
mkdirSync(dirname(output), { recursive: true });
cpSync(join(root, "target/script-guest/wasm32-unknown-unknown/release/whatever_script_guest.wasm"), output);
console.log(`Built ${output}`);
