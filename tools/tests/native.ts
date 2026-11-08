import { resolve } from "node:path";

const repository = resolve(import.meta.dirname, "../..");
const profile = process.env.WHATEVER_TEST_PROFILE ?? "debug";
if (profile !== "debug" && profile !== "release")
  throw Error("WHATEVER_TEST_PROFILE must be debug or release");

/** Run integration tests against the same binaries that will be distributed. */
export function nativeBinary(name: string): string {
  if (name === "Whatever" && process.env.WHATEVER_TEST_PLAYER)
    return resolve(process.env.WHATEVER_TEST_PLAYER);
  return resolve(
    repository,
    process.env.CARGO_TARGET_DIR ?? "target",
    profile,
    name + (process.platform === "win32" ? ".exe" : ""),
  );
}
