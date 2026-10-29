import { Engine, File, type CommandSpec } from "@whatever-engine/api";

export const sandboxCommand: CommandSpec = {
  name: "sandbox",
  description: "Verify the mod runtime's module and global allowlists",
  handler: async () => {
    const results: string[] = [];
    for (const name of [
      "node:fs",
      "fs",
      "node:child_process",
      "node:module",
      "bun",
      "bun:ffi",
      "not-an-allowed-package",
    ]) {
      try {
        await import(name);
        throw new Error("Import unexpectedly succeeded: " + name);
      } catch (error) {
        if (!String(error).includes("Whatever module policy denied:")) throw error;
        results.push("BLOCKED import " + name);
      }
    }
    for (const name of ["Bun", "process", "fetch", "WebSocket", "Worker", "ShadowRealm"]) {
      if ((globalThis as Record<string, unknown>)[name] !== undefined) {
        throw new Error("Global unexpectedly exposed: " + name);
      }
      results.push("BLOCKED global " + name);
    }
    const path = `runtime-policy-probe-${Date.now()}.txt`;
    try {
      await File.write(path, "broker works");
      if ((await File.read(path)) !== "broker works")
        throw new Error("File broker roundtrip failed");
      results.push("OK engine file broker");
    } finally {
      await File.delete(path);
    }
    for (const path of [
      "../escape.txt",
      "C:\\escape.txt",
      "/escape.txt",
      "folder\\..\\escape.txt",
    ]) {
      try {
        await File.read(path);
        throw new Error("File path unexpectedly accepted: " + path);
      } catch (error) {
        if (!String(error).includes("invalid relative path:")) throw error;
        results.push("BLOCKED file path " + path);
      }
    }
    for (const result of results) Engine.log("info", result);
    return results.length + " runtime policy checks passed";
  },
};
