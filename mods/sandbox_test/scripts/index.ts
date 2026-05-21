import {$} from "bun";
import {Console, Engine} from "@whatever/api";

async function run(label: string, fn: () => Promise<unknown>) {
  try {
    const result = await fn();
    Engine.log("info", `PASS (unexpected) ${label}: ${JSON.stringify(result)?.slice(0, 80)}`);
  } catch (e: any) {
    Engine.log("info", `BLOCKED ${label}: ${e?.code ?? e?.message ?? e}`);
  }
}

async function runExpectSuccess(label: string, fn: () => Promise<unknown>) {
  try {
    await fn();
    Engine.log("info", `OK ${label}`);
  } catch (e: any) {
    Engine.log("info", `FAIL (regression) ${label}: ${e?.code ?? e?.message ?? e}`);
  }
}

// --- FS: should be BLOCKED ---

Console.register({
  name: "sandboxtest",
  handler: async (_) => {
    await run("read ~/.bashrc", () =>
        Bun.file(process.env.HOME + "/.bashrc").text());

    await run("read ~/.ssh/id_rsa", () =>
        Bun.file(process.env.HOME + "/.ssh/id_rsa").text());

    await run("write to home dir", () =>
        Bun.write(process.env.HOME + "/sandbox_escape.txt", "pwned"));

    await run("write to /etc", () =>
        Bun.write("/etc/sandbox_escape", "pwned"));

    await run("read /proc/1/maps (other process)", () =>
        Bun.file("/proc/1/maps").text());

    await run("read /root/.bashrc", () =>
        Bun.file("/root/.bashrc").text());

    // --- Network: should be BLOCKED ---

    await run("fetch IPv4 (http)", () =>
        fetch("http://example.com").then(r => r.status));

    await run("fetch IPv4 (https)", () =>
        fetch("https://example.com").then(r => r.status));

    await run("raw TCP socket", async () => {
      return await Bun.connect({ hostname: "1.1.1.1", port: 53, socket: {} });
    });

    // --- Subprocess: should be BLOCKED (no Execute outside sys paths) ---
    // Note: if /usr got Execute, ls may work — that's a known trade-off

    await run("spawn ls /", async () => {
      const p = $`ls /`;
      return p.text();
    });

    await run("spawn curl", async () => {
      const p = $`curl -s https://example.com`;
      return p.text();
    });

    // --- FS: should SUCCEED ---

    await runExpectSuccess("write to /tmp", () =>
        Bun.write("/tmp/sandbox_ok.txt", "allowed"));

    await runExpectSuccess("read own script dir", () =>
        Bun.file(import.meta.dir + "/index.ts").text());

    return "yay?";
  }
});
