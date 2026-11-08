import { readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const destination = path.join(root, "website/src/content/docs");
const github = "https://github.com/MesterMan03/Whatever/blob/main/";
const edit = "https://github.com/MesterMan03/Whatever/edit/main/";
const read = (name) => readFile(path.join(root, name), "utf8").then((s) => s.replace(/\r\n/g, "\n"));
const slug = (text) => text.toLowerCase().replace(/[`]/g, "").replace(/[^\p{L}\p{N}_\-\s]/gu, "").trim().replace(/\s/g, "-");

// Split only real Markdown headings, never example code inside fenced blocks.
function sections(markdown) {
  const result = new Map();
  let current = "Introduction";
  let fence = null;
  result.set(current, []);
  for (const line of markdown.split("\n")) {
    const marker = line.match(/^\s*(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
    }
    if (!fence && line.startsWith("## ")) {
      current = line.slice(3);
      if (result.has(current)) throw new Error(`Duplicate reference heading: ${current}`);
      result.set(current, []);
    }
    result.get(current).push(line);
  }
  return result;
}

const scripting = sections(await read("docs/SCRIPTING.md"));
const groups = [
  ["overview", "Scripting overview", ["Introduction"]],
  ["engine", "Engine, input, and window", ["`Engine`", "`Input`", "`Window`"]],
  ["timing", "Ticks, snapshots, and pending results", ["`Time`", "Tick order and direct calls", "Live scene access and retained snapshots", "`Pending<T>` compatibility requests", "`Profiler`"]],
  ["components", "Schemas and custom components", ["`Type`, `Struct`, and custom `Component` classes"]],
  ["entities", "Entities and references", ["`Entity`", "Entity lifecycle and startup scenes"]],
  ["scenes", "Scenes and built-in components", ["`Scene`"]],
  ["physics", "Physics API", ["`Physics`"]],
  ["audio", "Audio", ["`Audio`"]],
  ["files", "Persistent files", ["`File`"]],
  ["mods", "Mods and messaging", ["`Mods`", "`Message`"]],
  ["console", "Console commands", ["`Console`"]],
  ["network", "Networking", ["`Network`"]],
  ["sides", "Runtime sides and entries", ["Mod sides and script entries"]],
];
const sectionRoutes = new Map(groups.flatMap(([route, , headings]) => headings.map((h) => [slug(h), `reference/api/${route}/`])));
const docRoutes = new Map([
  ["docs/ASSETS.md", "reference/assets/"],
  ["docs/EDITOR.md", "reference/editor/"],
  ["docs/PHYSICS.md", "reference/physics/"],
  ["docs/SHADER.md", "reference/shaders/"],
  ["docs/SANDBOX.md", "reference/sandbox/"],
  ["docs/IPC.md", "protocol/specification/"],
  ["docs/SCRIPTING.md", "reference/api/overview/"],
]);

function links(markdown, source) {
  return markdown.replace(/\]\(([^\s)]+)\)/g, (all, href) => {
    if (/^(?:[a-z]+:|#|\/)/i.test(href)) return all;
    const [filename, fragment] = href.split("#");
    const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(source), filename));
    let route = docRoutes.get(resolved);
    if (resolved === "docs/SCRIPTING.md" && fragment) route = sectionRoutes.get(fragment) || route;
    return route ? `](/${route}${fragment ? `#${fragment}` : ""})` : `](${github}${resolved}${fragment ? `#${fragment}` : ""})`;
  });
}

async function page(route, title, body, source, order) {
  const filename = path.join(destination, `${route}.md`);
  await mkdir(path.dirname(filename), { recursive: true });
  // Sources stay in their original contributor-facing locations; output is ignored.
  const frontmatter = ["---", `title: ${JSON.stringify(title)}`, `editUrl: ${edit}${source}`];
  if (order !== undefined) frontmatter.push(`sidebar: { order: ${order} }`);
  frontmatter.push("---", "", `<!-- Generated from ${source}. Edit the source, then run bun run docs:generate. -->`, "");
  await writeFile(filename, `${frontmatter.join("\n")}${links(body, source).replace(/^# [^\n]+\n/, "")}\n`);
}

const used = new Set(["Internal scripting ABI reference"]);
for (const [index, [route, title, headings]] of groups.entries()) {
  const body = headings.map((h) => {
    if (!scripting.has(h)) throw new Error(`Scripting reference changed: missing '${h}'. Update website/scripts/generate-docs.mjs.`);
    used.add(h);
    return scripting.get(h).join("\n");
  }).join("\n\n");
  await page(`reference/api/${route}`, title, body, "docs/SCRIPTING.md", index + 1);
}
const unmapped = [...scripting.keys()].filter((h) => !used.has(h));
if (unmapped.length) throw new Error(`Unmapped scripting reference sections: ${unmapped.join(", ")}`);
for (const [source, route] of docRoutes) {
  if (source === "docs/SCRIPTING.md") continue;
  const markdown = await read(source);
  const title = markdown.match(/^# (.+)$/m)?.[1];
  if (!title) throw new Error(`Missing title in ${source}`);
  await page(route.replace(/\/$/, ""), title, markdown, source);
}

// The native enums are simple named-field variants. Fail on unfamiliar syntax
// rather than silently omitting a new wire message. This is a catalogue generator,
// not a Rust parser or a claim that dynamic subsystem JSON is fully typed here.
function definition(source, kind, name) {
  const start = source.indexOf(`pub ${kind} ${name} {`);
  if (start < 0) throw new Error(`Native IPC definition missing: ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 1;
  let end = bodyStart + 1;
  for (; end < source.length && depth; end++) {
    if (source[end] === "{") depth++;
    else if (source[end] === "}") depth--;
  }
  if (depth) throw new Error(`Unclosed definition: ${name}`);
  return source.slice(bodyStart + 1, end - 1);
}

function wireType(type) {
  const generic = type.match(/^(Option|Vec|Box)<(.+)>$/);
  if (generic) {
    const inner = wireType(generic[2]);
    return generic[1] === "Option" ? `${inner} or null` : generic[1] === "Vec" ? `array of ${inner}` : inner;
  }
  const map = type.match(/^(?:HashMap|BTreeMap)<String, (.+)>$/);
  if (map) return `object mapping strings to ${wireType(map[1])}`;
  const array = type.match(/^\[(.+); (\d+)\]$/);
  if (array) return `array of exactly ${array[2]} ${wireType(array[1])} values`;
  if (type === "String") return "string";
  if (type === "bool") return "boolean";
  if (/^[ui]\d+$/.test(type)) return `${type} integer`;
  if (type === "usize") return "nonnegative native-size integer";
  if (/^f\d+$/.test(type)) return `${type} finite number`;
  if (type === "serde_json::Value" || type === "Value") return "JSON value";
  if (type.endsWith("RuntimeMode")) return '`"client"` or `"server"`';
  return `\`${type.split("::").at(-1)}\``;
}

function fields(body, direction) {
  const output = [];
  let attributes = "";
  for (const line of body.split("\n")) {
    if (line.trim().startsWith("#[")) { attributes += line.trim(); continue; }
    const match = line.match(/^\s*(?:pub )?(\w+): (.+),$/);
    if (!match) continue;
    const [, original, type] = match;
    const name = attributes.match(/rename = "([^"]+)"/)?.[1] || original;
    let requirement = "Required";
    if (attributes.includes("skip_serializing_if")) requirement = "May be omitted";
    else if (attributes.includes("default_recursive")) requirement = "Defaults to true";
    else if (attributes.includes("default")) requirement = type === "String" ? 'Defaults to `""`' : type === "bool" ? "Defaults to false" : "Defaults to empty";
    else if (type.startsWith("Option<")) requirement = direction === "input" ? "May be omitted or null" : "Present; may be null";
    if (attributes.includes("flatten")) {
      output.push("| *(flattened event fields)* | JSON object fields on the message itself | See network event specification; there is no `payload` property | ");
    } else output.push(`| \`${name}\` | ${wireType(type)} | ${requirement} |`);
    attributes = "";
  }
  if (!output.length) throw new Error("Native definition unexpectedly has no fields");
  return `| Field | JSON representation | Presence / default |\n| --- | --- | --- |\n${output.join("\n")}`;
}

const ipc = await read("engine/src/script/ipc.rs");
let catalogue = "These tables are generated from the native Rust definitions at build time. Property names and message tags are literal wire names. Commands execute through the in-process Wasm ABI; see the [internal ABI](/protocol/specification/) for sequencing, validation, dynamic subsystem payloads, and errors.\n\nInteger widths are significant: `u32` is 0–4294967295; `u64` is 0–18446744073709551615; signed integers use their corresponding signed range. JavaScript implementations need an exact-integer strategy for large counters. `f32` and `f64` are finite JSON numbers. A `JSON value` delegates validation to the named subsystem, rather than permitting arbitrary operations.\n";
for (const [name, title, direction] of [
  ["EngineMessage", "Engine → VM deliveries", "output"],
  ["ScriptMessage", "Application commands", "input"],
]) {
  const body = definition(ipc, "enum", name);
  const variants = [...body.matchAll(/^    (\w+) \{\n([\s\S]*?)^    \},?$/gm)];
  const residue = body.replace(/^    (\w+) \{\n([\s\S]*?)^    \},?$/gm, "").replace(/^\s*\/\/[^\n]*/gm, "").trim();
  if (!variants.length || residue) throw new Error(`Unsupported ${name} syntax; update catalogue generation: ${residue}`);
  catalogue += `\n## ${title}\n\n${variants.length} variants. Native definition: [\`${name}\`](${github}engine/src/script/ipc.rs).\n`;
  for (const [, variant, members] of variants) {
    catalogue += `\n### \`${variant}\`\n\nDiscriminator: \`"type": "${variant}"\`.\n\n${fields(members, direction)}\n`;
  }
}
catalogue += "\n## Shared objects\n\nThese objects appear inside messages; they do not have a `type` discriminator of their own. `TagQuery.includeDisabled` uses camelCase. Engine-emitted optional fields normally serialize as null; `EntityDelta.metadata` and `ModManifestDto.script` can be absent.\n";
for (const [source, names, direction] of [
  ["engine/src/script/ipc.rs", ["QueryResultDto", "ModManifestDto", "LoadOrderDto", "ScriptConfigDto"], "output"],
  ["engine/src/script/ipc.rs", ["CommandNodeSpec", "ArgSpecDto"], "input"],
  ["engine/src/ecs/snapshot.rs", ["SceneDelta", "EntitySnapshot", "EntityMetadata", "EntityDelta"], "output"],
  ["engine/src/scene/document.rs", ["LoadedScene"], "output"],
  ["engine/src/runtime/performance.rs", ["TickTimings", "FrameTimings"], "output"],
]) {
  const text = await read(source);
  for (const name of names) catalogue += `\n### \`${name}\`\n\n[Native definition](${github}${source}).\n\n${fields(definition(text, "struct", name), direction)}\n`;
}
catalogue += "\n### `TagQuery`\n\n```json\n{\"all\":[],\"any\":[],\"none\":[],\"includeDisabled\":false}\n```\n\nAll four fields default to empty arrays / false when omitted; unknown fields are rejected. Tag strings are 1–128 UTF-8 bytes with no control characters. An entity has at most 32 unique tags.\n";
await page("protocol/messages", "Native message catalogue", catalogue, "engine/src/script/ipc.rs");
console.log(`Generated ${groups.length + docRoutes.size} documentation pages from canonical sources.`);
