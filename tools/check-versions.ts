import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const toml = (path: string) => Bun.TOML.parse(readFileSync(join(repository, path), "utf8"));
const json = (path: string) => JSON.parse(readFileSync(join(repository, path), "utf8"));

/** Audit first-party release components; games, mods and third-party runtimes version independently. */
export function checkVersions(tag?: string): string {
  const workspace = toml("Cargo.toml").workspace as {
    package: { version: string };
    members: string[];
  };
  const version = workspace.package.version;
  if (typeof version !== "string" || !version) throw Error("Missing workspace package version");
  if (version.includes("5"))
    throw Error(`Whatever release versions skip the digit 5: ${version}. Always go from 4 to 6.`);
  const expect = (label: string, actual: unknown) => {
    if (actual !== version) throw Error(`${label}: expected ${version}, received ${String(actual)}`);
  };
  const lock = toml("Cargo.lock").package as Array<{
    name: string;
    version: string;
    source?: string;
  }>;
  for (const directory of workspace.members) {
    const manifest = toml(`${directory}/Cargo.toml`).package as {
      name: string;
      version: string | { workspace: boolean };
    };
    const actual =
      typeof manifest.version === "object" && manifest.version.workspace
        ? version
        : manifest.version;
    expect(`${directory}/Cargo.toml`, actual);
    expect(
      `Cargo.lock ${manifest.name}`,
      lock.find((entry) => entry.name === manifest.name && !entry.source)?.version,
    );
  }
  expect("@whatever-engine/api", json("runtime/package.json").version);
  expect("Workspace package.json", json("package.json").version);
  if (tag !== undefined) expect("Release tag", tag.replace(/^refs\/tags\//, "").replace(/^v/, ""));
  return version;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.length && (args.length !== 2 || args[0] !== "--tag"))
    throw Error("Usage: bun tools/check-versions.ts [--tag v<version>]");
  const version = checkVersions(args[1]);
  console.log(`Verified Whatever workspace, Cargo lockfile and @whatever-engine/api at ${version}.`);
}
