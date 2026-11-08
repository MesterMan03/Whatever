import { spawn, type SpawnOptionsWithoutStdio } from "node:child_process";
import { once } from "node:events";

/** Capture a child process and resolve its status once all output streams close. */
export function spawnCaptured([command, ...args]: string[], options: SpawnOptionsWithoutStdio = {}) {
  if (!command) throw Error("Missing child process command");
  const child = spawn(command, args, { ...options, stdio: "pipe", windowsHide: true });
  const exited: Promise<number> = once(child, "close").then(([code]) => code ?? 1);
  // Callers may consume a stream before awaiting the exit status.
  void exited.catch(() => {});
  return Object.assign(child, { exited });
}
