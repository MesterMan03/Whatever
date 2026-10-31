import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../dist/", import.meta.url));
const base = `/${(process.env.DOCS_BASE || "/").replace(/^\/+|\/+$/g, "")}`.replace(/\/$/, "") || "";
const origin = "https://docs.invalid";
async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map((e) => e.isDirectory() ? files(path.join(directory, e.name)) : path.join(directory, e.name)))).flat();
}
const htmlFiles = (await files(root)).filter((file) => file.endsWith(".html"));
const failures = [];
const contents = new Map();
const read = async (file) => {
  if (!contents.has(file)) contents.set(file, await readFile(file, "utf8"));
  return contents.get(file);
};
let checked = 0;
for (const file of htmlFiles) {
  const relative = path.relative(root, file).replaceAll(path.sep, "/");
  const current = new URL(`${base}/${relative.replace(/index\.html$/, "")}`, origin);
  const html = await read(file);
  for (const [, , raw] of html.matchAll(/\b(href|src)="([^"]+)"/g)) {
    const href = raw.replaceAll("&amp;", "&");
    if (/^(?:[a-z][\w+.-]*:|\/\/)/i.test(href)) continue;
    const url = new URL(href, current);
    if (url.origin !== origin) continue;
    if (base && url.pathname !== base && !url.pathname.startsWith(`${base}/`)) {
      failures.push(`${relative}: link escapes deployment base: ${href}`);
      continue;
    }
    const suffix = decodeURIComponent(url.pathname.slice(base.length));
    let target = path.join(root, suffix);
    try {
      if ((await stat(target)).isDirectory()) target = path.join(target, "index.html");
      await stat(target);
      if (url.hash && target.endsWith(".html")) {
        const id = decodeURIComponent(url.hash.slice(1));
        const targetHtml = await read(target);
        if (!targetHtml.includes(`id="${id}"`) && !targetHtml.includes(`name="${id}"`)) {
          failures.push(`${relative}: missing anchor ${href}`);
        }
      }
      checked++;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      failures.push(`${relative}: missing target ${href}`);
    }
  }
}
if (failures.length) throw new Error(`Broken documentation links:\n${[...new Set(failures)].join("\n")}`);
console.log(`Verified ${checked} internal links and assets across ${htmlFiles.length} HTML pages (base: ${base || "/"}).`);
