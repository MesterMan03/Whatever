import ts from "typescript";
import { readdirSync, readFileSync, realpathSync } from "node:fs";
import { resolve, relative, isAbsolute, extname, sep } from "node:path";
import apiDeclarations from "../dist/index.d.ts" with { type: "text" };
import standardLibrary from "../node_modules/typescript/lib/lib.es5.d.ts" with { type: "text" };
import { Type, DataType, defineStruct, toWire, type Schema } from "../src/schema.ts";
import type { ComponentDefinition, FieldEditor } from "../src/component.ts";

/** Source inspection only. Never imports a mod, executes an initializer, or reads project config. */
export function inspectComponents(directory: string, owner: string): ComponentDefinition[] {
  const root = realpathSync(directory);
  const apiPath = resolve(root, "__whatever_api__.d.ts");
  const libPath = resolve(root, "__whatever_lib__.d.ts");
  const sources = new Map<string, string>([
    [apiPath, apiDeclarations],
    [libPath, standardLibrary],
  ]);
  let bytes = 0;
  function contained(path: string): boolean {
    const local = relative(root, path);
    return !!local && !isAbsolute(local) && local !== ".." && !local.startsWith(".." + sep);
  }
  function walk(path: string): void {
    for (const item of readdirSync(path, { withFileTypes: true })) {
      if (
        ["node_modules", ".git", ".whatever", "assets"].includes(item.name) ||
        item.isSymbolicLink()
      )
        continue;
      const child = resolve(path, item.name);
      if (item.isDirectory()) walk(child);
      else if (/\.(?:[cm]?[jt]s|[jt]sx)$/.test(item.name) && !item.name.endsWith(".d.ts")) {
        const text = readFileSync(child, "utf8");
        bytes += Buffer.byteLength(text);
        if (sources.size > 10_000 || bytes > 32 * 1024 * 1024)
          throw new Error("Component source inspection exceeds its file/byte limit");
        sources.set(child, text);
      }
    }
  }
  walk(root);
  const options: ts.CompilerOptions = {
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.ESNext,
    strict: true,
    allowJs: true,
    noLib: true,
    types: [],
    typeRoots: [],
    noEmit: true,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
  };
  const host = ts.createCompilerHost(options);
  host.getSourceFile = (path, language) => {
    const source = sources.get(resolve(path));
    return source === undefined ? undefined : ts.createSourceFile(path, source, language, true);
  };
  host.getCurrentDirectory = () => root;
  host.getDirectories = () => [];
  host.directoryExists = (path) =>
    resolve(path) === root ||
    [...sources.keys()].some((file) => file.startsWith(resolve(path) + sep));
  host.fileExists = (path) => sources.has(resolve(path));
  host.readFile = (path) => sources.get(resolve(path));
  host.resolveModuleNames = (names, containing) =>
    names.map((name) => {
      if (name === "@whatever-engine/api")
        return { resolvedFileName: apiPath, extension: ts.Extension.Dts };
      if (!name.startsWith(".")) return undefined;
      const base = resolve(containing, "..", name);
      for (const candidate of [
        base,
        ...[".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", "/index.ts", "/index.js"].map(
          (ext) => base + ext,
        ),
      ]) {
        if (contained(candidate) && sources.has(candidate))
          return { resolvedFileName: candidate, extension: extname(candidate) as ts.Extension };
      }
      return undefined;
    });
  const program = ts.createProgram([...sources.keys()], options, host);
  const checker = program.getTypeChecker();
  const fail = (node: ts.Node, message: string): never => {
    const file = node.getSourceFile();
    const position = file.getLineAndCharacterOfPosition(node.getStart(file));
    throw new Error(
      `${relative(root, file.fileName)}:${position.line + 1}:${position.character + 1}: ${message}`,
    );
  };
  function apiName(node: ts.Node, name: string): boolean {
    let symbol = checker.getSymbolAtLocation(node);
    if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
    return (
      symbol?.name === name &&
      !!symbol.declarations?.some((d) => resolve(d.getSourceFile().fileName) === apiPath)
    );
  }
  function isComponent(type: ts.Type, seen = new Set<ts.Type>()): boolean {
    if (seen.has(type)) return false;
    seen.add(type);
    if (
      type.symbol?.name === "Component" &&
      type.symbol.declarations?.some((d) => resolve(d.getSourceFile().fileName) === apiPath)
    )
      return true;
    return !!(type as ts.InterfaceType).getBaseTypes?.()?.some((base) => isComponent(base, seen));
  }
  function literal(node: ts.Expression, seen = new Set<ts.Node>()): any {
    if (seen.size > 32 || seen.has(node))
      return fail(node, "default expression is recursive or too deep");
    seen = new Set(seen).add(node);
    if (
      ts.isParenthesizedExpression(node) ||
      ts.isAsExpression(node) ||
      ts.isSatisfiesExpression(node)
    )
      return literal(node.expression, seen);
    if (ts.isStringLiteralLike(node)) return node.text;
    if (ts.isNumericLiteral(node)) return Number(node.text);
    if (node.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (node.kind === ts.SyntaxKind.FalseKeyword) return false;
    if (node.kind === ts.SyntaxKind.NullKeyword) return null;
    if (ts.isIdentifier(node) && node.text === "undefined") return undefined;
    if (
      ts.isPrefixUnaryExpression(node) &&
      [ts.SyntaxKind.MinusToken, ts.SyntaxKind.PlusToken].includes(node.operator)
    ) {
      const value = literal(node.operand, seen);
      if (typeof value === "number")
        return node.operator === ts.SyntaxKind.MinusToken ? -value : value;
    }
    if (ts.isArrayLiteralExpression(node))
      return node.elements.map((item) =>
        ts.isExpression(item) ? literal(item, seen) : fail(item, "invalid array default"),
      );
    if (ts.isObjectLiteralExpression(node)) {
      const output: Record<string, unknown> = Object.create(null);
      for (const property of node.properties) {
        if (!ts.isPropertyAssignment(property) || ts.isComputedPropertyName(property.name))
          return fail(property, "defaults need literal object properties");
        const name = property.name.getText().replace(/^['"]|['"]$/g, "");
        if (["__proto__", "constructor", "prototype"].includes(name))
          return fail(property, "reserved property name");
        output[name] = literal(property.initializer, seen);
      }
      return output;
    }
    if (ts.isIdentifier(node)) {
      const declaration = checker.getSymbolAtLocation(node)?.valueDeclaration;
      if (
        declaration &&
        ts.isVariableDeclaration(declaration) &&
        declaration.initializer &&
        ts.isVariableDeclarationList(declaration.parent) &&
        declaration.parent.flags & ts.NodeFlags.Const
      )
        return literal(declaration.initializer, seen);
    }
    if (ts.isPropertyAccessExpression(node)) {
      const declaration = checker.getSymbolAtLocation(node)?.valueDeclaration;
      if (declaration && ts.isEnumMember(declaration)) {
        const value = checker.getConstantValue(declaration);
        if (value !== undefined) return value;
      }
    }
    return fail(
      node,
      "persisted defaults must be literals or literal constants; put dynamic initialization in start()",
    );
  }
  function descriptor(node: ts.Expression, seen = new Set<ts.Node>()): DataType<any> {
    if (seen.size > 32 || seen.has(node))
      return fail(node, "recursive or deeply nested field descriptor");
    seen = new Set(seen).add(node);
    if (ts.isIdentifier(node)) {
      let symbol = checker.getSymbolAtLocation(node);
      if (symbol && symbol.flags & ts.SymbolFlags.Alias) symbol = checker.getAliasedSymbol(symbol);
      const declaration = symbol?.valueDeclaration;
      if (
        declaration &&
        ts.isVariableDeclaration(declaration) &&
        declaration.initializer &&
        ts.isVariableDeclarationList(declaration.parent) &&
        declaration.parent.flags & ts.NodeFlags.Const
      )
        return descriptor(declaration.initializer, seen);
    }
    if (
      ts.isCallExpression(node) &&
      (apiName(node.expression, "Struct") || apiName(node.expression, "defineStruct"))
    ) {
      const argument = node.arguments[0];
      if (!argument || !ts.isObjectLiteralExpression(argument))
        return fail(node, "Struct fields must be a literal descriptor object");
      const fields: Record<string, DataType<unknown>> = Object.create(null);
      for (const property of argument.properties) {
        if (!ts.isPropertyAssignment(property) || ts.isComputedPropertyName(property.name))
          return fail(property, "Struct fields need literal property names");
        const name = property.name.getText().replace(/^['"]|['"]$/g, "");
        if (Object.hasOwn(fields, name)) return fail(property, "duplicate Struct field");
        fields[name] = descriptor(property.initializer, seen);
      }
      return defineStruct(fields);
    }
    if (ts.isPropertyAccessExpression(node) && apiName(node.expression, "Type")) {
      const type = (Type as any)[node.name.text];
      if (type instanceof DataType) return type;
    }
    if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)) {
      const access = node.expression;
      if (access.name.text === "with" && node.arguments.length === 1) {
        const base = descriptor(access.expression, seen) as any;
        if (typeof base.with === "function") return base.with(literal(node.arguments[0]!));
      }
      if (apiName(access.expression, "Type")) {
        const args = node.arguments;
        switch (access.name.text) {
          case "Array":
            return Type.Array(descriptor(args[0]!, seen), args[1] ? literal(args[1]) : {});
          case "Optional":
            return Type.Optional(descriptor(args[0]!, seen));
          case "Nullable":
            return Type.Nullable(descriptor(args[0]!, seen));
          case "Enum":
            return Type.Enum(literal(args[0]!));
          case "Tuple":
            return Type.Tuple(...args.map((arg) => descriptor(arg, seen)));
        }
      }
    }
    return fail(node, "field schema must use the shared Type descriptors");
  }
  function schemaFor(type: ts.Type, node: ts.Node, seen = new Set<ts.Type>()): Schema {
    if (seen.size > 32 || seen.has(type))
      return fail(node, "recursive field types are unsupported");
    seen = new Set(seen).add(type);
    if (type.isUnion()) {
      const nullType = type.types.find((t) => !!(t.flags & ts.TypeFlags.Null));
      const undefinedType = type.types.find((t) => !!(t.flags & ts.TypeFlags.Undefined));
      const rest = type.types.filter((t) => t !== nullType && t !== undefinedType);
      if (nullType || undefinedType) {
        if (nullType && undefinedType)
          return fail(node, "a field can be optional or nullable, not both");
        const inner = rest.length === 1 ? schemaFor(rest[0]!, node, seen) : union(rest, node);
        return { kind: nullType ? "nullable" : "optional", inner };
      }
      return union(type.types, node);
    }
    if (type.flags & ts.TypeFlags.StringLike) return { kind: "string" };
    if (type.flags & ts.TypeFlags.NumberLike) return { kind: "f64" };
    if (type.flags & ts.TypeFlags.BooleanLike) return { kind: "bool" };
    const symbol = type.getSymbol();
    if (
      ["Entity", "EntityReference", "EntityRef", "PendingEntity"].includes(symbol?.name ?? "") &&
      symbol?.declarations?.some((d) => resolve(d.getSourceFile().fileName) === apiPath)
    )
      return { kind: "entity" };
    if (checker.isTupleType(type))
      return {
        kind: "tuple",
        elements: checker
          .getTypeArguments(type as ts.TypeReference)
          .map((t) => schemaFor(t, node, seen)),
      };
    if (checker.isArrayType(type))
      return {
        kind: "array",
        element: schemaFor(checker.getTypeArguments(type as ts.TypeReference)[0]!, node, seen),
      };
    if (
      type.flags & ts.TypeFlags.Object &&
      !type.getCallSignatures().length &&
      !type.getConstructSignatures().length
    ) {
      const fields = checker.getPropertiesOfType(type).map((property) => {
        const declaration = property.valueDeclaration ?? property.declarations?.[0];
        if (!declaration || ts.isMethodDeclaration(declaration) || ts.isGetAccessor(declaration))
          return fail(node, "nested fields must contain plain data");
        return {
          name: property.name,
          schema: schemaFor(
            checker.getTypeOfSymbolAtLocation(property, declaration),
            declaration,
            seen,
          ),
        };
      });
      return { kind: "struct", fields };
    }
    return fail(
      node,
      `unsupported persisted field type '${checker.typeToString(type)}'; use a supported type or make the field private`,
    );
  }
  function union(types: readonly ts.Type[], node: ts.Node): Schema {
    if (types.every((t) => t.flags & ts.TypeFlags.BooleanLiteral)) return { kind: "bool" };
    if (types.every((t) => t.isStringLiteral()))
      return { kind: "enum", values: types.map((t) => (t as ts.StringLiteralType).value) };
    return fail(node, "only string choices and nullable/optional unions are supported");
  }
  const result: ComponentDefinition[] = [];
  const ids = new Set<string>();
  for (const source of program.getSourceFiles()) {
    if (source.isDeclarationFile) continue;
    for (const node of source.statements) {
      if (
        !ts.isClassDeclaration(node) ||
        !node.name ||
        !isComponent(checker.getTypeAtLocation(node))
      )
        continue;
      const flags = ts.getCombinedModifierFlags(node);
      if (flags & ts.ModifierFlags.Abstract) continue;
      if (!(flags & ts.ModifierFlags.Export))
        return fail(node, "component classes must be exported");
      const idField = node.members.find(
        (member) =>
          ts.isPropertyDeclaration(member) &&
          member.name.getText() === "type" &&
          ts.getCombinedModifierFlags(member) & ts.ModifierFlags.Static,
      ) as ts.PropertyDeclaration | undefined;
      const id = idField?.initializer ? literal(idField.initializer) : `${owner}:${node.name.text}`;
      if (
        typeof id !== "string" ||
        !id.startsWith(owner + ":") ||
        !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(id.slice(owner.length + 1))
      )
        return fail(node, "component type must be mod_id:name");
      if (ids.has(id)) return fail(node, `duplicate component '${id}'`);
      ids.add(id);
      const constructorType = checker.getTypeOfSymbolAtLocation(
        checker.getSymbolAtLocation(node.name)!,
        node,
      );
      for (const signature of constructorType.getConstructSignatures()) {
        if (
          signature.parameters.some((parameter) => {
            const declaration = parameter.valueDeclaration;
            return (
              declaration &&
              ts.isParameter(declaration) &&
              !declaration.initializer &&
              !declaration.questionToken &&
              !declaration.dotDotDotToken
            );
          })
        )
          return fail(
            node,
            "component constructors must work without arguments; initialize runtime state in start()",
          );
      }
      const fields: ComponentDefinition["fields"] = [];
      for (const property of checker.getPropertiesOfType(checker.getTypeAtLocation(node))) {
        const declaration = property.valueDeclaration;
        if (!declaration || !ts.isPropertyDeclaration(declaration)) continue;
        if (
          ts.getCombinedModifierFlags(declaration) &
            (ts.ModifierFlags.Static |
              ts.ModifierFlags.Private |
              ts.ModifierFlags.Protected |
              ts.ModifierFlags.Readonly) ||
          ts.isPrivateIdentifier(declaration.name)
        )
          continue;
        if (ts.isComputedPropertyName(declaration.name))
          return fail(declaration, "component fields need literal names");
        if (!declaration.initializer && !declaration.questionToken)
          return fail(declaration, "persisted fields need a default initializer");
        let schema: Schema;
        let initial: unknown;
        let editor: FieldEditor = {};
        if (
          declaration.initializer &&
          ts.isCallExpression(declaration.initializer) &&
          apiName(declaration.initializer.expression, "field")
        ) {
          const [typeArg, defaultArg, editorArg] = declaration.initializer.arguments;
          if (!typeArg || !defaultArg) return fail(declaration, "field needs a type and default");
          if (
            declaration.type &&
            !checker.isTypeAssignableTo(
              checker.getTypeAtLocation(declaration.initializer),
              checker.getTypeOfSymbolAtLocation(property, declaration),
            )
          )
            return fail(declaration, "field descriptor conflicts with its TypeScript annotation");
          schema = descriptor(typeArg).schema;
          initial = literal(defaultArg);
          editor = editorArg ? literal(editorArg) : {};
          if (
            !editor ||
            Object.keys(editor).some((key) => !["label", "tooltip"].includes(key)) ||
            Object.values(editor).some((value) => typeof value !== "string")
          )
            return fail(declaration, "invalid inspector metadata");
        } else {
          schema = schemaFor(checker.getTypeOfSymbolAtLocation(property, declaration), declaration);
          initial = declaration.initializer ? literal(declaration.initializer) : undefined;
        }
        const description = ts.displayPartsToString(property.getDocumentationComment(checker));
        if (description && !editor.tooltip) editor.tooltip = description;
        fields.push({ name: property.name, schema, default: toWire(schema, initial), editor });
      }
      result.push({
        id,
        source: relative(root, source.fileName).split(sep).join("/"),
        export: flags & ts.ModifierFlags.Default ? "default" : node.name.text,
        fields,
      });
    }
  }
  return result.sort((a, b) => a.id.localeCompare(b.id));
}
