import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { build, type Plugin } from "esbuild";

const root = resolve(import.meta.dirname, "..");
const output = resolve(root, "runtime/guest/generated");
const compilerEnvironment: Plugin = {
  name: "contained-compiler-environment",
  setup(build) {
    build.onResolve({ filter: /^(node:)?(fs|path|os|perf_hooks)$/ }, () => ({
      path: resolve(root, "runtime/compiler/environment.ts"),
    }));
  },
};

await build({
  absWorkingDir: root,
  entryPoints: ["runtime/src/index.ts"],
  bundle: true,
  platform: "browser",
  format: "esm",
  outfile: "runtime/dist/index.js",
  footer: { js: "// With love from the Whatever team <3\n// https://github.com/MesterMan03/Whatever" },
});

mkdirSync(output, { recursive: true });
for (const [name, entry, compiler] of [
  ["api", "runtime/embedded/entry.ts", false],
  ["compiler", "runtime/compiler/index.ts", true],
] as const) {
  const result = await build({
    absWorkingDir: root,
    entryPoints: [entry],
    bundle: true,
    write: false,
    platform: "browser",
    format: compiler ? "cjs" : "esm",
    minify: true,
    loader: { ".d.ts": "text" },
    plugins: compiler ? [compilerEnvironment] : [],
  });
  const source = result.outputFiles[0]?.text;
  if (!source) throw Error(`No generated source for ${name}`);
  writeFileSync(
    resolve(output, `${name}.js`),
    compiler ? `(function(module,exports){${source}\n})(globalThis.__bundle={exports:{}},globalThis.__bundle.exports);\n` : source,
  );
}
