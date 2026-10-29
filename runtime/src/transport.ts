// Trusted pipe handles are captured once before the native policy is activated.
// Describe only the private methods used here so public API declarations stay runtime-neutral.
const native = (globalThis as unknown as { Bun: {
  stdin: { stream(): { getReader(): { read(): Promise<{ done: boolean; value: Uint8Array }> } } };
  stdout: { writer(): { write(data: string): unknown; flush(): unknown } };
} }).Bun;
export const _stdin = native.stdin.stream().getReader();
const stdout = native.stdout.writer();
export function _write(data: string): void { stdout.write(data); stdout.flush(); }
export const _exit = (process as unknown as { reallyExit(code: number): never }).reallyExit.bind(process);