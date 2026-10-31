import { read as readFd, writeSync as writeFd } from "node:fs";

// Capture trusted operations before lockdown. Only the fixed IPC descriptors are
// used; neither the functions nor pipe handles are exposed through the public API.
const ReadPromise = Promise;
const encoder = new TextEncoder();
const encode = encoder.encode.bind(encoder);
export const _stdin = {
  read(): Promise<{ done: boolean; value: Uint8Array }> {
    const buffer = new Uint8Array(64 * 1024);
    return new ReadPromise((resolve, reject) => {
      readFd(0, buffer, 0, buffer.byteLength, null, (error, bytesRead) => {
        if (error) reject(error);
        else resolve({ done: bytesRead === 0, value: buffer.subarray(0, bytesRead) });
      });
    });
  },
};
export function _write(data: string): void {
  const bytes = encode(data);
  let offset = 0;
  while (offset < bytes.byteLength) {
    const written = writeFd(1, bytes, offset, bytes.byteLength - offset);
    if (written <= 0) throw new Error("The engine IPC pipe stopped accepting output");
    offset += written;
  }
}
export const _exit = (process as unknown as { reallyExit(code: number): never }).reallyExit.bind(
  process,
);

const hrtime = process.hrtime.bigint.bind(process.hrtime);
const memory = process.memoryUsage.bind(process);
const epoch = hrtime();
export const _now = (): number => Number(hrtime() - epoch) / 1e6;
export const _memory = (): { heap_used: number; rss: number } => {
  const m = memory();
  return { heap_used: m.heapUsed, rss: m.rss };
};
