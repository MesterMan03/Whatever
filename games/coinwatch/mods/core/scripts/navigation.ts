export type Point = readonly [number, number, number];
export type Node = { key: string; position: Point; links: readonly string[] };
export const distance = (a: Point, b: Point): number => Math.hypot(a[0] - b[0], a[2] - b[2]);
export const clamp = (x: number, low: number, high: number): number =>
  Math.max(low, Math.min(high, x));
export function nearest(nodes: readonly Node[], p: Point): Node | undefined {
  return nodes.reduce<Node | undefined>(
    (best, n) => (!best || distance(n.position, p) < distance(best.position, p) ? n : best),
    undefined,
  );
}
/** Dijkstra over authored links; disconnected destinations never produce a direct shortcut. */
export function route(nodes: readonly Node[], start: string, goal: string): Node[] {
  const byKey = new Map(nodes.map((n) => [n.key, n]));
  if (!byKey.has(start) || !byKey.has(goal)) return [];
  const costs = new Map([[start, 0]]),
    previous = new Map<string, string>();
  const open = new Set([start]);
  while (open.size) {
    const key = [...open].reduce((a, b) => (costs.get(a)! < costs.get(b)! ? a : b));
    open.delete(key);
    if (key === goal) {
      const path = [byKey.get(key)!];
      let cursor = key;
      while (previous.has(cursor)) {
        cursor = previous.get(cursor)!;
        path.unshift(byKey.get(cursor)!);
      }
      return path;
    }
    const node = byKey.get(key)!;
    for (const link of node.links) {
      const other = byKey.get(link);
      if (!other) continue;
      const cost = costs.get(key)! + distance(node.position, other.position);
      if (cost < (costs.get(link) ?? Infinity)) {
        costs.set(link, cost);
        previous.set(link, key);
        open.add(link);
      }
    }
  }
  return [];
}
export type HuntState = { mode: "roam" | "chase" | "cooldown"; remaining: number };
/** A full chase is followed by an unconditional roaming cooldown, even in continuous sight. */
export function advanceHunt(
  state: HuntState,
  dt: number,
  visible: boolean,
  chase: number,
  cooldown: number,
): HuntState {
  if (state.mode === "roam") return visible ? { mode: "chase", remaining: chase } : state;
  const left = state.remaining - dt;
  if (left > 0) return { mode: state.mode, remaining: left };
  if (state.mode === "chase") return { mode: "cooldown", remaining: cooldown };
  return visible ? { mode: "chase", remaining: chase } : { mode: "roam", remaining: 0 };
}
/** Last projectile sweep is clipped to the remaining range, rather than overshooting it. */
export const sweepLength = (speed: number, dt: number, travelled: number, range: number): number =>
  Math.max(0, Math.min(speed * dt, range - travelled));
