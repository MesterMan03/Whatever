import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "../../runtime/node_modules/typescript/lib/typescript.js";

const root = fileURLToPath(new URL("../../", import.meta.url));
const output = path.join(root, "website/.astro/examples");
await mkdir(output, { recursive: true });
const docs = ["getting-started/first-mod", "guides/components", "guides/scenes-and-assets", "guides/client-and-server", "guides/debugging"];
// Import shared context only for snippets that continue an earlier example.
// Complete examples keep their own imports unchanged.
const context = 'import { Engine, Input, Time, Window, Scene, BuiltInComponents, Component, Type, field, Console } from "@whatever-engine/api";\n';
const entries = [];
for (const doc of docs) {
  const text = await readFile(path.join(root, `website/src/content/docs/${doc}.md`), "utf8");
  let index = 0;
  for (const match of text.matchAll(/```ts[^\n]*\n([\s\S]*?)```/g)) {
    index++;
    const code = match[1];
    // The attachment example imports the class from the first components block.
    const filename = doc === "guides/components" && index === 1 ? "spin.ts" : `${doc.replaceAll("/", "-")}-${index}.ts`;
    const file = path.join(output, filename);
    await writeFile(file, `${code.includes("import ") ? "" : context}${code}`);
    entries.push(file);
  }
}
const overview = await readFile(path.join(root, "website/src/components/Overview.astro"), "utf8");
const homepageExample = overview.match(/const code = `([\s\S]*?)`;/)?.[1];
if (!homepageExample) throw new Error("Homepage example missing; update example extraction.");
const homepageFile = path.join(output, "homepage.ts");
await writeFile(homepageFile, homepageExample);
entries.push(homepageFile);

const program = ts.createProgram(entries, {
  noEmit: true,
  strict: true,
  skipLibCheck: false,
  target: ts.ScriptTarget.ESNext,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  allowImportingTsExtensions: true,
  types: [],
  paths: { "@whatever-engine/api": [path.join(root, "runtime/dist/index.d.ts")] },
});
const diagnostics = ts.getPreEmitDiagnostics(program);
if (diagnostics.length) {
  console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: (file) => file,
    getCurrentDirectory: () => root,
    getNewLine: () => "\n",
  }));
  process.exitCode = 1;
} else console.log(`Type-checked ${entries.length} documentation examples against the rebuilt API declarations.`);
