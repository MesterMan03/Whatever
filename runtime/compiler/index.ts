import ts from "typescript";
import { setSources } from "./environment.ts";
import { inspectComponents } from "./components.ts";

function commonJs(text: string, name: string, output: string): string {
  const source = ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true);
  const dependencies = new Set<string>();
  const exports = new Set<string>();
  let usesCommonJs = /\.[cm]?[jt]s$/.test(name) && /\.(cjs|cts)$/.test(name);
  function visit(node: ts.Node): void {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === "require"
    ) {
      usesCommonJs = true;
      const argument = node.arguments[0];
      if (node.arguments.length !== 1 || !argument || !ts.isStringLiteral(argument))
        throw Error(`${name}: require() needs a literal local source path`);
      dependencies.add(argument.text);
    }
    if (
      ts.isBinaryExpression(node) &&
      node.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
      ts.isPropertyAccessExpression(node.left)
    ) {
      const target = node.left.expression.getText(source);
      if (target === "exports" || target === "module.exports") {
        usesCommonJs = true;
        exports.add(node.left.name.text);
      } else if (node.left.getText(source) === "module.exports") {
        usesCommonJs = true;
        if (ts.isObjectLiteralExpression(node.right))
          for (const property of node.right.properties) {
            if (property.name && ts.isIdentifier(property.name)) exports.add(property.name.text);
          }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  if (!usesCommonJs) return output;
  const imports = [...dependencies]
    .map(
      (specifier, index) => `import * as __dependency${index} from ${JSON.stringify(specifier)};`,
    )
    .join("\n");
  const cases = [...dependencies]
    .map(
      (specifier, index) =>
        `case ${JSON.stringify(specifier)}:return __dependency${index}.default ?? __dependency${index};`,
    )
    .join("");
  const prelude = `const module={exports:{}};let exports=module.exports;const require=name=>{switch(name){${cases}default:throw Error('Module is not a declared local dependency');}};\n`;
  if (ts.isExternalModule(source)) return imports + "\n" + prelude + output;
  const named = [...exports]
    .filter((value) => value !== "default" && /^[A-Za-z_$][\w$]*$/.test(value))
    .map((value) => `export const ${value}=module.exports[${JSON.stringify(value)}];`)
    .join("\n");
  return (
    imports +
    "\n" +
    prelude +
    "(function(module,exports,require){\n" +
    output +
    "\n})(module,exports,require);\nexport default module.exports;\n" +
    named
  );
}

export function compileSources(files: Record<string, string>, owner: string) {
  setSources(files);
  const definitions = inspectComponents("/mod", owner);
  const modules: Record<string, string> = Object.create(null);
  for (const [name, text] of Object.entries(files)) {
    if (name.endsWith(".json")) {
      modules[name] = "export default " + JSON.stringify(JSON.parse(text));
      continue;
    }
    const result = ts.transpileModule(text, {
      fileName: name,
      reportDiagnostics: true,
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
        jsx: ts.JsxEmit.ReactJSX,
        esModuleInterop: true,
        useDefineForClassFields: true,
      },
    });
    const failures =
      result.diagnostics?.filter((d) => d.category === ts.DiagnosticCategory.Error) ?? [];
    if (failures.length)
      throw Error(
        name +
          ": " +
          failures.map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n")).join("\n"),
      );
    modules[name] =
      commonJs(text, name, result.outputText) + "\n//# sourceURL=" + owner + "://" + name;
  }
  return { definitions, modules };
}
(globalThis as any).__compileSources = compileSources;
