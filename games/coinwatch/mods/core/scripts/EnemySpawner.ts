import type { RunSession } from "./RunSession.ts";
import {
  BuiltInComponents as C,
  Component,
  Engine,
  Physics,
  Prefab,
  Scene,
  Type,
  field,
  type Entity,
} from "@whatever-engine/api";
import { EnemyNavigation } from "./EnemyNavigation.ts";
import { Health } from "./Health.ts";
import { ContactDamage } from "./ContactDamage.ts";
import { spawnNodes } from "./NavNode.ts";
import { distance } from "./navigation.ts";
import { playing, position, resolve, run, type Ref } from "./session.ts";
export type SpawnContext = {
  session: RunSession;
  world: Entity;
  target: Entity;
  graph: Entity;
  prefab: string;
};
/** Composition at the spawn boundary; behavior/tuning remains in the prefab's components. */
export async function spawnEnemy(context: SpawnContext, at: Physics.Vec3): Promise<Entity | null> {
  const generation = run(context.session)?.generation;
  const instance = await Prefab.spawn(context.prefab),
    root = instance.root;
  if (!playing(context.session) || generation !== run(context.session)?.generation) {
    root.destroy();
    return null;
  }
  root.setComponent(new C.Transform({ position: [at[0], 1, at[2]] })).addTag("enemy");
  root.setComponent(
    Physics.RigidBody.dynamic({
      world: context.world,
      lockedAxes: ["rotationX", "rotationY", "rotationZ"],
      canSleep: false,
      ccd: true,
    }),
  );
  root.setComponent(
    new Physics.Collider({
      shape: { type: "capsule", radius: 0.4, height: 1.8 },
      friction: 0,
      mass: 75,
      events: ["collision"],
    }),
  );
  root.editComponent(EnemyNavigation, (c) => {
    c.session = context.session;
    c.world = context.world;
    c.target = context.target;
    c.graph = context.graph;
  });
  root.editComponent(Health, (c) => {
    c.session = context.session;
  });
  root.editComponent(ContactDamage, (c) => {
    c.session = context.session;
    c.target = context.target;
  });
  return root;
}
export class EnemySpawner extends Component {
  static readonly type = "core:EnemySpawner";
  session: RunSession | null = null;
  world = field(Type.Nullable(Type.Entity), null);
  target = field(Type.Nullable(Type.Entity), null);
  graph = field(Type.Nullable(Type.Entity), null);
  prefab = field(Type.Asset, "core://prefabs/sentinel.wprefab");
  enabled = true;
  interval = field(Type.F64.with({ min: 0.25, max: 60 }), 3);
  firstDelay = field(Type.F64.with({ min: 0, max: 120 }), 5);
  maxEnemies = field(Type.I32.with({ min: 1, max: 64 }), 12);
  minDistance = field(Type.F64.with({ min: 2, max: 60 }), 14);
  separation = field(Type.F64.with({ min: 0, max: 20 }), 2);
  private timer = 0;
  private generation = -1;
  private spawning = false;
  override tick(dt: number): void {
    const state = run(this.session);
    if (!state) return;
    if (state.generation !== this.generation) {
      this.generation = state.generation;
      this.timer = this.firstDelay;
    }
    if (!playing(this.session)) return;
    this.timer -= dt;
    if (!this.enabled || this.timer > 0 || this.spawning) return;
    const world = resolve(this.world),
      target = resolve(this.target),
      graph = resolve(this.graph);
    const enemies = Scene.findByTag("enemy");
    if (!world || !target || !graph || !this.session || enemies.length >= this.maxEnemies) return;
    const p = position(target);
    const available = spawnNodes(graph).filter(
      (n) =>
        distance(position(n), p) >= this.minDistance &&
        enemies.every((e) => distance(position(e), position(n)) > this.separation),
    );
    if (!available.length) return;
    this.timer = this.interval;
    this.spawning = true;
    void spawnEnemy(
      { session: this.session, world, target, graph, prefab: this.prefab },
      position(available[Math.floor(Math.random() * available.length)]),
    )
      .catch((error) => Engine.log("error", `Enemy spawn failed: ${String(error)}`))
      .finally(() => {
        this.spawning = false;
      });
  }
}
