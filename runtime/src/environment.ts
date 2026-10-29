import type { RuntimeMode } from "./types.ts";
let mode: RuntimeMode = "client";
/** Internal host initialization; not exported by the public API. */
export function _configureMode(value: RuntimeMode): void { mode = value; }
export function _getMode(): RuntimeMode { return mode; }
export function _requireClient(operation: string): void {
  if (mode !== "client") throw new Error(operation + " is unavailable in server mode");
}


