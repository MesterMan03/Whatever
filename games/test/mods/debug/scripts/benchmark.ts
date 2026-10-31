import type { CommandSpec } from "@whatever-engine/api";
export const benchmarkCommand: CommandSpec = {
  name: "benchmark",
  description: "Bun Mandelbrot/ASCII benchmark (single mod thread)",
  args: [{ name: "iterations", type: "int", required: false }],
  handler: ({ iterations }) => {
    const limit = (iterations as number | undefined) ?? 128;
    if (limit < 1 || limit > 2048) throw new Error("Iterations must be 1–2048");
    const started = Date.now(),
      lines: string[] = [];
    const chars = " .:-=+*#%@";
    for (let y = 0; y < 32; y++) {
      let line = "";
      for (let x = 0; x < 80; x++) {
        const cx = -2.5 + (x / 79) * 3.5,
          cy = -0.9 + (y / 31) * 1.8;
        let zx = 0,
          zy = 0,
          n = 0;
        while (n < limit && zx * zx + zy * zy <= 4) {
          const next = zx * zx - zy * zy + cx;
          zy = 2 * zx * zy + cy;
          zx = next;
          n++;
        }
        line +=
          n === limit
            ? " "
            : chars[Math.min(chars.length - 1, Math.floor((n / limit) * chars.length))];
      }
      lines.push(line);
    }
    return [...lines, `Mandelbrot: ${limit} iterations, ${Date.now() - started} ms`];
  },
};
