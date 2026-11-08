let files: Record<string, string> = Object.create(null);
export function setSources(sources: Record<string, string>): void {
  files = Object.fromEntries(
    Object.entries(sources).map(([name, text]) => [resolve("/mod", name), text]),
  );
}
export const sep = "/";
export function resolve(...parts: string[]): string {
  const segments: string[] = [];
  for (const part of parts) {
    if (part.startsWith("/")) segments.length = 0;
    for (const segment of part.split("/")) {
      if (segment === "..") segments.pop();
      else if (segment && segment !== ".") segments.push(segment);
    }
  }
  return "/" + segments.join("/");
}
export const realpathSync = resolve;
export const isAbsolute = (path: string): boolean => path.startsWith("/");
export const extname = (path: string): string => /\.[^/.]+$/.exec(path)?.[0] ?? "";
export function relative(root: string, path: string): string {
  root = resolve(root);
  path = resolve(path);
  if (path === root) return "";
  if (!path.startsWith(root + "/")) return "../" + path.slice(1);
  return path.slice(root.length + 1);
}
export function readFileSync(path: string): string {
  const source = files[resolve(path)];
  if (source === undefined) throw Error("Source is unavailable: " + path);
  return source;
}
export function readdirSync(
  directory: string,
): { name: string; isDirectory(): boolean; isSymbolicLink(): boolean }[] {
  const prefix = resolve(directory) + "/";
  const names = new Map<string, boolean>();
  for (const path of Object.keys(files)) {
    if (!path.startsWith(prefix)) continue;
    const tail = path.slice(prefix.length),
      name = tail.split("/")[0]!;
    names.set(name, names.get(name) === true || tail.includes("/"));
  }
  return [...names].map(([name, directory]) => ({
    name,
    isDirectory: () => directory,
    isSymbolicLink: () => false,
  }));
}
